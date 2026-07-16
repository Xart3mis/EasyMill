# Implementation Plan: Gerber Parser Replacement

## Overview

Create `src/gerber.rs` using `gerber_parser` + `lyon` + custom rasterizer. Wire into `conversion.rs`. Keep PNG→G-code pipeline untouched.

## Task 1: Cargo.toml + lib.rs

**Cargo.toml** — add 4 deps under `[dependencies]`:

```toml
gerber_parser = "0.5"
gerber_viewer = { version = "0.7", default-features = false, features = ["types"] }
lyon = "1.0"
nalgebra = "0.34"
```

**src/lib.rs** — add module:

```rust
pub mod conversion;
pub mod gerber;
pub mod logging;
```

## Task 2: gerber.rs Foundation Types

Define these at the top of `src/gerber.rs`:

```rust
use std::collections::HashMap;
use std::path::Path;

use gerber_parser;
use gerber_types;
use image::{GrayImage, Luma};
use lyon::lyon_tessellation as tess;
use nalgebra::{Point2, Vector2};

use crate::conversion::{ConversionError, ConversionSettings, PngRenderResult};
```

### Shapes and Triangles

```rust
#[derive(Debug, Clone)]
pub struct Triangle {
    pub v0: Point2<f64>,
    pub v1: Point2<f64>,
    pub v2: Point2<f64>,
}

/// A single shape processed from Gerber commands, stored as triangles in mm space.
#[derive(Debug, Clone)]
pub struct Shape {
    pub triangles: Vec<Triangle>,
}
```

### LayerType

```rust
pub enum LayerType {
    Copper,
    Profile,
    Drill,
}
```

### MacroExpander — evaluates `gerber_types::Expression` with variable substitution

```rust
struct MacroExpander;

impl MacroExpander {
    /// Evaluate an expression with variable values substituted.
    /// Expression uses infix notation: [Number(1), Add, Variable(3), Mul, Number(2)]
    fn eval(expr: &gerber_types::Expression, vars: &[f64]) -> f64 {
        let terms = &expr.0;
        // 1. Substitute variables
        let substituted: Vec<MacroToken> = terms.iter().map(|t| match t {
            gerber_types::ExpressionTerm::Number(n) => MacroToken::Value(*n),
            gerber_types::ExpressionTerm::Variable(n) => {
                let idx = *n as usize;
                MacroToken::Value(if idx >= 1 && idx <= vars.len() { vars[idx - 1] } else { 0.0 })
            }
            gerber_types::ExpressionTerm::Add => MacroToken::Op('+'),
            gerber_types::ExpressionTerm::Sub => MacroToken::Op('-'),
            gerber_types::ExpressionTerm::Mul => MacroToken::Op('*'),
            gerber_types::ExpressionTerm::Div => MacroToken::Op('/'),
            gerber_types::ExpressionTerm::OpenParen => MacroToken::Open,
            gerber_types::ExpressionTerm::CloseParen => MacroToken::Close,
        }).collect();

        // 2. Shunting-yard to RPN, then evaluate
        let rpn = shunting_yard(&substituted);
        eval_rpn(&rpn)
    }

    fn shunting_yard(tokens: &[MacroToken]) -> Vec<MacroToken> { ... }
    fn eval_rpn(rpn: &[MacroToken]) -> f64 { ... }
}

enum MacroToken {
    Value(f64),
    Op(char),
    Open,
    Close,
}
```

### Aperture geometry helpers

```rust
fn tessellate_aperture_circle(center: Point2<f64>, diameter: f64) -> Vec<Triangle> { ... }
fn tessellate_aperture_rect(center: Point2<f64>, w: f64, h: f64) -> Vec<Triangle> { ... }
fn tessellate_aperture_oval(center: Point2<f64>, w: f64, h: f64) -> Vec<Triangle> { ... }
fn tessellate_aperture_polygon(center: Point2<f64>, diameter: f64, vertices: u32, rotation: f64) -> Vec<Triangle> { ... }
fn tessellate_circle_ngon(center: Point2<f64>, radius: f64, n: u32) -> Vec<Triangle> { ... }
fn tessellate_polygon(points: &[Point2<f64>]) -> Vec<Triangle> { ... }
fn tessellate_thick_line(start: Point2<f64>, end: Point2<f64>, width: f64) -> Vec<Triangle> { ... }
```

### Arc computation

```rust
fn compute_arc_points(
    start: Point2<f64>,
    end: Point2<f64>,
    i: f64, j: f64,
    clockwise: bool,
    quadrant_mode: QuadrantMode,
) -> Vec<Point2<f64>> { ... }
```

## Task 3: CommandProcessor State Machine

```rust
enum InterpolationMode { Linear, Clockwise, Counterclockwise }
enum QuadrantMode { Single, Multi }

struct GerberState {
    int_digits: usize,
    dec_digits: usize,
    scale_mm: f64,
    current: Point2<f64>,
    interpolation: InterpolationMode,
    quadrant: QuadrantMode,
    apertures: HashMap<u32, ApertureDef>,
    macros: HashMap<String, gerber_types::ApertureMacro>,
    current_aperture: Option<u32>,
    in_region: bool,
    region_start: Point2<f64>,
    region_points: Vec<Point2<f64>>,
    shapes: Vec<Vec<Triangle>>,
    in_block: bool,
    block_number: Option<u32>,
    block_shapes: Vec<Vec<Triangle>>,
    sr_active: bool,
    sr_ijk: (u32, u32),
    sr_current: (u32, u32),
    sr_offset: Vector2<f64>,
}
```

Main process function:

```rust
fn process_commands(commands: &[gerber_types::Command]) -> Vec<Vec<Triangle>> {
    let mut state = GerberState::new();

    for cmd in commands {
        match cmd {
            Command::ExtendedCode(ext) => match ext {
                ExtendedCode::CoordinateFormat(fmt) => {
                    state.int_digits = fmt.integer_digits as usize;
                    state.dec_digits = fmt.decimal_digits as usize;
                }
                ExtendedCode::Unit(unit) => {
                    state.scale_mm = match unit { Units::Millimeters => 1.0, _ => 25.4 };
                }
                ExtendedCode::ApertureDefinition(def) => {
                    state.apertures.insert(def.code, def.aperture.clone());
                }
                ExtendedCode::ApertureMacro(mac) => {
                    state.macros.insert(mac.name.clone(), mac.content.clone());
                }
                ExtendedCode::StepAndRepeat(sr) => {
                    match sr { StepRepeat::Open(i, j, x, y) => { ... }, StepRepeat::Close => { ... } }
                }
                ExtendedCode::ApertureBlock(ab) => {
                    match ab { ApertureBlock::Open(code) => { ... }, ApertureBlock::Close => { ... } }
                }
                _ => {}
            },
            Command::FunctionCode(func) => match func {
                FunctionCode::DCode(dcode) => match dcode {
                    DCode::Select(code) => { state.current_aperture = Some(*code); }
                    DCode::Operation(op) => match op {
                        Operation::Move(coords) => {
                            if let Some(c) = coords { state.current = decode_coords(c, &state); }
                        }
                        Operation::Interpolate(coords, offsets) => {
                            let target = coords.map(|c| decode_coords(&c, &state)).unwrap_or(state.current);
                            let i_opt = offsets.as_ref().and_then(|o| o.i.map(|v| v as f64 * state.scale_mm / 10f64.powi(state.dec_digits as i32)));
                            let j_opt = offsets.as_ref().and_then(|o| o.j.map(|v| v as f64 * state.scale_mm / 10f64.powi(state.dec_digits as i32)));

                            if state.in_region {
                                state.region_points.push(target);
                            } else {
                                let tris = match state.interpolation {
                                    InterpolationMode::Linear => {
                                        line_to_triangles(&state, target)
                                    }
                                    InterpolationMode::Clockwise | InterpolationMode::Counterclockwise => {
                                        arc_to_triangles(&state, target, i_opt, j_opt)
                                    }
                                };
                                state.emit(tris);
                            }
                            state.current = target;
                        }
                        Operation::Flash(_) => {
                            let tris = flash_aperture(&state);
                            state.emit(tris);
                        }
                    }
                },
                FunctionCode::GCode(gcode) => match gcode {
                    GCode::InterpolationMode(m) => {
                        state.interpolation = match m {
                            InterpolationModeG::Linear => InterpolationMode::Linear,
                            InterpolationModeG::Clockwise => InterpolationMode::Clockwise,
                            InterpolationModeG::Counterclockwise => InterpolationMode::Counterclockwise,
                        };
                    }
                    GCode::QuadrantMode(q) => {
                        state.quadrant = match q {
                            QuadrantModeG::Single => QuadrantMode::Single,
                            QuadrantModeG::Multi => QuadrantMode::Multi,
                        };
                    }
                    GCode::RegionMode(true) => {
                        state.in_region = true;
                        state.region_points.clear();
                    }
                    GCode::RegionMode(false) => {
                        state.in_region = false;
                        if state.region_points.len() >= 3 {
                            state.emit(tessellate_polygon(&state.region_points));
                        }
                        state.region_points.clear();
                    }
                    _ => {}
                },
            },
            _ => {}
        }
    }

    state.flush();
    state.shapes
}
```

### Coordinate decoding

```rust
fn decode_coords(coords: &gerber_types::Coordinates, state: &GerberState) -> Point2<f64> {
    let scale = state.scale_mm / 10f64.powi(state.dec_digits as i32);
    Point2::new(
        coords.x.map(|v| v as f64 * scale).unwrap_or(state.current.x),
        coords.y.map(|v| v as f64 * scale).unwrap_or(state.current.y),
    )
}
```

### Aperture definition resolution

When a macro aperture is encountered, expand it:

```rust
fn resolve_aperture(aperture: &gerber_types::Aperture, macros: &HashMap<..., ...>, args: &[f64]) -> Vec<Vec<Point2<f64>>> {
    match aperture {
        Aperture::Circle(d) => { /* approximate as polygon */ }
        Aperture::Rectangle(w, h) => { /* 4 corners */ }
        Aperture::Oval(w, h) => { /* rectangle + semicircles */ }
        Aperture::Polygon(d, verts, rot) => { /* regular polygon */ }
        Aperture::Macro(name, expr_args) => {
            let mac = macros.get(name)?;
            let mut vars = expr_args.iter().map(|e| MacroExpander::eval(e, &[])).collect::<Vec<_>>();
            // Evaluate macro content
            evaluate_macro_primitives(&mac, &vars)
        }
    }
}
```

### Macro primitive evaluation

```rust
fn evaluate_macro_primitives(mac: &gerber_types::ApertureMacro, vars: &[f64]) -> Vec<Triangle> {
    for content in &mac.0 {
        match content {
            MacroContent::Circle { exposure, diameter, center, rotation } => { ... }
            MacroContent::VectorLine { exposure, width, start, end, rotation } => { ... }
            MacroContent::CenterLine { exposure, width, height, center, rotation } => { ... }
            MacroContent::Outline { exposure, points, rotation } => { ... }
            MacroContent::Polygon { exposure, vertices, center, diameter, rotation } => { ... }
            MacroContent::VariableDefinition { number, expression } => {
                // $number = eval(expression)
            }
            _ => {}
        }
    }
}
```

## Task 4: Lyon Tessellation Wrapper

All tessellation goes through lyon's `FillTessellator`:

```rust
use tess::{FillTessellator, FillOptions, math::Point as LPoint};

fn tessellate_lyon(vertices: &[Point2<f64>]) -> Vec<Triangle> {
    let mut tess = FillTessellator::new();
    let mut geometry = tess::geometry_builder::VertexBuffers::new();

    let mut builder = tess::path::path::Builder::new();
    builder.begin(LPoint::new(vertices[0].x as f32, vertices[0].y as f32));
    for v in &vertices[1..] {
        builder.line_to(LPoint::new(v.x as f32, v.y as f32));
    }
    builder.end(true); // close

    let path = builder.build();
    tess.tessellate(&path, &FillOptions::default(), &mut geometry).unwrap();

    geometry.triangles.chunks(3).map(|tri| {
        Triangle {
            v0: Point2::new(geometry.vertices[tri[0] as usize].position.x as f64,
                            geometry.vertices[tri[0] as usize].position.y as f64),
            v1: Point2::new(geometry.vertices[tri[1] as usize].position.x as f64,
                            geometry.vertices[tri[1] as usize].position.y as f64),
            v2: Point2::new(geometry.vertices[tri[2] as usize].position.x as f64,
                            geometry.vertices[tri[2] as usize].position.y as f64),
        }
    }).collect()
}
```

## Task 5: Triangle Rasterizer

```rust
fn rasterize_triangles(
    triangles: &[Triangle],
    width: u32,
    height: u32,
    pixels_per_mm: f64,
    offset: Vector2<f64>,
    fill_color: u8,
) -> GrayImage {
    let mut image = GrayImage::from_pixel(width, height, Luma([0]));

    for tri in triangles {
        // Project mm → pixel space (x: l→r, y: bottom→top)
        let p0 = project(tri.v0, width, height, pixels_per_mm, offset);
        let p1 = project(tri.v1, width, height, pixels_per_mm, offset);
        let p2 = project(tri.v2, width, height, pixels_per_mm, offset);

        // Compute bounding box
        let min_y = (p0.y.min(p1.y).min(p2.y)).max(0.0).floor() as u32;
        let max_y = (p0.y.max(p1.y).max(p2.y)).min((height - 1) as f64).ceil() as u32;

        for y in min_y..=max_y {
            let mut intersections = [
                edge_x_intersect(p0, p1, y as f64 + 0.5),
                edge_x_intersect(p1, p2, y as f64 + 0.5),
                edge_x_intersect(p2, p0, y as f64 + 0.5),
            ];
            intersections.sort_by(|a, b| a.partial_cmp(b).unwrap());
            // Filter NaN, keep valid intersections
            let valid: Vec<f64> = intersections.iter().filter(|&&x| x.is_finite()).copied().collect();
            if valid.len() >= 2 {
                let x0 = valid[0].max(0.0).round() as u32;
                let x1 = valid[valid.len() - 1].min((width - 1) as f64).round() as u32;
                for x in x0..=x1 {
                    image.put_pixel(x, y, Luma([fill_color]));
                }
            }
        }
    }

    image
}
```

### Projection function

```rust
fn project(p: Point2<f64>, width: u32, height: u32, ppm: f64, offset: Vector2<f64>) -> Point2<f64> {
    let x = (p.x + offset.x) * ppm;
    let y = height as f64 - (p.y + offset.y) * ppm; // flip Y
    Point2::new(x, y)
}
```

## Task 6: Public API + conversion.rs Wiring

### gerber.rs public functions

```rust
/// Parse Gerber source into shapes (triangles in mm space).
pub fn parse_to_shapes(source: &str) -> Result<Vec<Triangle>, ConversionError> {
    let commands = gerber_parser::parse(source)
        .map_err(|e| ConversionError::GerberParseError(e.to_string()))?;
    Ok(process_commands(&commands))
}

/// Render shapes (triangles) to a GrayImage.
pub fn render_shapes(
    shapes: &[Vec<Triangle>],
    settings: &ConversionSettings,
    layer_type: LayerType,
) -> Result<GrayImage, ConversionError> {
    // Compute bounds
    let (bounds, offset) = compute_bounds_and_offset(shapes, settings.pixels_per_mm as f64);

    let width = (((bounds.2 - bounds.0) + 0.82 * 2.0) * settings.pixels_per_mm as f64).round() as u32;
    let height = (((bounds.3 - bounds.1) + 0.82 * 2.0) * settings.pixels_per_mm as f64).round() as u32;
    let width = width.max(1);
    let height = height.max(1);

    check_pixel_limit(width, height, settings.max_render_pixels)?;

    let (fill, bg) = match layer_type {
        LayerType::Copper | LayerType::Profile => (255u8, 0u8),
        LayerType::Drill => (0u8, 255u8),
    };

    let all_tris: Vec<Triangle> = shapes.iter().flat_map(|s| s.iter().cloned()).collect();

    let mut image = GrayImage::from_pixel(width, height, Luma([bg]));
    overlay_triangles(&mut image, &all_tris, width, height, settings.pixels_per_mm as f64, offset, fill);

    Ok(image)
}
```

### conversion.rs updated gerber_inputs_to_png

```rust
pub fn gerber_inputs_to_png(
    inputs: &[PathBuf],
    output_path: &Path,
    settings: ConversionSettings,
) -> Result<PngRenderResult, ConversionError> {
    // ... read sources (same as before) ...

    let mut all_shapes = Vec::new();
    for source in &gerber_sources {
        if is_excellon(source) {
            let primitives = parse_excellon(source);
            let tris = convert_excellon_to_triangles(&primitives, settings.pixels_per_mm);
            all_shapes.push(tris);
        } else {
            let shapes = gerber::parse_to_shapes(source)?;
            all_shapes.extend(shapes);
        }
    }

    let image = gerber::render_shapes(&[all_shapes], &settings, gerber::LayerType::Copper)?;
    image.save(output_path)?;

    let dark_pixels = image.pixels().filter(|p| p[0] < settings.threshold).count();
    Ok(PngRenderResult { path: output_path.to_buf(), width: image.width(), height: image.height(), dark_pixels })
}
```

Wait — the original flow merges all gerber sources into a single render pass. Each source file (top.gtl, bottom.gbl, edge_cuts.gko) is a separate layer. The current code just merges all primitives together. I'll do the same — merge all triangles from all sources:

```rust
let mut all_triangles = Vec::new();
for source in &gerber_sources {
    if is_excellon(source) {
        // Excellon drills: black-on-white via LayerType::Drill
        let result = render_drills(source, &settings)?;
        // merge PNGs
    } else {
        let shapes = gerber::parse_to_shapes(source)?;
        all_triangles.extend(shapes.into_iter().flat_map(|s| s));
    }
}

let image = gerber::render_triangles(&all_triangles, &settings, ...)?;
```

Hmm, actually the spec says drills should be handled differently. Let me check the original code more carefully.

The original `gerber_inputs_to_png` just calls `parse_gerber_or_drill` for each file and collects all primitives, then renders them all at once. There's no separate handling for drills.

But the spec says drills should be rendered black-on-white. However, the current code just renders everything as white-on-black. A drill "flash" just shows up as a white circle on black, which when inverted means the drill hole becomes a "cut" area in the G-code.

If I change drill rendering to black-on-white, then the drills would show up as black pixels on white, meaning the G-code would still cut them. But the existing workflow might depend on the current behavior.

I think the simplest approach is: keep the existing rendering behavior (all layers rendered white-on-black), but support the LayerType for future use. Let me re-read the spec:

> | Layer type  | Fill color | Background | Binary meaning              |
> | Traces      | White     | Black      | Dark=cut, Light=keep        |
> | Edge cuts   | White     | Black      | Board interior=keep, scrap=cut |
> | Drills      | Black     | White      | Hole=cut, board=keep        |

OK so the spec says drills should be black-on-white. But in the current code, all layers are white-on-black. Since the G-code pipeline just checks `pixel[0] < threshold` for dark pixels, both approaches produce dark pixels where material should be removed.

However, the current code doesn't distinguish between layer types. It just renders everything the same way. The spec adds this distinction but says to keep the existing behavior for now.

Let me simplify: in `gerber_inputs_to_png`, I'll render all layers white-on-black (same as before). The LayerType distinction is available for future use.

Actually wait, I think the right approach for the initial plan is:
1. First make it work identically to before (all white-on-black)
2. Then optionally add per-layer rendering

Let me plan for step 1 only.

So in `gerber_inputs_to_png`:
- Read all sources
- For each source parse into triangles (via gerber.rs for Gerber, via parse_excellon for drills)
- Render all triangles together white-on-black
- Save PNG, return result

### Excellon conversion helper

Since parse_excellon returns Vec<Primitive> (old type), I need to convert those to triangles:

```rust
fn excellon_primitives_to_triangles(primitives: &[Primitive]) -> Vec<Triangle> {
    let mut triangles = Vec::new();
    for prim in primitives {
        match prim {
            Primitive::Flash { center, shape, dimensions } => {
                let c = Point2::new(center.x as f64, center.y as f64);
                match shape {
                    ApertureShape::Circle => triangles.extend(tessellate_aperture_circle(c, dimensions[0] as f64)),
                    ApertureShape::Rectangle => triangles.extend(tessellate_aperture_rect(c, dimensions[0] as f64, dimensions[1] as f64)),
                    ApertureShape::Oval => triangles.extend(tessellate_aperture_oval(c, dimensions[0] as f64, dimensions[1] as f64)),
                    ApertureShape::Polygon => triangles.extend(tessellate_aperture_polygon(c, dimensions[0] as f64, dimensions[1] as u32, dimensions.get(2).cloned().unwrap_or(0.0) as f64)),
                }
            }
            Primitive::Segment { start, end, diameter_mm } => {
                let s = Point2::new(start.x as f64, start.y as f64);
                let e = Point2::new(end.x as f64, end.y as f64);
                triangles.extend(tessellate_thick_line(s, e, *diameter_mm as f64));
            }
            Primitive::Polygon { points } => {
                let pts: Vec<Point2<f64>> = points.iter().map(|p| Point2::new(p.x as f64, p.y as f64)).collect();
                triangles.extend(tessellate_polygon(&pts));
            }
        }
    }
    triangles
}
```

### render_triangles_to_png (replaces render_primitives_to_png)

```rust
pub fn render_triangles_to_png(
    triangles: &[Triangle],
    output_path: &Path,
    settings: ConversionSettings,
) -> Result<PngRenderResult, ConversionError> {
    if triangles.is_empty() {
        return Err(ConversionError::NoRenderableGerber);
    }

    // Compute bounds
    let mut min_x = f64::MAX; let mut min_y = f64::MAX;
    let mut max_x = f64::MIN; let mut max_y = f64::MIN;
    for tri in triangles {
        for p in [&tri.v0, &tri.v1, &tri.v2] {
            if p.x < min_x { min_x = p.x; }
            if p.y < min_y { min_y = p.y; }
            if p.x > max_x { max_x = p.x; }
            if p.y > max_y { max_y = p.y; }
        }
    }

    let margin_mm = 0.82f64;
    let ppm = settings.pixels_per_mm as f64;

    let board_w = max_x - min_x;
    let board_h = max_y - min_y;

    let width = ((board_w + margin_mm * 2.0) * ppm).round() as u32;
    let height = ((board_h + margin_mm * 2.0) * ppm).round() as u32;
    let width = width.max(1);
    let height = height.max(1);

    let pixels = u64::from(width) * u64::from(height);
    if pixels > settings.max_render_pixels {
        return Err(ConversionError::RenderTooLarge { width, height, pixels, max_pixels: settings.max_render_pixels });
    }

    // Offset to position content in the image
    let offset = Vector2::new(-min_x + margin_mm, -min_y + margin_mm);

    let mut image = GrayImage::from_pixel(width, height, Luma([0]));
    rasterize_triangles(&mut image, triangles, width, height, ppm, offset, 255);

    let dark_pixels = image.pixels().filter(|p| p[0] < settings.threshold).count();
    image.save(output_path)?;

    Ok(PngRenderResult { path: output_path.to_path_buf(), width, height, dark_pixels })
}
```

### conversion.rs update

The `gerber_inputs_to_png` function changes to:

```rust
pub fn gerber_inputs_to_png(
    inputs: &[PathBuf],
    output_path: &Path,
    settings: ConversionSettings,
) -> Result<PngRenderResult, ConversionError> {
    // ... read sources (unchanged) ...

    let mut triangles = Vec::new();
    for source in &gerber_sources {
        if is_excellon(source) {
            let prims = parse_excellon(source);
            triangles.extend(excellon_primitives_to_triangles(&prims));
        } else {
            let shapes = gerber::parse_to_shapes(source)?;
            for shape in shapes {
                triangles.extend(shape.triangles);
            }
        }
    }

    info!(triangle_count = triangles.len(), "triangulated all layers");
    gerber::render_triangles_to_png(&triangles, output_path, settings)
}
```

Remove from conversion.rs:
- `Primitive` enum
- `ApertureShape` enum  
- `Point` struct
- `Aperture` struct
- `parse_gerber()`
- `parse_gerber_or_drill()`
- `parse_format()`
- `parse_aperture()`
- `parse_operation()`
- `parse_axis()`
- `render_primitives_to_png()`
- `primitive_bounds()`
- `draw_thick_line()`
- `draw_filled_circle()`
- `draw_filled_rect()`
- `draw_filled_oval()`
- `draw_filled_polygon_aperture()`
- `fill_polygon()`

Keep:
- `ConversionSettings`
- `PngRenderResult`
- `ConversionError` (add GerberParseError variant)
- `GcodeResult`
- `gerber_inputs_to_png()` (updated)
- `png_to_gcode()`
- All G-code pipeline helpers
- `read_zip_gerbers()`
- `parse_excellon()` (keep as-is)
- `raster_to_gcode()`
- `is_zip()`
- `is_gerber_name()`
- All tests (updated)

Add to `ConversionError`:
```rust
#[error("gerber parse error: {0}")]
GerberParseError(String),
```

### State machine — detailed command handling

Some commands from gerber_types may have slightly different names from what I assumed above. I'll adapt during implementation. Key things to handle correctly:

1. **Coordinate format**: FSLAX24Y24* — 2 integer, 4 decimal digits → scale = 1/10^4 = 0.0001 mm
2. **Unit mode**: MOMM (mm, scale=1.0), MOIN (inches, scale=25.4)
3. **Aperture blocks (%AB...%)**: record mode — capture commands to replay on each D03
4. **Step and repeat (%SR...%)**: repeat following commands in a grid
5. **Region mode (G36/G37)**: collect points into polygon
6. **Circular interpolation**: compute arc points, tessellate thick arc
7. **Linear interpolation with aperture**: thick line tessellation
8. **Flash**: aperture shape placed at current position

For the arc to polyline conversion:
```
center = current + (i, j)
radius = sqrt(i² + j²)
start_angle = atan2(current.y - center.y, current.x - center.x)
end_angle = atan2(target.y - center.y, target.x - center.x)

G02 (CW): sweep from start_angle to end_angle in clockwise direction
G03 (CCW): sweep counterclockwise

If sweep crosses 0/2π boundary, adjust accordingly.
Generate N = ceil(sweep_angle / 0.05) intermediate points (∼3° steps).
```

## Task 7: Test Updates

### Updated tests in conversion.rs

```rust
#[cfg(test)]
mod tests {
    use std::{fs, io::Write};
    use image::{ImageBuffer, Luma};
    use tempfile::tempdir;
    use zip::{ZipWriter, write::SimpleFileOptions};
    use super::{
        ConversionError, ConversionSettings, DEFAULT_PIXELS_PER_MM,
        gerber_inputs_to_png, png_to_gcode,
    };

    const SIMPLE_GERBER: &str = "%FSLAX24Y24*%\n%MOMM*%\n%ADD10C,0.300*%\nD10*\nX000000Y000000D02*\nX010000Y010000D01*\nM02*\n";

    #[test]
    fn gerber_stroke_renders_to_png() { ... }  // unchanged

    #[test]
    fn default_gerber_render_uses_1000_dpi() { ... }  // unchanged

    #[test]
    fn zipped_gerber_package_renders_to_png() { ... }  // unchanged

    #[test]
    fn black_raster_generates_cutting_moves() { ... }  // unchanged

    #[test]
    fn oversized_gerber_render_fails_before_allocation() { ... }  // unchanged

    #[test]
    fn gcode_result_includes_toolpaths() { ... }  // unchanged

    #[test]
    fn real_gerber_zip_renders_to_expected_size() { ... }  // same assertions
}
```

The tests should pass with the new parser since the assertions are the same (valid PNG, expected pixel count constraints).

### New test in gerber.rs

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_triangle_rasterizes_correctly() {
        let tri = Triangle {
            v0: Point2::new(0.0, 0.0),
            v1: Point2::new(10.0, 0.0),
            v2: Point2::new(0.0, 10.0),
        };
        let mut image = GrayImage::from_pixel(20, 20, Luma([0u8]));
        rasterize_triangles(&mut image, &[tri], 20, 20, 1.0, Vector2::new(0.0, 0.0), 255u8);
        // Pixel (1,1) should be inside the triangle → white
        assert_eq!(image.get_pixel(1, 1)[0], 255);
        // Pixel (15,15) should be outside → black
        assert_eq!(image.get_pixel(15, 15)[0], 0);
    }

    #[test]
    fn parse_simple_gerber_produces_triangles() {
        let source = "%FSLAX24Y24*%\n%MOMM*%\n%ADD10C,0.300*%\nD10*\nX000000Y000000D02*\nX010000Y010000D01*\nM02*\n";
        let shapes = parse_to_shapes(source).unwrap();
        assert!(!shapes.is_empty());
        assert!(!shapes[0].triangles.is_empty());
    }
}
```

## Implementation Order

1. Add Cargo.toml deps, update lib.rs
2. Write full gerber.rs with all types, MacroExpander, CommandProcessor, tessellation, rasterizer, public API
3. Update conversion.rs: remove old parser/rasterizer, update gerber_inputs_to_png, keep parse_excellon, add excellon conversion helper
4. Update ConversionError to add GerberParseError variant
5. Update tests
6. `cargo build` → fix compilation errors
7. `cargo test` → verify all tests pass
8. If real_gerber_zip test exists, check output renders correctly

## Lyon API Notes

lyon 1.0 API:
```rust
use lyon::tessellation::{FillTessellator, FillOptions};
use lyon::tessellation::VertexBuffers;
use lyon::path::Path as LPath;

let mut tess = FillTessellator::new();
let mut geometry: VertexBuffers<LPoint, u16> = VertexBuffers::new();
```

The `Builder` API for paths:
```rust
use lyon::path::builder::NoAttributes;
use lyon::path::path::BuilderImpl;

let mut builder = lyon::path::Path::builder();
builder.begin(LPoint::new(x, y));
builder.line_to(LPoint::new(x2, y2));
// ...
builder.end(true); // close
let path = builder.build();
```

FillTessellator returns indexed triangles. Read as:
```rust
for tri in geometry.triangles.chunks(3) {
    let v0 = &geometry.vertices[tri[0] as usize];
    let v1 = &geometry.vertices[tri[1] as usize];
    let v2 = &geometry.vertices[tri[2] as usize];
}
```

## Arc computation detail

```rust
fn interpolate_arc(
    current: Point2<f64>,
    target: Point2<f64>,
    i: f64, j: f64,
    clockwise: bool,
    multi_quadrant: bool,
    num_segments: usize,
) -> Vec<Point2<f64>> {
    let center = Point2::new(current.x + i, current.y + j);
    let radius = (i * i + j * j).sqrt();
    if radius < 1e-10 { return vec![current, target]; }

    let start_angle = (current - center).y.atan2((current - center).x);
    let end_angle = (target - center).y.atan2((target - center).x);

    let mut sweep = end_angle - start_angle;
    if clockwise {
        // Normalize to negative sweep
        if sweep > 0.0 { sweep -= 2.0 * std::f64::consts::PI; }
    } else {
        // Normalize to positive sweep
        if sweep < 0.0 { sweep += 2.0 * std::f64::consts::PI; }
    }

    if !multi_quadrant {
        // Single quadrant: |sweep| <= PI/2
        sweep = sweep.signum() * (sweep.abs().min(std::f64::consts::PI / 2.0));
    }

    let num_pts = ((sweep.abs() / (2.0 * std::f64::consts::PI)) * num_segments as f64).ceil() as usize;
    let num_pts = num_pts.max(2);

    let mut pts = Vec::with_capacity(num_pts + 1);
    for k in 0..=num_pts {
        let t = k as f64 / num_pts as f64;
        let angle = start_angle + sweep * t;
        pts.push(Point2::new(center.x + radius * angle.cos(), center.y + radius * angle.sin()));
    }
    pts
}
```

## Potential pitfalls

1. **gerber_types API surface** — exact variant/field names may differ from my assumptions. Verify by compiling against actual gerber_types 0.5.
2. **Coordinate decoding** — gerber_types stores coordinates as i32 integer values. The format spec (FSLAX24Y24) tells how many digits are integer vs decimal. Scale = scale_mm / (10^dec_digits).
3. **Step and repeat** — repeat stateful. Each iteration applies (dx, dy) offset. Hard to implement correctly — may not be needed for basic test files.
4. **Aperture blocks** — record state then replay on D03. May not appear in test files.
5. **Floating point precision** — use f64 for mm coordinates, f32 for pixel positions (image crate uses u32).
6. **Empty layer** — handle case where no shapes produced (old code returns NoRenderableGerber error).
