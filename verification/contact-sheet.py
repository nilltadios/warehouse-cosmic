"""Tile layout-sweep captures into one labelled sheet per page.

uv run --python 3.15 verification/contact-sheet.py SWEEP_DIR
Writes SWEEP_DIR/sheet-<page>.png. Tiles are labelled with the size requested
in the file name; each keeps the aspect ratio of the actual capture, so a
window that stopped at its minimum size is not drawn squeezed.
"""
import pathlib, re, sys
from PIL import Image, ImageDraw

SCALE = 0.42
d = pathlib.Path(sys.argv[1])
shots = {}
for p in d.glob("*.png"):
    m = re.fullmatch(r"(.+)-(\d+)x(\d+)", p.stem)
    if m and not p.stem.startswith("sheet"):
        shots.setdefault(m[1], []).append((int(m[2]), int(m[3]), p))
for page, items in shots.items():
    items.sort(key=lambda t: -t[0] * t[1])
    tiles = []
    for w, h, p in items:
        im = Image.open(p).convert("RGB")
        width = round(w * SCALE * 2)  # 2 px per requested logical px
        im = im.resize((width, round(width * im.height / im.width)))
        tiles.append((f"asked {w}x{h}", im))
    pad, cols = 16, 4
    rows = [tiles[i:i + cols] for i in range(0, len(tiles), cols)]
    width = max(sum(t.width for _, t in r) + pad * (len(r) + 1) for r in rows)
    height = sum(max(t.height for _, t in r) + pad + 24 for r in rows) + pad
    sheet = Image.new("RGB", (width, height), (90, 90, 90))
    draw, y = ImageDraw.Draw(sheet), pad
    for r in rows:
        x = pad
        for label, t in r:
            draw.text((x, y), label, fill=(255, 255, 255))
            sheet.paste(t, (x, y + 20))
            x += t.width + pad
        y += max(t.height for _, t in r) + pad + 24
    sheet.save(d / f"sheet-{page}.png")
    print(d / f"sheet-{page}.png", sheet.size)
