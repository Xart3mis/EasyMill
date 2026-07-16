# Gerber Parser Replacement: gerber_parser + lyon Tessellation + Triangle Rasterizer

## Summary

Replace the hand-rolled RS-274X parser and rasterizer in `conversion.rs` (~900 lines) with a new `src/gerber.rs` module that uses `gerber_parser` for parsing, `lyon` for tessellation, and a custom scanline triangle rasterizer for PNG output. Keeps the existing PNG→G-code pipeline unchanged.

## Motivation

The current `parse_gerber()` implementation handles only a subset of RS-274X: basic apertures (circle, rect, oval, polygon), linear interpolation, flash, and regions. It does not handle aperture macros, step-repeat blocks, circular interpolation (G02/G03), block apertures, or many other Gerber features. Using `gerber_parser` gives us full RS-274X compliance. Using `lyon` for tessellation and a triangle rasterizer for rendering replaces the error-prone hand-rolled Bresenham/stroke rasterizer with a principled fill-based approach.

## Architecture

```
Gerber text → gerber_parser::parse() → Vec<Command>
                                         ↓
                              CommandProcessor
                              (state machine: apertures, G36/G37
                               regions, G01/G02/G03, SR blocks,
                               AB blocks, macro expansion)
                                         ↓
                              Vec<Shape> { Circle, Line, Arc, Polygon }
                                         ↓
                              lyon FillTessellator / StrokeTessellator
                                         ↓
                              Vec<Triangle> (2D vertices + indices)
                                         ↓
                              scanline triangle rasterizer
                              (polarity per layer type)
                                         ↓
                              GrayImage → PNG
```

## Layer-specific rendering polarity

| Layer type  | Fill color | Background | Binary meaning              |
|-------------|-----------|------------|-----------------------------|
| Traces      | White     | Black      | Dark=cut, Light=keep        |
| Edge cuts   | White     | Black      | Board interior=keep, scrap=cut |
| Drills      | Black     | White      | Hole=cut, board=keep        |

All three produce images where dark pixels are "remove material" when fed through `png_to_gcode()`.

## New file: `src/gerber.rs`

### Public API

Two-step for flexibility (or a convenience wrapper combining both):

```rust
// Step 1: parse + process commands into shapes
pub fn parse_to_shapes(source: &str, units: Option<Unit>) -> Result<Vec<Shape>>;

// Step 2: tessellate + rasterize shapes to image
pub fn render_shapes(shapes: &[Shape], settings: &ConversionSettings, layer_type: LayerType) -> Result<GrayImage>;
```

### Internal modules

**`CommandProcessor`** — state machine that walks `Vec<Command>` and produces `Vec<Shape>`:
- Tracks: `current_pos: Point2<f64>`, `current_aperture`, `interpolation_mode`, `quadrant_mode`, `region_active`, `step_repeat_stack`, `ab_replay_stack`
- Handles: D01 (interpolate → Line or Arc Shape), D02 (move), D03 (flash → Circle/Rect/Polygon from aperture def), G36/G37 (region → Polygon Shape), SR (step-repeat iteration)
- Reads aperture definitions (standard + macro) and builds shape representations for flash operations

**`Shape`** enum:
```rust
enum Shape {
    Circle { center: Point2<f64>, diameter: f64 },
    Line { start: Point2<f64>, end: Point2<f64>, width: f64 },
    Arc { center: Point2<f64>, radius: f64, start_angle: f64, sweep_angle: f64, width: f64 },
    Polygon { points: Vec<Point2<f64>> },
}
```

**Tessellation** — converts each `Shape` to triangle meshes via lyon:
- `Circle` → approximate as regular N-gon (N=64), then `FillTessellator` for filled circles
- `Line` → compute rectangle corners, `FillTessellator` for the filled rectangle
- `Arc` → generate arc polyline, thicken to polygon, `FillTessellator`
- `Polygon` → `FillTessellator` directly

Using `lyon::tessellation::FillTessellator` with `FillOptions::default()` for all shapes.

**Triangle rasterizer** — scanline fill:
```rust
fn rasterize_triangles(
    triangles: &[Triangle],
    width: u32,
    height: u32,
    fill_color: u8,
    bg_color: u8,
) -> GrayImage;
```
- For each triangle: compute integer Y range, clip to [0, height)
- For each scanline Y: edge-walk to find left/right X intersections, clip to [0, width), fill span with `fill_color`
- Top-left fill convention to avoid double-fill on shared edges

**`LayerType`** enum:
```rust
pub enum LayerType {
    Traces,
    Profile,
    Drill,
}
```
Controls polarity (fill/bg colors).

## Modified files

### `src/conversion.rs`

**Remove** (~900 lines):
- `parse_gerber()` function and all its helpers
- `parse_format_spec()`, `parse_coordinate()`, `parse_number()`
- `parse_aperture()`, `parse_standard_aperture()`, `parse_macro_aperture()`
- `parse_operation()`, `parse_excellon()`
- `Primitive` enum, `ApertureShape` enum, `Point` struct
- `render_primitives_to_png()` and `draw_thick_line()`, `fill_polygon()`
- `read_zip_gerbers()` (stays in conversion.rs — file I/O concern)

**Keep** (~1100 lines):
- `ConversionSettings` struct
- `png_to_gcode()` and all helpers (threshold, EDT, edge detection, vectorization, G-code gen)
- `GcodeResult` struct
- `gerber_inputs_to_png()` — update to delegate to `gerber::parse_gerber_to_image()`
- Existing unit tests — update to use new API

### `src/lib.rs`

Add `pub mod gerber;`

### `Cargo.toml`

Add:
```toml
gerber_parser = "0.5"
gerber_viewer = { version = "0.7", default-features = false, features = ["types"] }
lyon = "1.0"
nalgebra = "0.34"
```

Remove: nothing (keep `image`, `zip`, `iced`, `tokio`, `rfd`, `tracing`).

## Unit handling

`gerber_parser` returns coordinates in the file's units (mm or in, per `MOIN`/`MOMM`). `GerberDoc.units` gives the unit. The `CommandProcessor` converts all geometry to mm before creating `Shape` values.

## Tests

Update existing tests in `conversion.rs`:
- `gerber_stroke_renders_to_png` — render a simple trace, verify valid PNG output
- `default_gerber_render_uses_1000_dpi` — verify default resolution
- `zipped_gerber_package_renders_to_png` — verify .zip input works
- `oversized_gerber_render_fails_before_allocation` — verify size limit
- `real_gerber_zip_renders_to_expected_size` — verify real gerber output

New test in `gerber.rs`:
- `triangle_rasterizer_fills_correctly` — verify a single triangle renders to expected pixels
- `shape_tessellation_produces_non_empty_mesh` — verify lyon tessellation of each Shape variant

## Out of scope

- Excellon drill parsing: kept as-is (separate `parse_excellon` path in `gerber_inputs_to_png`)
- PNG→G-code pipeline: untouched
- iced GUI: untouched
