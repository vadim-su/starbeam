---
source: Context7 API
library: Bevy
package: bevy
topic: bevy::image::Image API for texture atlas construction
fetched: 2026-02-26T12:00:00Z
official_docs: https://docs.rs/bevy/latest/bevy/image/struct.Image.html
---

# `bevy::image::Image` — Full API Reference for Texture Atlas Construction

## 1. Struct Definition & Fields

```rust
pub struct Image {
    /// Raw pixel data. `None` if storage texture not needing CPU init.
    pub data: Option<Vec<u8>>,

    /// Controls wgpu buffer layout for layered/mipmapped textures.
    pub data_order: TextureDataOrder,

    /// GPU texture data layout (dimensions, format).
    pub texture_descriptor: TextureDescriptor<Option<&'static str>, &'static [TextureFormat]>,

    /// The sampler to use during rendering.
    pub sampler: ImageSampler,

    /// How the GPU texture should be interpreted (2D, array, cube map).
    pub texture_view_descriptor: Option<TextureViewDescriptor<Option<&'static str>>>,

    /// How the asset can be used across different Bevy worlds.
    pub asset_usage: RenderAssetUsages,

    /// Whether this image should be copied on the GPU when resized.
    pub copy_on_resize: bool,
}
```

**Key point**: `data` is `Option<Vec<u8>>` — it's an Option, not a bare Vec.

---

## 2. Constructors

### `Image::new()` — Create from raw pixel data

```rust
pub fn new(
    size: Extent3d,
    dimension: TextureDimension,
    data: Vec<u8>,
    format: TextureFormat,
    asset_usage: RenderAssetUsages,
) -> Image
```

**Panics** if `data.len()` doesn't match `size volume × format byte size`.

**Example for atlas construction:**
```rust
use bevy::image::Image;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::render_asset::RenderAssetUsages;

let atlas_width = 512u32;
let atlas_height = 512u32;
let bytes_per_pixel = 4; // Rgba8UnormSrgb
let data = vec![0u8; (atlas_width * atlas_height * bytes_per_pixel) as usize];

let atlas_image = Image::new(
    Extent3d {
        width: atlas_width,
        height: atlas_height,
        depth_or_array_layers: 1,
    },
    TextureDimension::D2,
    data,
    TextureFormat::Rgba8UnormSrgb,
    RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
);
```

### `Image::new_fill()` — Fill with repeated pixel

```rust
pub fn new_fill(
    size: Extent3d,
    dimension: TextureDimension,
    pixel: &[u8],
    format: TextureFormat,
    asset_usage: RenderAssetUsages,
) -> Image
```

### `Image::new_uninit()` — Uninitialized image

```rust
pub fn new_uninit(
    size: Extent3d,
    dimension: TextureDimension,
    format: TextureFormat,
    asset_usage: RenderAssetUsages,
) -> Image
```

### Other constructors

- `Image::transparent()` — 1×1 transparent white
- `Image::default_uninit()` — 1×1 uninitialized
- `Image::new_target_texture(width, height, format, view_format)` — zero-filled render target

---

## 3. Accessing Pixel Data

### The `data` field — `Option<Vec<u8>>`

```rust
// Direct field access (it's pub):
let raw: &Option<Vec<u8>> = &image.data;

// To get the bytes:
if let Some(ref bytes) = image.data {
    // bytes is &Vec<u8>
}

// Or for mutable access:
if let Some(ref mut bytes) = image.data {
    // bytes is &mut Vec<u8>, you can write pixels directly
}
```

### Per-pixel accessor methods

```rust
/// Byte offset for pixel at coords. Returns None if out of bounds.
/// For 2D textures, coords.z is the layer number.
pub fn pixel_data_offset(&self, coords: UVec3) -> Option<usize>

/// Immutable slice of pixel bytes at coords.
pub fn pixel_bytes(&self, coords: UVec3) -> Option<&[u8]>

/// Mutable slice of pixel bytes at coords.
pub fn pixel_bytes_mut(&mut self, coords: UVec3) -> Option<&mut [u8]>

/// Clear entire image with a repeated pixel value.
pub fn clear(&mut self, pixel: &[u8])

/// Read color at 1D coordinate.
pub fn get_color_at_1d(&self, x: u32) -> Result<Color, TextureAccessError>
```

**For atlas pixel-copying, two approaches:**

**Approach A — Bulk copy via `data` field (fastest for atlas construction):**
```rust
let src_data = source_image.data.as_ref().unwrap();
let dst_data = atlas_image.data.as_mut().unwrap();
let bpp = 4usize; // bytes per pixel for Rgba8UnormSrgb

for row in 0..src_height {
    let src_offset = (row * src_width * bpp as u32) as usize;
    let dst_offset = ((dst_y + row) * atlas_width * bpp as u32 + dst_x * bpp as u32) as usize;
    let row_bytes = (src_width as usize) * bpp;
    dst_data[dst_offset..dst_offset + row_bytes]
        .copy_from_slice(&src_data[src_offset..src_offset + row_bytes]);
}
```

**Approach B — Per-pixel via accessor methods:**
```rust
for y in 0..src_height {
    for x in 0..src_width {
        let pixel = source_image.pixel_bytes(UVec3::new(x, y, 0)).unwrap();
        let dst = atlas_image.pixel_bytes_mut(UVec3::new(dst_x + x, dst_y + y, 0)).unwrap();
        dst.copy_from_slice(pixel);
    }
}
```

---

## 4. Width / Height / Size Methods

```rust
/// Returns the width of a 2D image.
pub fn width(&self) -> u32

/// Returns the height of a 2D image.
pub fn height(&self) -> u32

/// Returns dimensions as UVec2.
pub fn size(&self) -> UVec2

/// Returns dimensions as Vec2 (f32).
pub fn size_f32(&self) -> Vec2

/// Returns width / height as AspectRatio.
pub fn aspect_ratio(&self) -> AspectRatio

/// Whether the texture format is compressed.
pub fn is_compressed(&self) -> bool
```

---

## 5. Setting the Sampler

The `sampler` field is **public** — you assign directly:

```rust
image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
    address_mode_u: ImageAddressMode::ClampToEdge,
    address_mode_v: ImageAddressMode::ClampToEdge,
    mag_filter: ImageFilterMode::Nearest,
    min_filter: ImageFilterMode::Nearest,
    ..default()
});
```

### `ImageSampler` enum

```rust
pub enum ImageSampler {
    /// Use the default sampler from ImagePlugin.
    Default,
    /// Custom sampler for this image (overrides global default).
    Descriptor(ImageSamplerDescriptor),
}
```

**Convenience constructors:**
```rust
ImageSampler::linear()   // Linear min + mag filters
ImageSampler::nearest()  // Nearest min + mag filters
```

### Loading with sampler settings

```rust
let image_handle = asset_server.load_with_settings(
    "my_texture.png",
    |s: &mut ImageLoaderSettings| {
        s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            ..default()
        });
    },
);
```

---

## 6. Import Paths

### Image & sampler types — `bevy::image`

```rust
use bevy::image::{
    Image,
    ImageSampler,
    ImageSamplerDescriptor,
    ImageFilterMode,
    ImageAddressMode,
    ImageLoaderSettings,    // if loading with custom settings
    TextureDataOrder,       // rarely needed
};
```

All of these are also available via `bevy::image::prelude::*` for `Image`.

### Render resource types — `bevy::render::render_resource`

```rust
use bevy::render::render_resource::{
    Extent3d,
    TextureDimension,
    TextureFormat,
    TextureUsages,          // if customizing usage flags
};
```

### Asset usage — `bevy::render::render_asset`

```rust
use bevy::render::render_asset::RenderAssetUsages;
```

### Combined imports for atlas construction

```rust
use bevy::image::{Image, ImageSampler, ImageSamplerDescriptor, ImageFilterMode, ImageAddressMode};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::math::UVec3; // for pixel_bytes / pixel_bytes_mut coords
```

---

## 7. Complete Atlas Construction Example

```rust
use bevy::prelude::*;
use bevy::image::{Image, ImageSampler, ImageSamplerDescriptor, ImageFilterMode};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::render_asset::RenderAssetUsages;

fn build_atlas(
    source_images: &[&Image],
    atlas_width: u32,
    atlas_height: u32,
) -> Image {
    let bpp = 4usize; // Rgba8UnormSrgb = 4 bytes per pixel
    let mut data = vec![0u8; (atlas_width * atlas_height) as usize * bpp];

    let mut cursor_x = 0u32;
    let mut cursor_y = 0u32;
    let mut row_height = 0u32;

    for src in source_images {
        let sw = src.width();
        let sh = src.height();

        if cursor_x + sw > atlas_width {
            cursor_x = 0;
            cursor_y += row_height;
            row_height = 0;
        }

        let src_data = src.data.as_ref().expect("source image has no CPU data");

        for row in 0..sh {
            let src_off = (row * sw) as usize * bpp;
            let dst_off = ((cursor_y + row) * atlas_width + cursor_x) as usize * bpp;
            let row_len = sw as usize * bpp;
            data[dst_off..dst_off + row_len]
                .copy_from_slice(&src_data[src_off..src_off + row_len]);
        }

        cursor_x += sw;
        row_height = row_height.max(sh);
    }

    let mut atlas = Image::new(
        Extent3d {
            width: atlas_width,
            height: atlas_height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );

    // Nearest-neighbor for pixel art; use ImageFilterMode::Linear for smooth
    atlas.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Nearest,
        min_filter: ImageFilterMode::Nearest,
        ..default()
    });

    atlas
}
```
