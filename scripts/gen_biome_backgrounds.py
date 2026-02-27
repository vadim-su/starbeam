#!/usr/bin/env python3
"""Generate placeholder background PNGs for biome parallax layers.

Creates simple gradient/colored images for each surface biome's parallax
backgrounds. These are developer placeholders — replace with real art later.

Usage:
    python3 scripts/gen_biome_backgrounds.py
"""

from pathlib import Path

from PIL import Image, ImageDraw

ASSETS_DIR = Path(__file__).resolve().parent.parent / "assets"
BIOMES_DIR = ASSETS_DIR / "world" / "biomes"

# Sky images are 1280x720, foreground layers are 640x360
SKY_SIZE = (1280, 720)
LAYER_SIZE = (640, 360)


def vertical_gradient(
    size: tuple[int, int], top_color: str, bottom_color: str
) -> Image.Image:
    """Create an RGBA image with a vertical gradient from top_color to bottom_color."""
    width, height = size
    img = Image.new("RGBA", size)
    draw = ImageDraw.Draw(img)

    r1, g1, b1 = _hex_to_rgb(top_color)
    r2, g2, b2 = _hex_to_rgb(bottom_color)

    for y in range(height):
        t = y / max(height - 1, 1)
        r = int(r1 + (r2 - r1) * t)
        g = int(g1 + (g2 - g1) * t)
        b = int(b1 + (b2 - b1) * t)
        draw.line([(0, y), (width, y)], fill=(r, g, b, 255))

    return img


def silhouette(size: tuple[int, int], color: str, fill_fraction: float) -> Image.Image:
    """Create an RGBA image with a solid color filling the bottom fraction, transparent above."""
    width, height = size
    img = Image.new("RGBA", size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    fill_start = int(height * (1.0 - fill_fraction))
    r, g, b = _hex_to_rgb(color)
    draw.rectangle([(0, fill_start), (width, height)], fill=(r, g, b, 255))

    return img


def _hex_to_rgb(hex_color: str) -> tuple[int, int, int]:
    """Convert a hex color string like '#87CEEB' to an (R, G, B) tuple."""
    h = hex_color.lstrip("#")
    return (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16))


# Biome definitions: (biome_name, sky_top, sky_bottom, layers)
# Each layer: (filename, color, fill_fraction)
BIOME_SPECS = [
    (
        "meadow",
        "#87CEEB",
        "#4A90D9",
        [
            ("far_hills.png", "#228B22", 0.5),
            ("near_hills.png", "#32CD32", 1.0 / 3.0),
        ],
    ),
    (
        "forest",
        "#6B8FB5",
        "#2C4F7C",
        [
            ("far_trees.png", "#006400", 0.5),
            ("near_trees.png", "#004D00", 1.0 / 3.0),
        ],
    ),
    (
        "rocky",
        "#A0A0A0",
        "#606060",
        [
            ("far_rocks.png", "#808080", 0.5),
            ("near_rocks.png", "#6B5B4F", 1.0 / 3.0),
        ],
    ),
]


def main() -> None:
    for biome_name, sky_top, sky_bottom, layers in BIOME_SPECS:
        bg_dir = BIOMES_DIR / biome_name / "backgrounds"
        bg_dir.mkdir(parents=True, exist_ok=True)

        # Sky gradient
        sky = vertical_gradient(SKY_SIZE, sky_top, sky_bottom)
        sky.save(bg_dir / "sky.png")
        print(f"  {biome_name}/backgrounds/sky.png ({SKY_SIZE[0]}x{SKY_SIZE[1]})")

        # Foreground silhouette layers
        for filename, color, fraction in layers:
            img = silhouette(LAYER_SIZE, color, fraction)
            img.save(bg_dir / filename)
            print(
                f"  {biome_name}/backgrounds/{filename} ({LAYER_SIZE[0]}x{LAYER_SIZE[1]})"
            )

    print("\nDone — generated all placeholder biome backgrounds.")


if __name__ == "__main__":
    main()
