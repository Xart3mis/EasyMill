# EasyMill — Conventions

## Code Style
- No comments in code (as enforced by project conventions)
- Iced 0.14 widget pattern: `fn view(state: &AppState) -> Element<'_, Message>`
- UI uses `super::palette` / `super::styles` via `src/ui/mod.rs` (which does not yet exist)
- TokyoNight theme via `Theme::TokyoNight`

## Architecture
- `StepState` enum for pipeline stage state machine
- Progress reported via `Arc<AtomicU32>` (stores 0–1000) + 100ms poll loop
- Heavy pipeline work on `spawn_blocking` to avoid blocking GUI
- `rfd::FileDialog` for native file pickers
- `ConversionSettings` carries all pipeline parameters (DPI, feed rates, tool dia, offsets, etc.)

## Error Handling
- `ConversionError` enum: Io, Image, EmptyInput, NoRenderableGerber, GerberParseError, RenderTooLarge
- Implements `Display` + `std::error::Error`
- Pipeline functions return `Result<_, ConversionError>`
- UI catches errors and formats via `.to_string()`

## Pipeline
- `gerber_inputs_to_png()` → `PngLayerResults` (copper/drills/outline `PngRenderResult`)
- `png_to_gcode()` → `GcodeResult` (gcode string, paths, distances, estimates)
- Progress callback: `Option<Box<dyn Fn(f32) + Send>>`
