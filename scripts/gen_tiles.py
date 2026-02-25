#!/usr/bin/env python3
"""Generate a simple pixel-art tile atlas for Starbeam.

Output: assets/terrain/tiles.png  (96x32, 3 tiles in a row)
  Index 0 — Grass  (green top + dirt body)
  Index 1 — Dirt   (brown with pebbles)
  Index 2 — Stone  (gray with cracks)
"""

import random
from pathlib import Path
from PIL import Image

TILE = 32
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
    """Grass tile: green blades on top 8px, dirt body below."""
    px = img.load()
    # Fill body with dirt
    for y in range(TILE):
        for x in range(TILE):
            px[ox + x, oy + y] = pick(DIRT_BASE)

    # Add dirt variation — small lighter/darker spots
    for _ in range(25):
        x, y = random.randint(0, TILE - 1), random.randint(8, TILE - 1)
        px[ox + x, oy + y] = pick(DIRT_LIGHT if random.random() > 0.5 else DIRT_PEBBLE)

    # Grass layer (rows 0-7) — green base
    for y in range(8):
        for x in range(TILE):
            px[ox + x, oy + y] = pick(GRASS_BLADE)

    # Grass blade tips — irregular top edge (rows 0-3)
    for x in range(TILE):
        blade_h = random.randint(1, 4)
        for y in range(blade_h):
            px[ox + x, oy + y] = pick(GRASS_TIP)

    # A few extra tall blades
    for _ in range(8):
        x = random.randint(0, TILE - 1)
        for y in range(random.randint(0, 2)):
            px[ox + x, oy + y] = pick(GRASS_TIP)


def gen_dirt(img: Image.Image, ox: int, oy: int):
    """Dirt tile: brown with pebble/root variation."""
    px = img.load()
    for y in range(TILE):
        for x in range(TILE):
            px[ox + x, oy + y] = pick(DIRT_BASE)

    # Lighter spots
    for _ in range(30):
        x, y = random.randint(0, TILE - 1), random.randint(0, TILE - 1)
        px[ox + x, oy + y] = pick(DIRT_LIGHT)

    # Dark pebbles (2x2 clusters)
    for _ in range(6):
        bx, by = random.randint(1, TILE - 3), random.randint(1, TILE - 3)
        c = pick(DIRT_PEBBLE)
        for dy in range(2):
            for dx in range(2):
                px[ox + bx + dx, oy + by + dy] = c

    # Tiny root-like lines
    for _ in range(3):
        sx = random.randint(2, TILE - 4)
        sy = random.randint(2, TILE - 4)
        length = random.randint(3, 6)
        c = pick(DIRT_PEBBLE)
        for i in range(length):
            nx = sx + i
            ny = sy + random.choice([-1, 0, 0, 1])
            if 0 <= nx < TILE and 0 <= ny < TILE:
                px[ox + nx, oy + ny] = c


def gen_stone(img: Image.Image, ox: int, oy: int):
    """Stone tile: gray with cracks and highlights."""
    px = img.load()
    for y in range(TILE):
        for x in range(TILE):
            px[ox + x, oy + y] = pick(STONE_BASE)

    # Light specks
    for _ in range(25):
        x, y = random.randint(0, TILE - 1), random.randint(0, TILE - 1)
        px[ox + x, oy + y] = pick(STONE_LIGHT)

    # Crack lines (horizontal-ish)
    for _ in range(3):
        sx = random.randint(0, TILE // 2)
        sy = random.randint(2, TILE - 3)
        length = random.randint(6, 14)
        c = pick(STONE_CRACK)
        for i in range(length):
            nx = sx + i
            ny = sy + random.choice([-1, 0, 0, 0, 1])
            if 0 <= nx < TILE and 0 <= ny < TILE:
                px[ox + nx, oy + ny] = c

    # Crack lines (vertical-ish)
    for _ in range(2):
        sx = random.randint(2, TILE - 3)
        sy = random.randint(0, TILE // 2)
        length = random.randint(4, 10)
        c = pick(STONE_CRACK)
        for i in range(length):
            nx = sx + random.choice([-1, 0, 0, 0, 1])
            ny = sy + i
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
