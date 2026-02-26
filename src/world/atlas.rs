use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Parameters of the combined atlas for UV computation.
#[derive(Resource, Debug, Clone)]
#[allow(dead_code)]
pub struct AtlasParams {
    pub tile_size: u32,    // 16
    pub rows: u32,         // 47
    pub atlas_width: u32,  // N_types * tile_size
    pub atlas_height: u32, // rows * tile_size = 752
}

/// Combined atlas texture handle + layout parameters.
#[derive(Resource)]
#[allow(dead_code)]
pub struct TileAtlas {
    pub image: Handle<Image>,
    pub params: AtlasParams,
}

/// Build a combined horizontal atlas from individual per-type spritesheet images.
/// Each source image is a single column of `rows` sprites (`tile_size` × `rows*tile_size` px).
/// Returns the combined Image + column index mapping.
///
/// `sources` is an ordered list of (name, Image) pairs.
/// Returns (combined Image, HashMap<name, column_index>).
pub fn build_combined_atlas(
    sources: &[(&str, &Image)],
    tile_size: u32,
    rows: u32,
) -> (Image, std::collections::HashMap<String, u32>) {
    use std::collections::HashMap;

    let num_types = sources.len() as u32;
    let atlas_width = num_types * tile_size;
    let atlas_height = rows * tile_size;

    // Create RGBA8 image buffer (zeroed = fully transparent)
    let mut data = vec![0u8; (atlas_width * atlas_height * 4) as usize];

    let mut column_map = HashMap::new();

    for (col_idx, (name, src_image)) in sources.iter().enumerate() {
        column_map.insert(name.to_string(), col_idx as u32);

        let src_data = src_image
            .data
            .as_ref()
            .expect("source image must have pixel data");
        let src_width = src_image.width();
        let src_height = src_image.height();

        // Copy pixel by pixel from source into atlas column
        let copy_h = src_height.min(atlas_height);
        let copy_w = src_width.min(tile_size);

        for y in 0..copy_h {
            for x in 0..copy_w {
                let src_idx = ((y * src_width + x) * 4) as usize;
                let dst_x = col_idx as u32 * tile_size + x;
                let dst_idx = ((y * atlas_width + dst_x) * 4) as usize;

                if src_idx + 3 < src_data.len() && dst_idx + 3 < data.len() {
                    data[dst_idx] = src_data[src_idx];
                    data[dst_idx + 1] = src_data[src_idx + 1];
                    data[dst_idx + 2] = src_data[src_idx + 2];
                    data[dst_idx + 3] = src_data[src_idx + 3];
                }
            }
        }
    }

    let mut image = Image::new(
        Extent3d {
            width: atlas_width,
            height: atlas_height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        default(),
    );

    // Pixel art: nearest filtering, clamp to edge
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Nearest,
        min_filter: ImageFilterMode::Nearest,
        mipmap_filter: ImageFilterMode::Nearest,
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        ..default()
    });

    (image, column_map)
}

/// Compute UV coordinates for a tile sprite in the combined atlas.
/// Returns (u_min, u_max, v_min, v_max) with half-pixel inset to prevent texture bleeding.
pub fn atlas_uv(column: u32, row: u32, params: &AtlasParams) -> (f32, f32, f32, f32) {
    let ts = params.tile_size as f32;
    let half = 0.5;

    let u_min = (column as f32 * ts + half) / params.atlas_width as f32;
    let u_max = (column as f32 * ts + ts - half) / params.atlas_width as f32;
    let v_min = (row as f32 * ts + half) / params.atlas_height as f32;
    let v_max = (row as f32 * ts + ts - half) / params.atlas_height as f32;

    (u_min, u_max, v_min, v_max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_uv_first_tile() {
        let params = AtlasParams {
            tile_size: 16,
            rows: 47,
            atlas_width: 48,   // 3 types
            atlas_height: 752, // 47 * 16
        };
        let (u_min, u_max, v_min, v_max) = atlas_uv(0, 0, &params);
        assert!(u_min > 0.0, "half-pixel inset");
        assert!(u_max < 16.0 / 48.0);
        assert!(v_min > 0.0);
        assert!(v_max < 16.0 / 752.0);
    }

    #[test]
    fn atlas_uv_second_column() {
        let params = AtlasParams {
            tile_size: 16,
            rows: 47,
            atlas_width: 48,
            atlas_height: 752,
        };
        let (u_min, _, _, _) = atlas_uv(1, 0, &params);
        let expected = (16.0 + 0.5) / 48.0;
        assert!((u_min - expected).abs() < 0.001);
    }

    #[test]
    fn atlas_uv_last_row() {
        let params = AtlasParams {
            tile_size: 16,
            rows: 47,
            atlas_width: 48,
            atlas_height: 752,
        };
        let (_, _, v_min, v_max) = atlas_uv(0, 46, &params);
        assert!(v_min > 46.0 * 16.0 / 752.0);
        assert!(v_max < 47.0 * 16.0 / 752.0);
    }
}
