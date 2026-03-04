# Multi-Fluid Cells Design

## Problem

When two immiscible liquids (e.g., lava at 44% fill and water at 100% fill above) occupy adjacent cells, a visual gap appears. The lava renders from the bottom to 44% of the cell, leaving the top 56% visually empty. The cell above (water) fills its own cell completely, but the space between the lava surface and the water cell bottom is a hole.

Root cause: each cell can only hold one fluid type. Flow is blocked between cells with different fluid IDs, so water cannot fill the remaining space in a lava cell.

## Solution: Dual-Slot Cells

Extend `FluidCell` to hold two fluid slots — a primary (heavy, bottom) and secondary (light, top).

### Data Structure

```rust
pub struct FluidSlot {
    pub fluid_id: FluidId,  // FluidId::NONE = empty slot
    pub mass: f32,
}

pub struct FluidCell {
    pub primary: FluidSlot,    // heavy fluid (renders at bottom)
    pub secondary: FluidSlot,  // light fluid (renders on top)
}
```

### Invariants

- `primary.mass + secondary.mass <= 1.0` (plus max_compress for pressure)
- `density(primary) >= density(secondary)` — swap slots if violated
- If only one slot is used, it is always primary; secondary is empty
- Only liquid+liquid mixing; gases remain single-slot

### Simulation

**Flow (vertical/horizontal):**
- Each slot flows independently toward cells with matching `fluid_id`
- When entering a cell: if `fluid_id` matches primary → add to primary; matches secondary → add to secondary; both slots occupied by different types → flow blocked
- Empty cell → enters primary slot
- Primary occupied by different type, secondary empty → enters secondary (if density is correct)
- Overflow: if `primary.mass + secondary.mass + flow > 1.0` → accept only `1.0 - total`, remainder stays

**Density displacement:**
- Intra-cell: if secondary is heavier than primary → instant slot swap
- Inter-cell: swap matching slots between cells (heavy secondary of one cell with light primary of another)

### Rendering

Each multi-fluid cell generates **2 quads**:
- **Primary quad**: `y0 = world_y`, `y1 = world_y + primary.mass * tile_size`
- **Secondary quad**: `y0 = world_y + primary.mass * tile_size`, `y1 = world_y + (primary.mass + secondary.mass) * tile_size`

Color, emission, wave parameters come from each slot's FluidDef. Surface detection operates on the secondary (topmost) slot.

### Reactions

- **Intra-cell**: if primary and secondary have a registered reaction → trigger each tick
- **Inter-cell**: check all slot combinations (primary↔primary, primary↔secondary, secondary↔primary, secondary↔secondary of neighbor)

### Debug Overlay

Update F8 overlay to show both slots:
```
Fluid: lava / water
Mass:  0.44 / 0.56
Fill:  44% / 56%
```

### Files Affected

| File | Changes |
|------|---------|
| `cell.rs` | New FluidSlot + FluidCell structure |
| `simulation.rs` | flow_vertical/horizontal for dual slots |
| `reactions.rs` | Intra-cell reactions + multi-slot displacement |
| `render.rs` | Two quads per cell, surface detection |
| `fluid_world.rs` | Adapt read/write/swap for slots |
| `wave.rs` | Waves on secondary (topmost) slot |
| `systems.rs` | Debug overlay updates |
| `fluid.wgsl` | Likely no changes (quads arrive pre-built) |

### Constraints

- No backward compatibility with old saves needed
- Only liquid+liquid mixing (not gas+liquid or gas+gas)
- Fixed 2 slots per cell (not dynamic)
- Sum of masses <= 1.0 per cell
