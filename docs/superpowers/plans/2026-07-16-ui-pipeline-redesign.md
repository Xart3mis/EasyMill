# EasyMill UI Pipeline Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the single-button coupled pipeline with two independent stage buttons, support direct PNG upload (skipping Gerber rasterization), show a 2D toolpath canvas after stage 2, and add Save PNG alongside Save GCode.

**Architecture:** `AppState.active_png: Option<PathBuf>` is the handoff point between stages — it can be populated by stage 1 (Gerber→PNG) or by a direct user file pick. Each stage fires its own `Task::perform` backed by `spawn_blocking`. A `Tick` subscription (40 ms) drives a sawtooth animation on the running progress bar. Toolpath paths are collected during G-code generation (same scale calculation, zero extra cost) and stored as `Vec<Vec<[f32;2]>>` for a `canvas::Program` renderer.

**Tech Stack:** Rust edition 2024, iced 0.14 (features: image, canvas), tokio 1 (already added), rfd 0.15, tracing 0.1

## Global Constraints
- All blocking CPU work must be inside `tokio::task::spawn_blocking`
- iced functional API: `iced::application(state, update, view).theme(theme).title(title).subscription(sub).run()`
- `cargo test` must stay green after every task
- No new crate dependencies — only add `canvas` feature to iced in Cargo.toml

---

### Task 1: Add 2D path data to `GcodeResult`

**Files:**
- Modify: `src/conversion.rs`

**Interfaces:**
- Produces: `GcodeResult.paths: Vec<Vec<[f32; 2]>>` — (x, y) mm coordinates, one inner `Vec` per toolpath segment

- [ ] **Step 1: Add `paths` field to `GcodeResult`**

`GcodeResult` is at line ~205 in `src/conversion.rs`. Add the field:

```rust
#[derive(Debug, Clone)]
pub struct GcodeResult {
    pub gcode: String,
    pub estimated_time_secs: f32,
    pub cut_distance_mm: f32,
    pub width_mm: f32,
    pub height_mm: f32,
    pub paths: Vec<Vec<[f32; 2]>>,   // ← add this line
}
```

- [ ] **Step 2: Collect 2D paths inside `png_to_gcode`**

`png_to_gcode` is at line ~214. After the line `let gcode_stats = generate_gcode(...)` and before `let result = GcodeResult { ... }`, insert:

```rust
// Collect 2D toolpath coords — same scale factor that generate_gcode uses.
let nx_f = image.width() as f32;
let path_scale = nx_f / (settings.pixels_per_mm * (nx_f - 1.0));
let paths_2d: Vec<Vec<[f32; 2]>> = path_with_depth
    .iter()
    .map(|seg| seg.iter().map(|pt| [pt[0] * path_scale, pt[1] * path_scale]).collect())
    .collect();
```

Then update the `GcodeResult` construction to include `paths: paths_2d`:

```rust
let result = GcodeResult {
    gcode: gcode_stats.gcode,
    estimated_time_secs: gcode_stats.estimated_time_secs,
    cut_distance_mm: gcode_stats.cut_distance_mm,
    width_mm,
    height_mm,
    paths: paths_2d,   // ← add this
};
```

- [ ] **Step 3: Add a test for the new field**

Inside the `tests` module in `src/conversion.rs`, add after the existing tests:

```rust
#[test]
fn gcode_result_includes_toolpaths() {
    let dir = tempfile::tempdir().unwrap();
    let png_path = dir.path().join("board.png");
    let test_zip = std::path::Path::new("test_files/inputs/gerber.zip");
    let settings = ConversionSettings::default();

    let png = gerber_inputs_to_png(
        &[test_zip.to_path_buf()],
        &png_path,
        settings.clone(),
    )
    .unwrap();

    let result = png_to_gcode(&png.path, settings).unwrap();

    assert!(!result.paths.is_empty(), "toolpaths must not be empty");
    assert!(
        result.paths.iter().all(|seg| !seg.is_empty()),
        "no segment may be empty"
    );
    // Coordinates are in mm — all x values should be positive
    let max_x = result
        .paths
        .iter()
        .flat_map(|seg| seg.iter())
        .map(|pt| pt[0])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(max_x > 0.0, "toolpath x coords must be positive mm values");
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test
```

Expected output (last lines):
```
test conversion::tests::gcode_result_includes_toolpaths ... ok
test result: ok. 7 passed; 0 failed; 0 ignored
```

- [ ] **Step 5: Commit**

```bash
git add src/conversion.rs
git commit -m "feat(conversion): add 2D toolpath coords to GcodeResult"
```

---

### Task 2: Add `canvas` iced feature

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Enable the canvas feature**

```toml
iced = { version = "0.14.0", features = ["image", "canvas"] }
```

- [ ] **Step 2: Verify it builds**

```bash
cargo check
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: enable iced canvas feature"
```

---

### Task 3: Restructure `AppState`, `Message`, and `update()`

This task touches all three together because they are tightly coupled — changing `Message` breaks `update()`, and changing `AppState` fields breaks `Default` and `update()`. The view function is minimally patched here to keep the code compiling; the full view redesign happens in Tasks 4 and 5.

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `GcodeResult.paths` from Task 1
- Produces (for Tasks 4–5):
  - `AppState.gerber_paths: Vec<PathBuf>`
  - `AppState.gerber_labels: Vec<String>`
  - `AppState.active_png: Option<PathBuf>`
  - `AppState.png_source_label: Option<String>`
  - `AppState.generated_gcode: Option<String>`
  - `AppState.toolpaths: Vec<Vec<[f32; 2]>>`
  - `AppState.tick: f32`
  - `Message::RunGerberToPng`, `Message::PngReady(Result<PngRenderResult, String>)`
  - `Message::RunPngToGcode`, `Message::GcodeReady(Result<GcodeResult, String>)`
  - `Message::SelectPngFile`, `Message::PngFilePicked(Option<PathBuf>)`
  - `Message::SavePng`, `Message::PngSavePathPicked(Option<PathBuf>)`
  - `Message::Tick`

- [ ] **Step 1: Replace `AppState` struct**

Replace the struct body at lines ~44–63 (keep the `struct AppState {` line, replace everything between the braces):

```rust
struct AppState {
    // Stage 1 inputs (Gerber files or ZIP)
    gerber_paths: Vec<PathBuf>,
    gerber_labels: Vec<String>,

    // Active PNG — from stage 1 conversion OR direct upload
    active_png: Option<PathBuf>,
    png_source_label: Option<String>,

    // Stage states
    gerber_to_png: StepState,
    png_to_gcode: StepState,

    // Stage 2 result
    generated_gcode: Option<String>,
    toolpaths: Vec<Vec<[f32; 2]>>,

    // Progress bar animation (0.0–1.0 sawtooth, driven by Tick)
    tick: f32,

    // Settings (unchanged names)
    dpi_input: String,
    cut_z_mm_input: String,
    safe_z_mm_input: String,
    feed_rate_input: String,
    plunge_rate_input: String,
    spindle_speed_input: String,
    tool_diameter_mm_input: String,
    offset_number_input: String,
    offset_stepover_input: String,

    // Job stats display
    estimated_time: String,
    cut_distance: String,
    board_dimensions: String,
    status: String,
}
```

- [ ] **Step 2: Replace `impl Default for AppState`**

Replace the entire `impl Default for AppState` block:

```rust
impl Default for AppState {
    fn default() -> Self {
        Self {
            gerber_paths: Vec::new(),
            gerber_labels: Vec::new(),
            active_png: None,
            png_source_label: None,
            gerber_to_png: StepState::default(),
            png_to_gcode: StepState::default(),
            generated_gcode: None,
            toolpaths: Vec::new(),
            tick: 0.0,
            dpi_input: format!("{DEFAULT_DPI:.0}"),
            cut_z_mm_input: "-0.1".to_owned(),
            safe_z_mm_input: "2.0".to_owned(),
            feed_rate_input: "300.0".to_owned(),
            plunge_rate_input: "120.0".to_owned(),
            spindle_speed_input: "12000".to_owned(),
            tool_diameter_mm_input: "0.4".to_owned(),
            offset_number_input: "4".to_owned(),
            offset_stepover_input: "0.5".to_owned(),
            estimated_time: "--:--:--".to_owned(),
            cut_distance: "0.0 mm".to_owned(),
            board_dimensions: "0.0 x 0.0 mm".to_owned(),
            status: String::new(),
        }
    }
}
```

- [ ] **Step 3: Replace `Message` enum**

Replace the entire `Message` enum (lines ~109–129):

```rust
#[derive(Debug, Clone)]
enum Message {
    // Gerber file selection
    SelectGerberFiles,
    SelectZipArchive,
    GerberFilesPicked(Option<Vec<PathBuf>>),
    ZipArchivePicked(Option<PathBuf>),

    // Direct PNG upload (skips stage 1)
    SelectPngFile,
    PngFilePicked(Option<PathBuf>),

    // Stage 1: Gerber → PNG
    RunGerberToPng,
    PngReady(Result<PngRenderResult, String>),

    // Stage 2: PNG → GCode
    RunPngToGcode,
    GcodeReady(Result<GcodeResult, String>),

    // Outputs
    SavePng,
    PngSavePathPicked(Option<PathBuf>),
    SaveGcode,
    GcodeSavePathPicked(Option<PathBuf>),

    // Animation
    Tick,

    // Misc
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

- [ ] **Step 4: Delete `PipelineOutput` struct**

Delete the `PipelineOutput` struct entirely (was ~lines 131–135).

- [ ] **Step 5: Replace `update()`**

Replace the entire `update` function body with:

```rust
fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::SelectGerberFiles => {
            info!("opening gerber file picker");
            state.status = "Waiting for Gerber file selection...".to_owned();
            Task::perform(
                async {
                    rfd::FileDialog::new()
                        .add_filter("Gerber", &["gbr", "grb", "gtl", "gbl", "gko", "gto", "gbo"])
                        .add_filter("Drill", &["drl"])
                        .set_title("Select Gerber and drill files")
                        .pick_files()
                },
                Message::GerberFilesPicked,
            )
        }

        Message::SelectZipArchive => {
            info!("opening zip archive picker");
            state.status = "Waiting for archive selection...".to_owned();
            Task::perform(
                async {
                    rfd::FileDialog::new()
                        .add_filter("ZIP archive", &["zip"])
                        .set_title("Select zipped Gerber package")
                        .pick_file()
                },
                Message::ZipArchivePicked,
            )
        }

        Message::SelectPngFile => {
            info!("opening png file picker");
            state.status = "Waiting for PNG file selection...".to_owned();
            Task::perform(
                async {
                    rfd::FileDialog::new()
                        .add_filter("PNG image", &["png"])
                        .set_title("Select PNG board image")
                        .pick_file()
                },
                Message::PngFilePicked,
            )
        }

        Message::GerberFilesPicked(files) => {
            if let Some(files) = files {
                if files.is_empty() {
                    warn!("gerber file picker returned empty selection");
                    state.status = "No files selected.".to_owned();
                    return Task::none();
                }
                state.gerber_paths = files;
                state.gerber_labels = state.gerber_paths.iter().cloned().map(path_to_label).collect();
                state.gerber_to_png = StepState::Ready;
                state.active_png = None;
                state.png_source_label = None;
                state.png_to_gcode = StepState::Idle;
                state.generated_gcode = None;
                state.toolpaths = Vec::new();
                info!(count = state.gerber_paths.len(), "selected gerber files");
                state.status = "Gerber inputs selected. Run Stage 01 to rasterize.".to_owned();
            } else {
                state.status = "Gerber selection canceled.".to_owned();
            }
            Task::none()
        }

        Message::ZipArchivePicked(file) => {
            if let Some(file) = file {
                let label = path_to_label(file.clone());
                state.gerber_paths = vec![file];
                state.gerber_labels = vec![label];
                state.gerber_to_png = StepState::Ready;
                state.active_png = None;
                state.png_source_label = None;
                state.png_to_gcode = StepState::Idle;
                state.generated_gcode = None;
                state.toolpaths = Vec::new();
                info!(path = %state.gerber_paths[0].display(), "selected gerber zip");
                state.status = "Archive loaded. Run Stage 01 to rasterize.".to_owned();
            } else {
                state.status = "Archive selection canceled.".to_owned();
            }
            Task::none()
        }

        Message::PngFilePicked(file) => {
            if let Some(file) = file {
                let label = path_to_label(file.clone());
                state.active_png = Some(file);
                state.png_source_label = Some(format!("uploaded: {label}"));
                // Treat uploaded PNG as stage 1 complete so stage 2 becomes available
                state.gerber_to_png = StepState::Complete;
                state.png_to_gcode = StepState::Ready;
                state.generated_gcode = None;
                state.toolpaths = Vec::new();
                info!(label = %state.png_source_label.as_deref().unwrap_or(""), "loaded png directly");
                state.status = "PNG loaded. Run Stage 02 to generate G-code.".to_owned();
            } else {
                state.status = "PNG selection canceled.".to_owned();
            }
            Task::none()
        }

        Message::RunGerberToPng => {
            if state.gerber_paths.is_empty() {
                warn!("stage 1 requested without gerber inputs");
                state.status = "Select Gerber files or a .zip archive first.".to_owned();
                return Task::none();
            }
            state.gerber_to_png = StepState::Running;
            state.active_png = None;
            state.png_source_label = None;
            state.png_to_gcode = StepState::Idle;
            state.generated_gcode = None;
            state.toolpaths = Vec::new();
            state.status = "Stage 01: Rasterizing Gerber files...".to_owned();

            let inputs = state.gerber_paths.clone();
            let settings = state.get_settings();
            let png_path = temporary_png_path();
            info!(input_count = inputs.len(), "starting stage 1");
            Task::perform(run_gerber_to_png(inputs, png_path, settings), Message::PngReady)
        }

        Message::PngReady(result) => {
            match result {
                Ok(png) => {
                    let label = path_to_label(png.path.clone());
                    state.gerber_to_png = StepState::Complete;
                    state.png_to_gcode = StepState::Ready;
                    state.active_png = Some(png.path.clone());
                    state.png_source_label = Some(format!("generated: {label}"));
                    info!(path = %png.path.display(), dark_pixels = png.dark_pixels, "stage 1 complete");
                    state.status = format!(
                        "Stage 01 complete — {} dark pixels. Run Stage 02 to generate G-code.",
                        png.dark_pixels
                    );
                }
                Err(err) => {
                    state.gerber_to_png = StepState::Ready;
                    error!(error = %err, "stage 1 failed");
                    state.status = format!("Stage 01 failed: {err}");
                }
            }
            Task::none()
        }

        Message::RunPngToGcode => {
            let Some(png_path) = state.active_png.clone() else {
                warn!("stage 2 requested without active png");
                state.status = "Run Stage 01 or load a PNG first.".to_owned();
                return Task::none();
            };
            state.png_to_gcode = StepState::Running;
            state.generated_gcode = None;
            state.toolpaths = Vec::new();
            state.status = "Stage 02: Generating G-code...".to_owned();

            let settings = state.get_settings();
            info!(png = %png_path.display(), "starting stage 2");
            Task::perform(run_png_to_gcode(png_path, settings), Message::GcodeReady)
        }

        Message::GcodeReady(result) => {
            match result {
                Ok(gcode) => {
                    let h = (gcode.estimated_time_secs / 3600.0) as i32;
                    let m = ((gcode.estimated_time_secs % 3600.0) / 60.0) as i32;
                    let s = (gcode.estimated_time_secs % 60.0) as i32;
                    state.estimated_time = format!("{h:02}:{m:02}:{s:02}");
                    state.cut_distance = format!("{:.1} mm", gcode.cut_distance_mm);
                    state.board_dimensions = format!("{:.1} x {:.1} mm", gcode.width_mm, gcode.height_mm);
                    state.png_to_gcode = StepState::Complete;
                    state.toolpaths = gcode.paths.clone();
                    state.generated_gcode = Some(gcode.gcode);
                    info!(
                        time = %state.estimated_time,
                        dist = %state.cut_distance,
                        "stage 2 complete"
                    );
                    state.status = format!(
                        "Stage 02 complete. Est. time: {}. Cut distance: {}.",
                        state.estimated_time, state.cut_distance
                    );
                }
                Err(err) => {
                    state.png_to_gcode = StepState::Ready;
                    state.toolpaths = Vec::new();
                    error!(error = %err, "stage 2 failed");
                    state.status = format!("Stage 02 failed: {err}");
                }
            }
            Task::none()
        }

        Message::SavePng => {
            if state.active_png.is_none() {
                warn!("save png requested without active png");
                state.status = "No PNG available to save.".to_owned();
                return Task::none();
            }
            info!("opening png save dialog");
            Task::perform(
                async {
                    rfd::FileDialog::new()
                        .add_filter("PNG image", &["png"])
                        .set_file_name("board.png")
                        .set_title("Save board PNG")
                        .save_file()
                },
                Message::PngSavePathPicked,
            )
        }

        Message::PngSavePathPicked(path) => {
            if let Some(dest) = path {
                if let Some(src) = &state.active_png {
                    match fs::copy(src, &dest) {
                        Ok(_) => {
                            info!(path = %dest.display(), "saved png");
                            state.status = format!("PNG saved to {}.", path_to_label(dest));
                        }
                        Err(err) => {
                            error!(error = %err, "failed to save png");
                            state.status = format!("Failed to save PNG: {err}");
                        }
                    }
                }
            } else {
                state.status = "PNG save canceled.".to_owned();
            }
            Task::none()
        }

        Message::SaveGcode => {
            if state.generated_gcode.is_none() {
                warn!("save gcode requested before generation");
                state.status = "Run Stage 02 before saving G-code.".to_owned();
                return Task::none();
            }
            info!("opening gcode save dialog");
            Task::perform(
                async {
                    rfd::FileDialog::new()
                        .add_filter("G-code", &["gcode", "nc", "tap"])
                        .set_file_name("output.gcode")
                        .set_title("Save generated G-code")
                        .save_file()
                },
                Message::GcodeSavePathPicked,
            )
        }

        Message::GcodeSavePathPicked(path) => {
            if let Some(path) = path {
                if let Some(gcode) = &state.generated_gcode {
                    match fs::write(&path, gcode) {
                        Ok(()) => {
                            info!(path = %path.display(), bytes = gcode.len(), "saved gcode");
                            state.status = format!("G-code saved to {}.", path_to_label(path));
                        }
                        Err(err) => {
                            error!(path = %path.display(), error = %err, "failed to save gcode");
                            state.status = format!("Failed to save G-code: {err}");
                        }
                    }
                }
            } else {
                state.status = "Save canceled.".to_owned();
            }
            Task::none()
        }

        Message::Tick => {
            state.tick = (state.tick + 0.025) % 1.0;
            Task::none()
        }

        Message::Reset => {
            info!("resetting application state");
            *state = AppState::default();
            Task::none()
        }

        Message::DpiChanged(v) => { state.dpi_input = v; Task::none() }
        Message::CutZChanged(v) => { state.cut_z_mm_input = v; Task::none() }
        Message::SafeZChanged(v) => { state.safe_z_mm_input = v; Task::none() }
        Message::FeedRateChanged(v) => { state.feed_rate_input = v; Task::none() }
        Message::PlungeRateChanged(v) => { state.plunge_rate_input = v; Task::none() }
        Message::SpindleSpeedChanged(v) => { state.spindle_speed_input = v; Task::none() }
        Message::ToolDiameterChanged(v) => { state.tool_diameter_mm_input = v; Task::none() }
        Message::OffsetNumberChanged(v) => { state.offset_number_input = v; Task::none() }
        Message::OffsetStepoverChanged(v) => { state.offset_stepover_input = v; Task::none() }
    }
}
```

- [ ] **Step 6: Replace the two pipeline async functions**

Delete the old `run_conversion_pipeline` function. In its place (just before `temporary_png_path`), add:

```rust
async fn run_gerber_to_png(
    inputs: Vec<PathBuf>,
    png_path: PathBuf,
    settings: ConversionSettings,
) -> Result<PngRenderResult, String> {
    tokio::task::spawn_blocking(move || gerber_inputs_to_png(&inputs, &png_path, settings))
        .await
        .map_err(|e| format!("PNG render thread panicked: {e}"))?
        .map_err(|e| e.to_string())
}

async fn run_png_to_gcode(
    png_path: PathBuf,
    settings: ConversionSettings,
) -> Result<GcodeResult, String> {
    tokio::task::spawn_blocking(move || png_to_gcode(&png_path, settings))
        .await
        .map_err(|e| format!("G-code thread panicked: {e}"))?
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 7: Fix minimal view() field references to compile**

The view function still references old field names. Apply these targeted fixes (do not redesign view yet — that's Task 4):

In `view()`, find the selected items list that uses `state.selected_inputs` and replace with `state.gerber_labels`:

```rust
// Find this pattern:
state.selected_inputs.iter()

// Replace with:
state.gerber_labels.iter()
```

In `generated_output()`, it references `state.generated_png: Option<String>`. Replace with `state.active_png: Option<PathBuf>`. The full function becomes:

```rust
fn generated_output<'a>(state: &'a AppState) -> Element<'a, Message> {
    let header = text("OUTPUT PREVIEW")
        .size(11)
        .font(Font::MONOSPACE)
        .color(Color::from_rgb(0.47, 0.53, 0.60));

    if let Some(png_path) = &state.active_png {
        let png_label = path_to_label(png_path.clone());
        let img_widget = image(iced::widget::image::Handle::from_path(png_path))
            .width(Length::Fill)
            .height(Length::Fixed(180.0));

        container(
            column![
                header,
                text(format!("Render: {png_label}"))
                    .size(12)
                    .font(Font::MONOSPACE)
                    .color(Color::from_rgb(0.75, 0.81, 0.85)),
                container(img_widget)
                    .width(Length::Fill)
                    .height(Length::Fixed(180.0))
                    .style(inset_style()),
            ]
            .spacing(8)
        )
        .width(Length::Fill)
        .padding(14)
        .style(inset_style())
        .into()
    } else {
        container(
            column![
                header,
                text("No preview available. Run Stage 01 or load a PNG.")
                    .size(13)
                    .font(Font::MONOSPACE)
                    .color(Color::from_rgb(0.48, 0.55, 0.60)),
            ]
            .spacing(8)
        )
        .width(Length::Fill)
        .padding(14)
        .style(inset_style())
        .into()
    }
}
```

- [ ] **Step 8: Verify compilation**

```bash
cargo check
```

Expected: no errors (dead code / unused variable warnings are fine).

- [ ] **Step 9: Verify tests still pass**

```bash
cargo test
```

Expected: all tests pass (7 tests).

- [ ] **Step 10: Commit**

```bash
git add src/main.rs
git commit -m "feat: split pipeline into independent stage 1 + stage 2 tasks"
```

---

### Task 4: Tick subscription and animated progress bar

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `AppState.tick`, `AppState.gerber_to_png`, `AppState.png_to_gcode`
- Consumes: `Message::Tick`

- [ ] **Step 1: Add `time` import**

In the `use iced::{...}` import block at the top of `src/main.rs`, add `time` to the iced imports:

```rust
use iced::{
    self, Alignment, Background, Color, Element, Font, Length, Shadow, Task, Theme, Vector,
    border, time,
    widget::{button, column, container, progress_bar, row, text, text_input, image},
};
use std::time::Duration;
```

- [ ] **Step 2: Add the `subscription` function**

Add this function just before `fn main()`:

```rust
fn subscription(state: &AppState) -> iced::Subscription<Message> {
    if state.gerber_to_png == StepState::Running || state.png_to_gcode == StepState::Running {
        time::every(Duration::from_millis(40)).map(|_| Message::Tick)
    } else {
        iced::Subscription::none()
    }
}
```

- [ ] **Step 3: Wire the subscription into `main()`**

Replace the `main` function body:

```rust
pub fn main() -> iced::Result {
    if let Err(err) = init_logging() {
        eprintln!("failed to initialize logging: {err}");
    }
    info!("starting easymill ui");

    iced::application(AppState::default, update, view)
        .theme(theme)
        .title("EasyMill")
        .subscription(subscription)
        .run()
}
```

- [ ] **Step 4: Update `step_card` to accept `tick`**

Change the `step_card` signature to accept a `tick: f32` parameter and use it when `Running`:

```rust
fn step_card<'a>(
    stage: &'a str,
    title: &'a str,
    subtitle: &'a str,
    state: StepState,
    accent: Color,
    tick: f32,
) -> Element<'a, Message> {
    let status_pill = container(
        text(state.label())
            .size(11)
            .font(Font::MONOSPACE)
            .color(Color::from_rgb(0.86, 0.90, 0.92)),
    )
    .padding([5, 8])
    .style(move |_| {
        container::Style::default()
            .background(Background::Color(Color { a: 0.20, ..accent }))
            .border(
                border::rounded(8.0)
                    .width(1.0)
                    .color(Color { a: 0.46, ..accent }),
            )
    });

    // Use animated sawtooth while Running, otherwise fixed progress value
    let bar_value = if state == StepState::Running { tick } else { state.progress() };

    container(
        column![
            row![
                text(stage)
                    .size(11)
                    .font(Font::MONOSPACE)
                    .color(Color::from_rgb(0.47, 0.53, 0.60)),
                container("").width(Length::Fill),
                status_pill,
            ]
            .width(Length::Fill)
            .align_y(Alignment::Center),
            text(title)
                .size(21)
                .font(Font::MONOSPACE)
                .color(Color::from_rgb(0.90, 0.92, 0.93)),
            text(subtitle)
                .size(14)
                .color(Color::from_rgb(0.58, 0.64, 0.70)),
            progress_bar(0.0..=1.0, bar_value),
        ]
        .spacing(10),
    )
    .padding(14)
    .style(inset_style())
    .into()
}
```

- [ ] **Step 5: Fix the two `step_card` call sites in `view()`**

Each call to `step_card` now needs a `tick` argument. Find and update both:

```rust
// Stage 01 call — was:
step_card(
    "STAGE 01",
    "GERBER -> PNG",
    "Rasterization",
    state.gerber_to_png,
    Color::from_rgb(0.39, 0.89, 0.64),
),
// becomes:
step_card(
    "STAGE 01",
    "GERBER -> PNG",
    "Rasterization",
    state.gerber_to_png,
    Color::from_rgb(0.39, 0.89, 0.64),
    state.tick,
),

// Stage 02 call — was:
step_card(
    "STAGE 02",
    "PNG -> GCODE",
    "Toolpath generation",
    state.png_to_gcode,
    Color::from_rgb(0.91, 0.74, 0.38),
),
// becomes:
step_card(
    "STAGE 02",
    "PNG -> GCODE",
    "Toolpath generation",
    state.png_to_gcode,
    Color::from_rgb(0.91, 0.74, 0.38),
    state.tick,
),
```

- [ ] **Step 6: Verify compilation**

```bash
cargo check
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add src/main.rs
git commit -m "feat: animate progress bar while stage is running via Tick subscription"
```

---

### Task 5: Redesign the pipeline panel — per-stage run buttons and "Load PNG" input

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `AppState.gerber_paths`, `AppState.active_png`, `AppState.gerber_to_png`, `AppState.png_to_gcode`
- Consumes: `Message::RunGerberToPng`, `Message::RunPngToGcode`, `Message::SelectPngFile`

- [ ] **Step 1: Add `run_stage_button` helper**

Add this small helper just before `step_card`:

```rust
fn run_stage_button<'a>(label: &'a str, action: Option<Message>) -> Element<'a, Message> {
    let btn = button(
        text(label)
            .size(12)
            .font(Font::MONOSPACE)
            .color(Color::from_rgb(0.12, 0.14, 0.16)),
    )
    .style(primary_action_style)
    .padding([6, 12]);

    if let Some(msg) = action {
        btn.on_press(msg).into()
    } else {
        btn.into()
    }
}
```

- [ ] **Step 2: Update `step_card` to embed a run button**

Replace the `step_card` function again to accept an `Option<Message>` run action:

```rust
fn step_card<'a>(
    stage: &'a str,
    title: &'a str,
    subtitle: &'a str,
    state: StepState,
    accent: Color,
    tick: f32,
    run_action: Option<Message>,
) -> Element<'a, Message> {
    let status_pill = container(
        text(state.label())
            .size(11)
            .font(Font::MONOSPACE)
            .color(Color::from_rgb(0.86, 0.90, 0.92)),
    )
    .padding([5, 8])
    .style(move |_| {
        container::Style::default()
            .background(Background::Color(Color { a: 0.20, ..accent }))
            .border(
                border::rounded(8.0)
                    .width(1.0)
                    .color(Color { a: 0.46, ..accent }),
            )
    });

    let bar_value = if state == StepState::Running { tick } else { state.progress() };

    // Header row: "STAGE 01" label, spacer, status pill, run button
    let run_btn = run_stage_button("▶ Run", run_action);
    let header_row = row![
        text(stage)
            .size(11)
            .font(Font::MONOSPACE)
            .color(Color::from_rgb(0.47, 0.53, 0.60)),
        container("").width(Length::Fill),
        status_pill,
        run_btn,
    ]
    .spacing(8)
    .width(Length::Fill)
    .align_y(Alignment::Center);

    container(
        column![
            header_row,
            text(title)
                .size(21)
                .font(Font::MONOSPACE)
                .color(Color::from_rgb(0.90, 0.92, 0.93)),
            text(subtitle)
                .size(14)
                .color(Color::from_rgb(0.58, 0.64, 0.70)),
            progress_bar(0.0..=1.0, bar_value),
        ]
        .spacing(10),
    )
    .padding(14)
    .style(inset_style())
    .into()
}
```

- [ ] **Step 3: Update both `step_card` call sites in `view()`**

Each call now needs the `run_action` argument. Replace both:

```rust
// Stage 01: enabled when gerber_paths non-empty and not currently running
let stage1_action = if !state.gerber_paths.is_empty()
    && state.gerber_to_png != StepState::Running
{
    Some(Message::RunGerberToPng)
} else {
    None
};

// Stage 02: enabled when a PNG is available and not currently running
let stage2_action = if state.active_png.is_some()
    && state.png_to_gcode != StepState::Running
{
    Some(Message::RunPngToGcode)
} else {
    None
};
```

Then pass them in the `step_card` calls:

```rust
step_card(
    "STAGE 01",
    "GERBER -> PNG",
    "Rasterization",
    state.gerber_to_png,
    Color::from_rgb(0.39, 0.89, 0.64),
    state.tick,
    stage1_action,
),
step_card(
    "STAGE 02",
    "PNG -> GCODE",
    "Toolpath generation",
    state.png_to_gcode,
    Color::from_rgb(0.91, 0.74, 0.38),
    state.tick,
    stage2_action,
),
```

- [ ] **Step 4: Remove the old "Run Pipeline" button from `view()`**

In the `pipeline_panel` column, delete:

```rust
button("Run Pipeline")
    .style(primary_action_style)
    .width(Length::Fill)
    .padding([12, 16])
    .on_press(Message::RunPipeline),
```

- [ ] **Step 5: Add "Load PNG directly" button to the source panel in `view()`**

In the `source_actions` column inside `view()`, add a third button after the `row![Load .zip, Reset]`:

```rust
let source_actions = column![
    button("Load Gerber set")
        .style(primary_action_style)
        .width(Length::Fill)
        .padding([11, 16])
        .on_press(Message::SelectGerberFiles),
    row![
        button("Load .zip")
            .style(secondary_action_style)
            .width(Length::Fill)
            .padding([10, 14])
            .on_press(Message::SelectZipArchive),
        button("Reset")
            .style(ghost_action_style)
            .width(Length::Fill)
            .padding([10, 14])
            .on_press(Message::Reset),
    ]
    .spacing(10),
    button("Load PNG directly")
        .style(secondary_action_style)
        .width(Length::Fill)
        .padding([10, 14])
        .on_press(Message::SelectPngFile),
]
.spacing(10);
```

- [ ] **Step 6: Show png_source_label in the loaded assets list**

In `view()`, after the `selected_items` block, show the active PNG source if set. Add a `png_label_row` element just above or below the `selected_items` container. Inside the `source_panel` column, add after the `selected_items` binding:

```rust
let png_status: Element<'_, Message> = if let Some(label) = &state.png_source_label {
    text(format!("[PNG] {label}"))
        .size(12)
        .font(Font::MONOSPACE)
        .color(Color::from_rgb(0.39, 0.89, 0.64))
        .into()
} else {
    text("")
        .size(12)
        .into()
};
```

Add `png_status` to the `source_panel` column below `selected_items`.

- [ ] **Step 7: Verify compilation**

```bash
cargo check
```

Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add src/main.rs
git commit -m "feat: per-stage run buttons and direct PNG upload in source panel"
```

---

### Task 6: Save PNG button

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `AppState.active_png`, `Message::SavePng`

- [ ] **Step 1: Add `save_png_button` helper**

Add this function alongside `save_gcode_button`:

```rust
fn save_png_button<'a>(state: &AppState) -> Element<'a, Message> {
    let btn = button("Save board PNG")
        .style(secondary_action_style)
        .width(Length::Fill)
        .padding([12, 16]);

    if state.active_png.is_some() {
        btn.on_press(Message::SavePng).into()
    } else {
        btn.into()
    }
}
```

- [ ] **Step 2: Add the button to the pipeline panel in `view()`**

In the `pipeline_panel` column, add `save_png_button(state)` directly above `save_gcode_button(state)`:

```rust
save_png_button(state),
save_gcode_button(state),
```

- [ ] **Step 3: Verify compilation**

```bash
cargo check
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: Save PNG button available after stage 1"
```

---

### Task 7: Toolpath canvas visualization

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `AppState.toolpaths: Vec<Vec<[f32; 2]>>`, `AppState.board_dimensions` (for width_mm/height_mm which we re-derive from `GcodeResult`)

Note: `GcodeResult.width_mm` and `GcodeResult.height_mm` are currently discarded after `GcodeReady`. Store them in `AppState` so the canvas knows board dimensions.

- [ ] **Step 1: Add board dimension fields to `AppState`**

In the `AppState` struct, after `toolpaths`, add:

```rust
board_width_mm: f32,
board_height_mm: f32,
```

In `impl Default for AppState`, add:

```rust
board_width_mm: 0.0,
board_height_mm: 0.0,
```

In `Message::GcodeReady(Ok(gcode))` in `update()`, after setting `state.toolpaths`, add:

```rust
state.board_width_mm = gcode.width_mm;
state.board_height_mm = gcode.height_mm;
```

In `Message::Reset`, these are already reset by `*state = AppState::default()`.

- [ ] **Step 2: Add canvas import**

Add to the `use iced::{...}` import block:

```rust
use iced::widget::canvas::{self, Canvas, Frame, Path, Stroke};
```

- [ ] **Step 3: Implement `ToolpathCanvas`**

Add this struct and its `canvas::Program` implementation just before the `view` function:

```rust
struct ToolpathCanvas {
    paths: Vec<Vec<[f32; 2]>>,
    width_mm: f32,
    height_mm: f32,
}

impl canvas::Program<Message> for ToolpathCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: iced::advanced::mouse::Cursor,
    ) -> Vec<canvas::Geometry<iced::Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());

        if self.width_mm <= 0.0 || self.height_mm <= 0.0 || self.paths.is_empty() {
            return vec![frame.into_geometry()];
        }

        let scale = (bounds.width / self.width_mm).min(bounds.height / self.height_mm);

        for segment in &self.paths {
            if segment.len() < 2 {
                continue;
            }
            let path = Path::new(|b| {
                let [x0, y0] = segment[0];
                // Flip Y: canvas Y increases downward, GCode Y increases upward
                b.move_to(iced::Point::new(x0 * scale, bounds.height - y0 * scale));
                for pt in &segment[1..] {
                    b.line_to(iced::Point::new(pt[0] * scale, bounds.height - pt[1] * scale));
                }
            });
            frame.stroke(
                &path,
                Stroke {
                    style: canvas::stroke::Style::Solid(Color::from_rgb(0.39, 0.89, 0.64)),
                    width: 1.5,
                    line_cap: canvas::LineCap::Round,
                    line_join: canvas::LineJoin::Round,
                    ..Stroke::default()
                },
            );
        }

        vec![frame.into_geometry()]
    }
}
```

- [ ] **Step 4: Replace `generated_output` to show canvas when GCode is available**

Replace the entire `generated_output` function:

```rust
fn generated_output<'a>(state: &'a AppState) -> Element<'a, Message> {
    let header = text("VISUALIZATION")
        .size(11)
        .font(Font::MONOSPACE)
        .color(Color::from_rgb(0.47, 0.53, 0.60));

    // Stage 2 complete → show toolpath canvas
    if !state.toolpaths.is_empty() {
        let canvas_widget = Canvas::new(ToolpathCanvas {
            paths: state.toolpaths.clone(),
            width_mm: state.board_width_mm,
            height_mm: state.board_height_mm,
        })
        .width(Length::Fill)
        .height(Length::Fixed(200.0));

        return container(
            column![
                header,
                text("Toolpath view (G1 cut moves)")
                    .size(12)
                    .font(Font::MONOSPACE)
                    .color(Color::from_rgb(0.75, 0.81, 0.85)),
                container(canvas_widget)
                    .width(Length::Fill)
                    .style(inset_style()),
            ]
            .spacing(8),
        )
        .width(Length::Fill)
        .padding(14)
        .style(inset_style())
        .into();
    }

    // Stage 1 complete → show PNG preview
    if let Some(png_path) = &state.active_png {
        let png_label = path_to_label(png_path.clone());
        let img_widget = image(iced::widget::image::Handle::from_path(png_path))
            .width(Length::Fill)
            .height(Length::Fixed(180.0));

        return container(
            column![
                header,
                text(format!("Raster preview: {png_label}"))
                    .size(12)
                    .font(Font::MONOSPACE)
                    .color(Color::from_rgb(0.75, 0.81, 0.85)),
                container(img_widget)
                    .width(Length::Fill)
                    .height(Length::Fixed(180.0))
                    .style(inset_style()),
            ]
            .spacing(8),
        )
        .width(Length::Fill)
        .padding(14)
        .style(inset_style())
        .into();
    }

    // Nothing available yet
    container(
        column![
            header,
            text("Run Stage 01 to see a raster preview, Stage 02 for toolpath visualization.")
                .size(13)
                .font(Font::MONOSPACE)
                .color(Color::from_rgb(0.48, 0.55, 0.60)),
        ]
        .spacing(8),
    )
    .width(Length::Fill)
    .padding(14)
    .style(inset_style())
    .into()
}
```

- [ ] **Step 5: Verify compilation**

```bash
cargo check
```

If the `canvas::Program` trait bounds don't match exactly due to iced 0.14 API specifics, adjust the signature to match what the compiler expects — the draw logic and types are stable, only trait parameters may vary.

- [ ] **Step 6: Full test run**

```bash
cargo test
```

Expected: all tests pass (7 tests).

- [ ] **Step 7: Commit**

```bash
git add src/main.rs
git commit -m "feat: toolpath canvas visualization and PNG preview after each stage"
```

---

## Self-Review

**Spec coverage check:**

| Spec requirement | Task |
|---|---|
| Fix hang (spawn_blocking) | ✅ Done before plan (committed) |
| Independent stage buttons | Task 5 |
| Direct PNG upload | Task 3 (update), Task 5 (view) |
| PNG preview after stage 1 | Task 7 (generated_output) |
| Toolpath canvas after stage 2 | Task 7 |
| Save PNG | Task 3 (update handler), Task 6 (button) |
| Save GCode | Task 3 (handler unchanged), Task 6 (button placement) |
| Animated progress bar | Task 4 |
| GcodeResult.paths | Task 1 |

**Placeholder scan:** No TBDs, TODOs, or "similar to Task N" shortcuts. All code is written out in full.

**Type consistency check:**
- `AppState.gerber_paths` defined Task 3 Step 1, consumed Task 5 Step 3 ✓
- `AppState.active_png: Option<PathBuf>` defined Task 3 Step 1, consumed Tasks 5, 6, 7 ✓
- `AppState.toolpaths: Vec<Vec<[f32; 2]>>` defined Task 3 Step 1, populated Task 3 Step 5 (`GcodeReady`), consumed Task 7 ✓
- `AppState.board_width_mm` / `board_height_mm` defined Task 7 Step 1, populated same step ✓
- `Message::PngReady(Result<PngRenderResult, String>)` defined Task 3 Step 3, dispatched Task 3 Step 6, handled Task 3 Step 5 ✓
- `Message::GcodeReady(Result<GcodeResult, String>)` defined Task 3 Step 3, dispatched Task 3 Step 6, handled Task 3 Step 5 ✓
- `step_card` signature: `(stage, title, subtitle, state, accent, tick, run_action)` — set Task 4 Step 4, call sites updated Task 4 Step 5, extended Task 5 Step 2, call sites updated Task 5 Step 3 ✓
- `GcodeResult.paths` added Task 1, consumed Task 3 Step 5 ✓
