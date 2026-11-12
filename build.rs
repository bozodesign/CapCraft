//! Puts the app icon (assets/icon/icon.ico) into the Windows program, so the file, the taskbar
//! and shortcuts show it. No resource compiler is needed: this writes the small object file a
//! resource compiler would make (a `.rsrc` section with the icon group) and links it in.

use std::path::PathBuf;

fn main() {
    let ico = PathBuf::from("assets/icon/icon.ico");
    println!("cargo:rerun-if-changed={}", ico.display());
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let Ok(bytes) = std::fs::read(&ico) else {
        println!("cargo:warning=no app icon at {}", ico.display());
        return;
    };
    let Some(obj) = icon_object(&bytes) else {
        println!("cargo:warning=the app icon isn't a readable .ico file");
        return;
    };
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("app_icon.o");
    std::fs::write(&out, obj).unwrap();
    println!("cargo:rustc-link-arg-bins={}", out.display());
}

/// The pictures in an .ico file: (width, height, bit count, bytes).
fn read_ico(d: &[u8]) -> Option<Vec<(u8, u8, u16, Vec<u8>)>> {
    let u16_at = |o: usize| d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if u16_at(0)? != 0 || u16_at(2)? != 1 {
        return None;
    }
    let n = u16_at(4)? as usize;
    let mut v = vec![];
    for k in 0..n {
        let e = 6 + 16 * k;
        let (w, h) = (*d.get(e)?, *d.get(e + 1)?);
        let bits = u16_at(e + 6)?.max(32);
        let (size, off) = (u32_at(e + 8)? as usize, u32_at(e + 12)? as usize);
        v.push((w, h, bits, d.get(off..off + size)?.to_vec()));
    }
    (!v.is_empty()).then_some(v)
}

fn icon_object(ico: &[u8]) -> Option<Vec<u8>> {
    const RT_ICON: u32 = 3;
    const RT_GROUP_ICON: u32 = 14;
    const LANG: u32 = 0x0409;
    let pics = read_ico(ico)?;
    let n = pics.len();

    // the icon group: the .ico header with each picture's resource number instead of its place
    let mut group = vec![];
    group.extend(0u16.to_le_bytes());
    group.extend(1u16.to_le_bytes());
    group.extend((n as u16).to_le_bytes());
    for (i, (w, h, bits, data)) in pics.iter().enumerate() {
        group.extend([*w, *h, 0, 0]);
        group.extend(1u16.to_le_bytes());
        group.extend(bits.to_le_bytes());
        group.extend((data.len() as u32).to_le_bytes());
        group.extend((i as u16 + 1).to_le_bytes());
    }

    // the resource tree: type -> number -> language -> data
    let dir = |entries: usize| 16 + 8 * entries;
    let root = 0;
    let icon_names = root + dir(2);
    let group_names = icon_names + dir(n);
    let icon_langs = group_names + dir(1);
    let group_lang = icon_langs + n * dir(1);
    let entries = group_lang + dir(1);
    let mut blob_at = entries + 16 * (n + 1);
    let mut blobs: Vec<(usize, Vec<u8>)> = vec![];
    for (_, _, _, data) in &pics {
        blob_at = (blob_at + 7) & !7;
        blobs.push((blob_at, data.clone()));
        blob_at += data.len();
    }
    blob_at = (blob_at + 7) & !7;
    blobs.push((blob_at, group));

    let mut sec = vec![];
    let mut relocs: Vec<u32> = vec![];
    let put_dir = |sec: &mut Vec<u8>, ids: &[(u32, usize)]| {
        sec.extend([0u8; 12]);
        sec.extend(0u16.to_le_bytes());
        sec.extend((ids.len() as u16).to_le_bytes());
        for &(id, sub) in ids {
            sec.extend(id.to_le_bytes());
            sec.extend((sub as u32 | 0x8000_0000).to_le_bytes());
        }
    };
    put_dir(&mut sec, &[(RT_ICON, icon_names), (RT_GROUP_ICON, group_names)]);
    put_dir(&mut sec, &(0..n).map(|i| (i as u32 + 1, icon_langs + i * dir(1))).collect::<Vec<_>>());
    put_dir(&mut sec, &[(1, group_lang)]);
    // language level: data entries are not subdirectories, so no high bit
    for i in 0..=n {
        sec.extend([0u8; 12]);
        sec.extend(0u16.to_le_bytes());
        sec.extend(1u16.to_le_bytes());
        sec.extend(LANG.to_le_bytes());
        sec.extend(((entries + 16 * i) as u32).to_le_bytes());
    }
    assert_eq!(sec.len(), entries);
    for (at, data) in &blobs {
        relocs.push(sec.len() as u32);
        sec.extend((*at as u32).to_le_bytes()); // made into an address by the linker
        sec.extend((data.len() as u32).to_le_bytes());
        sec.extend([0u8; 8]);
    }
    for (at, data) in &blobs {
        sec.resize(*at, 0);
        sec.extend(data);
    }
    sec.resize((sec.len() + 7) & !7, 0);

    // COFF object: header, one section, its relocations, one symbol, empty string table
    let data_at = 20 + 40;
    let reloc_at = data_at + sec.len();
    let sym_at = reloc_at + 10 * relocs.len();
    let mut o = vec![];
    o.extend(0x8664u16.to_le_bytes());
    o.extend(1u16.to_le_bytes());
    o.extend(0u32.to_le_bytes());
    o.extend((sym_at as u32).to_le_bytes());
    o.extend(1u32.to_le_bytes());
    o.extend(0u16.to_le_bytes());
    o.extend(0u16.to_le_bytes());
    o.extend(*b".rsrc\0\0\0");
    o.extend(0u32.to_le_bytes());
    o.extend(0u32.to_le_bytes());
    o.extend((sec.len() as u32).to_le_bytes());
    o.extend((data_at as u32).to_le_bytes());
    o.extend((reloc_at as u32).to_le_bytes());
    o.extend(0u32.to_le_bytes());
    o.extend((relocs.len() as u16).to_le_bytes());
    o.extend(0u16.to_le_bytes());
    // initialized data, readable, 8-byte aligned
    o.extend((0x0000_0040u32 | 0x4000_0000 | 0x0040_0000).to_le_bytes());
    o.extend(&sec);
    for r in &relocs {
        o.extend(r.to_le_bytes());
        o.extend(0u32.to_le_bytes()); // symbol 0: the section
        o.extend(3u16.to_le_bytes()); // IMAGE_REL_AMD64_ADDR32NB
    }
    o.extend(*b".rsrc\0\0\0");
    o.extend(0u32.to_le_bytes());
    o.extend(1i16.to_le_bytes());
    o.extend(0u16.to_le_bytes());
    o.push(3); // static
    o.push(0);
    o.extend(4u32.to_le_bytes());
    Some(o)
}
