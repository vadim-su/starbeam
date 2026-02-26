---
source: Context7 API (docs.rs/bevy/latest, bevy-cheatbook)
library: Bevy
package: bevy
topic: OnEnter schedule, system chaining, state transitions
fetched: 2026-02-26T00:00:00Z
official_docs: https://docs.rs/bevy/latest/bevy/state/prelude/struct.OnEnter.html
---

# OnEnter & System Chaining (Bevy 0.18)

## OnEnter Schedule Label

`OnEnter<S>` is a `ScheduleLabel` that runs when the game state enters state `S`.

```rust
pub struct OnEnter<S>(pub S) where S: States;
```

## Chaining Systems in OnEnter — CONFIRMED VALID

**Yes, you can chain systems in OnEnter.** `OnEnter(MyState::InGame)` is a schedule label, and `add_systems` works with any schedule label. `.chain()` works on any tuple of systems regardless of the schedule:

```rust
app.add_systems(OnEnter(MyState::InGame), (system_a, system_b).chain())
```

This is valid Bevy 0.18 code.

## .chain() Method

Treats a collection of systems as a sequence — each system runs after the previous one completes (with deferred commands applied between them).

```rust
// These run in order: spawn_particles → animate_particles → debug_particle_statistics
app.add_systems(Update, (
    spawn_particles,
    animate_particles,
    debug_particle_statistics,
).chain());
```

## .chain_ignore_deferred()

Same as `.chain()` but does NOT apply deferred commands between systems.

```rust
fn chain_ignore_deferred(self) -> ScheduleConfigs<T>
```

## Other Ordering Methods

### .before() / .after()

```rust
app.add_systems(Update, (
    player_movement
        .before(enemy_movement)
        .after(input_handling),
));
```

### Ordering groups

```rust
app.add_systems(Update, (
    (spawn_monsters, spawn_zombies, spawn_spiders).before(enemy_movement),
));
```

## State Transitions

### NextState

```rust
fn start_game(mut next_game_state: ResMut<NextState<GameState>>) {
    next_game_state.set(GameState::InGame);
    // Triggers OnExit(current_state) and OnEnter(GameState::InGame)
}
```

- `set()` — queues transition, triggers `OnEnter` and `OnExit` schedules
- `set_if_neq()` — same but skips if already in that state
- `reset()` — removes pending transition

### States derive

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, States)]
enum GameState {
    #[default]
    MainMenu,
    SettingsMenu,
    InGame,
}
```

## Combined Example: OnEnter with Chained Systems

```rust
app
    .init_state::<GameState>()
    .add_systems(OnEnter(GameState::InGame), (
        spawn_player,
        spawn_enemies,
        setup_ui,
    ).chain())
    .add_systems(Update, (
        player_movement,
        enemy_ai,
        check_collisions,
    ).chain().run_if(in_state(GameState::InGame)));
```
