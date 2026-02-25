---
source: Context7 API + docs.rs/bevy/0.18.0
library: Bevy
package: bevy
version: "0.18.0"
topic: System ordering, scheduling, SystemSet
fetched: 2025-02-25T00:00:00Z
official_docs: https://docs.rs/bevy/0.18.0/bevy/ecs/schedule/trait.SystemSet.html
---

# System Ordering & Scheduling (Bevy 0.18)

## Adding Systems to Schedules

```rust
app.add_systems(Startup, setup);                          // runs once
app.add_systems(Update, my_system);                       // runs every frame
app.add_systems(Update, (system_a, system_b, system_c));  // multiple systems
app.add_systems(FixedUpdate, physics_step);               // fixed timestep
```

## Ordering with .before() / .after()

```rust
app.add_systems(Update, (
    system_two,
    system_one.before(system_two),
    system_three.after(system_two),
));
```

## Chaining Systems (Sequential Execution)

```rust
// All three run in order: print_first → print_mid → print_last
app.add_systems(Update, (print_first, print_mid, print_last).chain());
```

## Defining SystemSets

### Unit Struct (Single Set)

```rust
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct PhysicsSystems;
```

### Enum (Multiple Related Sets)

```rust
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum GameSystems {
    Input,
    Movement,
    Collision,
    Rendering,
}
```

## Configuring SystemSets

```rust
app.configure_sets(Update,
    (
        GameSystems::Input,
        GameSystems::Movement,
        GameSystems::Collision,
        GameSystems::Rendering,
    ).chain()
);
```

## Adding Systems to Sets

```rust
// Single system to a set
app.add_systems(Update, handle_input.in_set(GameSystems::Input));

// Multiple systems to a set
app.add_systems(Update, (
    player_movement,
    enemy_movement,
).in_set(GameSystems::Movement));
```

## Run Conditions

```rust
use bevy::input::common_conditions::*;

app.add_systems(Update, (
    handle_jump.run_if(input_just_pressed(KeyCode::Space)),
    handle_shooting.run_if(input_pressed(KeyCode::Enter)),
));
```

### Custom Run Conditions

```rust
app.add_systems(Update,
    my_system.run_if(|state: Res<GameState>| state.is_playing)
);
```

## Full IntoScheduleConfigs API

```rust
fn in_set(self, set: impl SystemSet) -> ScheduleConfigs<T>
fn before<M>(self, set: impl IntoSystemSet<M>) -> ScheduleConfigs<T>
fn after<M>(self, set: impl IntoSystemSet<M>) -> ScheduleConfigs<T>
fn before_ignore_deferred<M>(self, set: impl IntoSystemSet<M>) -> ScheduleConfigs<T>
fn after_ignore_deferred<M>(self, set: impl IntoSystemSet<M>) -> ScheduleConfigs<T>
fn distributive_run_if<M>(self, condition: impl SystemCondition<M> + Clone) -> ScheduleConfigs<T>
fn run_if<M>(self, condition: impl SystemCondition<M>) -> ScheduleConfigs<T>
fn ambiguous_with<M>(self, set: impl IntoSystemSet<M>) -> ScheduleConfigs<T>
fn ambiguous_with_all(self) -> ScheduleConfigs<T>
fn chain(self) -> ScheduleConfigs<T>
fn chain_ignore_deferred(self) -> ScheduleConfigs<T>
```

## System Ordering Notes

- By default, system execution is **parallel and non-deterministic**
- Systems that mutably access the same data are **incompatible** and cannot run in parallel
- Use `.before()` / `.after()` / `.chain()` to define explicit ordering
- Use `SystemSet` + `configure_sets` to order many systems at once
- System order ambiguities exist when incompatible systems have no explicit ordering

## Complete Plugin Example with SystemSets

```rust
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum CombatSystems {
    TargetSelection,
    DamageCalculation,
    Cleanup,
}

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Update,
            (
                CombatSystems::TargetSelection,
                CombatSystems::DamageCalculation,
                CombatSystems::Cleanup,
            ).chain()
        );

        app.add_systems(Update, target_selection.in_set(CombatSystems::TargetSelection));
        app.add_systems(Update, (
            player_damage_calculation,
            enemy_damage_calculation,
        ).in_set(CombatSystems::DamageCalculation));
    }
}
```
