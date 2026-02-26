#!/usr/bin/env python3
"""Generate a 47-tile blob tileset with variants.

Modes:
  generate  JSON manifest or 5x4 PNG template -> spritesheet + RON mapping
  split     5x4 PNG template -> individual tile PNGs + JSON manifest

Usage:
    python autotile47.py tileset.json -o output.png
    python autotile47.py template.png -o output.png
    python autotile47.py --split template.png -o tiles_dir/
    python autotile47.py tilesets/*.json --output-dir generated/
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import NamedTuple

from PIL import Image, ImageDraw

# ---------------------------------------------------------------------------
# Constants: 8-bit bitmask neighbor directions
# ---------------------------------------------------------------------------

N = 1
NE = 2
E = 4
SE = 8
S = 16
SW = 32
W = 64
NW = 128

QUADRANT_OFFSETS: dict[str, tuple[int, int]] = {
    "TL": (0, 0),
    "TR": (0, 1),
    "BL": (1, 0),
    "BR": (1, 1),
}


class QuadrantRule(NamedTuple):
    """Which neighbors and source tiles to check for one output quadrant."""

    cardinal1: int  # vertical   (N for TL/TR, S for BL/BR)
    cardinal2: int  # horizontal (W for TL/BL, E for TR/BR)
    diagonal: int  # diagonal between them
    corner_tile: str  # both cardinals absent
    edge_c1_tile: str  # only cardinal1 present
    edge_c2_tile: str  # only cardinal2 present
    inner_tile: str  # both cardinals, no diagonal


QUADRANT_RULES: dict[str, QuadrantRule] = {
    "TL": QuadrantRule(N, W, NW, "outer_tl", "outer_left", "outer_top", "inner_d"),
    "TR": QuadrantRule(N, E, NE, "outer_tr", "outer_right", "outer_top", "inner_c"),
    "BL": QuadrantRule(S, W, SW, "outer_bl", "outer_left", "outer_bottom", "inner_b"),
    "BR": QuadrantRule(S, E, SE, "outer_br", "outer_right", "outer_bottom", "inner_a"),
}

REQUIRED_ROLES = [
    "center",
    "outer_tl",
    "outer_top",
    "outer_tr",
    "outer_left",
    "outer_right",
    "outer_bl",
    "outer_bottom",
    "outer_br",
    "inner_a",
    "inner_b",
    "inner_c",
    "inner_d",
    "single",
]

OPTIONAL_ROLES = ["inner_e", "inner_f"]
ALL_VALID_ROLES = frozenset(REQUIRED_ROLES + OPTIONAL_ROLES)

# 5x4 template grid: role -> (row, col)
TEMPLATE_POSITIONS: dict[str, tuple[int, int]] = {
    "outer_tl": (0, 0),
    "outer_top": (0, 1),
    "outer_tr": (0, 2),
    "inner_a": (0, 3),
    "inner_b": (0, 4),
    "outer_left": (1, 0),
    "center": (1, 1),
    "outer_right": (1, 2),
    "inner_c": (1, 3),
    "inner_d": (1, 4),
    "outer_bl": (2, 0),
    "outer_bottom": (2, 1),
    "outer_br": (2, 2),
    "inner_e": (2, 3),
    "inner_f": (2, 4),
    "single": (3, 0),
}

# Prime stride to decorrelate variant picks across roles
_ROLE_STRIDE = 7

# Spatial hash primes (Teschner et al.) for deterministic position-based selection
_HASH_PRIME_X = 73856093
_HASH_PRIME_Y = 19349663

# ---------------------------------------------------------------------------
# Data types
# ---------------------------------------------------------------------------


@dataclass
class SourceEntry:
    """One source image variant for a role."""

    role: str
    file: str
    weight: float
    image: Image.Image


@dataclass
class TileVariant:
    """One generated output tile variant."""

    image: Image.Image
    weight: float


# role name -> list of source variants
RoleVariants = dict[str, list[SourceEntry]]


# ---------------------------------------------------------------------------
# Bitmask computation
# ---------------------------------------------------------------------------


def reduce_bitmask(raw: int) -> int:
    """Zero out diagonal bits when adjacent cardinals are absent."""
    reduced = raw
    if not (raw & N and raw & E):
        reduced &= ~NE
    if not (raw & S and raw & E):
        reduced &= ~SE
    if not (raw & S and raw & W):
        reduced &= ~SW
    if not (raw & N and raw & W):
        reduced &= ~NW
    return reduced


def compute_47_bitmasks() -> list[int]:
    """Return the 47 unique canonical bitmask values, sorted ascending."""
    unique = {reduce_bitmask(raw) for raw in range(256)}
    result = sorted(unique)
    if len(result) != 47:
        raise RuntimeError(f"Expected 47 unique bitmasks, got {len(result)}")
    return result


def describe_bitmask(mask: int) -> str:
    """Human-readable description of a bitmask."""
    names = [
        name
        for name, bit in [
            ("N", N),
            ("NE", NE),
            ("E", E),
            ("SE", SE),
            ("S", S),
            ("SW", SW),
            ("W", W),
            ("NW", NW),
        ]
        if mask & bit
    ]
    return "+".join(names) if names else "isolated"


# ---------------------------------------------------------------------------
# JSON manifest parsing & source loading
# ---------------------------------------------------------------------------


def parse_manifest(path: Path) -> tuple[int, list[dict]]:
    """Parse a JSON manifest file.  Returns (tile_size, sources_list)."""
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise ValueError(f"Invalid JSON in {path}: {exc}") from exc

    if "tile_size" not in data:
        raise ValueError(f"Missing 'tile_size' in {path}")
    if "sources" not in data:
        raise ValueError(f"Missing 'sources' in {path}")

    tile_size = int(data["tile_size"])
    if tile_size < 2 or tile_size % 2 != 0:
        raise ValueError(f"tile_size must be a positive even number, got {tile_size}")

    return tile_size, data["sources"]


def load_sources(
    base_dir: Path,
    tile_size: int,
    sources: list[dict],
) -> RoleVariants:
    """Load all PNG sources from the manifest, validate, return RoleVariants."""
    roles: RoleVariants = {}

    for idx, src in enumerate(sources):
        for field in ("role", "file", "weight"):
            if field not in src:
                raise ValueError(f"Source #{idx}: missing required field '{field}'")

        role = src["role"].lower()
        file_rel = src["file"]
        weight = float(src["weight"])

        if role not in ALL_VALID_ROLES:
            raise ValueError(
                f"Source #{idx}: unknown role '{role}'. "
                f"Valid: {sorted(ALL_VALID_ROLES)}"
            )
        if weight <= 0:
            raise ValueError(
                f"Source #{idx} (role={role}, file={file_rel}): weight must be > 0"
            )

        file_path = base_dir / file_rel
        if not file_path.exists():
            raise FileNotFoundError(
                f"Source #{idx} (role={role}): file not found: {file_path}"
            )

        img = Image.open(file_path)
        if img.mode != "RGBA":
            img = img.convert("RGBA")
        img.load()  # read pixels into memory, release file handle

        w, h = img.size
        if w != tile_size or h != tile_size:
            raise ValueError(
                f"Source #{idx} (role={role}, file={file_rel}): "
                f"expected {tile_size}x{tile_size}, got {w}x{h}"
            )

        entry = SourceEntry(role=role, file=file_rel, weight=weight, image=img)
        roles.setdefault(role, []).append(entry)

    missing = [r for r in REQUIRED_ROLES if r not in roles]
    if missing:
        raise ValueError(f"Missing required roles: {missing}")

    return roles


# ---------------------------------------------------------------------------
# 5x4 PNG template loading & splitting
# ---------------------------------------------------------------------------


def _load_template_image(
    path: Path,
    tile_size: int | None = None,
) -> tuple[Image.Image, int]:
    """Load and validate a 5x4 template PNG.  Returns (image, tile_size)."""
    if not path.exists():
        raise FileNotFoundError(f"Template not found: {path}")

    img = Image.open(path)
    if img.mode != "RGBA":
        img = img.convert("RGBA")
    img.load()

    w, h = img.size
    if tile_size is None:
        tile_size = w // 5

    expected_w = tile_size * 5
    expected_h = tile_size * 4
    if w != expected_w or h != expected_h:
        raise ValueError(
            f"Template {w}x{h} doesn't match 5x4 grid of {tile_size}x{tile_size} "
            f"(expected {expected_w}x{expected_h})"
        )

    return img, tile_size


def load_template_as_roles(
    path: Path,
    tile_size: int | None = None,
) -> tuple[RoleVariants, int]:
    """Load a 5x4 PNG template directly into RoleVariants (no files written)."""
    img, ts = _load_template_image(path, tile_size)

    roles: RoleVariants = {}
    for role_name, (row, col) in TEMPLATE_POSITIONS.items():
        x, y = col * ts, row * ts
        tile_img = img.crop((x, y, x + ts, y + ts))

        # Skip fully transparent optional tiles
        if role_name in OPTIONAL_ROLES:
            alpha = tile_img.getchannel("A")
            if alpha.getextrema()[1] == 0:
                continue

        entry = SourceEntry(
            role=role_name,
            file=f"<template:{path.name}:{role_name}>",
            weight=1.0,
            image=tile_img,
        )
        roles[role_name] = [entry]

    missing = [r for r in REQUIRED_ROLES if r not in roles]
    if missing:
        raise ValueError(f"Template missing required tiles: {missing}")

    return roles, ts


def split_template(
    template_path: Path,
    output_dir: Path,
    tile_size: int | None = None,
) -> Path:
    """Split a 5x4 PNG template into individual tile PNGs + JSON manifest.

    Returns path to the generated manifest JSON.
    """
    img, ts = _load_template_image(template_path, tile_size)
    output_dir.mkdir(parents=True, exist_ok=True)

    sources: list[dict] = []
    for role_name, (row, col) in TEMPLATE_POSITIONS.items():
        x, y = col * ts, row * ts
        tile_img = img.crop((x, y, x + ts, y + ts))

        # Skip fully transparent optional tiles
        if role_name in OPTIONAL_ROLES:
            alpha = tile_img.getchannel("A")
            if alpha.getextrema()[1] == 0:
                continue

        filename = f"{role_name}.png"
        tile_img.save(output_dir / filename)
        sources.append({"role": role_name, "file": filename, "weight": 1.0})

    manifest = {"tile_size": ts, "sources": sources}
    manifest_path = output_dir / "tileset.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

    print(f"  Split {template_path.name} -> {output_dir}/")
    print(f"  Tile size: {ts}x{ts}")
    print(f"  Extracted {len(sources)} tiles")
    print(f"  Manifest: {manifest_path}")

    return manifest_path


# ---------------------------------------------------------------------------
# Quadrant helpers
# ---------------------------------------------------------------------------


def extract_quadrant(tile: Image.Image, quadrant: str, ts: int) -> Image.Image:
    """Crop one quadrant (TL/TR/BL/BR) from a tile image."""
    half = ts // 2
    dy, dx = QUADRANT_OFFSETS[quadrant]
    x, y = dx * half, dy * half
    return tile.crop((x, y, x + half, y + half))


def _mirror_quadrant(quadrant: str, has_c1: bool, has_c2: bool) -> str:
    """Determine which source quadrant to sample.

    When a cardinal is absent the outer tile's content sits on the
    opposite side, so we flip the corresponding axis.
    """
    row, col = quadrant[0], quadrant[1]
    if not has_c1:  # missing vertical -> flip T<->B
        row = "B" if row == "T" else "T"
    if not has_c2:  # missing horizontal -> flip L<->R
        col = "R" if col == "L" else "L"
    return row + col


def _alpha_composite(fg: Image.Image, bg: Image.Image) -> Image.Image:
    """Alpha-composite *fg* over *bg* (both same-size RGBA, Porter-Duff over)."""
    return Image.alpha_composite(bg, fg)


# ---------------------------------------------------------------------------
# Quadrant role resolution
# ---------------------------------------------------------------------------


def _get_quadrant_role(quadrant: str, mask: int) -> tuple[str, bool]:
    """Return (primary_role, needs_inner_composite) for a quadrant+bitmask."""
    rule = QUADRANT_RULES[quadrant]
    has_c1 = bool(mask & rule.cardinal1)
    has_c2 = bool(mask & rule.cardinal2)
    has_diag = bool(mask & rule.diagonal)

    if not has_c1 and not has_c2:
        return rule.corner_tile, False
    if has_c1 and not has_c2:
        return rule.edge_c1_tile, False
    if not has_c1 and has_c2:
        return rule.edge_c2_tile, False
    if has_c1 and has_c2 and not has_diag:
        return rule.inner_tile, True
    return "center", False


def _get_roles_for_bitmask(mask: int, roles: RoleVariants) -> set[str]:
    """Collect every role that participates in composing *mask*."""
    if mask == 0:
        return {"single"}

    # Special path for all-four-inner-corners
    if mask == 85 and "inner_e" in roles and "inner_f" in roles:
        return {"inner_e", "inner_f", "center"}

    used: set[str] = set()
    for q_name in QUADRANT_OFFSETS:
        role, is_inner = _get_quadrant_role(q_name, mask)
        used.add(role)
        if is_inner:
            used.add("center")
    return used


# ---------------------------------------------------------------------------
# Variant generation
# ---------------------------------------------------------------------------


def _variant_count_for_mask(
    mask: int,
    roles: RoleVariants,
    max_variants: int | None,
) -> int:
    """How many variants to generate for one bitmask configuration."""
    used = _get_roles_for_bitmask(mask, roles)
    counts = [len(roles[r]) for r in used if r in roles]
    k = max(counts) if counts else 1
    if max_variants is not None:
        k = min(k, max_variants)
    return max(1, k)


def _pick_per_role(
    roles: RoleVariants,
    variant_idx: int,
    seed: int,
) -> dict[str, SourceEntry]:
    """Deterministically pick one variant per role.

    Uses ``(variant_idx + seed-derived offset) % len`` so that
    different seeds produce different visual combinations while
    cycling through all available variants.
    """
    chosen: dict[str, SourceEntry] = {}
    for i, (role_name, variants) in enumerate(sorted(roles.items())):
        if len(variants) == 1:
            chosen[role_name] = variants[0]
        else:
            offset = (seed + i * _ROLE_STRIDE) % len(variants)
            idx = (variant_idx + offset) % len(variants)
            chosen[role_name] = variants[idx]
    return chosen


def _compose_quadrant(
    quadrant: str,
    mask: int,
    chosen: dict[str, SourceEntry],
    ts: int,
) -> tuple[Image.Image, float]:
    """Build one output quadrant.  Returns (image, primary_weight)."""
    rule = QUADRANT_RULES[quadrant]
    has_c1 = bool(mask & rule.cardinal1)
    has_c2 = bool(mask & rule.cardinal2)

    role, is_inner = _get_quadrant_role(quadrant, mask)
    source = chosen[role]

    src_q = _mirror_quadrant(quadrant, has_c1, has_c2)
    quad_img = extract_quadrant(source.image, src_q, ts)

    if is_inner:
        center_quad = extract_quadrant(chosen["center"].image, src_q, ts)
        quad_img = _alpha_composite(quad_img, center_quad)

    return quad_img, source.weight


def _compose_bitmask85(
    chosen: dict[str, SourceEntry],
    ts: int,
) -> tuple[Image.Image, float]:
    """Special composition for bitmask 85 (N+E+S+W, no diagonals).

    Uses INNER_E (notches TR+BL, filled TL+BR) and
    INNER_F (notches TL+BR, filled TR+BL) composited over CENTER.
    """
    half = ts // 2
    result = Image.new("RGBA", (ts, ts), (0, 0, 0, 0))

    ie = chosen["inner_e"]
    if_entry = chosen["inner_f"]
    center = chosen["center"]

    # Which source tile & quadrant to use for each output quadrant
    quad_map = {
        "TL": (ie, "TL"),
        "TR": (if_entry, "TR"),
        "BL": (if_entry, "BL"),
        "BR": (ie, "BR"),
    }

    weight = 1.0
    for q_name, (src_entry, src_q) in quad_map.items():
        dy, dx = QUADRANT_OFFSETS[q_name]

        inner_q = extract_quadrant(src_entry.image, src_q, ts)
        center_q = extract_quadrant(center.image, q_name, ts)
        composited = _alpha_composite(inner_q, center_q)
        result.paste(composited, (dx * half, dy * half))
        weight *= src_entry.weight

    return result, weight


def _generate_single_variant(
    mask: int,
    roles: RoleVariants,
    variant_idx: int,
    seed: int,
    ts: int,
) -> TileVariant:
    """Generate one tile variant for a given bitmask configuration."""
    chosen = _pick_per_role(roles, variant_idx, seed)

    # --- mask 0: isolated tile ---
    if mask == 0:
        entry = chosen["single"]
        return TileVariant(image=entry.image.copy(), weight=entry.weight)

    # --- mask 85: all four inner corners (special) ---
    if mask == 85 and "inner_e" in chosen and "inner_f" in chosen:
        img, w = _compose_bitmask85(chosen, ts)
        return TileVariant(image=img, weight=w)

    # --- general quadrant composition ---
    half = ts // 2
    result = Image.new("RGBA", (ts, ts), (0, 0, 0, 0))
    total_weight = 1.0

    for q_name, (dy, dx) in QUADRANT_OFFSETS.items():
        quad_img, qw = _compose_quadrant(q_name, mask, chosen, ts)
        result.paste(quad_img, (dx * half, dy * half))
        total_weight *= qw

    return TileVariant(image=result, weight=total_weight)


def generate_all_variants(
    roles: RoleVariants,
    bitmasks: list[int],
    max_variants: int | None,
    seed: int,
    ts: int,
) -> tuple[dict[int, list[TileVariant]], int]:
    """Generate every variant for every bitmask.

    Returns ``(bitmask -> [TileVariant, ...], actual_max_variants)``.
    Weights are normalised within each bitmask so they sum to 1.
    """
    all_results: dict[int, list[TileVariant]] = {}
    global_max = 0

    for mask in bitmasks:
        k = _variant_count_for_mask(mask, roles, max_variants)
        global_max = max(global_max, k)

        variants: list[TileVariant] = []
        for vi in range(k):
            variants.append(_generate_single_variant(mask, roles, vi, seed, ts))

        # Normalise weights within this bitmask
        total_w = sum(v.weight for v in variants)
        if total_w > 0:
            for v in variants:
                v.weight = round(v.weight / total_w, 6)

        all_results[mask] = variants

    return all_results, global_max


# ---------------------------------------------------------------------------
# Spritesheet layout
# ---------------------------------------------------------------------------


def layout_spritesheet(
    all_variants: dict[int, list[TileVariant]],
    bitmasks: list[int],
    ts: int,
    actual_max: int,
    layout: str,
) -> Image.Image:
    """Arrange tiles into a spritesheet PNG.

    variants_y  (default): cols = actual_max, rows = 47
    variants_x:            cols = 47,         rows = actual_max
    """
    if layout == "variants_x":
        cols, rows = len(bitmasks), actual_max
    else:
        cols, rows = actual_max, len(bitmasks)

    out = Image.new("RGBA", (cols * ts, rows * ts), (0, 0, 0, 0))

    for bm_idx, mask in enumerate(bitmasks):
        for vi, variant in enumerate(all_variants[mask]):
            if layout == "variants_x":
                c, r = bm_idx, vi
            else:
                c, r = vi, bm_idx
            out.paste(variant.image, (c * ts, r * ts))

    return out


# ---------------------------------------------------------------------------
# RON mapping output
# ---------------------------------------------------------------------------


def _atlas_dims(
    bitmasks: list[int],
    actual_max: int,
    layout: str,
) -> tuple[int, int]:
    if layout == "variants_x":
        return len(bitmasks), actual_max
    return actual_max, len(bitmasks)


def generate_ron_mapping(
    all_variants: dict[int, list[TileVariant]],
    bitmasks: list[int],
    ts: int,
    actual_max: int,
    layout: str,
) -> str:
    """Build a Bevy-compatible .ron mapping string."""
    atlas_cols, atlas_rows = _atlas_dims(bitmasks, actual_max, layout)

    lines = [
        "(",
        f"    tile_size: {ts},",
        f"    atlas_columns: {atlas_cols},",
        f"    atlas_rows: {atlas_rows},",
        "    tiles: {",
    ]

    for bm_idx, mask in enumerate(bitmasks):
        desc = describe_bitmask(mask)
        variants = all_variants[mask]

        lines.append(f"        {mask}: (")
        lines.append(f'            description: "{desc}",')
        lines.append("            variants: [")

        for vi, variant in enumerate(variants):
            if layout == "variants_x":
                c, r = bm_idx, vi
            else:
                c, r = vi, bm_idx
            lines.append(
                f"                (index: {vi}, col: {c}, row: {r}, "
                f"weight: {variant.weight}),"
            )

        lines.append("            ],")
        lines.append("        ),")

    lines.append("    },")
    lines.append(")")
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------
# Validation & verification
# ---------------------------------------------------------------------------


def validate_roles(roles: RoleVariants) -> list[str]:
    """Return warnings about loaded roles."""
    warnings: list[str] = []
    for role_name in REQUIRED_ROLES:
        if role_name not in roles:
            warnings.append(f"Missing required role: {role_name}")
            continue
        for entry in roles[role_name]:
            alpha = entry.image.getchannel("A")
            if alpha.getextrema()[1] == 0:
                warnings.append(
                    f"Role '{role_name}' ({entry.file}) is fully transparent"
                )
    return warnings


def verify_output(
    all_variants: dict[int, list[TileVariant]],
    roles: RoleVariants,
    bitmasks: list[int],
) -> list[str]:
    """Run automatic sanity checks on generated tiles."""
    messages: list[str] = []

    # bitmask=255 first variant should match center (only reliable with 1 variant)
    if 255 in all_variants and all_variants[255] and len(roles["center"]) == 1:
        v255 = all_variants[255][0]
        center_img = roles["center"][0].image
        if v255.image.tobytes() == center_img.tobytes():
            messages.append("\u2713 bitmask 255 (variant 0) matches CENTER")
        else:
            messages.append(
                "\u26a0 bitmask 255 differs from CENTER \u2014 check mapping"
            )

    # bitmask=0 first variant should match single (only reliable with 1 variant)
    if 0 in all_variants and all_variants[0] and len(roles["single"]) == 1:
        v0 = all_variants[0][0]
        single_img = roles["single"][0].image
        if v0.image.tobytes() == single_img.tobytes():
            messages.append("\u2713 bitmask 0 (variant 0) matches SINGLE")
        else:
            messages.append("\u26a0 bitmask 0 differs from SINGLE")

    # Transparent tiles
    empty: list[tuple[int, int]] = []
    for mask in bitmasks:
        for vi, v in enumerate(all_variants.get(mask, [])):
            alpha = v.image.getchannel("A")
            if alpha.getextrema()[1] == 0:
                empty.append((mask, vi))
    if empty:
        shown = empty[:10]
        messages.append(f"\u26a0 {len(empty)} tile variants fully transparent: {shown}")
    else:
        total = sum(len(all_variants.get(m, [])) for m in bitmasks)
        messages.append(f"\u2713 All {total} tile variants have non-transparent pixels")

    return messages


# ---------------------------------------------------------------------------
# Preview
# ---------------------------------------------------------------------------


def generate_preview(
    all_variants: dict[int, list[TileVariant]],
    bitmasks: list[int],
    ts: int,
    scale: int = 4,
) -> Image.Image:
    """Enlarged preview showing first variant of each bitmask with labels."""
    cols = 8
    n_rows = (len(bitmasks) + cols - 1) // cols

    cell_w = ts * scale + 4
    cell_h = ts * scale + 18
    margin = 8

    pw = cols * cell_w + margin * 2
    ph = n_rows * cell_h + margin * 2
    preview = Image.new("RGBA", (pw, ph), (32, 32, 40, 255))
    draw = ImageDraw.Draw(preview)

    arrow_map = {
        N: "\u2191",
        NE: "\u2197",
        E: "\u2192",
        SE: "\u2198",
        S: "\u2193",
        SW: "\u2199",
        W: "\u2190",
        NW: "\u2196",
    }

    for idx, mask in enumerate(bitmasks):
        c, r = idx % cols, idx // cols
        x = margin + c * cell_w + 2
        y = margin + r * cell_h + 2

        tile = all_variants[mask][0].image
        scaled = tile.resize(
            (ts * scale, ts * scale),
            Image.Resampling.NEAREST,
        )
        preview.paste(scaled, (x, y), scaled)

        draw.rectangle(
            [x - 1, y - 1, x + ts * scale, y + ts * scale],
            outline=(80, 80, 100, 180),
        )

        arrows = "".join(sym for bit, sym in arrow_map.items() if mask & bit)
        nv = len(all_variants[mask])
        label = f"{mask} {arrows} ({nv}v)" if arrows else f"0 ({nv}v)"
        draw.text(
            (x + 1, y + ts * scale + 2),
            label,
            fill=(200, 200, 220, 255),
        )

    return preview


# ---------------------------------------------------------------------------
# Test map
# ---------------------------------------------------------------------------

_TEST_PATTERN: list[list[int]] = [
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 1, 1, 1, 0, 0, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0],
    [0, 1, 1, 1, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0],
    [0, 1, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0],
    [0, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0],
    [0, 1, 1, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0],
    [0, 1, 1, 1, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0],
    [0, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 1, 0, 1, 0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
]


def _cell_filled(pattern: list[list[int]], y: int, x: int) -> bool:
    h, w = len(pattern), len(pattern[0])
    return 0 <= y < h and 0 <= x < w and bool(pattern[y][x])


def _compute_cell_mask(pattern: list[list[int]], y: int, x: int) -> int:
    mask = 0
    if _cell_filled(pattern, y - 1, x):
        mask |= N
    if _cell_filled(pattern, y, x + 1):
        mask |= E
    if _cell_filled(pattern, y + 1, x):
        mask |= S
    if _cell_filled(pattern, y, x - 1):
        mask |= W
    if (mask & N) and (mask & E) and _cell_filled(pattern, y - 1, x + 1):
        mask |= NE
    if (mask & S) and (mask & E) and _cell_filled(pattern, y + 1, x + 1):
        mask |= SE
    if (mask & S) and (mask & W) and _cell_filled(pattern, y + 1, x - 1):
        mask |= SW
    if (mask & N) and (mask & W) and _cell_filled(pattern, y - 1, x - 1):
        mask |= NW
    return mask


def generate_test_map(
    all_variants: dict[int, list[TileVariant]],
    ts: int,
) -> Image.Image:
    """Render a test tile-map with deterministic position-based variant selection."""
    pattern = _TEST_PATTERN
    map_h, map_w = len(pattern), len(pattern[0])
    out = Image.new("RGBA", (map_w * ts, map_h * ts), (0, 0, 0, 0))

    for y in range(map_h):
        for x in range(map_w):
            if not pattern[y][x]:
                continue
            mask = _compute_cell_mask(pattern, y, x)
            variants = all_variants.get(mask)
            if not variants:
                continue

            # Deterministic hash of world position -> weighted variant pick
            h = (x * _HASH_PRIME_X) ^ (y * _HASH_PRIME_Y)
            t = (h & 0xFFFFFFFF) / 0xFFFFFFFF

            cumulative = 0.0
            chosen = variants[0]
            for v in variants:
                cumulative += v.weight
                if t < cumulative:
                    chosen = v
                    break

            out.paste(chosen.image, (x * ts, y * ts), chosen.image)

    return out


# ---------------------------------------------------------------------------
# Pipeline
# ---------------------------------------------------------------------------


def process_single(
    input_path: Path,
    output_path: Path | None,
    ron_path: Path | None,
    max_variants: int | None,
    seed: int,
    layout: str,
    preview: bool,
    test_map: bool,
) -> None:
    """Full pipeline for one input file."""
    if output_path is None:
        output_path = input_path.with_name(f"{input_path.stem}_47.png")
    if ron_path is None:
        ron_path = output_path.with_suffix(".ron")

    # ---- load ----
    suffix = input_path.suffix.lower()
    if suffix == ".json":
        print(f"Loading manifest: {input_path}")
        tile_size, sources = parse_manifest(input_path)
        roles = load_sources(input_path.parent, tile_size, sources)
        ts = tile_size
    elif suffix == ".png":
        print(f"Loading 5x4 template: {input_path}")
        roles, ts = load_template_as_roles(input_path)
    else:
        raise ValueError(f"Unsupported input format: {suffix} (expected .json or .png)")

    print(f"  Tile size: {ts}x{ts}")
    role_summary = {r: len(v) for r, v in sorted(roles.items())}
    print(f"  Roles loaded: {role_summary}")

    for warn in validate_roles(roles):
        print(f"  \u26a0 {warn}")

    # ---- generate ----
    bitmasks = compute_47_bitmasks()
    print(f"  Generating variants for {len(bitmasks)} configurations...")
    all_variants, actual_max = generate_all_variants(
        roles,
        bitmasks,
        max_variants,
        seed,
        ts,
    )
    total_tiles = sum(len(v) for v in all_variants.values())
    print(f"  Generated {total_tiles} tiles ({actual_max} max variants)")

    for msg in verify_output(all_variants, roles, bitmasks):
        print(f"  {msg}")

    # ---- spritesheet ----
    output_path.parent.mkdir(parents=True, exist_ok=True)
    sheet = layout_spritesheet(all_variants, bitmasks, ts, actual_max, layout)
    sheet.save(output_path)
    print(f"  Saved spritesheet: {output_path} ({sheet.width}x{sheet.height})")

    # ---- RON mapping ----
    ron_content = generate_ron_mapping(
        all_variants,
        bitmasks,
        ts,
        actual_max,
        layout,
    )
    ron_path.write_text(ron_content, encoding="utf-8")
    print(f"  Saved RON mapping: {ron_path}")

    # ---- optional: preview ----
    if preview:
        preview_path = output_path.with_name(f"{output_path.stem}_preview.png")
        preview_img = generate_preview(all_variants, bitmasks, ts)
        preview_img.save(preview_path)
        print(f"  Saved preview: {preview_path}")

    # ---- optional: test map ----
    if test_map:
        map_path = output_path.with_name(f"{output_path.stem}_testmap.png")
        map_img = generate_test_map(all_variants, ts)
        map_img.save(map_path)
        print(f"  Saved test map: {map_path}")

    print("  Done!")


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def resolve_inputs(raw: Path) -> list[Path]:
    """Resolve a path argument to a list of input files."""
    if raw.is_dir():
        found = sorted(raw.glob("*.json")) + sorted(raw.glob("*.png"))
        if not found:
            print(f"Error: no JSON/PNG files in {raw}", file=sys.stderr)
            sys.exit(1)
        return found

    if "*" in raw.name or "?" in raw.name:
        found = sorted(raw.parent.glob(raw.name))
        if not found:
            print(f"Error: no files matching {raw}", file=sys.stderr)
            sys.exit(1)
        return found

    if not raw.exists():
        print(f"Error: file not found: {raw}", file=sys.stderr)
        sys.exit(1)
    return [raw]


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Generate a 47-tile blob tileset with variants.",
        epilog=(
            "Examples:\n"
            "  python autotile47.py tileset.json -o output.png\n"
            "  python autotile47.py template.png -o output.png\n"
            "  python autotile47.py --split template.png -o tiles_dir/\n"
            "  python autotile47.py tilesets/*.json --output-dir generated/"
        ),
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument(
        "input",
        type=Path,
        help="JSON manifest (.json) or 5x4 PNG template (.png)",
    )
    p.add_argument(
        "-o",
        "--output",
        type=Path,
        default=None,
        help="Output spritesheet path (default: {input}_47.png)",
    )
    p.add_argument(
        "--split",
        action="store_true",
        help="Split a 5x4 PNG template into individual tiles + JSON manifest",
    )
    p.add_argument(
        "--tile-size",
        type=int,
        default=None,
        help="Override tile size for PNG templates (default: auto = width/5)",
    )
    p.add_argument(
        "--ron",
        type=Path,
        default=None,
        help="RON mapping path (default: {output}.ron)",
    )
    p.add_argument(
        "--max-variants",
        type=int,
        default=None,
        help="Max variants per configuration (default: auto from manifest)",
    )
    p.add_argument(
        "--seed",
        type=int,
        default=42,
        help="Seed for deterministic variant selection (default: 42)",
    )
    p.add_argument(
        "--layout",
        choices=["variants_y", "variants_x"],
        default="variants_y",
        help="Spritesheet layout (default: variants_y = rows=47, cols=variants)",
    )
    p.add_argument(
        "--preview",
        action="store_true",
        help="Generate enlarged preview image",
    )
    p.add_argument(
        "--test-map",
        action="store_true",
        help="Generate a test tile map",
    )
    p.add_argument(
        "--output-dir",
        type=Path,
        default=None,
        help="Output directory for batch processing",
    )
    return p.parse_args(argv)


def main(argv: list[str] | None = None) -> None:
    args = parse_args(argv)

    if args.output_dir and args.ron:
        print("Error: --ron cannot be used with --output-dir", file=sys.stderr)
        sys.exit(1)

    # ---- split mode ----
    if args.split:
        input_path = args.input
        if input_path.suffix.lower() != ".png":
            print("Error: --split requires a PNG template", file=sys.stderr)
            sys.exit(1)
        output_dir = args.output or input_path.with_suffix("")
        try:
            split_template(input_path, output_dir, args.tile_size)
        except (FileNotFoundError, ValueError) as exc:
            print(f"Error: {exc}", file=sys.stderr)
            sys.exit(1)
        return

    # ---- generate mode ----
    inputs = resolve_inputs(args.input)

    for input_path in inputs:
        out = args.output
        if args.output_dir:
            args.output_dir.mkdir(parents=True, exist_ok=True)
            out = args.output_dir / f"{input_path.stem}_47.png"

        try:
            process_single(
                input_path=input_path,
                output_path=out,
                ron_path=args.ron,
                max_variants=args.max_variants,
                seed=args.seed,
                layout=args.layout,
                preview=args.preview,
                test_map=args.test_map,
            )
        except (FileNotFoundError, ValueError) as exc:
            print(f"Error: {exc}", file=sys.stderr)
            if len(inputs) == 1:
                sys.exit(1)
        except Exception as exc:
            print(
                f"Unexpected error processing {input_path}: {exc}",
                file=sys.stderr,
            )
            if len(inputs) == 1:
                sys.exit(1)


if __name__ == "__main__":
    main()
