---
source: GitHub (vladbat00/bevy_egui v0.39.1 examples/side_panel.rs)
library: bevy_egui
package: bevy_egui
topic: side panel example, viewport adjustment, overlay camera
fetched: 2025-02-25T12:00:00Z
official_docs: https://github.com/vladbat00/bevy_egui/blob/v0.39.1/examples/side_panel.rs
---

# Official side_panel.rs Example (bevy_egui 0.39.1)

This example shows how to use SidePanel with a separate egui camera overlay,
adjusting the game camera viewport to avoid overlap.

```rust
use bevy::{
    camera::{CameraOutputMode, Viewport, visibility::RenderLayers},
    prelude::*,
    window::PrimaryWindow,
};
use bevy_egui::{
    EguiContext, EguiContexts, EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass,
    PrimaryEguiContext, egui,
};
use wgpu_types::BlendState;

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.25, 0.25, 0.25)))
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin::default())
        .add_systems(Startup, setup_system)
        .add_systems(EguiPrimaryContextPass, ui_example_system)
        .run();
}

fn ui_example_system(
    mut contexts: EguiContexts,
    mut camera: Single<&mut Camera, Without<EguiContext>>,
    window: Single<&mut Window, With<PrimaryWindow>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    let mut left = egui::SidePanel::left("left_panel")
        .resizable(true)
        .show(ctx, |ui| {
            ui.label("Left resizeable panel");
            ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
        })
        .response
        .rect
        .width();

    let mut right = egui::SidePanel::right("right_panel")
        .resizable(true)
        .show(ctx, |ui| {
            ui.label("Right resizeable panel");
            ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
        })
        .response
        .rect
        .width();

    let mut top = egui::TopBottomPanel::top("top_panel")
        .resizable(true)
        .show(ctx, |ui| {
            ui.label("Top resizeable panel");
            ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
        })
        .response
        .rect
        .height();

    let mut bottom = egui::TopBottomPanel::bottom("bottom_panel")
        .resizable(true)
        .show(ctx, |ui| {
            ui.label("Bottom resizeable panel");
            ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
        })
        .response
        .rect
        .height();

    // Scale from logical units to physical units
    left *= window.scale_factor();
    right *= window.scale_factor();
    top *= window.scale_factor();
    bottom *= window.scale_factor();

    let pos = UVec2::new(left as u32, top as u32);
    let size = UVec2::new(window.physical_width(), window.physical_height())
        - pos
        - UVec2::new(right as u32, bottom as u32);

    camera.viewport = Some(Viewport {
        physical_position: pos,
        physical_size: size,
        ..default()
    });

    Ok(())
}

fn setup_system(
    mut commands: Commands,
    mut egui_global_settings: ResMut<EguiGlobalSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // Disable auto primary context to set it up manually
    egui_global_settings.auto_create_primary_context = false;

    // Spawn game entities...
    commands.spawn((
        Mesh2d(meshes.add(Circle::new(50.))),
        MeshMaterial2d(materials.add(ColorMaterial::from_color(Color::srgb(0.2, 0.1, 0.0)))),
        Transform::from_translation(Vec3::new(-150., 0., 0.)),
    ));

    // World camera (renders game)
    commands.spawn(Camera2d);

    // Egui camera (renders UI overlay on top)
    commands.spawn((
        PrimaryEguiContext,
        Camera2d,
        RenderLayers::none(),  // Don't render game entities
        Camera {
            order: 1,          // Render after game camera
            output_mode: CameraOutputMode::Write {
                blend_state: Some(BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
    ));
}
```

## Key Takeaways for Debug Overlay

1. **Separate camera for UI overlay**: Use a second `Camera2d` with `PrimaryEguiContext` and `RenderLayers::none()` so it only renders egui.
2. **Alpha blending**: Set `BlendState::ALPHA_BLENDING` on the UI camera for semi-transparent panels.
3. **`auto_create_primary_context = false`**: Needed when manually controlling which camera gets the egui context.
4. **For a simple overlay** (no viewport adjustment needed): You can skip the dual-camera setup entirely. Just use `EguiPlugin::default()` with a single camera and egui will overlay on top. The dual-camera setup is only needed if you want to adjust the game viewport to avoid panel overlap.

### Simple Overlay (No Viewport Adjustment)

If you just want a semi-transparent debug panel floating over your game:

```rust
fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin::default())
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera2d);
        })
        .add_systems(EguiPrimaryContextPass, debug_panel)
        .run();
}

fn debug_panel(mut contexts: EguiContexts) -> Result {
    let ctx = contexts.ctx_mut()?;

    egui::SidePanel::left("debug")
        .default_width(250.0)
        .frame(
            egui::Frame::NONE
                .fill(egui::Color32::from_rgba_unmultiplied(15, 15, 25, 200))
                .inner_margin(egui::Margin::same(8))
        )
        .show(ctx, |ui| {
            ui.heading("Debug");
            // ... your debug UI ...
        });

    Ok(())
}
```
