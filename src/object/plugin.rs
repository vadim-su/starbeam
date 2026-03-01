use std::collections::HashMap;

use bevy::prelude::*;

use super::definition::{ObjectDef, ObjectId, ObjectType, PlacementRule};
use super::registry::ObjectRegistry;
use crate::item::DropDef;
use crate::registry::AppState;
use crate::sets::GameSet;
use crate::world::lit_sprite::{FallbackLightmap, LitSpriteMaterial};

/// Per-type shared materials and animation state for rendered objects.
#[derive(Resource)]
pub struct ObjectSpriteMaterials {
    pub materials: HashMap<ObjectId, Handle<LitSpriteMaterial>>,
    pub animated: Vec<AnimatedObjectType>,
}

/// Tracks animation state for one object type.
pub struct AnimatedObjectType {
    pub object_id: ObjectId,
    pub material: Handle<LitSpriteMaterial>,
    pub timer: Timer,
    pub current_frame: u32,
    pub total_frames: u32,
    pub columns: u32,
    pub rows: u32,
}

pub struct ObjectPlugin;

fn load_object_sprites(
    mut commands: Commands,
    object_registry: Res<ObjectRegistry>,
    asset_server: Res<AssetServer>,
    fallback_lm: Res<FallbackLightmap>,
    mut lit_materials: ResMut<Assets<LitSpriteMaterial>>,
) {
    let mut materials = HashMap::new();
    let mut animated = Vec::new();

    for idx in 0..object_registry.len() {
        let id = ObjectId(idx as u16);
        if id == ObjectId::NONE {
            continue;
        }
        let def = object_registry.get(id);
        if def.sprite.is_empty() {
            continue;
        }

        let texture: Handle<Image> = asset_server.load(&def.sprite);
        let total_frames = def.sprite_columns * def.sprite_rows;
        let scale_x = 1.0 / def.sprite_columns as f32;
        let scale_y = 1.0 / def.sprite_rows as f32;

        let material = lit_materials.add(LitSpriteMaterial {
            sprite: texture,
            lightmap: fallback_lm.0.clone(),
            lightmap_uv_rect: Vec4::new(1.0, 1.0, 0.0, 0.0),
            sprite_uv_rect: Vec4::new(scale_x, scale_y, 0.0, 0.0),
        });

        materials.insert(id, material.clone());

        if def.sprite_fps > 0.0 && total_frames > 1 {
            animated.push(AnimatedObjectType {
                object_id: id,
                material,
                timer: Timer::from_seconds(1.0 / def.sprite_fps, TimerMode::Repeating),
                current_frame: 0,
                total_frames,
                columns: def.sprite_columns,
                rows: def.sprite_rows,
            });
        }
    }

    commands.insert_resource(ObjectSpriteMaterials {
        materials,
        animated,
    });
}

/// Advance animation frames for all animated object types.
/// Updates the shared material's sprite_uv_rect so all instances animate in sync.
fn object_animation_system(
    time: Res<Time>,
    mut object_sprites: Option<ResMut<ObjectSpriteMaterials>>,
    mut lit_materials: ResMut<Assets<LitSpriteMaterial>>,
) {
    let Some(ref mut object_sprites) = object_sprites else {
        return;
    };
    for anim in &mut object_sprites.animated {
        anim.timer.tick(time.delta());
        if anim.timer.just_finished() {
            anim.current_frame = (anim.current_frame + 1) % anim.total_frames;
            let col = anim.current_frame % anim.columns;
            let row = anim.current_frame / anim.columns;
            let scale_x = 1.0 / anim.columns as f32;
            let scale_y = 1.0 / anim.rows as f32;
            let offset_x = col as f32 * scale_x;
            let offset_y = row as f32 * scale_y;

            if let Some(mat) = lit_materials.get_mut(&anim.material) {
                mat.sprite_uv_rect = Vec4::new(scale_x, scale_y, offset_x, offset_y);
            }
        }
    }
}

impl Plugin for ObjectPlugin {
    fn build(&self, app: &mut App) {
        // Hardcoded registry for now (will move to RON loading later)
        app.insert_resource(ObjectRegistry::from_defs(vec![
            // Index 0: NONE placeholder (ObjectId::NONE)
            ObjectDef {
                id: "none".into(),
                display_name: "None".into(),
                size: (1, 1),
                sprite: "".into(),
                solid_mask: vec![false],
                placement: PlacementRule::Any,
                light_emission: [0, 0, 0],
                object_type: ObjectType::Decoration,
                drops: vec![],
                sprite_columns: 1,
                sprite_rows: 1,
                sprite_fps: 0.0,
                flicker_speed: 0.0,
                flicker_strength: 0.0,
                flicker_min: 1.0,
            },
            ObjectDef {
                id: "torch_object".into(),
                display_name: "Torch".into(),
                size: (1, 1),
                sprite: "objects/torch.png".into(),
                solid_mask: vec![false],
                placement: PlacementRule::FloorOrWall,
                light_emission: [255, 170, 40],
                object_type: ObjectType::LightSource,
                drops: vec![DropDef {
                    item_id: "torch".into(),
                    min: 1,
                    max: 1,
                    chance: 1.0,
                }],
                sprite_columns: 4,
                sprite_rows: 5,
                sprite_fps: 10.0,
                flicker_speed: 3.0,
                flicker_strength: 0.5,
                flicker_min: 0.5,
            },
            ObjectDef {
                id: "wooden_chest".into(),
                display_name: "Wooden Chest".into(),
                size: (2, 1),
                sprite: "objects/wooden_chest.png".into(),
                solid_mask: vec![true, true],
                placement: PlacementRule::Floor,
                light_emission: [0, 0, 0],
                object_type: ObjectType::Container { slots: 16 },
                drops: vec![],
                sprite_columns: 1,
                sprite_rows: 1,
                sprite_fps: 0.0,
                flicker_speed: 0.0,
                flicker_strength: 0.0,
                flicker_min: 1.0,
            },
            ObjectDef {
                id: "wooden_table".into(),
                display_name: "Wooden Table".into(),
                size: (3, 2),
                sprite: "objects/wooden_table.png".into(),
                solid_mask: vec![true, false, true, false, false, false],
                placement: PlacementRule::Floor,
                light_emission: [0, 0, 0],
                object_type: ObjectType::Decoration,
                drops: vec![],
                sprite_columns: 1,
                sprite_rows: 1,
                sprite_fps: 0.0,
                flicker_speed: 0.0,
                flicker_strength: 0.0,
                flicker_min: 1.0,
            },
        ]));
        app.add_systems(OnEnter(AppState::InGame), load_object_sprites);
        app.add_systems(Update, object_animation_system.in_set(GameSet::WorldUpdate));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sprite_uv_rect_for_frame() {
        let columns = 4u32;
        let rows = 5u32;
        let scale_x = 1.0 / columns as f32;
        let scale_y = 1.0 / rows as f32;

        // Frame 0: col=0, row=0
        let frame = 0u32;
        assert_eq!((frame % columns, frame / columns), (0, 0));

        // Frame 3: col=3, row=0
        let frame = 3u32;
        assert_eq!((frame % columns, frame / columns), (3, 0));
        assert!((3.0 * scale_x - 0.75).abs() < f32::EPSILON);

        // Frame 4: col=0, row=1
        let frame = 4u32;
        assert_eq!((frame % columns, frame / columns), (0, 1));
        assert!((1.0 * scale_y - 0.2).abs() < f32::EPSILON);

        // Frame 19 (last): col=3, row=4
        let frame = 19u32;
        assert_eq!((frame % columns, frame / columns), (3, 4));
    }
}
