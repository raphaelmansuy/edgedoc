#!/usr/bin/env python3
"""Draw the PNG assets that the examples embed.

Run this file from the repository root:

    python3 tools/make_assets.py

The script uses only the standard library. It writes a logo and a bar chart into
`crates/edgedoc-demos/assets/`. The output is deterministic, so a rebuild
produces the same bytes.
"""

import os
import struct
import zlib

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
OUT = os.path.join(ROOT, "crates", "edgedoc-demos", "assets")

INK = (17, 24, 39, 255)          # near black
NAVY = (30, 58, 138, 255)        # brand blue
INDIGO = (79, 70, 229, 255)      # accent
SLATE = (148, 163, 184, 255)     # gridline
MIST = (241, 245, 249, 255)      # panel
WHITE = (255, 255, 255, 255)
TEAL = (13, 148, 136, 255)
LILAC = (199, 210, 254, 255)
PERIWINKLE = (129, 140, 248, 255)


class Canvas:
    def __init__(self, width, height, background=(255, 255, 255, 0)):
        self.width = width
        self.height = height
        self.pixels = [[background for _ in range(width)] for _ in range(height)]

    def blend(self, x, y, colour):
        if not (0 <= x < self.width and 0 <= y < self.height):
            return
        r, g, b, a = colour
        if a == 255:
            self.pixels[y][x] = colour
            return
        br, bg, bb, ba = self.pixels[y][x]
        weight = a / 255.0
        mix = (
            round(r * weight + br * (1 - weight)),
            round(g * weight + bg * (1 - weight)),
            round(b * weight + bb * (1 - weight)),
            max(ba, a),
        )
        self.pixels[y][x] = mix

    def rect(self, x, y, width, height, colour):
        for row in range(y, y + height):
            for col in range(x, x + width):
                self.blend(col, row, colour)

    def rounded(self, x, y, width, height, radius, colour):
        for row in range(y, y + height):
            for col in range(x, x + width):
                dx = min(col - x, x + width - 1 - col)
                dy = min(row - y, y + height - 1 - row)
                if dx < radius and dy < radius:
                    ox = radius - dx
                    oy = radius - dy
                    if ox * ox + oy * oy > radius * radius:
                        continue
                self.blend(col, row, colour)

    def write(self, path):
        raw = bytearray()
        for row in self.pixels:
            raw.append(0)
            for pixel in row:
                raw.extend(pixel)
        payload = zlib.compress(bytes(raw), 9)

        def chunk(kind, data):
            body = kind + data
            return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

        header = struct.pack(">IIBBBBB", self.width, self.height, 8, 6, 0, 0, 0)
        png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", payload) + chunk(b"IEND", b"")
        with open(path, "wb") as handle:
            handle.write(png)
        return len(png)


def logo():
    canvas = Canvas(420, 96)
    canvas.rounded(0, 0, 96, 96, 22, NAVY)
    # Three ascending bars read as a report mark at any size.
    canvas.rounded(22, 56, 12, 24, 3, WHITE)
    canvas.rounded(42, 42, 12, 38, 3, LILAC)
    canvas.rounded(62, 26, 12, 54, 3, PERIWINKLE)
    # Wordmark: abstract letterforms, no font dependency.
    canvas.rounded(124, 28, 10, 40, 5, INDIGO)
    canvas.rounded(146, 28, 34, 9, 4, INK)
    canvas.rounded(146, 44, 26, 9, 4, INK)
    canvas.rounded(146, 60, 34, 9, 4, INK)
    canvas.rounded(196, 28, 9, 41, 4, SLATE)
    canvas.rounded(216, 28, 10, 40, 5, INK)
    canvas.rounded(234, 28, 28, 10, 5, INK)
    canvas.rounded(234, 44, 10, 24, 5, INK)
    canvas.rounded(234, 60, 28, 9, 4, INK)
    return canvas.write(os.path.join(OUT, "logo.png"))


def chart():
    width, height = 760, 340
    canvas = Canvas(width, height)
    canvas.rect(0, 0, width, height, WHITE)
    left, right, top, bottom = 56, width - 24, 28, height - 48
    # Horizontal gridlines with a light fill behind the plot.
    steps = 5
    for step in range(steps + 1):
        y = bottom - round((bottom - top) * step / steps)
        canvas.rect(left, y, right - left, 1, SLATE if step else INK)
    canvas.rect(left, top, 1, bottom - top, SLATE)

    groups = [(0.42, 0.55), (0.68, 0.61), (0.54, 0.78), (0.86, 0.92)]
    slot = (right - left) / len(groups)
    bar = 34
    gap = 10
    for index, (first, second) in enumerate(groups):
        centre = left + slot * index + slot / 2
        for offset, value, colour in ((-(bar + gap / 2), first, SLATE), (gap / 2, second, NAVY)):
            x = round(centre + offset)
            bar_height = round((bottom - top) * value)
            canvas.rounded(x, bottom - bar_height, bar, bar_height, 6, colour)
    canvas.rect(left, bottom, right - left, 2, INK)
    return canvas.write(os.path.join(OUT, "chart.png"))


def seal():
    canvas = Canvas(200, 200)
    canvas.rounded(0, 0, 200, 200, 100, MIST)
    canvas.rounded(28, 28, 144, 144, 72, TEAL)
    canvas.rounded(86, 58, 28, 84, 10, WHITE)
    return canvas.write(os.path.join(OUT, "seal.png"))


def main():
    os.makedirs(OUT, exist_ok=True)
    for name, writer in (("logo.png", logo), ("chart.png", chart), ("seal.png", seal)):
        size = writer()
        print(f"{name}: {size} bytes")


if __name__ == "__main__":
    main()
