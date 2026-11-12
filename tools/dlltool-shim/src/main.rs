//! Stands in for GNU dlltool (which needs an assembler this computer lacks): turns rustc's
//! `dlltool -d x.def -D name.dll -l out.lib -m i386:x86-64 ...` call into
//! `rust-lld -flavor link /lib /def:x.def /out:out.lib /machine:x64`.

use std::path::PathBuf;
use std::process::{exit, Command};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut def = None;
    let mut out = None;
    let mut machine = "x64".to_string();
    let mut i = 0;
    while i < args.len() {
        let next = args.get(i + 1).cloned();
        match args[i].as_str() {
            "-d" | "--input-def" => {
                def = next;
                i += 1;
            }
            "-l" | "--output-lib" => {
                out = next;
                i += 1;
            }
            "-m" | "--machine" => {
                machine = match next.as_deref() {
                    Some("i386") => "x86".into(),
                    Some("arm64") => "arm64".into(),
                    _ => "x64".into(),
                };
                i += 1;
            }
            "-D" | "--dllname" | "--temp-prefix" => i += 1,
            _ => {}
        }
        i += 1;
    }
    let (Some(def), Some(out)) = (def, out) else {
        eprintln!("dlltool-shim: need -d and -l");
        exit(2);
    };
    let sysroot = Command::new("rustc").arg("--print").arg("sysroot").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let mut lld = PathBuf::from(sysroot.unwrap_or_default());
    lld.push("lib/rustlib/x86_64-pc-windows-gnu/bin/rust-lld.exe");
    if !lld.exists() {
        if let Ok(home) = std::env::var("USERPROFILE") {
            lld = PathBuf::from(home).join(".rustup/toolchains/stable-x86_64-pc-windows-gnu/lib/rustlib/x86_64-pc-windows-gnu/bin/rust-lld.exe");
        }
    }
    if !lld.exists() {
        lld = PathBuf::from("rust-lld.exe");
    }
    let st = Command::new(&lld)
        .arg("-flavor")
        .arg("link")
        .arg("/lib")
        .arg(format!("/def:{def}"))
        .arg(format!("/out:{out}"))
        .arg(format!("/machine:{machine}"))
        .status();
    match st {
        Ok(s) if s.success() => {}
        Ok(s) => exit(s.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("dlltool-shim: couldn't run {}: {e}", lld.display());
            exit(1);
        }
    }
}
