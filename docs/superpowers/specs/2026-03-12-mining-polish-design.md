# Mining Polish Design

## Overview

Polish the mining/digging mechanic with visual feedback, tool durability, and pickaxe icons.

## 1. Block Crack Overlay

Overlay sprite (16x16) rendered on top of damaged tiles showing mining progress.

- **4 stages**: 25%, 50%, 75%, 100% of hardness reached
- Sprite atlas with 4 frames of increasing crack severity
- Progress ratio: `accumulated / hardness` from existing `BlockDamageMap`
- Rendered in chunk render pass, on top of the tile, same layer
- Cracks disappear when damage regens (2 sec idle → regen at 0.5/sec)
- No cracks shown below 25% threshold

## 2. Mining Particles

Small debris particles (2-4 per tick) while the player is actively mining a block.

- **Trigger**: on each damage tick, throttled to ~0.15 sec interval (not every frame)
- **No particles on block destruction** — only while mining
- **Color**: sampled from tile's primary color (add `particle_color: Color` field to `TileDef`)
- **Behavior**: spawn at hit point, fly upward and sideways, gravity pulls down
- **Pool**: use existing `ParticlePool` (capacity 3000)
- **Lifetime**: ~0.3-0.5 sec per particle

## 3. Tool Durability

Each block broken costs 1 durability from the active tool. Tool breaks (disappears) when durability reaches 0.

### Stats

| Tool | Mining Power | Durability |
|------|-------------|------------|
| Stone Pickaxe | 2.0 | 100 |
| Iron Pickaxe | 5.0 | 200 |
| Advanced Pickaxe | 10.0 | 400 |

### Data Model

- Add `durability: Option<u32>` to `ItemStats` in `definition.rs` (max value from .item.ron)
- Add `current_durability: Option<u32>` to inventory slot data (runtime state)
- On craft/pickup: `current_durability = durability` (max)
- On block break: `current_durability -= 1`
- On `current_durability == 0`: remove item from slot, fallback to bare hands (mining_power 1.0)

### Durability Bar

- Thin bar at bottom of item icon, rendered in both hotbar and inventory UI
- Color gradient: green (>50%) → yellow (25%-50%) → red (<25%)
- Only shown when `current_durability < max` (full durability = no bar)
- Bar width proportional to `current / max`

## 4. Pickaxe Icons

Generate 3 pixel art icons (16x16) via PixelLab:

- **Stone Pickaxe**: grey stone head + wooden handle
- **Iron Pickaxe**: dark grey metal head + wooden handle
- **Advanced Pickaxe**: blue/purple crystal head + metallic handle

Save to `assets/content/items/<name>/icon.png`, reference in `.item.ron` files.

## Key Files

| Aspect | File |
|--------|------|
| Mining loop | `src/interaction/block_action.rs` |
| Block damage storage | `src/combat/block_damage.rs` |
| Item stats | `src/item/definition.rs` |
| Tile definitions | `src/registry/tile.rs` |
| Particle pool | `src/particles/` |
| Chunk rendering | `src/world/chunk_render.rs` (or equivalent) |
| Hotbar UI | `src/ui/game_ui/` |
| Pickaxe configs | `assets/content/items/*_pickaxe/*.item.ron` |
| Tile configs | `assets/content/tiles/*/*.tile.ron` |
