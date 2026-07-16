use std::collections::HashMap;
use std::path::Path;

use gerber_types::{
    Aperture, ApertureBlock, Command, DCode, ExtendedCode, FunctionCode, GCode,
    InterpolationMode, MacroBoolean, MacroContent, MacroDecimal, MacroInteger, Operation,
    QuadrantMode, StepAndRepeat, Unit,
};
use gerber_types::{CoordinateOffset, Coordinates};
use image::{GrayImage, Luma};
use lyon::path::Path as LPath;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers,
};
use lyon::math::Point as LPoint;
use nalgebra::{Point2, Vector2};

use crate::conversion::{ConversionError, ConversionSettings, PngRenderResult};

#[derive(Debug, Clone, Copy)]
pub struct Triangle {
    pub v0: Point2<f64>,
    pub v1: Point2<f64>,
    pub v2: Point2<f64>,
}

#[derive(Debug, Clone)]
pub struct Shape {
    pub triangles: Vec<Triangle>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayerType {
    Copper,
    Profile,
    Drill,
}

#[derive(Debug, Clone)]
pub struct TaggedTriangle {
    pub triangle: Triangle,
    pub layer: LayerType,
}

pub type BoundingBox = (f64, f64, f64, f64);

#[derive(Debug, Clone)]
enum MacroToken {
    Value(f64),
    Variable(u32),
    Op(char),
    Open,
    Close,
}

fn shunting_yard(tokens: &[MacroToken]) -> Vec<MacroToken> {
    let mut output: Vec<MacroToken> = Vec::new();
    let mut ops: Vec<&MacroToken> = Vec::new();

    for token in tokens {
        match token {
            MacroToken::Value(_) | MacroToken::Variable(_) => output.push(token.clone()),
            MacroToken::Open => ops.push(token),
            MacroToken::Close => {
                while let Some(op) = ops.pop() {
                    match op {
                        MacroToken::Open => break,
                        _ => output.push(op.clone()),
                    }
                }
            }
            MacroToken::Op(c) => {
                let prec = |op: char| match op { '+' | '-' => 1, 'x' | '/' => 2, _ => 0 };
                while let Some(MacroToken::Op(top)) = ops.last() {
                    if prec(*c) <= prec(*top) {
                        output.push(ops.pop().unwrap().clone());
                    } else {
                        break;
                    }
                }
                ops.push(token);
            }
        }
    }

    while let Some(op) = ops.pop() {
        if !matches!(op, MacroToken::Open) {
            output.push(op.clone());
        }
    }

    output
}

fn eval_rpn(rpn: &[MacroToken]) -> f64 {
    let mut stack: Vec<f64> = Vec::new();

    for token in rpn {
        match token {
            MacroToken::Value(v) => stack.push(*v),
            MacroToken::Op('+') => {
                let b = stack.pop().unwrap_or(0.0);
                let a = stack.pop().unwrap_or(0.0);
                stack.push(a + b);
            }
            MacroToken::Op('-') => {
                let b = stack.pop().unwrap_or(0.0);
                if stack.is_empty() {
                    stack.push(-b);
                } else {
                    let a = stack.pop().unwrap_or(0.0);
                    stack.push(a - b);
                }
            }
            MacroToken::Op('x') | MacroToken::Op('*') => {
                let b = stack.pop().unwrap_or(0.0);
                let a = stack.pop().unwrap_or(0.0);
                stack.push(a * b);
            }
            MacroToken::Op('/') => {
                let b = stack.pop().unwrap_or(1.0);
                let a = stack.pop().unwrap_or(0.0);
                stack.push(if b != 0.0 { a / b } else { 0.0 });
            }
            _ => {}
        }
    }

    stack.pop().unwrap_or(0.0)
}

fn tokenize_expression(expr: &str) -> Vec<MacroToken> {
    let mut tokens = Vec::new();
    let mut chars = expr.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '+' => tokens.push(MacroToken::Op('+')),
            '-' => tokens.push(MacroToken::Op('-')),
            'x' | '*' => tokens.push(MacroToken::Op('x')),
            '/' => tokens.push(MacroToken::Op('/')),
            '(' => tokens.push(MacroToken::Open),
            ')' => tokens.push(MacroToken::Close),
            '$' => {
                let mut var_str = String::new();
                while let Some(d) = chars.peek() {
                    if d.is_ascii_digit() {
                        var_str.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                let var_idx: u32 = var_str.parse().unwrap_or(0);
                tokens.push(MacroToken::Variable(var_idx));
            }
            _ if ch.is_ascii_digit() || ch == '.' => {
                let mut num_str = String::new();
                num_str.push(ch);
                while let Some(d) = chars.peek() {
                    if d.is_ascii_digit() || *d == '.' {
                        num_str.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                let val: f64 = num_str.parse().unwrap_or(0.0);
                tokens.push(MacroToken::Value(val));
            }
            _ => {}
        }
    }

    tokens
}

fn parse_macro_decimal(dec: &gerber_types::MacroDecimal, vars: &[f64]) -> f64 {
    match dec {
        gerber_types::MacroDecimal::Value(v) => *v,
        gerber_types::MacroDecimal::Variable(idx) => {
            let i = *idx as usize;
            if i >= 1 && i.saturating_sub(1) < vars.len() {
                vars[i.saturating_sub(1)]
            } else {
                0.0
            }
        }
        gerber_types::MacroDecimal::Expression(s) => eval_expression_str(s, vars),
    }
}

pub fn eval_expression_str(expr: &str, vars: &[f64]) -> f64 {
    let tokens = tokenize_expression(expr);

    let substituted: Vec<MacroToken> = tokens
        .into_iter()
        .map(|t| match t {
            MacroToken::Variable(idx) => {
                let i = idx as usize;
                MacroToken::Value(if i >= 1 && i.saturating_sub(1) < vars.len() {
                    vars[i.saturating_sub(1)]
                } else {
                    0.0
                })
            }
            other => other,
        })
        .collect();

    let rpn = shunting_yard(&substituted);
    eval_rpn(&rpn)
}

pub fn eval_expression(expr: &gerber_types::MacroDecimal, vars: &[f64]) -> f64 {
    parse_macro_decimal(expr, vars)
}

pub fn tessellate_polygon(points: &[Point2<f64>]) -> Vec<Triangle> {
    if points.len() < 3 {
        return Vec::new();
    }

    let mut tess = FillTessellator::new();
    let mut geometry: VertexBuffers<LPoint, u16> = VertexBuffers::new();

    let mut builder = LPath::builder();
    builder.begin(LPoint::new(points[0].x as f32, points[0].y as f32));
    for p in &points[1..] {
        builder.line_to(LPoint::new(p.x as f32, p.y as f32));
    }
    builder.close();

    let path = builder.build();
    if tess
        .tessellate(
            &path,
            &FillOptions::default(),
            &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| vertex.position()),
        )
        .is_err()
    {
        return Vec::new();
    }

    geometry
        .indices
        .chunks(3)
        .map(|tri| Triangle {
            v0: Point2::new(
                geometry.vertices[tri[0] as usize].x as f64,
                geometry.vertices[tri[0] as usize].y as f64,
            ),
            v1: Point2::new(
                geometry.vertices[tri[1] as usize].x as f64,
                geometry.vertices[tri[1] as usize].y as f64,
            ),
            v2: Point2::new(
                geometry.vertices[tri[2] as usize].x as f64,
                geometry.vertices[tri[2] as usize].y as f64,
            ),
        })
        .collect()
}

pub fn tessellate_circle_ngon(center: Point2<f64>, radius: f64, n: u32) -> Vec<Triangle> {
    let n = n.max(3);
    let mut pts = Vec::with_capacity(n as usize);
    for i in 0..n {
        let angle = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
        pts.push(Point2::new(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
    }
    tessellate_polygon(&pts)
}

pub fn tessellate_aperture_circle(center: Point2<f64>, diameter: f64) -> Vec<Triangle> {
    tessellate_circle_ngon(center, diameter / 2.0, 64)
}

pub fn tessellate_aperture_rect(center: Point2<f64>, w: f64, h: f64) -> Vec<Triangle> {
    let hw = w / 2.0;
    let hh = h / 2.0;
    tessellate_polygon(&[
        Point2::new(center.x - hw, center.y - hh),
        Point2::new(center.x + hw, center.y - hh),
        Point2::new(center.x + hw, center.y + hh),
        Point2::new(center.x - hw, center.y + hh),
    ])
}

pub fn tessellate_aperture_oval(center: Point2<f64>, w: f64, h: f64) -> Vec<Triangle> {
    use std::f64::consts::PI;
    let n: u32 = 32;

    if w > h {
        let radius = h / 2.0;
        let offset = (w - h) / 2.0;
        let mut pts = Vec::with_capacity((n * 2 + 2) as usize);

        for i in 0..=n {
            let angle = -PI / 2.0 + PI * (i as f64) / (n as f64);
            pts.push(Point2::new(
                center.x + offset + radius * angle.cos(),
                center.y + radius * angle.sin(),
            ));
        }
        for i in 1..n {
            let angle = PI / 2.0 + PI * (i as f64) / (n as f64);
            pts.push(Point2::new(
                center.x - offset + radius * angle.cos(),
                center.y + radius * angle.sin(),
            ));
        }
        tessellate_polygon(&pts)
    } else if h > w {
        let radius = w / 2.0;
        let offset = (h - w) / 2.0;
        let mut pts = Vec::with_capacity((n * 2 + 2) as usize);

        for i in 0..=n {
            let angle = PI * (i as f64) / (n as f64);
            pts.push(Point2::new(
                center.x + radius * angle.cos(),
                center.y + offset + radius * angle.sin(),
            ));
        }
        for i in 1..n {
            let angle = PI + PI * (i as f64) / (n as f64);
            pts.push(Point2::new(
                center.x + radius * angle.cos(),
                center.y - offset + radius * angle.sin(),
            ));
        }
        tessellate_polygon(&pts)
    } else {
        tessellate_aperture_circle(center, w)
    }
}

pub fn tessellate_aperture_polygon(
    center: Point2<f64>,
    diameter: f64,
    vertices: u32,
    rotation_deg: f64,
) -> Vec<Triangle> {
    let radius = diameter / 2.0;
    let n = vertices.max(3);
    let rot = rotation_deg.to_radians();
    let mut pts = Vec::with_capacity(n as usize);
    for i in 0..n {
        let angle = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64) + rot;
        pts.push(Point2::new(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
    }
    tessellate_polygon(&pts)
}

pub fn tessellate_thick_line(
    start: Point2<f64>,
    end: Point2<f64>,
    width: f64,
) -> Vec<Triangle> {
    if width <= 0.0 {
        return Vec::new();
    }

    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = (dx * dx + dy * dy).sqrt();

    if len < 1e-10 {
        return tessellate_aperture_circle(start, width);
    }

    let half = width / 2.0;
    let nx = -dy / len * half;
    let ny = dx / len * half;

    let body = tessellate_polygon(&[
        Point2::new(start.x + nx, start.y + ny),
        Point2::new(start.x - nx, start.y - ny),
        Point2::new(end.x - nx, end.y - ny),
        Point2::new(end.x + nx, end.y + ny),
    ]);

    fn half_circle_ccw(
        center: Point2<f64>,
        nx: f64,
        ny: f64,
        n: u32,
    ) -> Vec<Point2<f64>> {
        let mut pts = Vec::with_capacity((n + 1) as usize);
        for i in 0..=n {
            let angle = std::f64::consts::PI * (i as f64) / (n as f64);
            let cos = angle.cos();
            let sin = angle.sin();
            let px = nx * cos - ny * sin;
            let py = nx * sin + ny * cos;
            pts.push(Point2::new(center.x + px, center.y + py));
        }
        pts
    }

    fn half_circle_cw(
        center: Point2<f64>,
        nx: f64,
        ny: f64,
        n: u32,
    ) -> Vec<Point2<f64>> {
        let mut pts = Vec::with_capacity((n + 1) as usize);
        for i in 0..=n {
            let angle = std::f64::consts::PI * (i as f64) / (n as f64);
            let cos = angle.cos();
            let sin = angle.sin();
            let px = -nx * cos + ny * sin;
            let py = -nx * sin - ny * cos;
            pts.push(Point2::new(center.x + px, center.y + py));
        }
        pts
    }

    let n: u32 = 16;
    let mut start_cap = half_circle_ccw(start, nx, ny, n);
    start_cap.push(Point2::new(start.x + nx, start.y + ny));

    let mut end_cap = half_circle_cw(end, nx, ny, n);
    end_cap.push(Point2::new(end.x - nx, end.y - ny));

    let mut result = body;
    result.extend(tessellate_polygon(&start_cap));
    result.extend(tessellate_polygon(&end_cap));
    result
}

pub fn interpolate_arc(
    current: Point2<f64>,
    target: Point2<f64>,
    i: f64,
    j: f64,
    clockwise: bool,
    multi_quadrant: bool,
    num_segments: usize,
) -> Vec<Point2<f64>> {
    let center = Point2::new(current.x + i, current.y + j);
    let radius = (i * i + j * j).sqrt();
    if radius < 1e-10 {
        return vec![current, target];
    }

    let start_angle = (current - center).y.atan2((current - center).x);
    let end_angle = (target - center).y.atan2((target - center).x);

    let mut sweep = end_angle - start_angle;

    if clockwise {
        if sweep > 0.0 {
            sweep -= 2.0 * std::f64::consts::PI;
        }
        if !multi_quadrant {
            sweep = sweep.max(-std::f64::consts::PI / 2.0);
        }
    } else {
        if sweep < 0.0 {
            sweep += 2.0 * std::f64::consts::PI;
        }
        if !multi_quadrant {
            sweep = sweep.min(std::f64::consts::PI / 2.0);
        }
    }

    let n = ((sweep.abs() / (2.0 * std::f64::consts::PI)) * num_segments as f64)
        .ceil().max(2.0) as usize;

    let mut pts = Vec::with_capacity(n + 1);
    for k in 0..=n {
        let t = k as f64 / n as f64;
        let angle = start_angle + sweep * t;
        pts.push(Point2::new(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
    }
    pts
}

pub fn rasterize_triangles(
    image: &mut GrayImage,
    triangles: &[Triangle],
    width: u32,
    height: u32,
    ppm: f64,
    offset: Vector2<f64>,
    fill_color: u8,
) {
    let project = |p: Point2<f64>| -> (f64, f64) {
        let x = (p.x + offset.x) * ppm;
        let y = (p.y + offset.y) * ppm;
        (x, y)
    };

    for tri in triangles {
        let (p0x, p0y) = project(tri.v0);
        let (p1x, p1y) = project(tri.v1);
        let (p2x, p2y) = project(tri.v2);

        let flip_y = |y: f64| -> f64 { height as f64 - y };

        let p0y = flip_y(p0y);
        let p1y = flip_y(p1y);
        let p2y = flip_y(p2y);

        let min_y = p0y.min(p1y).min(p2y).max(0.0).floor() as u32;
        let max_y = p0y.max(p1y).max(p2y).min((height - 1) as f64).ceil() as u32;

        for y in min_y..=max_y {
            let yf = y as f64 + 0.5;

            let mut xs = Vec::with_capacity(3);
            if let Some(x) = edge_x_intersect(p0x, p0y, p1x, p1y, yf) {
                xs.push(x);
            }
            if let Some(x) = edge_x_intersect(p1x, p1y, p2x, p2y, yf) {
                xs.push(x);
            }
            if let Some(x) = edge_x_intersect(p2x, p2y, p0x, p0y, yf) {
                xs.push(x);
            }

            if xs.len() >= 2 {
                xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let x0 = xs.first().unwrap().max(0.0).round() as u32;
                let x1 = xs.last().unwrap().min((width - 1) as f64).round() as u32;
                for x in x0..=x1 {
                    image.put_pixel(x, y, Luma([fill_color]));
                }
            }
        }
    }
}

fn edge_x_intersect(x0: f64, y0: f64, x1: f64, y1: f64, y: f64) -> Option<f64> {
    if (y0 <= y && y1 > y) || (y1 <= y && y0 > y) {
        Some(x0 + (y - y0) / (y1 - y0) * (x1 - x0))
    } else {
        None
    }
}

#[allow(dead_code)]
struct GerberState {
    scale_mm: f64,
    current: Point2<f64>,
    interpolation: InterpolationMode,
    quadrant_mode: QuadrantMode,
    apertures: HashMap<i32, Aperture>,
    macros: HashMap<String, gerber_types::ApertureMacro>,
    current_aperture: Option<i32>,
    region_points: Vec<Point2<f64>>,
    in_region: bool,
    triangles: Vec<Triangle>,

    sr_active: bool,
    sr_original: (i32, i32),
    sr_distance: (f64, f64),
    sr_saved_count: usize,
    sr_saved_current: Point2<f64>,
    sr_recorded: Vec<Command>,

    ab_active: bool,
    ab_code: Option<i32>,
    ab_recorded: Vec<Command>,
    ab_definitions: HashMap<i32, Vec<Command>>,
}

impl GerberState {
    fn new() -> Self {
        GerberState {
            scale_mm: 1.0,
            current: Point2::new(0.0, 0.0),
            interpolation: InterpolationMode::Linear,
            quadrant_mode: QuadrantMode::Multi,
            apertures: HashMap::new(),
            macros: HashMap::new(),
            current_aperture: None,
            region_points: Vec::new(),
            in_region: false,
            triangles: Vec::new(),
            sr_active: false,
            sr_original: (0, 0),
            sr_distance: (0.0, 0.0),
            sr_saved_count: 0,
            sr_saved_current: Point2::new(0.0, 0.0),
            sr_recorded: Vec::new(),
            ab_active: false,
            ab_code: None,
            ab_recorded: Vec::new(),
            ab_definitions: HashMap::new(),
        }
    }

    fn process_commands(&mut self, commands: &[Command]) {
        for cmd in commands {
            self.process_command(cmd);
        }
    }

    fn process_command(&mut self, cmd: &Command) {
        if self.sr_active {
            match cmd {
                Command::ExtendedCode(ExtendedCode::StepAndRepeat(_)) => {}
                _ => self.sr_recorded.push(cmd.clone()),
            }
        }
        if self.ab_active {
            match cmd {
                Command::ExtendedCode(ExtendedCode::ApertureBlock(_)) => {}
                _ => self.ab_recorded.push(cmd.clone()),
            }
        }
        match cmd {
            Command::ExtendedCode(ext) => self.process_extended(ext),
            Command::FunctionCode(func) => self.process_function(func),
        }
    }

    fn process_extended(&mut self, ext: &ExtendedCode) {
        match ext {
            ExtendedCode::CoordinateFormat(_fmt) => {}
            ExtendedCode::Unit(unit) => {
                self.scale_mm = match unit {
                    Unit::Millimeters => 1.0,
                    Unit::Inches => 25.4,
                };
            }
            ExtendedCode::ApertureDefinition(def) => {
                self.apertures.insert(def.code, def.aperture.clone());
            }
            ExtendedCode::ApertureMacro(mac) => {
                self.macros.insert(mac.name.clone(), mac.clone());
            }
            ExtendedCode::StepAndRepeat(sr) => match sr {
                StepAndRepeat::Open {
                    repeat_x,
                    repeat_y,
                    distance_x,
                    distance_y,
                } => {
                    self.sr_active = true;
                    self.sr_original = (*repeat_x as i32, *repeat_y as i32);
                    self.sr_distance = (*distance_x, *distance_y);
                    self.sr_saved_count = self.triangles.len();
                    self.sr_saved_current = self.current;
                    self.sr_recorded.clear();
                }
                StepAndRepeat::Close => {
                    if self.sr_active {
                        let base: Vec<Triangle> = self.triangles.drain(self.sr_saved_count..).collect();
                        let dx = self.sr_distance.0 * self.scale_mm;
                        let dy = self.sr_distance.1 * self.scale_mm;
                        for rep_x in 0..self.sr_original.0 {
                            for rep_y in 0..self.sr_original.1 {
                                if rep_x == 0 && rep_y == 0 {
                                    continue;
                                }
                                let ox = rep_x as f64 * dx;
                                let oy = rep_y as f64 * dy;
                                for tri in &base {
                                    self.triangles.push(Triangle {
                                        v0: Point2::new(tri.v0.x + ox, tri.v0.y + oy),
                                        v1: Point2::new(tri.v1.x + ox, tri.v1.y + oy),
                                        v2: Point2::new(tri.v2.x + ox, tri.v2.y + oy),
                                    });
                                }
                            }
                        }
                        self.current = self.sr_saved_current;
                        self.sr_active = false;
                    }
                }
            },
            ExtendedCode::ApertureBlock(ab) => match ab {
                ApertureBlock::Open { code } => {
                    self.ab_active = true;
                    self.ab_code = Some(*code);
                    self.ab_recorded.clear();
                }
                ApertureBlock::Close => {
                    if self.ab_active {
                        if let Some(code) = self.ab_code {
                            self.ab_definitions
                                .insert(code, std::mem::take(&mut self.ab_recorded));
                        }
                        self.ab_active = false;
                    }
                }
            },
            _ => {}
        }
    }

    fn process_function(&mut self, func: &FunctionCode) {
        match func {
            FunctionCode::DCode(dcode) => self.process_dcode(dcode),
            FunctionCode::GCode(gcode) => self.process_gcode(gcode),
            FunctionCode::MCode(_) => {}
        }
    }

    fn process_dcode(&mut self, dcode: &DCode) {
        match dcode {
            DCode::SelectAperture(code) => {
                self.current_aperture = Some(*code);
            }
            DCode::Operation(op) => match op {
                Operation::Move(coords) => {
                    if let Some(c) = coords {
                        self.current = self.coords_to_point(c);
                    }
                }
                Operation::Interpolate(coords, offset) => {
                    let target = coords
                        .as_ref()
                        .map(|c| self.coords_to_point(c))
                        .unwrap_or(self.current);

                    if self.in_region {
                        self.region_points.push(target);
                    } else {
                        match self.interpolation {
                            InterpolationMode::Linear => {
                                self.emit_thick_line(self.current, target);
                            }
                            InterpolationMode::ClockwiseCircular => {
                                let (i, j) = self.offset_to_ij(offset);
                                let pts = interpolate_arc(
                                    self.current,
                                    target,
                                    i,
                                    j,
                                    true,
                                    self.quadrant_mode == QuadrantMode::Multi,
                                    64,
                                );
                                self.emit_thick_polyline(&pts);
                            }
                            InterpolationMode::CounterclockwiseCircular => {
                                let (i, j) = self.offset_to_ij(offset);
                                let pts = interpolate_arc(
                                    self.current,
                                    target,
                                    i,
                                    j,
                                    false,
                                    self.quadrant_mode == QuadrantMode::Multi,
                                    64,
                                );
                                self.emit_thick_polyline(&pts);
                            }
                        }
                    }

                    self.current = target;
                }
                Operation::Flash(coords) => {
                    let pos = match coords {
                        Some(c) => {
                            let p = self.coords_to_point(c);
                            self.current = p;
                            p
                        }
                        None => self.current,
                    };
                    if let Some(ref aperture_code) = self.current_aperture {
                        if let Some(aperture) = self.apertures.get(aperture_code) {
                            let tris = self.tessellate_aperture(aperture, pos);
                            self.triangles.extend(tris);
                        }
                    }
                }
            },
        }
    }

    fn coords_to_point(&self, coords: &Coordinates) -> Point2<f64> {
        let x = coords
            .x
            .as_ref()
            .map(|cn| f64::from(*cn) * self.scale_mm)
            .unwrap_or(self.current.x);
        let y = coords
            .y
            .as_ref()
            .map(|cn| f64::from(*cn) * self.scale_mm)
            .unwrap_or(self.current.y);
        Point2::new(x, y)
    }

    fn offset_to_ij(&self, offset: &Option<CoordinateOffset>) -> (f64, f64) {
        match offset {
            Some(o) => {
                let i = o
                    .x
                    .as_ref()
                    .map(|cn| f64::from(*cn) * self.scale_mm)
                    .unwrap_or(0.0);
                let j = o
                    .y
                    .as_ref()
                    .map(|cn| f64::from(*cn) * self.scale_mm)
                    .unwrap_or(0.0);
                (i, j)
            }
            None => (0.0, 0.0),
        }
    }

    fn process_gcode(&mut self, gcode: &GCode) {
        match gcode {
            GCode::InterpolationMode(mode) => {
                self.interpolation = *mode;
            }
            GCode::RegionMode(enabled) => {
                if *enabled {
                    self.in_region = true;
                    self.region_points.clear();
                } else {
                    self.in_region = false;
                    if self.region_points.len() >= 3 {
                        let pts = std::mem::take(&mut self.region_points);
                        let tris = tessellate_polygon(&pts);
                        self.triangles.extend(tris);
                    } else {
                        self.region_points.clear();
                    }
                }
            }
            GCode::QuadrantMode(mode) => {
                self.quadrant_mode = *mode;
            }
            _ => {}
        }
    }

    fn tessellate_aperture(&self, aperture: &Aperture, center: Point2<f64>) -> Vec<Triangle> {
        match aperture {
            Aperture::Circle(c) => {
                tessellate_aperture_circle(center, c.diameter * self.scale_mm)
            }
            Aperture::Rectangle(r) => {
                tessellate_aperture_rect(center, r.x * self.scale_mm, r.y * self.scale_mm)
            }
            Aperture::Obround(r) => {
                tessellate_aperture_oval(center, r.x * self.scale_mm, r.y * self.scale_mm)
            }
            Aperture::Polygon(p) => {
                let rotation = p.rotation.unwrap_or(0.0);
                tessellate_aperture_polygon(
                    center,
                    p.diameter * self.scale_mm,
                    p.vertices as u32,
                    rotation,
                )
            }
            Aperture::Macro(name, args) => {
                if let Some(mac) = self.macros.get(name) {
                    self.expand_macro(mac, args.as_deref().unwrap_or(&[]), center)
                } else {
                    Vec::new()
                }
            }
        }
    }

    fn expand_macro(
        &self,
        mac: &gerber_types::ApertureMacro,
        args: &[MacroDecimal],
        center: Point2<f64>,
    ) -> Vec<Triangle> {
        let mut vars: Vec<f64> = args.iter().map(|a| eval_expression(a, &[])).collect();

        let mut triangles = Vec::new();

        for content in &mac.content {
            match content {
                MacroContent::VariableDefinition(var_def) => {
                    let idx = var_def.number;
                    let val = eval_expression_str(&var_def.expression, &vars);
                    let idx_usize = idx as usize;
                    if idx_usize >= 1 {
                        if idx_usize <= vars.len() {
                            vars[idx_usize - 1] = val;
                        } else {
                            vars.resize(idx_usize, 0.0);
                            vars[idx_usize - 1] = val;
                        }
                    }
                }
                MacroContent::Circle(c) => {
                    let exposure = eval_macro_bool(&c.exposure, &vars);
                    if exposure.is_none() || exposure == Some(false) {
                        continue;
                    }
                    let diameter = eval_decimal(&c.diameter, &vars) * self.scale_mm;
                    let cx = eval_decimal(&c.center.0, &vars) * self.scale_mm + center.x;
                    let cy = eval_decimal(&c.center.1, &vars) * self.scale_mm + center.y;
                    triangles.extend(tessellate_aperture_circle(
                        Point2::new(cx, cy),
                        diameter,
                    ));
                }
                MacroContent::VectorLine(vl) => {
                    let exposure = eval_macro_bool(&vl.exposure, &vars);
                    if exposure.is_none() || exposure == Some(false) {
                        continue;
                    }
                    let width = eval_decimal(&vl.width, &vars) * self.scale_mm;
                    let sx = eval_decimal(&vl.start.0, &vars) * self.scale_mm;
                    let sy = eval_decimal(&vl.start.1, &vars) * self.scale_mm;
                    let ex = eval_decimal(&vl.end.0, &vars) * self.scale_mm;
                    let ey = eval_decimal(&vl.end.1, &vars) * self.scale_mm;

                    let angle = eval_decimal(&vl.angle, &vars);
                    let (sx, sy, ex, ey) = if angle.abs() > 1e-10 {
                        let rad = angle.to_radians();
                        let cos = rad.cos();
                        let sin = rad.sin();
                        (
                            sx * cos - sy * sin,
                            sx * sin + sy * cos,
                            ex * cos - ey * sin,
                            ex * sin + ey * cos,
                        )
                    } else {
                        (sx, sy, ex, ey)
                    };

                    let start = Point2::new(sx + center.x, sy + center.y);
                    let end = Point2::new(ex + center.x, ey + center.y);
                    triangles.extend(tessellate_thick_line(start, end, width));
                }
                MacroContent::CenterLine(cl) => {
                    let exposure = eval_macro_bool(&cl.exposure, &vars);
                    if exposure.is_none() || exposure == Some(false) {
                        continue;
                    }
                    let w = eval_decimal(&cl.dimensions.0, &vars) * self.scale_mm;
                    let h = eval_decimal(&cl.dimensions.1, &vars) * self.scale_mm;
                    let cx = eval_decimal(&cl.center.0, &vars) * self.scale_mm;
                    let cy = eval_decimal(&cl.center.1, &vars) * self.scale_mm;
                    let angle = eval_decimal(&cl.angle, &vars);
                    if angle.abs() > 1e-10 {
                        let rad = angle.to_radians();
                        let cos = rad.cos();
                        let sin = rad.sin();
                        let hw = w / 2.0;
                        let hh = h / 2.0;
                        let corners = [
                            (cx - hw, cy - hh),
                            (cx + hw, cy - hh),
                            (cx + hw, cy + hh),
                            (cx - hw, cy + hh),
                        ];
                        let pts: Vec<Point2<f64>> = corners
                            .iter()
                            .map(|(px, py)| {
                                Point2::new(
                                    px * cos - py * sin + center.x,
                                    px * sin + py * cos + center.y,
                                )
                            })
                            .collect();
                        triangles.extend(tessellate_polygon(&pts));
                    } else {
                        triangles.extend(tessellate_aperture_rect(
                            Point2::new(cx + center.x, cy + center.y),
                            w,
                            h,
                        ));
                    }
                }
                MacroContent::Outline(ol) => {
                    let exposure = eval_macro_bool(&ol.exposure, &vars);
                    if exposure.is_none() || exposure == Some(false) {
                        continue;
                    }
                    let angle = eval_decimal(&ol.angle, &vars);
                    let pts: Vec<Point2<f64>> = if angle.abs() > 1e-10 {
                        let rad = angle.to_radians();
                        let cos = rad.cos();
                        let sin = rad.sin();
                        ol.points
                            .iter()
                            .map(|(mx, my)| {
                                let x = eval_decimal(mx, &vars) * self.scale_mm;
                                let y = eval_decimal(my, &vars) * self.scale_mm;
                                Point2::new(
                                    x * cos - y * sin + center.x,
                                    x * sin + y * cos + center.y,
                                )
                            })
                            .collect()
                    } else {
                        ol.points
                            .iter()
                            .map(|(mx, my)| {
                                let x = eval_decimal(mx, &vars) * self.scale_mm + center.x;
                                let y = eval_decimal(my, &vars) * self.scale_mm + center.y;
                                Point2::new(x, y)
                            })
                            .collect()
                    };
                    if pts.len() >= 3 {
                        triangles.extend(tessellate_polygon(&pts));
                    }
                }
                MacroContent::Polygon(pol) => {
                    let exposure = eval_macro_bool(&pol.exposure, &vars);
                    if exposure.is_none() || exposure == Some(false) {
                        continue;
                    }
                    let diameter = eval_decimal(&pol.diameter, &vars) * self.scale_mm;
                    let vertices = eval_macro_int(&pol.vertices, &vars);
                    let rotation = eval_decimal(&pol.angle, &vars);
                    let cx_val = eval_decimal(&pol.center.0, &vars) * self.scale_mm + center.x;
                    let cy_val = eval_decimal(&pol.center.1, &vars) * self.scale_mm + center.y;
                    triangles.extend(tessellate_aperture_polygon(
                        Point2::new(cx_val, cy_val),
                        diameter,
                        vertices,
                        rotation,
                    ));
                }
                _ => {}
            }
        }

        triangles
    }

    fn emit_thick_line(&mut self, start: Point2<f64>, end: Point2<f64>) {
        let width = self.current_aperture_width();
        if width > 0.0 {
            self.triangles
                .extend(tessellate_thick_line(start, end, width));
        }
    }

    fn emit_thick_polyline(&mut self, pts: &[Point2<f64>]) {
        if pts.len() < 2 {
            return;
        }
        let width = self.current_aperture_width();
        if width <= 0.0 {
            return;
        }
        for i in 0..(pts.len() - 1) {
            self.triangles
                .extend(tessellate_thick_line(pts[i], pts[i + 1], width));
        }
    }

    fn current_aperture_width(&self) -> f64 {
        self.current_aperture
            .and_then(|code| {
                self.apertures.get(&code).map(|ap| match ap {
                    Aperture::Circle(c) => c.diameter * self.scale_mm,
                    Aperture::Rectangle(r) => r.x.min(r.y) * self.scale_mm,
                    Aperture::Obround(r) => r.x.min(r.y) * self.scale_mm,
                    Aperture::Polygon(p) => p.diameter * self.scale_mm,
                    Aperture::Macro(_, _) => 0.15,
                })
            })
            .unwrap_or(0.15)
    }
}

pub fn eval_decimal(dec: &MacroDecimal, vars: &[f64]) -> f64 {
    match dec {
        MacroDecimal::Value(v) => *v,
        MacroDecimal::Variable(idx) => {
            let i = *idx as usize;
            if i >= 1 && i.saturating_sub(1) < vars.len() {
                vars[i.saturating_sub(1)]
            } else {
                0.0
            }
        }
        MacroDecimal::Expression(s) => eval_expression_str(s, vars),
    }
}

pub fn eval_macro_bool(b: &MacroBoolean, vars: &[f64]) -> Option<bool> {
    match b {
        MacroBoolean::Value(v) => Some(*v),
        MacroBoolean::Variable(idx) => {
            let i = *idx as usize;
            if i >= 1 && i.saturating_sub(1) < vars.len() {
                Some(vars[i.saturating_sub(1)] != 0.0)
            } else {
                None
            }
        }
        MacroBoolean::Expression(s) => {
            let v = eval_expression_str(s, vars);
            Some(v != 0.0)
        }
    }
}

pub fn eval_macro_int(val: &MacroInteger, vars: &[f64]) -> u32 {
    match val {
        MacroInteger::Value(v) => *v,
        MacroInteger::Variable(idx) => {
            let i = *idx as usize;
            if i >= 1 && i.saturating_sub(1) < vars.len() {
                vars[i.saturating_sub(1)] as u32
            } else {
                0
            }
        }
        MacroInteger::Expression(s) => eval_expression_str(s, vars) as u32,
    }
}

pub fn parse_to_shapes(source: &str) -> Result<Vec<Triangle>, ConversionError> {
    use std::io::{BufReader, Cursor};

    let cursor = Cursor::new(source);
    let reader = BufReader::new(cursor);

    let doc = match gerber_parser::parse(reader) {
        Ok(doc) => doc,
        Err((doc, _err)) => doc,
    };

    let commands = doc.into_commands();
    let mut state = GerberState::new();
    state.process_commands(&commands);
    Ok(state.triangles)
}

pub fn compute_bounding_box(triangles: &[Triangle]) -> BoundingBox {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for tri in triangles {
        for p in [&tri.v0, &tri.v1, &tri.v2] {
            if p.x < min_x { min_x = p.x; }
            if p.y < min_y { min_y = p.y; }
            if p.x > max_x { max_x = p.x; }
            if p.y > max_y { max_y = p.y; }
        }
    }
    (min_x, min_y, max_x, max_y)
}

pub struct RenderLayout {
    pub width: u32,
    pub height: u32,
    pub ppm: f64,
    pub offset: Vector2<f64>,
}

pub fn compute_render_layout(
    bbox: BoundingBox,
    settings: &ConversionSettings,
) -> Result<RenderLayout, ConversionError> {
    let margin_mm = 0.82f64;
    let ppm = settings.pixels_per_mm as f64;
    let (min_x, min_y, max_x, max_y) = bbox;
    let board_w = max_x - min_x;
    let board_h = max_y - min_y;
    let width = ((board_w + margin_mm * 2.0) * ppm).round() as u32;
    let height = ((board_h + margin_mm * 2.0) * ppm).round() as u32;
    let width = width.max(1);
    let height = height.max(1);
    let pixels = u64::from(width) * u64::from(height);
    if pixels > settings.max_render_pixels {
        return Err(ConversionError::RenderTooLarge {
            width, height, pixels,
            max_pixels: settings.max_render_pixels,
        });
    }
    let offset = Vector2::new(-min_x + margin_mm, -min_y + margin_mm);
    Ok(RenderLayout { width, height, ppm, offset })
}

pub fn render_triangles_to_image(
    image: &mut GrayImage,
    triangles: &[Triangle],
    layout: &RenderLayout,
    fill_color: u8,
) {
    rasterize_triangles(
        image, triangles, layout.width, layout.height,
        layout.ppm, layout.offset, fill_color,
    );
}

pub fn invert_image_gray(image: &mut GrayImage) {
    for pixel in image.pixels_mut() {
        pixel[0] = 255 - pixel[0];
    }
}

pub fn flood_fill_holes(image: &GrayImage) -> GrayImage {
    let (w, h) = image.dimensions();
    let mut visited = vec![false; (w * h) as usize];
    let mut stack = Vec::new();

    for x in 0..w {
        if image.get_pixel(x, 0)[0] < 128 { stack.push((x, 0)); }
        if image.get_pixel(x, h - 1)[0] < 128 { stack.push((x, h - 1)); }
    }
    for y in 0..h {
        if image.get_pixel(0, y)[0] < 128 { stack.push((0, y)); }
        if image.get_pixel(w - 1, y)[0] < 128 { stack.push((w - 1, y)); }
    }

    while let Some((cx, cy)) = stack.pop() {
        let idx = (cy * w + cx) as usize;
        if visited[idx] { continue; }
        visited[idx] = true;
        for (dx, dy) in &[(-1i32,0i32),(1,0),(0,-1),(0,1)] {
            let nx = cx as i32 + dx;
            let ny = cy as i32 + dy;
            if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                let nidx = (ny as u32 * w + nx as u32) as usize;
                if !visited[nidx] && image.get_pixel(nx as u32, ny as u32)[0] < 128 {
                    stack.push((nx as u32, ny as u32));
                }
            }
        }
    }

    let mut out = image.clone();
    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            if !visited[idx] {
                out.put_pixel(x, y, Luma([255]));
            }
        }
    }
    out
}

pub fn render_triangles_to_png(
    triangles: &[Triangle],
    output_path: &Path,
    settings: &ConversionSettings,
) -> Result<PngRenderResult, ConversionError> {
    if triangles.is_empty() {
        return Err(ConversionError::NoRenderableGerber);
    }
    let bbox = compute_bounding_box(triangles);
    let layout = compute_render_layout(bbox, settings)?;
    let mut image = GrayImage::from_pixel(layout.width, layout.height, Luma([0]));
    render_triangles_to_image(&mut image, triangles, &layout, 255);
    let dark_pixels = image.pixels().filter(|p| p[0] < settings.threshold).count();
    image.save(output_path)?;
    Ok(PngRenderResult {
        path: output_path.to_path_buf(),
        width: layout.width,
        height: layout.height,
        dark_pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_simple_decimal() {
        assert_eq!(eval_decimal(&MacroDecimal::Value(3.14), &[]), 3.14);
    }

    #[test]
    fn eval_variable_decimal() {
        assert_eq!(eval_decimal(&MacroDecimal::Variable(1), &[42.0]), 42.0);
    }

    #[test]
    fn eval_expression_addition() {
        let e = MacroDecimal::Expression("$1+$2".to_string());
        assert_eq!(eval_decimal(&e, &[10.0, 20.0]), 30.0);
    }

    #[test]
    fn eval_expression_complex() {
        let e = MacroDecimal::Expression("$1*($2+$3)/2".to_string());
        assert_eq!(eval_decimal(&e, &[2.0, 10.0, 20.0]), 30.0);
    }

    #[test]
    fn eval_bool_expression() {
        let b = MacroBoolean::Expression("$1".to_string());
        assert_eq!(eval_macro_bool(&b, &[5.0, 3.0]), Some(true));
        assert_eq!(eval_macro_bool(&b, &[0.0]), Some(false));
    }

    #[test]
    fn tessellate_circle_returns_triangles() {
        let tris = tessellate_aperture_circle(Point2::new(0.0, 0.0), 1.0);
        assert!(!tris.is_empty());
    }

    #[test]
    fn tessellate_rect_returns_triangles() {
        let tris = tessellate_aperture_rect(Point2::new(0.0, 0.0), 2.0, 1.0);
        assert!(!tris.is_empty());
    }

    #[test]
    fn tessellate_oval_returns_triangles() {
        let tris = tessellate_aperture_oval(Point2::new(0.0, 0.0), 2.0, 1.0);
        assert!(!tris.is_empty());
    }

    #[test]
    fn tessellate_thick_line_returns_triangles() {
        let tris = tessellate_thick_line(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0), 0.5);
        assert!(!tris.is_empty());
    }

    #[test]
    fn tessellate_polygon_returns_triangles() {
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 0.0),
            Point2::new(10.0, 10.0),
            Point2::new(0.0, 10.0),
        ];
        let tris = tessellate_polygon(&pts);
        assert!(!tris.is_empty());
    }

    #[test]
    fn rasterize_triangles_produces_dark_pixels() {
        let width = 100u32;
        let height = 100u32;
        let mut image = image::GrayImage::from_pixel(width, height, image::Luma([0]));
        let tris = tessellate_aperture_circle(Point2::new(50.0, 50.0), 20.0);
        let offset = nalgebra::Vector2::new(0.0, 0.0);
        rasterize_triangles(&mut image, &tris, width, height, 1.0, offset, 255);
        let dark = image.pixels().filter(|p| p[0] > 0).count();
        assert!(dark > 0);
    }

    #[test]
    fn interpolate_arc_produces_points() {
        let pts = interpolate_arc(
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 0.0),
            0.0,
            10.0,
            false,
            false,
            64,
        );
        assert!(pts.len() >= 2);
    }

    #[test]
    fn simple_gerber_line_produces_triangles() {
        let gerber = "%FSLAX24Y24*%\n%MOMM*%\n%ADD10C,0.300*%\nD10*\nX000000Y000000D02*\nX010000Y000000D01*\nM02*\n";
        let tris = parse_to_shapes(gerber).unwrap();
        assert!(!tris.is_empty(), "expected triangles from a simple line");
    }

    #[test]
    fn simple_gerber_flash_produces_triangles() {
        let gerber = "%FSLAX24Y24*%\n%MOMM*%\n%ADD10C,0.300*%\nD10*\nX050000Y050000D03*\nM02*\n";
        let tris = parse_to_shapes(gerber).unwrap();
        assert!(!tris.is_empty(), "expected triangles from a flash");
    }

    #[test]
    fn parse_to_shapes_empty_on_invalid_gerber() {
        let tris = parse_to_shapes("not gerber data").unwrap();
        assert!(tris.is_empty());
    }

    #[test]
    fn render_triangles_creates_png() {
        let dir = tempfile::tempdir().unwrap();
        let png_path = dir.path().join("render_test.png");
        let tris = tessellate_aperture_circle(Point2::new(10.0, 10.0), 5.0);
        let settings = ConversionSettings {
            pixels_per_mm: 10.0,
            ..ConversionSettings::default()
        };
        let result = render_triangles_to_png(&tris, &png_path, &settings).unwrap();
        assert!(png_path.exists());
        assert!(result.dark_pixels > 0);
    }

    #[test]
    fn centerline_macro_rotation_applied() {
        // 2x1mm rectangle centered at (0,0) rotated 90° via aperture macro
        // Unrotated bounding box: x in [-1, 1], y in [-0.5, 0.5]
        // Rotated 90° around origin: x in [-0.5, 0.5], y in [-1, 1]
        // Flashed at (5,5): x in [4.5, 5.5], y in [4, 6]
        let gerber = [
            "%FSLAX24Y24*%",
            "%MOMM*%",
            "%AMRECT*",
            "21,1,2,1,0,0,90*",
            "%",
            "%ADD10RECT*%",
            "D10*",
            "X050000Y050000D03*",
            "M02*",
        ].join("\n");
        let tris = parse_to_shapes(&gerber).unwrap();
        assert!(!tris.is_empty(), "expected triangles from rotated CenterLine macro");
        let min_x = tris.iter().map(|t| t.v0.x.min(t.v1.x).min(t.v2.x)).reduce(f64::min).unwrap();
        let max_x = tris.iter().map(|t| t.v0.x.max(t.v1.x).max(t.v2.x)).reduce(f64::max).unwrap();
        let min_y = tris.iter().map(|t| t.v0.y.min(t.v1.y).min(t.v2.y)).reduce(f64::min).unwrap();
        let max_y = tris.iter().map(|t| t.v0.y.max(t.v1.y).max(t.v2.y)).reduce(f64::max).unwrap();
        // Rotated 90°: width should be ~1 (original height), height ~2 (original width)
        let w = max_x - min_x;
        let h = max_y - min_y;
        assert!(w < 1.5, "rotated width should be ~1, got {w}");
        assert!(h > 1.5, "rotated height should be ~2, got {h}");
    }

    #[test]
    fn centerline_macro_rotation_zero() {
        // Without rotation the original orientation is preserved
        let gerber = [
            "%FSLAX24Y24*%",
            "%MOMM*%",
            "%AMRECT*",
            "21,1,2,1,0,0,0*",
            "%",
            "%ADD10RECT*%",
            "D10*",
            "X050000Y050000D03*",
            "M02*",
        ].join("\n");
        let tris = parse_to_shapes(&gerber).unwrap();
        assert!(!tris.is_empty());
        let max_x = tris.iter().flat_map(|t| [t.v0.x, t.v1.x, t.v2.x]).reduce(f64::max).unwrap();
        let min_x = tris.iter().flat_map(|t| [t.v0.x, t.v1.x, t.v2.x]).reduce(f64::min).unwrap();
        let max_y = tris.iter().flat_map(|t| [t.v0.y, t.v1.y, t.v2.y]).reduce(f64::max).unwrap();
        let min_y = tris.iter().flat_map(|t| [t.v0.y, t.v1.y, t.v2.y]).reduce(f64::min).unwrap();
        // Unrotated: width = 2, height = 1
        assert!((max_x - min_x) - 2.0 < 0.01);
        assert!((max_y - min_y) - 1.0 < 0.01);
    }

    #[test]
    fn outline_rotation_swaps_bounds() {
        // 2x1mm rectangle outline, originally width=2 height=1
        // Rotated 90° around macro origin: width becomes 1, height becomes 2
        // Flashed at (5, 5)
        let gerber = [
            "%FSLAX24Y24*%",
            "%MOMM*%",
            "%AMBAR*",
            "4,1,4,0,0,2,0,2,1,0,1,0,0,90*",
            "%",
            "%ADD10BAR*%",
            "D10*",
            "X050000Y050000D03*",
            "M02*",
        ].join("\n");
        let tris = parse_to_shapes(&gerber).unwrap();
        assert!(!tris.is_empty());
        let all_x: Vec<f64> = tris.iter().flat_map(|t| [t.v0.x, t.v1.x, t.v2.x]).collect();
        let all_y: Vec<f64> = tris.iter().flat_map(|t| [t.v0.y, t.v1.y, t.v2.y]).collect();
        let min_x = all_x.iter().cloned().reduce(f64::min).unwrap();
        let max_x = all_x.iter().cloned().reduce(f64::max).unwrap();
        let min_y = all_y.iter().cloned().reduce(f64::min).unwrap();
        let max_y = all_y.iter().cloned().reduce(f64::max).unwrap();
        let w = max_x - min_x;
        let h = max_y - min_y;
        // After 90° rotation: w ≈ 1, h ≈ 2
        assert!(w < 1.5, "rotated outline width ~1, got {w}");
        assert!(h > 1.5, "rotated outline height ~2, got {h}");
    }

    #[test]
    fn outline_macro_basic_parses() {
        // Outline macro — 4-vertex rectangle, closed (last=first)
        // Data: 4,exposure,4, x1,y1, x2,y2, x3,y3, x4,y4, x5=x1,y5=y1, angle
        // gerber_parser reads num_vertices+1 points (uses <=), so 4+1=5 points
        let gerber = [
            "%FSLAX24Y24*%",
            "%MOMM*%",
            "%AMBOX*",
            "4,1,4,0,0,10,0,10,10,0,10,0,0,0*",
            "%",
            "%ADD10BOX*%",
            "D10*",
            "X050000Y050000D03*",
            "M02*",
        ].join("\n");
        let tris = parse_to_shapes(&gerber).unwrap();
        assert!(!tris.is_empty(), "expected triangles from Outline macro");
    }

    #[test]
    fn render_empty_triangles_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let png_path = dir.path().join("empty.png");
        let settings = ConversionSettings::default();
        let result = render_triangles_to_png(&[], &png_path, &settings);
        assert!(result.is_err());
    }

    #[test]
    fn compute_bounding_box_returns_correct_extents() {
        let tris = vec![
            Triangle { v0: Point2::new(0.0, 0.0), v1: Point2::new(10.0, 0.0), v2: Point2::new(10.0, 10.0) },
        ];
        let bbox = compute_bounding_box(&tris);
        assert_eq!(bbox, (0.0, 0.0, 10.0, 10.0));
    }

    #[test]
    fn invert_image_gray_flips_pixels() {
        let mut img = GrayImage::from_pixel(2, 1, Luma([0]));
        img.put_pixel(1, 0, Luma([255]));
        invert_image_gray(&mut img);
        assert_eq!(img.get_pixel(0, 0)[0], 255);
        assert_eq!(img.get_pixel(1, 0)[0], 0);
    }

    #[test]
    fn flood_fill_holes_fills_enclosed_region() {
        let mut img = GrayImage::from_pixel(5, 5, Luma([0]));
        for x in 1..4 { img.put_pixel(x, 1, Luma([255])); }
        for x in 1..4 { img.put_pixel(x, 3, Luma([255])); }
        img.put_pixel(1, 2, Luma([255]));
        img.put_pixel(3, 2, Luma([255]));
        let filled = flood_fill_holes(&img);
        assert_eq!(filled.get_pixel(2, 2)[0], 255, "enclosed black pixel should become white");
        assert_eq!(filled.get_pixel(0, 0)[0], 0, "border-connected black should stay black");
    }

    #[test]
    fn compute_render_layout_rejects_oversized() {
        let bbox = (0.0, 0.0, 1000.0, 1000.0);
        let settings = ConversionSettings {
            pixels_per_mm: 100.0,
            max_render_pixels: 10_000,
            ..ConversionSettings::default()
        };
        let result = compute_render_layout(bbox, &settings);
        assert!(result.is_err());
    }
}
