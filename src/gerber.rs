use image::{GrayImage, Luma};
use lyon::path::Path as LPath;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers,
};
use lyon::math::Point as LPoint;
use nalgebra::{Point2, Vector2};

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

pub enum LayerType {
    Copper,
    Profile,
    Drill,
}

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
        _radius: f64,
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
        _radius: f64,
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
    let mut start_cap = half_circle_ccw(start, nx, ny, half, n);
    start_cap.push(Point2::new(start.x + nx, start.y + ny));

    let mut end_cap = half_circle_cw(end, nx, ny, half, n);
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
