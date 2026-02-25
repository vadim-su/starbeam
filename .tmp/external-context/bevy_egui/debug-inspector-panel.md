---
source: Context7 API + egui docs
library: bevy_egui + egui
package: bevy_egui
topic: debug inspector panel, SidePanel, CollapsingHeader, semi-transparent, labeled rows
fetched: 2025-02-25T12:00:00Z
official_docs: https://docs.rs/egui/0.33/egui/
---

# Debug Inspector Panel with bevy_egui

## Complete Example: Semi-Transparent Debug Panel

```rust
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin, EguiPrimaryContextPass};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin::default())
        .add_systems(Startup, setup)
        .add_systems(EguiPrimaryContextPass, debug_inspector_panel)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn debug_inspector_panel(mut contexts: EguiContexts) -> Result {
    let ctx = contexts.ctx_mut()?;

    // Semi-transparent side panel with custom frame
    egui::SidePanel::left("debug_inspector")
        .default_width(280.0)
        .resizable(true)
        .frame(
            egui::Frame::NONE
                .fill(egui::Color32::from_rgba_unmultiplied(20, 20, 30, 200))
                .inner_margin(egui::Margin::same(8))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)))
        )
        .show(ctx, |ui| {
            ui.heading("Debug Inspector");
            ui.separator();

            // --- Performance Section ---
            egui::CollapsingHeader::new("Performance")
                .default_open(true)
                .show(ui, |ui| {
                    // Grid for aligned label-value rows
                    egui::Grid::new("perf_grid")
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("FPS:");
                            ui.colored_label(egui::Color32::LIGHT_GREEN, "60.0");
                            ui.end_row();

                            ui.label("Frame Time:");
                            ui.label("16.6ms");
                            ui.end_row();

                            ui.label("Entities:");
                            ui.label("1,234");
                            ui.end_row();
                        });
                });

            // --- Transform Section ---
            egui::CollapsingHeader::new("Player Transform")
                .default_open(true)
                .show(ui, |ui| {
                    egui::Grid::new("transform_grid")
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Position:");
                            ui.monospace("(120.5, -45.2)");
                            ui.end_row();

                            ui.label("Rotation:");
                            ui.monospace("0.0 deg");
                            ui.end_row();

                            ui.label("Scale:");
                            ui.monospace("(1.0, 1.0)");
                            ui.end_row();
                        });
                });

            // --- Collapsed by default ---
            egui::CollapsingHeader::new("Advanced")
                .default_open(false)
                .show(ui, |ui| {
                    ui.label("Detailed diagnostics here...");
                });
        });

    Ok(())
}
```

## SidePanel API

### Creating a SidePanel

```rust
// Left panel (most common for debug)
egui::SidePanel::left("panel_id").show(ctx, |ui| { /* ... */ });

// Right panel
egui::SidePanel::right("panel_id").show(ctx, |ui| { /* ... */ });
```

### SidePanel Configuration Methods

```rust
egui::SidePanel::left("my_panel")
    .default_width(250.0)          // Initial width (default: 200.0)
    .min_width(100.0)              // Minimum width
    .max_width(400.0)              // Maximum width
    .exact_width(300.0)            // Fixed width (disables resize)
    .width_range(100.0..=500.0)    // Width range
    .resizable(true)               // Allow drag-resize (default: true)
    .show_separator_line(true)     // Show edge line (default: true)
    .frame(custom_frame)           // Custom Frame for background/margins
    .show(ctx, |ui| { /* ... */ });
```

### Semi-Transparent Background

```rust
// Method 1: Using from_rgba_unmultiplied (most explicit)
let frame = egui::Frame::NONE
    .fill(egui::Color32::from_rgba_unmultiplied(20, 20, 30, 200))  // RGBA, A=200/255 opacity
    .inner_margin(egui::Margin::same(8))
    .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)));

// Method 2: Using from_black_alpha / from_white_alpha
let frame = egui::Frame::NONE
    .fill(egui::Color32::from_black_alpha(180));  // Semi-transparent black

// Method 3: Modify the default panel frame
let mut frame = egui::Frame::side_top_panel(ui.style());
frame.fill = egui::Color32::from_rgba_unmultiplied(30, 30, 40, 220);
```

## CollapsingHeader API

### Basic Usage

```rust
// Full form
egui::CollapsingHeader::new("Section Title")
    .default_open(true)    // Start expanded (default: false)
    .show(ui, |ui| {
        ui.label("Content here");
    });

// Shortcut form
ui.collapsing("Section Title", |ui| {
    ui.label("Content here");
});

// Shortcut with default_open
ui.collapsing_header("Section Title", true, |ui| {
    ui.label("Content here");
});
```

### CollapsingHeader with Icon/Indicator

```rust
egui::CollapsingHeader::new(
    egui::RichText::new("Performance")
        .strong()
        .color(egui::Color32::LIGHT_BLUE)
)
.default_open(true)
.show(ui, |ui| {
    // ...
});
```

## Labeled Text Rows (Grid Pattern)

### Using Grid for aligned key-value pairs

```rust
egui::Grid::new("unique_grid_id")
    .num_columns(2)
    .spacing([20.0, 4.0])       // [horizontal, vertical] spacing
    .striped(true)               // Alternating row backgrounds
    .show(ui, |ui| {
        // Row 1
        ui.label("FPS:");
        ui.colored_label(egui::Color32::LIGHT_GREEN, format!("{:.1}", fps));
        ui.end_row();

        // Row 2
        ui.label("Frame Time:");
        ui.label(format!("{:.2}ms", frame_time_ms));
        ui.end_row();

        // Row 3 with monospace
        ui.label("Position:");
        ui.monospace(format!("({:.1}, {:.1})", pos.x, pos.y));
        ui.end_row();
    });
```

### Using ui.horizontal for simple inline rows

```rust
ui.horizontal(|ui| {
    ui.label("FPS:");
    ui.label(
        egui::RichText::new(format!("{:.1}", fps))
            .monospace()
            .color(egui::Color32::LIGHT_GREEN)
    );
});
```

### RichText for styled labels

```rust
// Bold
ui.label(egui::RichText::new("Important").strong());

// Colored
ui.label(egui::RichText::new("60.0").color(egui::Color32::GREEN));

// Monospace
ui.label(egui::RichText::new("(120.5, -45.2)").monospace());

// Combined
ui.label(
    egui::RichText::new("WARNING")
        .strong()
        .color(egui::Color32::YELLOW)
        .size(14.0)
);

// Shortcut for colored label
ui.colored_label(egui::Color32::RED, "Error!");
```

## EguiContexts System Parameter

### Type signature in systems

```rust
// Standard mutable access (most common)
fn my_ui_system(mut contexts: EguiContexts) -> Result {
    let ctx = contexts.ctx_mut()?;  // Returns Result<&mut egui::Context, QuerySingleError>
    // ... draw UI ...
    Ok(())
}

// With other Bevy queries
fn my_ui_system(
    mut contexts: EguiContexts,
    time: Res<Time>,
    query: Query<&Transform, With<Player>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    // ... use time, query, etc. ...
    Ok(())
}
```

### Important: `EguiContexts` vs `EguiContext`

- **`EguiContexts`** = `SystemParam` (use in system function signatures)
- **`EguiContext`** = `Component` on camera entities (use in queries if needed)

```rust
// Using EguiContexts (preferred, simpler)
fn ui_system(mut contexts: EguiContexts) -> Result {
    let ctx = contexts.ctx_mut()?;
    Ok(())
}

// Using raw query (advanced, e.g. exclusive world access)
fn ui_system(world: &mut World) {
    let mut query = world.query_filtered::<&mut EguiContext, With<PrimaryEguiContext>>();
    // ...
}
```
