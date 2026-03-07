#!/usr/bin/env python3
"""
Split PixelLab character sprites into modular body parts using keypoint data.

Uses the metadata.json keypoints (NOSE, NECK, SHOULDER, ELBOW, ARM, HIP, KNEE, LEG)
to determine body region boundaries, then flood-fills connected pixel regions
to assign each pixel to: head, body, front_arm, or back_arm.

Usage:
    python3 tools/split_character.py /tmp/pixellab-adventurer assets/content/characters/adventurer

Reads from the PixelLab export directory and writes per-part frame PNGs.
"""

import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image


def load_metadata(pixellab_dir: Path) -> dict:
    with open(pixellab_dir / "metadata.json") as f:
        return json.load(f)


def keypoints_to_pixels(keypoints: list[dict], w: int, h: int) -> dict[str, tuple[int, int]]:
    """Convert normalized keypoints to pixel coordinates."""
    result = {}
    for kp in keypoints:
        x = int(kp["x"] * w)
        y = int(kp["y"] * h)
        result[kp["label"]] = (x, y)
    return result


def dist_to_segment(px: int, py: int, ax: int, ay: int, bx: int, by: int) -> float:
    """Squared distance from point (px,py) to line segment (ax,ay)-(bx,by)."""
    dx, dy = bx - ax, by - ay
    if dx == 0 and dy == 0:
        return (px - ax) ** 2 + (py - ay) ** 2
    t = max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)))
    proj_x = ax + t * dx
    proj_y = ay + t * dy
    return (px - proj_x) ** 2 + (py - proj_y) ** 2


def dist_to_chain(px: int, py: int, chain: list[tuple[int, int]]) -> float:
    """Min squared distance from point to a chain of connected segments."""
    if len(chain) < 2:
        return (px - chain[0][0]) ** 2 + (py - chain[0][1]) ** 2
    return min(
        dist_to_segment(px, py, chain[i][0], chain[i][1], chain[i + 1][0], chain[i + 1][1])
        for i in range(len(chain) - 1)
    )


def split_frame(img: Image.Image, kp: dict[str, tuple[int, int]]) -> dict[str, Image.Image]:
    """
    Split a single frame into head, body, front_arm, back_arm.

    Uses distance to skeleton chains (shoulder->elbow->hand vs neck->hip->knee->leg)
    to assign each pixel to the nearest body part.

    For east-facing side view:
    - RIGHT = front arm (closer to camera)
    - LEFT = back arm (further from camera)
    """
    w, h = img.size
    rgba = np.array(img)
    alpha = rgba[:, :, 3]

    # Key positions
    neck = kp.get("NECK", (w // 2, h // 3))
    r_shoulder = kp.get("RIGHT SHOULDER", (w // 2 + 3, neck[1]))
    l_shoulder = kp.get("LEFT SHOULDER", (w // 2 - 3, neck[1]))
    r_elbow = kp.get("RIGHT ELBOW", (r_shoulder[0], neck[1] + 6))
    l_elbow = kp.get("LEFT ELBOW", (l_shoulder[0], neck[1] + 6))
    r_arm = kp.get("RIGHT ARM", (r_elbow[0], r_elbow[1] + 5))
    l_arm = kp.get("LEFT ARM", (l_elbow[0], l_elbow[1] + 5))
    r_hip = kp.get("RIGHT HIP", (w // 2 + 2, neck[1] + 8))
    l_hip = kp.get("LEFT HIP", (w // 2 - 2, neck[1] + 8))
    r_knee = kp.get("RIGHT KNEE", (r_hip[0], r_hip[1] + 6))
    l_knee = kp.get("LEFT KNEE", (l_hip[0], l_hip[1] + 6))
    r_leg = kp.get("RIGHT LEG", (r_knee[0], r_knee[1] + 6))
    l_leg = kp.get("LEFT LEG", (l_knee[0], l_knee[1] + 6))

    # Head cut: 2px below neck
    head_cut_y = neck[1] + 2

    # Skeleton chains for distance calculation
    front_arm_chain = [r_shoulder, r_elbow, r_arm]  # RIGHT = front in east view
    back_arm_chain = [l_shoulder, l_elbow, l_arm]    # LEFT = back in east view
    # Body chain: spine + both legs
    body_spine = [neck, ((r_hip[0] + l_hip[0]) // 2, (r_hip[1] + l_hip[1]) // 2)]
    body_right_leg = [r_hip, r_knee, r_leg]
    body_left_leg = [l_hip, l_knee, l_leg]

    # Create part masks
    head_mask = np.zeros((h, w), dtype=bool)
    body_mask = np.zeros((h, w), dtype=bool)
    front_arm_mask = np.zeros((h, w), dtype=bool)
    back_arm_mask = np.zeros((h, w), dtype=bool)

    # Arm thickness: pixels within this distance of arm chain can be claimed by arm
    ARM_BIAS = 2.0  # Subtract from arm distance to favor arm over body in overlap zones

    for y in range(h):
        for x in range(w):
            if alpha[y, x] == 0:
                continue

            if y < head_cut_y:
                head_mask[y, x] = True
                continue

            # Distance to each skeleton chain
            d_front = dist_to_chain(x, y, front_arm_chain) - ARM_BIAS ** 2
            d_back = dist_to_chain(x, y, back_arm_chain) - ARM_BIAS ** 2
            d_body = min(
                dist_to_chain(x, y, body_spine),
                dist_to_chain(x, y, body_right_leg),
                dist_to_chain(x, y, body_left_leg),
            )

            # Assign to closest skeleton
            min_dist = min(d_front, d_back, d_body)
            if min_dist == d_front:
                front_arm_mask[y, x] = True
            elif min_dist == d_back:
                back_arm_mask[y, x] = True
            else:
                body_mask[y, x] = True

    # Create output images
    parts = {}
    for name, mask in [("head", head_mask), ("body", body_mask),
                       ("front_arm", front_arm_mask), ("back_arm", back_arm_mask)]:
        part_rgba = np.zeros_like(rgba)
        part_rgba[mask] = rgba[mask]
        parts[name] = Image.fromarray(part_rgba)

    return parts


def process_animation(
    pixellab_dir: Path,
    output_dir: Path,
    anim_pixellab_name: str,
    anim_output_name: str,
    direction: str,
    keypoints_data: dict,
):
    """Process all frames of one animation."""
    kp_key = f"{anim_pixellab_name}_{direction}"
    frame_keypoints = keypoints_data["keypoints"]["animations"].get(kp_key, [])

    frames_dir = pixellab_dir / "animations" / anim_pixellab_name / direction
    if not frames_dir.exists():
        print(f"  SKIP {frames_dir} (not found)")
        return

    frame_files = sorted(frames_dir.glob("frame_*.png"))
    print(f"  {anim_output_name}: {len(frame_files)} frames")

    for i, frame_file in enumerate(frame_files):
        img = Image.open(frame_file).convert("RGBA")
        w, h = img.size

        # Get keypoints for this frame
        if i < len(frame_keypoints):
            kp = keypoints_to_pixels(frame_keypoints[i], w, h)
        else:
            # Fallback: use last available keypoints
            kp = keypoints_to_pixels(frame_keypoints[-1], w, h) if frame_keypoints else {}

        if not kp:
            print(f"    WARNING: no keypoints for frame {i}, copying to body")
            for part_name in ["head", "body", "front_arm", "back_arm"]:
                part_dir = output_dir / f"sprites/{part_name}/{anim_output_name}"
                part_dir.mkdir(parents=True, exist_ok=True)
                if part_name == "body":
                    img.save(part_dir / f"frame_{i:03d}.png")
                else:
                    # Empty transparent frame
                    empty = Image.new("RGBA", (w, h), (0, 0, 0, 0))
                    empty.save(part_dir / f"frame_{i:03d}.png")
            continue

        parts = split_frame(img, kp)

        for part_name, part_img in parts.items():
            part_dir = output_dir / f"sprites/{part_name}/{anim_output_name}"
            part_dir.mkdir(parents=True, exist_ok=True)
            part_img.save(part_dir / f"frame_{i:03d}.png")


def main():
    if len(sys.argv) < 3:
        print(f"Usage: {sys.argv[0]} <pixellab_dir> <output_dir>")
        sys.exit(1)

    pixellab_dir = Path(sys.argv[1])
    output_dir = Path(sys.argv[2])
    direction = "east"  # Side view for our game

    print(f"Loading metadata from {pixellab_dir}")
    metadata = load_metadata(pixellab_dir)

    canvas_w = metadata["character"]["size"]["width"]
    canvas_h = metadata["character"]["size"]["height"]
    print(f"Canvas: {canvas_w}x{canvas_h}")

    # Animation mapping: pixellab_name -> output_name
    animations = {
        "breathing-idle": "staying",
        "running-6-frames": "running",
        "jumping-1": "jumping",
    }

    for pl_name, out_name in animations.items():
        process_animation(pixellab_dir, output_dir, pl_name, out_name, direction, metadata)

    # Summary
    print("\nDone! Output structure:")
    for part in ["head", "body", "front_arm", "back_arm"]:
        part_dir = output_dir / f"sprites/{part}"
        if part_dir.exists():
            total = sum(1 for _ in part_dir.rglob("*.png"))
            print(f"  sprites/{part}/: {total} frames")


if __name__ == "__main__":
    main()
