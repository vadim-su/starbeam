---
source: Context7 API + docs.rs/bevy/0.18.0
library: Bevy
package: bevy
version: "0.18.0"
topic: Sprite component, colored rectangles, 2D rendering
fetched: 2025-02-25T00:00:00Z
official_docs: https://docs.rs/bevy/0.18.0/bevy/prelude/struct.Sprite.html
---

# Sprite & 2D Rendering (Bevy 0.18)

## Sprite Struct

```rust
pub struct Sprite {
    pub image: Handle<Image>,
    pub texture_atlas: Option<TextureAtlas>,
    pub color: Color,
    pub flip_x: bool,
    pub flip_y: bool,
    pub custom_size: Option<Vec2>,
    pub rect: Option<Rect>,
    pub image_mode: SpriteImageMode,
}
```

### Required Components (auto-inserted when spawning Sprite)

- `Transform`
- `Visibility`
- `VisibilityClass`
- `Anchor`

## Sprite Constructors

### from_color — Colored Rectangle (NO image needed)

```rust
pub fn from_color(color: impl Into<Color>, size: Vec2) -> Sprite
```

**This is the key method for rendering colored rectangles without textures.**

```rust
// Spawn a green 64x64 rectangle
commands.spawn((
    Sprite::from_color(Color::srgb(0.2, 0.8, 0.3), Vec2::new(64.0, 64.0)),
    Transform::from_xyz(0.0, 0.0, 0.0),
));

// Spawn a red 32x32 tile
commands.spawn((
    Sprite::from_color(Color::srgb(1.0, 0.0, 0.0), Vec2::new(32.0, 32.0)),
    Transform::from_xyz(100.0, 50.0, 0.0),
));
```

### from_image — Sprite from Image Asset

```rust
pub fn from_image(image: Handle<Image>) -> Sprite
```

```rust
fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn(Sprite::from_image(asset_server.load("player.png")));
}
```

### from_atlas_image — Sprite Sheet

```rust
pub fn from_atlas_image(image: Handle<Image>, atlas: TextureAtlas) -> Sprite
```

```rust
commands.spawn((
    Sprite::from_atlas_image(
        texture,
        TextureAtlas {
            layout: texture_atlas_layout,
            index: 0,
        },
    ),
    Transform::from_scale(Vec3::splat(6.0)),
));
```

### sized — Custom Size (with default image)

```rust
pub fn sized(custom_size: Vec2) -> Sprite
```

## Sprite with Struct Syntax

```rust
commands.spawn(Sprite {
    image: asset_server.load("branding/bevy_bird_dark.png"),
    color: Color::srgb(5.0, 5.0, 5.0),  // color tint
    custom_size: Some(Vec2::splat(160.0)),
    ..default()
});
```

## Color API

### Creating Colors

```rust
// sRGB (standard, what you usually want)
Color::srgb(r: f32, g: f32, b: f32) -> Color          // alpha = 1.0
Color::srgba(r: f32, g: f32, b: f32, a: f32) -> Color  // with alpha

// Linear RGB (for HDR / bloom effects)
LinearRgba::new(r: f32, g: f32, b: f32, a: f32) -> LinearRgba
LinearRgba::rgb(r: f32, g: f32, b: f32) -> LinearRgba

// From Srgba struct
Srgba::new(r: f32, g: f32, b: f32, a: f32) -> Srgba
Srgba::rgb(r: f32, g: f32, b: f32) -> Srgba
```

### Color Constants

```rust
Color::WHITE    // fully white, alpha 1.0
Color::BLACK    // fully black, alpha 1.0
Color::NONE     // fully transparent

LinearRgba::WHITE
LinearRgba::BLACK
LinearRgba::RED
LinearRgba::GREEN
LinearRgba::BLUE
LinearRgba::NONE
```

### Color Conversion

```rust
color.to_linear() -> LinearRgba
color.to_srgba() -> Srgba
```

## Rendering Colored Rectangles (Tile-Based Game Pattern)

```rust
const TILE_SIZE: f32 = 32.0;

fn spawn_tile(commands: &mut Commands, x: f32, y: f32, color: Color) {
    commands.spawn((
        Sprite::from_color(color, Vec2::splat(TILE_SIZE)),
        Transform::from_xyz(x * TILE_SIZE, y * TILE_SIZE, 0.0),
    ));
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    // Spawn a grid of tiles
    for x in 0..10 {
        for y in 0..10 {
            let color = if (x + y) % 2 == 0 {
                Color::srgb(0.3, 0.7, 0.3)  // grass
            } else {
                Color::srgb(0.6, 0.4, 0.2)  // dirt
            };
            spawn_tile(&mut commands, x as f32, y as f32, color);
        }
    }
}
```

## Alternative: Mesh2d + ColorMaterial (for shapes)

```rust
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn(Camera2d);

    // Rectangle via mesh
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(100.0, 50.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.5, 0.0, 0.5))),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    // Circle via mesh
    commands.spawn((
        Mesh2d(meshes.add(Circle::new(50.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.0, 0.5, 1.0))),
        Transform::from_xyz(200.0, 0.0, 0.0),
    ));
}
```

### ColorMaterial Struct

```rust
pub struct ColorMaterial {
    pub color: Color,
    pub alpha_mode: AlphaMode2d,
    pub uv_transform: Affine2,
    pub texture: Option<Handle<Image>>,
}
```

## NOTE: SpriteBundle Does NOT Exist in 0.18

`SpriteBundle` was removed. Use `Sprite` directly as a component.
Required components (Transform, Visibility, etc.) are auto-inserted.
