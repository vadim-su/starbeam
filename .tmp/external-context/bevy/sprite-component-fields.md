---
source: Context7 API (docs.rs/bevy/latest)
library: Bevy
package: bevy
topic: Sprite component fields, constructors, image-based sprites, flip, replacing from_color
fetched: 2026-02-26T00:00:00Z
official_docs: https://docs.rs/bevy/latest/bevy/sprite/prelude/struct.Sprite.html
---

# Sprite Component Fields & Constructors (Bevy 0.18)

## Struct Definition (Confirmed from docs.rs)

```rust
pub struct Sprite {
    pub image: Handle<Image>,          // ← field name is `image`, NOT `texture`
    pub texture_atlas: Option<TextureAtlas>,
    pub color: Color,
    pub flip_x: bool,
    pub flip_y: bool,
    pub custom_size: Option<Vec2>,     // None = render at natural pixel size
    pub rect: Option<Rect>,
    pub image_mode: SpriteImageMode,
}
```

## Field Details

| Field | Type | Default | Notes |
|-------|------|---------|-------|
| **`image`** | `Handle<Image>` | default handle | **The texture handle. Field name is `image`, NOT `texture`.** |
| **`texture_atlas`** | `Option<TextureAtlas>` | `None` | For sprite sheet rendering |
| **`color`** | `Color` | `Color::WHITE` | Color tint applied to the sprite |
| **`flip_x`** | `bool` | `false` | Flip horizontally |
| **`flip_y`** | `bool` | `false` | Flip vertically |
| **`custom_size`** | `Option<Vec2>` | `None` | **`None` = natural pixel size from image** |
| **`rect`** | `Option<Rect>` | `None` | Sub-region of image to render |
| **`image_mode`** | `SpriteImageMode` | default | How the image is scaled |

## Constructor Methods

### `Sprite::from_image(image: Handle<Image>) -> Sprite`

Creates a sprite with all defaults: no flip, no custom_size (natural pixel size), white color tint.

```rust
fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn(Camera2d);
    commands.spawn(Sprite::from_image(asset_server.load("branding/icon.png")));
}
```

### `Sprite::from_color(color: impl Into<Color>, size: Vec2) -> Sprite`

Creates a colored rectangle (no image texture, solid color with given size).

```rust
commands.spawn(Sprite::from_color(Color::srgb(0.2, 0.8, 0.3), Vec2::new(64.0, 64.0)));
```

### `Sprite::from_atlas_image(image: Handle<Image>, atlas: TextureAtlas) -> Sprite`

Creates a sprite from an image with a texture atlas (for sprite sheets).

### `Sprite::sized(custom_size: Vec2) -> Sprite`

Creates a Sprite with a custom size.

## Loading Individual Images

```rust
// asset_server.load() returns Handle<Image>
let handle: Handle<Image> = asset_server.load("path/to/image.png");
```

`AssetServer::load()` returns `Handle<Image>` for image files. Queues loading in background. You can use the handle immediately — entities render once the asset is loaded. Calling `load()` multiple times with the same path returns the same handle.

## Replacing from_color with Image-Based Sprite

```rust
// BEFORE (colored rectangle):
Sprite::from_color(Color::srgb(0.2, 0.8, 0.3), Vec2::new(32.0, 32.0))

// AFTER option 1 — constructor (natural pixel size, no flip):
Sprite::from_image(asset_server.load("sprites/player.png"))

// AFTER option 2 — struct syntax (with flip_x, natural pixel size):
Sprite {
    image: asset_server.load("sprites/player.png"),
    flip_x: true,  // flip horizontally
    // custom_size: None by default — renders at natural pixel size
    ..default()
}
```

### Key points for the migration:
- **`image`** is the field name (NOT `texture`)
- **`custom_size: None`** (the default) renders at natural pixel size — do NOT set it
- **`flip_x: true`** flips horizontally
- To swap the image for animation: `sprite.image = new_handle.clone();`

## Swapping Image for Animation (Individual Images)

```rust
// Mutate the `image` field directly to swap the displayed texture:
fn animate_sprite(
    time: Res<Time>,
    mut query: Query<(&mut Sprite, &mut AnimationTimer, &mut AnimationFrames)>,
) {
    for (mut sprite, mut timer, mut frames) in &mut query {
        timer.0.tick(time.delta());
        if timer.0.just_finished() {
            frames.current = (frames.current + 1) % frames.frames.len();
            sprite.image = frames.frames[frames.current].clone();
        }
    }
}
```
