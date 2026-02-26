#!/usr/bin/env python3
"""Generate a simple pixel-art tile atlas for Starbeam.

Output: assets/terrain/tiles.png  (24x8, 3 tiles in a row)
  Index 0 — Grass  (green top + dirt body)
  Index 1 — Dirt   (brown with spots)
  Index 2 — Stone  (gray with crack)
"""

import random
from pathlib import Path
from PIL import Image

TILE = 8
random.seed(42)  # deterministic output

# ── Palette ───────────────────────────────────────────────
GRASS_BLADE = [(58, 125, 44), (74, 158, 58), (52, 110, 38)]
GRASS_TIP = [(92, 184, 72), (80, 168, 60)]
DIRT_BASE = [(139, 105, 20), (160, 121, 42), (128, 96, 18)]
DIRT_PEBBLE = [(110, 82, 14), (100, 74, 12)]
DIRT_LIGHT = [(180, 146, 62), (170, 136, 56)]
STONE_BASE = [(107, 107, 107), (120, 120, 120), (96, 96, 96)]
STONE_LIGHT = [(145, 145, 145), (155, 150, 148)]
STONE_CRACK = [(68, 68, 68), (58, 58, 58)]


def pick(palette: list[tuple]) -> tuple:
    return random.choice(palette)


def gen_grass(img: Image.Image, ox: int, oy: int):
    """Grass tile: green blades on top ~3px, dirt body below."""
    px = img.load()
    # Fill body with dirt
    for y in range(TILE):
        for x in range(TILE):
            px[ox + x, oy + y] = pick(DIRT_BASE)

    # Dirt variation — a few lighter/darker spots
    for _ in range(3):
        x, y = random.randint(0, TILE - 1), random.randint(3, TILE - 1)
        px[ox + x, oy + y] = pick(DIRT_LIGHT if random.random() > 0.5 else DIRT_PEBBLE)

    # Grass layer (rows 0-2) — green base
    for y in range(3):
        for x in range(TILE):
            px[ox + x, oy + y] = pick(GRASS_BLADE)

    # Grass blade tips — irregular top edge (rows 0-1)
    for x in range(TILE):
        blade_h = random.randint(0, 1)
        for y in range(blade_h + 1):
            px[ox + x, oy + y] = pick(GRASS_TIP)


def gen_dirt(img: Image.Image, ox: int, oy: int):
    """Dirt tile: brown with spot variation."""
    px = img.load()
    for y in range(TILE):
        for x in range(TILE):
            px[ox + x, oy + y] = pick(DIRT_BASE)

    # Lighter spots
    for _ in range(4):
        x, y = random.randint(0, TILE - 1), random.randint(0, TILE - 1)
        px[ox + x, oy + y] = pick(DIRT_LIGHT)

    # Dark spots
    for _ in range(3):
        x, y = random.randint(0, TILE - 1), random.randint(0, TILE - 1)
        px[ox + x, oy + y] = pick(DIRT_PEBBLE)


def gen_stone(img: Image.Image, ox: int, oy: int):
    """Stone tile: gray with a short crack and highlights."""
    px = img.load()
    for y in range(TILE):
        for x in range(TILE):
            px[ox + x, oy + y] = pick(STONE_BASE)

    # Light specks
    for _ in range(3):
        x, y = random.randint(0, TILE - 1), random.randint(0, TILE - 1)
        px[ox + x, oy + y] = pick(STONE_LIGHT)

    # One short crack
    sx = random.randint(0, 3)
    sy = random.randint(2, 5)
    length = random.randint(3, 5)
    c = pick(STONE_CRACK)
    for i in range(length):
        nx = sx + i
        ny = sy + random.choice([-1, 0, 0, 1])
        if 0 <= nx < TILE and 0 <= ny < TILE:
            px[ox + nx, oy + ny] = c


def main():
    atlas = Image.new("RGBA", (TILE * 3, TILE), (0, 0, 0, 0))

    gen_grass(atlas, 0 * TILE, 0)
    gen_dirt(atlas, 1 * TILE, 0)
    gen_stone(atlas, 2 * TILE, 0)

    out = Path(__file__).resolve().parent.parent / "assets" / "terrain" / "tiles.png"
    out.parent.mkdir(parents=True, exist_ok=True)
    atlas.save(out)
    print(f"Saved {out}  ({atlas.width}x{atlas.height})")


if __name__ == "__main__":
    main()
