# Main Menu Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a main menu screen with animated starfield shader background, pixel-font STARBEAM title (blue+orange), and New Game / Exit buttons.

**Architecture:** New `MainMenu` state added before `Loading`. Menu module with custom WGSL starfield shader (Material2d), Bevy native UI for title and buttons. Menu entities despawned on state exit.

**Tech Stack:** Bevy 0.18 (UI, Material2d, WGSL shaders), Silkscreen pixel font (OFL)

---

### Task 1: Add Silkscreen Font

**Files:**
- Create: `assets/fonts/Silkscreen-Regular.ttf`
- Create: `assets/fonts/Silkscreen-Bold.ttf`

Download Silkscreen font from Google Fonts and place in `assets/fonts/`.

### Task 2: Add MainMenu State

**Files:**
- Modify: `src/registry/mod.rs` — add `MainMenu` variant as `#[default]`
- Modify: `src/registry/mod.rs` — change `start_loading` from `Startup` to `OnEnter(AppState::Loading)`

### Task 3: Create Starfield Shader

**Files:**
- Create: `assets/engine/shaders/starfield.wgsl`

Procedural star field with:
- Dark gradient background (#0a0e1a to #050810)
- ~150 procedural stars via hash function
- Per-star random size, fall speed, twinkle frequency
- Time uniform for animation

### Task 4: Create Menu Module

**Files:**
- Create: `src/menu/mod.rs` — MenuPlugin, MenuEntity marker, camera + fullscreen quad spawn/despawn
- Create: `src/menu/starfield.rs` — StarfieldMaterial (Material2d), time update system
- Create: `src/menu/ui.rs` — UI layout (title + buttons), button interaction systems

### Task 5: Integrate in main.rs

**Files:**
- Modify: `src/main.rs` — add `mod menu`, register `MenuPlugin`

### Task 6: Build & Verify

Run `cargo build` and fix any compilation errors.
Run the game and verify menu appears, shader animates, buttons work.
