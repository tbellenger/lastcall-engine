#!/usr/bin/env python3
"""Draw the original placeholder art for The Gatehouse. Deterministic, no libraries."""

import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent
W, H = 384, 216


def chunk(tag: bytes, data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)


def write_png(path: Path, width: int, height: int, data: bytes, color: int) -> None:
    raw = b"".join(b"\x00" + data[y * width * (4 if color == 6 else 1) : (y + 1) * width * (4 if color == 6 else 1)] for y in range(height))
    ihdr = struct.pack(">IIBBBBB", width, height, 8, color, 0, 0, 0)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(png)


def blank(w: int, h: int, color=(0, 0, 0, 0)) -> bytearray:
    img = bytearray(w * h * 4)
    for i in range(0, len(img), 4):
        img[i : i + 4] = bytes(color)
    return img


def fill(img: bytearray, w: int, x0: int, y0: int, x1: int, y1: int, color) -> None:
    x0, y0 = max(0, x0), max(0, y0)
    h = len(img) // (w * 4)
    x1, y1 = min(w, x1), min(h, y1)
    px = bytes(color)
    for y in range(y0, y1):
        for x in range(x0, x1):
            i = (y * w + x) * 4
            img[i : i + 4] = px


def put(img: bytearray, w: int, x: int, y: int, color) -> None:
    h = len(img) // (w * 4)
    if 0 <= x < w and 0 <= y < h:
        i = (y * w + x) * 4
        img[i : i + 4] = bytes(color)


def porch() -> None:
    img = blank(W, H, (16, 14, 32, 255))
    fill(img, W, 0, 88, W, 150, (48, 40, 58, 255))
    fill(img, W, 0, 150, W, H, (78, 58, 40, 255))
    for x in range(0, W, 14):
        fill(img, W, x, 150, x + 1, H, (54, 40, 28, 255))
    fill(img, W, 120, 64, 156, 78, (96, 92, 104, 255))
    fill(img, W, 124, 78, 152, H, (118, 114, 122, 255))
    fill(img, W, 128, 90, 148, 150, (90, 86, 98, 255))
    cx, cy, rx, ry = 221, 186, 30, 16
    for y in range(cy - ry - 8, cy + ry + 6):
        for x in range(cx - rx - 4, cx + rx + 4):
            dx = (x - cx) / rx
            dy = (y - cy) / ry
            if dx * dx + dy * dy <= 1:
                put(img, W, x, y, (62, 64, 72, 255))
            if dx * dx + dy * dy <= 0.45:
                put(img, W, x, y, (12, 10, 16, 255))
    fill(img, W, 332, 96, 370, 204, (86, 52, 32, 255))
    fill(img, W, 338, 108, 364, 170, (42, 24, 18, 255))
    fill(img, W, 348, 132, 354, 140, (212, 176, 64, 255))
    fill(img, W, 64, 172, 122, 180, (112, 74, 40, 255))
    fill(img, W, 68, 180, 76, 198, (72, 48, 28, 255))
    fill(img, W, 110, 180, 118, 198, (72, 48, 28, 255))
    fill(img, W, 28, 16, 58, 40, (232, 226, 196, 255))
    write_png(ROOT / "rooms" / "porch" / "picture.png", W, H, bytes(img), 6)

    paint = bytearray(W * H)
    for y in range(90, 211):
        for x in range(120, 156):
            paint[y * W + x] = 11
    write_png(ROOT / "rooms" / "porch" / "priority.png", W, H, bytes(paint), 0)

    key = blank(12, 8)
    gold = (212, 176, 64, 255)
    for x in range(1, 8):
        put(key, 12, x, 3, gold)
        put(key, 12, x, 4, gold)
    for y in range(1, 7):
        put(key, 12, 2, y, gold)
        put(key, 12, 5, y, gold)
    put(key, 12, 8, 2, gold)
    put(key, 12, 8, 5, gold)
    put(key, 12, 10, 2, gold)
    put(key, 12, 10, 5, gold)
    write_png(ROOT / "rooms" / "porch" / "props.png", 12, 8, bytes(key), 6)


def yard() -> None:
    img = blank(W, H, (14, 16, 28, 255))
    fill(img, W, 0, 150, W, H, (58, 72, 48, 255))
    fill(img, W, 0, 100, W, 150, (36, 40, 48, 255))
    for x in range(24, 360, 16):
        fill(img, W, x, 108, x + 3, 156, (24, 26, 32, 255))
    fill(img, W, 24, 108, 360, 114, (24, 26, 32, 255))
    fill(img, W, 70, 168, 200, 200, (86, 78, 52, 255))
    write_png(ROOT / "rooms" / "yard" / "picture.png", W, H, bytes(img), 6)


def hero_sheet() -> None:
    fw, fh, count = 16, 24, 12
    img = blank(fw * count, fh)
    coat = (36, 72, 176, 255)
    hat = (20, 16, 28, 255)
    skin = (214, 164, 120, 255)
    pants = (28, 30, 48, 255)
    shoe = (12, 10, 14, 255)
    # 0-2 south, 3-5 north, 6-8 east, 9-11 west. Odd walk frames shift a pixel.
    for index, facing in enumerate(["s", "s", "s", "n", "n", "n", "e", "e", "e", "w", "w", "w"]):
        ox = index * fw
        shift = 1 if index % 3 == 2 else 0
        for y in range(2, 7):
            for x in range(4, 12):
                put(img, fw * count, ox + x, y, hat)
        if facing != "n":
            for y in range(7, 12):
                for x in range(5, 11):
                    put(img, fw * count, ox + x, y, skin)
        if facing == "s":
            put(img, fw * count, ox + 6, 9, (40, 24, 20, 255))
            put(img, fw * count, ox + 9, 9, (40, 24, 20, 255))
        elif facing == "e":
            put(img, fw * count, ox + 10, 9, (40, 24, 20, 255))
        elif facing == "w":
            put(img, fw * count, ox + 5, 9, (40, 24, 20, 255))
        for y in range(12, 20):
            for x in range(4, 12):
                put(img, fw * count, ox + x, y + (shift if y > 16 else 0), coat)
        for x in range(5, 8):
            put(img, fw * count, ox + x + shift, 20, pants)
            put(img, fw * count, ox + x + 3 - shift, 20, pants)
            put(img, fw * count, ox + x + shift, 22, shoe)
            put(img, fw * count, ox + x + 3 - shift, 22, shoe)
    write_png(ROOT / "actors" / "hero" / "sheet.png", fw * count, fh, bytes(img), 6)


if __name__ == "__main__":
    porch()
    yard()
    hero_sheet()
