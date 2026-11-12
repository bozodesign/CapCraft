"""Build the interface font for Thai: Kanit-UI.ttf plus Kanit-UI.map.

The interface (egui) draws letters one at a time and never asks the font where a vowel or tone
mark should go, so stacked marks collide. This takes the shaped positions printed by
`cargo run --example thai_shape` and gives every placement that differs from the plain letter
its own private-use character (U+E000 up), drawn already in place. The map lists, for each
letter-with-marks, the characters to draw instead (the same count, so text boxes still line up).

    cargo run -q --example thai_shape > shape.tsv
    python tools/thai_ui_font.py shape.tsv assets/fonts/Kanit-Regular.ttf assets/fonts
"""
import sys, os
from fontTools.ttLib import TTFont
from fontTools.ttLib.tables._g_l_y_f import Glyph, GlyphComponent

tsv, src, outdir = sys.argv[1], sys.argv[2], sys.argv[3]
font = TTFont(src)
order = font.getGlyphOrder()
cmap = font.getBestCmap()
glyf = font['glyf']
hmtx = font['hmtx']

made = {}          # placement -> new code point
new_cmap = {}
next_cp = 0xE000
lines = []

def code_for(entries, adv):
    global next_cp
    key = (tuple(entries), adv)
    if key in made:
        return made[key]
    cp = next_cp
    next_cp += 1
    assert next_cp < 0xF8FF
    if len(entries) == 1 and entries[0][1:] == (0, 0) and hmtx[order[entries[0][0]]][0] == adv:
        # an alternate glyph the font already has, used as it is
        new_cmap[cp] = order[entries[0][0]]
    else:
        name = 'thui%04X' % cp
        g = Glyph()
        if entries:
            g.numberOfContours = -1
            g.components = []
            for gid, dx, dy in entries:
                c = GlyphComponent()
                c.glyphName = order[gid]
                c.x, c.y = dx, dy
                c.flags = 0x0004  # ROUND_XY_TO_GRID
                g.components.append(c)
        else:
            g.numberOfContours = 0
        glyf.glyphs[name] = g
        order.append(name)
        hmtx.metrics[name] = (adv, 0)
        new_cmap[cp] = name
    made[key] = cp
    return cp

for line in open(tsv, encoding='utf-8'):
    line = line.strip()
    if not line:
        continue
    key, parts = line.split('\t')
    chars = [int(h, 16) for h in key.split('-')]
    out = []
    changed = False
    for ch, part in zip(chars, parts.split(';')):
        bits = part.split('|')
        adv = int(bits[0])
        entries = [tuple(int(v) for v in b.split(',')) for b in bits[1:]]
        plain = ch in cmap and entries == [(order.index(cmap[ch]), 0, 0)] and hmtx[cmap[ch]][0] == adv
        if plain:
            out.append(ch)
        else:
            out.append(code_for(entries, adv))
            changed = True
    if changed:
        lines.append('%s\t%s' % (key, '-'.join('%04X' % c for c in out)))

font.setGlyphOrder(order)
for name in [n for n in order if n.startswith('thui')]:
    glyf[name].recalcBounds(glyf)
    hmtx.metrics[name] = (hmtx.metrics[name][0], getattr(glyf[name], 'xMin', 0))
for t in font['cmap'].tables:
    if t.isUnicode():
        t.cmap.update(new_cmap)
if 'DSIG' in font:
    del font['DSIG']
# a different family name, so it never passes for the original font
for rec in font['name'].names:
    if rec.nameID in (1, 3, 4, 6, 16):
        s = rec.toUnicode()
        rec.string = s.replace('Kanit', 'KanitUI' if rec.nameID == 6 else 'Kanit UI')
font.save(os.path.join(outdir, 'Kanit-UI.ttf'))
with open(os.path.join(outdir, 'Kanit-UI.map'), 'w', encoding='ascii', newline='\n') as f:
    f.write('# letter with marks (code points) -> what the interface draws instead; made by tools/thai_ui_font.py\n')
    f.write('\n'.join(lines) + '\n')
print('glyph codes', next_cp - 0xE000, 'clusters', len(lines))
