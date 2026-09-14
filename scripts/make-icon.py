#!/usr/bin/env python3
"""
Generate the source app icon by rasterising the canonical clip-path polygon.

Drawing the icon by hand would put a second, drifting definition of Clawd's
silhouette in the repo. This reads the same polygon the renderer uses, so the
icon cannot fall out of step with the sprite.
"""
import struct
import zlib
from pathlib import Path

SIZE = 1024
BODY = (0xE8, 0x74, 0x3B, 0xFF)
INK = (0x17, 0x14, 0x0F, 0xFF)

# Straight from --clawd in renderer/clawd.css, as (x%, y%).
POLYGON = [
    (18.75, 21.875), (81.25, 21.875), (81.25, 37.5), (96.875, 37.5),
    (96.875, 53.125), (81.25, 53.125), (81.25, 87.5), (68.75, 87.5),
    (68.75, 71.875), (64.583, 71.875), (64.583, 87.5), (52.083, 87.5),
    (52.083, 71.875), (47.917, 71.875), (47.917, 87.5), (35.417, 87.5),
    (35.417, 71.875), (31.25, 71.875), (31.25, 87.5), (18.75, 87.5),
    (18.75, 53.125), (3.125, 53.125), (3.125, 37.5), (18.75, 37.5),
]
EYES = [(26.5, 29.5, 9.5, 9.5), (64.0, 29.5, 9.5, 9.5)]


def inside(x, y, poly):
    hit = False
    j = len(poly) - 1
    for i, (xi, yi) in enumerate(poly):
        xj, yj = poly[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi) + xi:
            hit = not hit
        j = i
    return hit


def render(size):
    px = [[(0, 0, 0, 0)] * size for _ in range(size)]
    scale = 100.0 / size
    for row in range(size):
        y = (row + 0.5) * scale
        for col in range(size):
            x = (col + 0.5) * scale
            if inside(x, y, POLYGON):
                px[row][col] = BODY
    for ex, ey, ew, eh in EYES:
        for row in range(int(ey * size / 100), int((ey + eh) * size / 100)):
            for col in range(int(ex * size / 100), int((ex + ew) * size / 100)):
                px[row][col] = INK
    return px


def write_png(path, px):
    size = len(px)
    raw = b"".join(
        b"\x00" + b"".join(bytes(p) for p in row) for row in px
    )

    def chunk(tag, data):
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    Path(path).write_bytes(png)


if __name__ == "__main__":
    out = Path(__file__).resolve().parent.parent / "src-tauri" / "app-icon.png"
    out.parent.mkdir(parents=True, exist_ok=True)
    write_png(out, render(SIZE))
    print(f"wrote {out} ({out.stat().st_size} bytes)")
