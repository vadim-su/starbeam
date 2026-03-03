# Wave System Refactor Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Eliminate wave rendering artifacts at chunk boundaries, limit wave amplitude to realistic levels, and improve splash particle visuals.

**Architecture:** Four independent changes to the wave system: (1) double-buffer WaveBuffer to eliminate per-tick allocations, (2) fix boundary reconciliation to sync both height and velocity + cross-chunk impulse spread, (3) clamp wave amplitude and add nonlinear damping, (4) tune splash particles for visual quality.

**Tech Stack:** Rust, Bevy ECS, custom fluid simulation

---

### Task 1: WaveBuffer double-buffer refactor

**Files:**
- Modify: `src/fluid/wave.rs:15-128`

**Step 1: Update WaveBuffer struct to add prev_height field**

In `src/fluid/wave.rs`, add `prev_height` field to `WaveBuffer`:

```rust
pub struct WaveBuffer {
    pub height: Vec<f32>,
    pub velocity: Vec<f32>,
    prev_height: Vec<f32>,  // NEW: double-buffer to avoid clone in step()
    pub chunk_size: u32,
}
```

Update `new()` to initialize `prev_height`:

```rust
pub fn new(chunk_size: u32) -> Self {
    let len = (chunk_size * chunk_size) as usize;
    Self {
        height: vec![0.0; len],
        velocity: vec![0.0; len],
        prev_height: vec![0.0; len],
        chunk_size,
    }
}
```

**Step 2: Rewrite step() to use swap instead of clone**

Replace the `step()` method body. Key changes:
- `std::mem::swap(&mut self.height, &mut self.prev_height)` instead of `let prev_height = self.height.clone()`
- Read from `self.prev_height`, write to `self.height`
- Damping only on velocity: `self.velocity[i] *= config.damping`
- Height update without double damping: `self.height[i] = self.prev_height[i] + self.velocity[i]`

```rust
pub fn step(&mut self, fluids: &[FluidCell], config: &WaveConfig) {
    let size = self.chunk_size;
    let len = (size * size) as usize;

    // Swap buffers: prev_height now holds the state we read from,
    // height is the buffer we write into.
    std::mem::swap(&mut self.height, &mut self.prev_height);

    for i in 0..len {
        if fluids[i].is_empty() {
            self.height[i] = 0.0;
            self.velocity[i] = 0.0;
            continue;
        }

        let x = (i as u32) % size;
        let y = (i as u32) / size;

        let mut sum = 0.0;
        let mut count = 0u32;

        if x > 0 {
            let ni = (y * size + (x - 1)) as usize;
            if !fluids[ni].is_empty() {
                sum += self.prev_height[ni];
                count += 1;
            }
        }
        if x + 1 < size {
            let ni = (y * size + (x + 1)) as usize;
            if !fluids[ni].is_empty() {
                sum += self.prev_height[ni];
                count += 1;
            }
        }
        if y > 0 {
            let ni = ((y - 1) * size + x) as usize;
            if !fluids[ni].is_empty() {
                sum += self.prev_height[ni];
                count += 1;
            }
        }
        if y + 1 < size {
            let ni = ((y + 1) * size + x) as usize;
            if !fluids[ni].is_empty() {
                sum += self.prev_height[ni];
                count += 1;
            }
        }

        if count > 0 {
            let avg = sum / count as f32;
            self.velocity[i] += (avg - self.prev_height[i]) * config.speed;
        }

        // Damping only on velocity (not height)
        self.velocity[i] *= config.damping;
        self.height[i] = self.prev_height[i] + self.velocity[i];

        // Clamp height
        self.height[i] = self.height[i].clamp(-config.max_height, config.max_height);

        // Zero out near-zero values
        if self.height[i].abs() < config.epsilon && self.velocity[i].abs() < config.epsilon {
            self.height[i] = 0.0;
            self.velocity[i] = 0.0;
        }
    }
}
```

**Step 3: Run existing tests**

Run: `cargo test fluid::wave`
Expected: All 6 existing tests pass (the behavior change from removing double damping may shift numeric values but should not change test outcomes since tests check directional properties, not exact values).

**Step 4: Commit**

```
git add src/fluid/wave.rs
git commit -m "refactor(wave): double-buffer WaveBuffer to eliminate per-tick clone"
```

---

### Task 2: Wave amplitude limiting + nonlinear damping

**Files:**
- Modify: `src/fluid/wave.rs:132-153` (WaveConfig)
- Modify: `src/fluid/wave.rs:41-46` (apply_impulse)
- Modify: `src/fluid/wave.rs:53-127` (step)

**Step 1: Add max_impulse to WaveConfig and lower max_height**

```rust
pub struct WaveConfig {
    pub speed: f32,
    pub damping: f32,
    pub epsilon: f32,
    pub max_height: f32,
    pub max_impulse: f32,          // NEW
    pub high_wave_threshold: f32,  // NEW: fraction of max_height above which extra damping kicks in
    pub high_wave_damping: f32,    // NEW: damping for waves above threshold
}

impl Default for WaveConfig {
    fn default() -> Self {
        Self {
            speed: 0.4,
            damping: 0.98,
            epsilon: 0.001,
            max_height: 1.5,        // was 5.0 — now ~12px max displacement
            max_impulse: 2.0,       // NEW: clamp input impulse
            high_wave_threshold: 0.7, // NEW: 70% of max_height
            high_wave_damping: 0.90,  // NEW: aggressive damping for large waves
        }
    }
}
```

**Step 2: Clamp impulse in apply_impulse()**

```rust
pub fn apply_impulse(&mut self, local_x: u32, local_y: u32, impulse: f32, max_impulse: f32) {
    let idx = (local_y * self.chunk_size + local_x) as usize;
    if idx < self.velocity.len() {
        self.velocity[idx] += impulse.clamp(-max_impulse, max_impulse);
    }
}
```

**Step 3: Add nonlinear damping to step()**

After the velocity damping line in `step()`, add:

```rust
// Nonlinear damping: large waves decay faster
if self.height[i].abs() > config.max_height * config.high_wave_threshold {
    self.velocity[i] *= config.high_wave_damping / config.damping; // extra damping factor
}
```

**Step 4: Update all apply_impulse() callers to pass max_impulse**

Search for `apply_impulse(` and update:
- `src/fluid/systems.rs:535` — `buf.apply_impulse(local_x, local_y, impulse, wave_config.max_impulse);`
- `src/fluid/systems.rs:541` — `buf.apply_impulse(local_x - 1, local_y, spread, wave_config.max_impulse);`
- `src/fluid/systems.rs:544` — `buf.apply_impulse(local_x + 1, local_y, spread, wave_config.max_impulse);`
- `src/fluid/splash.rs:220` — `buf.apply_impulse(local_x, local_y, impulse, 2.0);` (reabsorb impulse is tiny, hardcode is fine)

Also need to pass `WaveConfig` to `wave_consume_events` — it already doesn't have it. Add `wave_config: Res<WaveConfig>` parameter.

**Step 5: Run tests**

Run: `cargo test fluid::wave && cargo test fluid::splash`
Expected: All pass. The `max_height_clamped` test may need updating since max_height changed from 5.0 to 1.5.

**Step 6: Commit**

```
git add src/fluid/wave.rs src/fluid/systems.rs src/fluid/splash.rs
git commit -m "fix(wave): limit amplitude with max_impulse clamp and nonlinear damping"
```

---

### Task 3: Boundary reconciliation fix

**Files:**
- Modify: `src/fluid/wave.rs:171-227` (reconcile_wave_boundaries)
- Modify: `src/fluid/systems.rs:508-547` (wave_consume_events)

**Step 1: Write failing test for velocity reconciliation**

Add to `src/fluid/wave.rs` tests module:

```rust
#[test]
fn reconcile_averages_velocity_at_boundary() {
    let chunk_size = 4u32;
    let mut wave_state = WaveState::default();
    let mut active = HashSet::new();

    // Two adjacent chunks: (0,0) and (1,0)
    active.insert((0, 0));
    active.insert((1, 0));

    let mut left = WaveBuffer::new(chunk_size);
    let mut right = WaveBuffer::new(chunk_size);

    // Set velocity at boundary: left's right edge has vel=2.0, right's left edge has vel=0.0
    let left_idx = (0 * chunk_size + (chunk_size - 1)) as usize; // row 0, rightmost col
    let right_idx = (0 * chunk_size + 0) as usize; // row 0, leftmost col
    left.velocity[left_idx] = 2.0;
    right.velocity[right_idx] = 0.0;

    wave_state.buffers.insert((0, 0), left);
    wave_state.buffers.insert((1, 0), right);

    reconcile_wave_boundaries(&mut wave_state, &active, chunk_size, 2);

    let left_vel = wave_state.buffers[&(0, 0)].velocity[left_idx];
    let right_vel = wave_state.buffers[&(1, 0)].velocity[right_idx];
    assert!(
        (left_vel - 1.0).abs() < 1e-5,
        "left boundary velocity should be averaged to 1.0, got {left_vel}"
    );
    assert!(
        (right_vel - 1.0).abs() < 1e-5,
        "right boundary velocity should be averaged to 1.0, got {right_vel}"
    );
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test fluid::wave::tests::reconcile_averages_velocity`
Expected: FAIL (velocity not averaged yet).

**Step 3: Update reconcile_wave_boundaries to average velocity too**

Change the updates vector type to include velocity:
```rust
let mut updates: Vec<((i32, i32), (i32, i32), u32, f32, f32)> = Vec::new();
//                                                         ^height ^velocity
```

In the collection loop, add velocity averaging:
```rust
let avg_h = (left_buf.height[left_idx] + right_buf.height[right_idx]) * 0.5;
let avg_v = (left_buf.velocity[left_idx] + right_buf.velocity[right_idx]) * 0.5;
updates.push((left_key, right_key, local_y, avg_h, avg_v));
```

In the apply loop, set both:
```rust
for (left_key, right_key, local_y, avg_h, avg_v) in updates {
    let left_idx = (local_y * chunk_size + (chunk_size - 1)) as usize;
    let right_idx = (local_y * chunk_size) as usize;

    if let Some(buf) = wave_state.buffers.get_mut(&left_key) {
        buf.height[left_idx] = avg_h;
        buf.velocity[left_idx] = avg_v;
    }
    if let Some(buf) = wave_state.buffers.get_mut(&right_key) {
        buf.height[right_idx] = avg_h;
        buf.velocity[right_idx] = avg_v;
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test fluid::wave::tests::reconcile_averages_velocity`
Expected: PASS.

**Step 5: Add cross-chunk impulse spread in wave_consume_events**

In `src/fluid/systems.rs`, in the splash spread section (after line 538), add cross-chunk spread:

```rust
if matches!(event.kind, ImpactKind::Splash) {
    let spread = impulse * 0.5;
    if local_x > 0 {
        buf.apply_impulse(local_x - 1, local_y, spread, wave_config.max_impulse);
    } else {
        // Spread to left neighbor chunk's rightmost column
        let left_cx = active_world.wrap_chunk_x(data_cx - 1);
        let left_buf = wave_state
            .buffers
            .entry((left_cx, cy))
            .or_insert_with(|| WaveBuffer::new(chunk_size));
        left_buf.apply_impulse(chunk_size - 1, local_y, spread, wave_config.max_impulse);
    }
    if local_x + 1 < chunk_size {
        buf.apply_impulse(local_x + 1, local_y, spread, wave_config.max_impulse);
    } else {
        // Spread to right neighbor chunk's leftmost column
        let right_cx = active_world.wrap_chunk_x(data_cx + 1);
        let right_buf = wave_state
            .buffers
            .entry((right_cx, cy))
            .or_insert_with(|| WaveBuffer::new(chunk_size));
        right_buf.apply_impulse(0, local_y, spread, wave_config.max_impulse);
    }
}
```

Note: this requires reorganizing the borrow on `buf` since we need mutable access to potentially different entries in `wave_state.buffers`. The main impulse must be applied first, then spread handled separately. May need to split the logic:
1. Compute and store impulse/spread values
2. Apply main impulse
3. Apply left spread (may be different chunk)
4. Apply right spread (may be different chunk)

**Step 6: Run all wave tests**

Run: `cargo test fluid::wave && cargo test fluid::splash`
Expected: All pass.

**Step 7: Commit**

```
git add src/fluid/wave.rs src/fluid/systems.rs
git commit -m "fix(wave): reconcile velocity at chunk boundaries + cross-chunk impulse spread"
```

---

### Task 4: Enhanced splash particles

**Files:**
- Modify: `src/fluid/splash.rs:30-39` (SplashConfig defaults)
- Modify: `src/fluid/splash.rs:88-157` (spawn_splash_particles particle spawning)

**Step 1: Update SplashConfig defaults**

```rust
impl Default for SplashConfig {
    fn default() -> Self {
        Self {
            splash_displacement: 0.15,   // was 0.3 — less CA mass removal
            particles_per_mass: 25.0,    // was 15.0 — more visual density
            particle_lifetime: 1.5,
            particle_size: 4.0,          // base size, will be randomized
            min_splash_velocity: 5.0,
        }
    }
}
```

**Step 2: Add size variation + ripple ring particles**

In the Splash branch of `spawn_splash_particles`, after spawning the main fan particles:

a) Randomize size per particle: replace fixed `splash_config.particle_size` with `rng.gen_range(2.0..6.0)` using `rand` or a simple hash-based pseudo-random.

Note: Check if `rand` is already a dependency. If not, use a deterministic approach:
```rust
let size = splash_config.particle_size * (0.5 + (i as f32 / particle_count as f32));
```
This gives sizes from 50% to 100% of base size.

b) Add ripple ring particles after the main fan loop:
```rust
// Ripple ring: horizontal particles that spread outward on the surface
let ripple_count = (particle_count / 3).clamp(2, 8);
let ripple_mass = 0.0; // visual only, no CA mass
for j in 0..ripple_count {
    let t = j as f32 / ripple_count as f32;
    let direction = if t < 0.5 { -1.0 } else { 1.0 };
    let spread_speed = speed * 0.6 * (0.5 + t.fract());
    let vx = direction * spread_speed;
    let vy = speed * 0.1; // slight upward

    pool.spawn(
        event.position,
        Vec2::new(vx, vy),
        ripple_mass,
        FluidId::NONE,   // no reabsorption (visual only)
        0.4,              // short lifetime
        splash_config.particle_size * 0.5, // small
        [color[0] * 1.3, color[1] * 1.3, color[2] * 1.3, color[3] * 0.6], // lighter, semi-transparent
        0.3,              // low gravity — stays near surface
        true,
    );
}
```

**Step 3: Run tests**

Run: `cargo test fluid::splash`
Expected: All pass.

**Step 4: Commit**

```
git add src/fluid/splash.rs
git commit -m "feat(splash): enhance particles with size variation and ripple ring effect"
```

---

### Task 5: Integration verification

**Step 1: Run full test suite**

Run: `cargo test`
Expected: All tests pass.

**Step 2: Visual verification**

Run: `cargo run`
Check:
- Jump into water: small wave displacement (~1-1.5 tiles max), visible particle spray
- Walk through water: subtle wake without massive mesh deformation
- At chunk boundaries: waves propagate smoothly, foam appears on both sides
- Ripple ring particles spread horizontally on splash

**Step 3: Final commit if any adjustments needed**

Tweak constants if visual result needs adjustment (max_height, particles_per_mass, etc.)
