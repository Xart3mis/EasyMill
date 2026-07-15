# EasyMill UI Pipeline Redesign

**Date:** 2026-07-16  
**Status:** Approved

---

## Problem

1. **Hang bug (fixed):** `run_conversion_pipeline` called CPU-heavy blocking functions inside an `async fn` without `spawn_blocking`, starving iced's tokio executor and freezing the UI.
2. **Coupled pipeline:** Both conversion stages ran together under one button with no way to run them independently.
3. **No direct PNG input:** Users couldn't supply their own PNG to skip stage 1.
4. **No per-stage output actions:** PNG couldn't be saved; GCode preview was text-only.
5. **No toolpath visualization:** GCode result had no graphical output.
6. **Static progress bar:** Running state showed a fixed 70% — no animation.

---

## Goals

- Run Gerber→PNG and PNG→GCode as independent stages with their own trigger buttons.
- Accept a user-uploaded PNG as input to stage 2 (bypassing stage 1 entirely).
- Show a PNG preview after stage 1 and a 2D toolpath canvas after stage 2.
- Save PNG and Save GCode as separate actions, each available as soon as their stage completes.
- Animate the progress bar while a stage is running.

---

## Architecture

### AppState

```rust
struct AppState {
    // Stage 1 input
    gerber_paths: Vec<PathBuf>,
    gerber_labels: Vec<String>,

    // Active PNG: produced by stage 1 OR loaded directly
    active_png: Option<PathBuf>,
    png_source_label: Option<String>,   // "generated" | "uploaded: <name>"

    // Stage states
    gerber_to_png: StepState,           // Idle/Ready/Running/Complete
    png_to_gcode: StepState,            // Idle/Ready/Running/Complete

    // Stage 2 result
    generated_gcode: Option<String>,    // raw GCode string
    gcode_stats: Option<GcodeStats>,    // estimated_time, cut_distance, width_mm, height_mm
    toolpaths: Vec<Vec<[f32; 2]>>,      // (x,y) coords in mm for canvas

    // Animation
    tick: f32,                          // 0.0→1.0 sawtooth while any stage Running

    // Settings (unchanged)
    dpi_input: String,
    cut_z_mm_input: String,
    safe_z_mm_input: String,
    feed_rate_input: String,
    plunge_rate_input: String,
    spindle_speed_input: String,
    tool_diameter_mm_input: String,
    offset_number_input: String,
    offset_stepover_input: String,

    // Job stats display (unchanged)
    estimated_time: String,
    cut_distance: String,
    board_dimensions: String,
    status: String,
}
```

**Removed:** `selected_paths`, `selected_inputs`, `generated_png` (replaced by `active_png`), `generated_gcode: Option<String>` (split into `generated_gcode` + `gcode_stats` + `toolpaths`).

### Messages

```rust
enum Message {
    // File pickers
    SelectGerberFiles,
    SelectZipArchive,
    SelectPngFile,                              // NEW
    GerberFilesPicked(Option<Vec<PathBuf>>),
    ZipArchivePicked(Option<PathBuf>),
    PngFilePicked(Option<PathBuf>),             // NEW

    // Stage 1
    RunGerberToPng,                             // replaces RunPipeline
    PngReady(Result<PngRenderResult, String>),  // replaces PipelineFinished

    // Stage 2
    RunPngToGcode,                              // NEW
    GcodeReady(Result<GcodeResult, String>),    // NEW

    // Outputs
    SavePng,                                    // NEW
    PngSavePathPicked(Option<PathBuf>),         // NEW
    SaveGcode,
    GcodeSavePathPicked(Option<PathBuf>),

    // Animation
    Tick,                                       // NEW

    // Misc (unchanged)
    Reset,
    DpiChanged(String),
    CutZChanged(String),
    SafeZChanged(String),
    FeedRateChanged(String),
    PlungeRateChanged(String),
    SpindleSpeedChanged(String),
    ToolDiameterChanged(String),
    OffsetNumberChanged(String),
    OffsetStepoverChanged(String),
}
```

**Removed:** `RunPipeline`, `PipelineFinished`.

---

## conversion.rs changes

### GcodeResult gains a `paths` field

```rust
pub struct GcodeResult {
    pub gcode: String,
    pub estimated_time_secs: f32,
    pub cut_distance_mm: f32,
    pub width_mm: f32,
    pub height_mm: f32,
    pub paths: Vec<Vec<[f32; 2]>>,   // NEW: (x,y) in mm, collected during generate_gcode
}
```

`generate_gcode` already iterates every `[x, y, z]` point per segment. Collecting `[x, y]` into a parallel `Vec` costs nothing extra and avoids a separate parsing step.

---

## UI Layout

No column count changes. Three-column layout is preserved.

### Left column — Input Source panel

```
[ Load Gerber set ▶ ]
[ Load .zip ]  [ Reset ]
─────────────────────────
[ Load PNG directly ▶ ]   ← NEW
─────────────────────────
Loaded assets
> file1.gbr
> file2.drl
```

When a PNG is loaded directly, the "Loaded assets" section shows the PNG filename with a `[PNG]` tag and `gerber_to_png` immediately flips to `Complete`.

### Right column — Pipeline panel

```
PIPELINE                                   [N% COMPLETE]

┌─ STAGE 01: GERBER → PNG ─────── [READY] ── [▶ Run] ─┐
│  Rasterization                                        │
│  ██████████████░░░░░  ← animated when Running        │
└───────────────────────────────────────────────────────┘

┌─ STAGE 02: PNG → GCODE ─────── [WAITING]             ┐
│  Toolpath generation              [▶ Run] ← gated     │
│  ░░░░░░░░░░░░░░░░░░░                                  │
└───────────────────────────────────────────────────────┘

┌─ VISUALIZATION ───────────────────────────────────────┐
│  · After stage 1: scaled PNG image                    │
│  · After stage 2: 2D toolpath canvas                  │
│    G1 cuts = accent-green lines                       │
│    G0 rapids = thin dim lines                         │
└───────────────────────────────────────────────────────┘

[ Save PNG ▶ ]   ← enabled when active_png is Some
[ Save G-code ▶ ] ← enabled when generated_gcode is Some
```

Stage 1 "▶ Run" button: enabled when `gerber_paths` non-empty and `gerber_to_png ≠ Running`.  
Stage 2 "▶ Run" button: enabled when `active_png` is `Some` and `png_to_gcode ≠ Running`.

---

## Toolpath visualization

A `ToolpathCanvas` struct holds `paths: Vec<Vec<[f32; 2]>>` and implements `canvas::Program<Message>`.

- Points are in mm. On draw, scale to canvas pixel bounds: `scale = canvas_w / board_w`.
- Each sub-vec is a segment. The first point of each segment is a G0 rapid (pen-up), remaining points are G1 cuts.
- G1 lines: 1.5px wide, accent green (`0.39, 0.89, 0.64`).
- G0 rapids: 0.5px, dim gray (`0.30, 0.35, 0.40`), or simply omitted — decide during implementation.
- Canvas height: 200px fixed, width fills the panel.
- No interaction in phase 1 (zoom/pan deferred).

---

## Progress animation

Subscription active when `gerber_to_png == Running || png_to_gcode == Running`:

```rust
fn subscription(&self) -> Subscription<Message> {
    if self.gerber_to_png == StepState::Running || self.png_to_gcode == StepState::Running {
        iced::time::every(Duration::from_millis(40)).map(|_| Message::Tick)
    } else {
        Subscription::none()
    }
}
```

`Tick` increments `state.tick` by `0.025`, wrapping at `1.0` (sawtooth).

In `step_card`, when `state == StepState::Running`, pass `tick` instead of `state.progress()` to `progress_bar`.

---

## async tasks

```rust
// Stage 1
async fn run_gerber_to_png(inputs: Vec<PathBuf>, png_path: PathBuf, settings: ConversionSettings)
    -> Result<PngRenderResult, String>
{
    tokio::task::spawn_blocking(move || gerber_inputs_to_png(&inputs, &png_path, settings))
        .await
        .map_err(|e| format!("thread panicked: {e}"))?
        .map_err(|e| e.to_string())
}

// Stage 2
async fn run_png_to_gcode(png_path: PathBuf, settings: ConversionSettings)
    -> Result<GcodeResult, String>
{
    tokio::task::spawn_blocking(move || png_to_gcode(&png_path, settings))
        .await
        .map_err(|e| format!("thread panicked: {e}"))?
        .map_err(|e| e.to_string())
}
```

`run_conversion_pipeline` is removed.

---

## Save PNG flow

```
SavePng
  → if active_png is None: warn, return
  → open FileDialog (filter: *.png, default name: "board.png")
  → PngSavePathPicked(Some(path)): fs::copy(active_png, path)
  → PngSavePathPicked(None): status = "Save canceled."
```

---

## Error handling

No change to error model. Errors are displayed in the status bar, and the failed stage reverts to `Ready` so the user can retry.

---

## Testing

Existing 6 conversion tests are unaffected (pure library functions). No new tests are required for UI code in this phase; the `spawn_blocking` fix is already verified by the passing test suite.

---

## Out of scope (phase 2)

- Settings validation (red borders on invalid fields)
- GCode stored as temp file instead of `String` in `AppState`
- Settings persistence (`~/.config/easymill.toml`)
- Cancel in-flight conversion
- Zoom/pan on toolpath canvas
