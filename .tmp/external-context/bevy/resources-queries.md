---
source: Context7 API + docs.rs/bevy/0.18.0
library: Bevy
package: bevy
version: "0.18.0"
topic: Resources, Queries, system parameters
fetched: 2025-02-25T00:00:00Z
official_docs: https://docs.rs/bevy/0.18.0/bevy/ecs/prelude/struct.Query.html
---

# Resources & Queries (Bevy 0.18)

## Defining Resources

```rust
#[derive(Resource)]
struct GameState {
    score: u32,
    is_playing: bool,
}

#[derive(Resource, Default)]
struct GameConfig {
    tile_size: f32,
    map_width: u32,
    map_height: u32,
}
```

## Inserting Resources

```rust
// With a value
app.insert_resource(GameState { score: 0, is_playing: true });

// Using Default trait
app.init_resource::<GameConfig>();

// At runtime via Commands
fn setup(mut commands: Commands) {
    commands.insert_resource(GameState { score: 0, is_playing: true });
}
```

## Accessing Resources in Systems

### Read-Only: `Res<T>`

```rust
fn read_score(state: Res<GameState>) {
    println!("Score: {}", state.score);

    // Change detection
    if state.is_changed() {
        println!("State changed!");
    }
}
```

### Mutable: `ResMut<T>`

```rust
fn update_score(mut state: ResMut<GameState>) {
    state.score += 1;
}
```

## Query System Parameter

```rust
Query<D, F>
// D = QueryData (what to fetch)
// F = QueryFilter (optional, defaults to ())
```

### Basic Queries

```rust
// Read-only access to one component
fn system(query: Query<&Position>) {
    for position in &query {
        println!("{:?}", position);
    }
}

// Mutable access
fn system(mut query: Query<&mut Position>) {
    for mut position in &mut query {
        position.x += 1.0;
    }
}

// Multiple components
fn system(query: Query<(&Position, &Velocity)>) {
    for (pos, vel) in &query {
        println!("pos: {:?}, vel: {:?}", pos, vel);
    }
}

// With Entity ID
fn system(query: Query<(Entity, &Position, &mut Velocity)>) {
    for (entity, pos, mut vel) in &query {
        // ...
    }
}
```

### Query Filters

```rust
// With filter — entities that HAVE a component (don't fetch it)
fn system(query: Query<&Position, With<Player>>) {
    for position in &query {
        // Only entities that have both Position AND Player
    }
}

// Without filter — entities that DON'T have a component
fn system(query: Query<&Position, Without<Enemy>>) {
    for position in &query {
        // Only entities with Position but NOT Enemy
    }
}

// Combined filters
fn system(query: Query<&Position, (With<Player>, Without<Dead>)>) {
    // ...
}

// Changed filter — only entities where component changed this frame
fn system(query: Query<&Position, Changed<Velocity>>) {
    for position in &query {
        // Only entities whose Velocity changed
    }
}

// Added filter — only entities where component was just added
fn system(query: Query<&Position, Added<Velocity>>) {
    for position in &query {
        // Only entities that just got a Velocity component
    }
}
```

### Single Entity Queries

```rust
// Exactly one matching entity (panics if 0 or 2+)
fn system(query: Single<&Transform, With<Player>>) {
    let transform = *query;
    // ...
}

// Optional single (0 or 1)
fn system(query: Option<Single<&Transform, With<Player>>>) {
    if let Some(transform) = query {
        // ...
    }
}
```

### Query Methods

```rust
query.iter()           -> impl Iterator       // iterate read-only
query.iter_mut()       -> impl Iterator       // iterate mutable
query.single()         -> Result<Item, Error> // exactly one match
query.get(entity)      -> Result<Item, Error> // get by entity ID
query.get_mut(entity)  -> Result<Item, Error> // get mutable by entity ID
query.is_empty()       -> bool
query.contains(entity) -> bool
```

## Other System Parameters

### Commands

```rust
fn system(mut commands: Commands) {
    commands.spawn(/* ... */);
    commands.entity(entity).despawn();
    commands.insert_resource(/* ... */);
}
```

### EventReader / EventWriter

```rust
#[derive(Event)]
struct MyEvent { value: i32 }

fn send_events(mut writer: EventWriter<MyEvent>) {
    writer.write(MyEvent { value: 42 });
}

fn receive_events(mut reader: EventReader<MyEvent>) {
    for event in reader.read() {
        println!("Got event: {}", event.value);
    }
}
```

### Local State

```rust
fn system(mut counter: Local<u32>) {
    *counter += 1;
    println!("Called {} times", *counter);
}
```

### AssetServer

```rust
fn system(asset_server: Res<AssetServer>) {
    let handle: Handle<Image> = asset_server.load("textures/player.png");
}
```

### Assets<T>

```rust
fn system(
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let mesh_handle = meshes.add(Rectangle::new(100.0, 50.0));
    let material_handle = materials.add(Color::srgb(1.0, 0.0, 0.0));
}
```
