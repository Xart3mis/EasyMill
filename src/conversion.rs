use image::{GrayImage, Luma};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use tracing::{debug, info, warn};
use zip::ZipArchive;

pub const DEFAULT_DPI: f32 = 1000.0;
pub const MM_PER_INCH: f32 = 25.4;
pub const DEFAULT_PIXELS_PER_MM: f32 = DEFAULT_DPI / MM_PER_INCH;
pub const DEFAULT_MAX_RENDER_PIXELS: u64 = 25_000_000;

#[derive(Debug, Clone)]
pub struct ConversionSettings {
    pub pixels_per_mm: f32,
    pub max_render_pixels: u64,
    pub threshold: u8,
    pub safe_z_mm: f32,
    pub cut_z_mm: f32,
    pub feed_rate_mm_min: f32,
    pub plunge_rate_mm_min: f32,
    pub spindle_speed_rpm: u32,
    pub tool_diameter_mm: f32,
    pub offset_number: u32,
    pub offset_stepover: f32,
}

impl Default for ConversionSettings {
    fn default() -> Self {
        Self {
            pixels_per_mm: DEFAULT_PIXELS_PER_MM,
            max_render_pixels: DEFAULT_MAX_RENDER_PIXELS,
            threshold: 128,
            safe_z_mm: 2.0,
            cut_z_mm: -0.1,
            feed_rate_mm_min: 300.0,
            plunge_rate_mm_min: 120.0,
            spindle_speed_rpm: 12_000,
            tool_diameter_mm: 0.4,
            offset_number: 4,
            offset_stepover: 0.5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PngRenderResult {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub dark_pixels: usize,
}

#[derive(Debug)]
pub enum ConversionError {
    Io(std::io::Error),
    Image(image::ImageError),
    Zip(zip::result::ZipError),
    EmptyInput,
    NoRenderableGerber,
    RenderTooLarge {
        width: u32,
        height: u32,
        pixels: u64,
        max_pixels: u64,
    },
}

impl std::fmt::Display for ConversionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(formatter, "I/O error: {err}"),
            Self::Image(err) => write!(formatter, "image error: {err}"),
            Self::Zip(err) => write!(formatter, "ZIP error: {err}"),
            Self::EmptyInput => write!(formatter, "no input files were provided"),
            Self::NoRenderableGerber => write!(formatter, "no supported Gerber geometry was found"),
            Self::RenderTooLarge {
                width,
                height,
                pixels,
                max_pixels,
            } => write!(
                formatter,
                "render would be too large at the selected resolution ({width} x {height}, {pixels} pixels; limit is {max_pixels})"
            ),
        }
    }
}

impl std::error::Error for ConversionError {}

impl From<std::io::Error> for ConversionError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<image::ImageError> for ConversionError {
    fn from(value: image::ImageError) -> Self {
        Self::Image(value)
    }
}

impl From<zip::result::ZipError> for ConversionError {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Zip(value)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApertureShape {
    Circle,
    Rectangle,
    Oval,
    Polygon,
}

#[derive(Debug, Clone)]
pub enum Primitive {
    Segment {
        start: Point,
        end: Point,
        diameter_mm: f32,
    },
    Flash {
        center: Point,
        shape: ApertureShape,
        dimensions: Vec<f32>,
    },
    Polygon {
        points: Vec<Point>,
    },
}

#[derive(Debug, Clone)]
struct Aperture {
    shape: ApertureShape,
    dimensions: Vec<f32>,
}

pub fn gerber_inputs_to_png(
    inputs: &[PathBuf],
    output_path: &Path,
    settings: ConversionSettings,
) -> Result<PngRenderResult, ConversionError> {
    info!(
        input_count = inputs.len(),
        output = %output_path.display(),
        pixels_per_mm = settings.pixels_per_mm,
        threshold = settings.threshold,
        "starting gerber to png conversion"
    );

    if inputs.is_empty() {
        warn!("gerber to png conversion received no inputs");
        return Err(ConversionError::EmptyInput);
    }

    let mut gerber_sources = Vec::new();
    for input in inputs {
        if is_zip(input) {
            info!(path = %input.display(), "reading gerber zip archive");
            gerber_sources.extend(read_zip_gerbers(input)?);
        } else {
            let source = fs::read_to_string(input)?;
            debug!(
                path = %input.display(),
                bytes = source.len(),
                "read gerber source file"
            );
            gerber_sources.push(source);
        }
    }

    let primitives = gerber_sources
        .iter()
        .flat_map(|source| parse_gerber_or_drill(source))
        .collect::<Vec<_>>();

    info!(
        source_count = gerber_sources.len(),
        primitive_count = primitives.len(),
        "parsed gerber inputs"
    );

    let result = render_primitives_to_png(&primitives, output_path, settings)?;
    info!(
        output = %result.path.display(),
        width = result.width,
        height = result.height,
        dark_pixels = result.dark_pixels,
        "finished gerber to png conversion"
    );
    Ok(result)
}

#[derive(Debug, Clone)]
pub struct GcodeResult {
    pub gcode: String,
    pub estimated_time_secs: f32,
    pub cut_distance_mm: f32,
    pub width_mm: f32,
    pub height_mm: f32,
    pub paths: Vec<Vec<[f32; 2]>>,
}

pub fn png_to_gcode(
    png_path: &Path,
    settings: ConversionSettings,
) -> Result<GcodeResult, ConversionError> {
    info!(
        input = %png_path.display(),
        pixels_per_mm = settings.pixels_per_mm,
        tool_diameter_mm = settings.tool_diameter_mm,
        offset_number = settings.offset_number,
        offset_stepover = settings.offset_stepover,
        feed_rate_mm_min = settings.feed_rate_mm_min,
        plunge_rate_mm_min = settings.plunge_rate_mm_min,
        spindle_speed_rpm = settings.spindle_speed_rpm,
        "starting png to gcode conversion"
    );

    let image = image::open(png_path)?.to_luma8();

    let nx = image.width() as usize;
    let ny = image.height() as usize;

    info!(width = nx, height = ny, "loaded raster image");

    // 1. Threshold input image
    let mut binary_image = vec![false; nx * ny];
    let mut material_pixels = 0usize;
    for y in 0..ny {
        for x in 0..nx {
            let pixel_val = image.get_pixel(x as u32, (ny - 1 - y) as u32)[0];
            binary_image[y * nx + x] = pixel_val > settings.threshold;
            if binary_image[y * nx + x] {
                material_pixels += 1;
            }
        }
    }

    info!(
        material_pixels,
        total_pixels = nx * ny,
        threshold = settings.threshold,
        "thresholded raster image"
    );

    // 2. Compute Euclidean distance transform
    let distances = compute_distance_transform(&binary_image, nx, ny);
    debug!("computed distance transform");

    // 3. Offset and vectorize loop
    let tool_dia_pixels = settings.tool_diameter_mm * settings.pixels_per_mm;
    let stepover = settings.offset_stepover;
    let number = settings.offset_number;

    let mut accumulated_paths = Vec::new();
    let mut offset_count = 0;

    loop {
        if number > 0 && offset_count >= number {
            break;
        }

        let offset_val = (0.5 + offset_count as f32 * stepover) * tool_dia_pixels;

        let binary = threshold_distances(&distances, offset_val, nx, ny);
        let edges = detect_edges(&binary, nx, ny);
        let mut oriented = orient_edges(&edges, nx, ny);
        let paths = vectorize_oriented_edges(&mut oriented, nx, ny, 1.0, true);

        debug!(
            offset_index = offset_count,
            offset_pixels = offset_val,
            path_count = paths.len(),
            "vectorized offset pass"
        );

        if paths.is_empty() {
            break;
        }

        accumulate_paths(
            &mut accumulated_paths,
            paths,
            false, // conventional = false (default climb)
            true,  // sort = true
            true,  // forward = true
        );

        offset_count += 1;
    }

    info!(
        offset_passes = offset_count,
        path_count = accumulated_paths.len(),
        "completed offset vectorization"
    );

    // 4. Merge paths
    let dmerge = settings.pixels_per_mm * settings.tool_diameter_mm; // default merge = 1.0 diameter
    merge_paths(&mut accumulated_paths, dmerge);

    debug!(
        merge_distance_pixels = dmerge,
        path_count = accumulated_paths.len(),
        "merged adjacent toolpaths"
    );

    // 5. Add depth
    let cut_mm = settings.cut_z_mm.abs();
    let max_mm = cut_mm;
    let path_with_depth = add_depth(&accumulated_paths, cut_mm, max_mm, settings.pixels_per_mm);

    debug!(
        path_count = path_with_depth.len(),
        cut_depth_mm = settings.cut_z_mm,
        "added toolpath depth"
    );

    // 6. Generate G-code and statistics
    let gcode_stats = generate_gcode(&path_with_depth, image.width(), image.height(), &settings);

    let width_mm = image.width() as f32 / settings.pixels_per_mm;
    let height_mm = image.height() as f32 / settings.pixels_per_mm;

    // Collect 2D toolpath coords — same scale factor that generate_gcode uses.
    let nx_f = image.width() as f32;
    let path_scale = nx_f / (settings.pixels_per_mm * (nx_f - 1.0));
    let paths_2d: Vec<Vec<[f32; 2]>> = path_with_depth
        .iter()
        .map(|seg| seg.iter().map(|pt| [pt[0] * path_scale, pt[1] * path_scale]).collect())
        .collect();

    let result = GcodeResult {
        gcode: gcode_stats.gcode,
        estimated_time_secs: gcode_stats.estimated_time_secs,
        cut_distance_mm: gcode_stats.cut_distance_mm,
        width_mm,
        height_mm,
        paths: paths_2d,
    };

    info!(
        gcode_bytes = result.gcode.len(),
        estimated_time_secs = result.estimated_time_secs,
        cut_distance_mm = result.cut_distance_mm,
        width_mm = result.width_mm,
        height_mm = result.height_mm,
        "finished png to gcode conversion"
    );

    Ok(result)
}

// --- Modsproject Milling G-code Generation Pipeline Helpers ---

const STATE_BOUNDARY: u8 = 1;
const STATE_INTERIOR: u8 = 2;
const STATE_EXTERIOR: u8 = 3;

#[derive(Clone, Copy, Default)]
struct OrientedPixel {
    northsouth: u8,
    eastwest: u8,
    startstop: u8,
}

fn compute_distance_transform(binary_image: &[bool], nx: usize, ny: usize) -> Vec<f32> {
    let mut g = vec![0u32; nx * ny];
    
    for y in 0..ny {
        let mut closest = -(nx as isize);
        for x in 0..nx {
            if binary_image[y * nx + x] {
                g[y * nx + x] = 0;
                closest = x as isize;
            } else {
                g[y * nx + x] = (x as isize - closest) as u32;
            }
        }
        let mut closest = 2 * nx as isize;
        for x in (0..nx).rev() {
            if binary_image[y * nx + x] {
                closest = x as isize;
            } else {
                let d = (closest - x as isize) as u32;
                if d < g[y * nx + x] {
                    g[y * nx + x] = d;
                }
            }
        }
    }
    
    let distance = |g: &[u32], x: usize, y: usize, i: usize| -> f32 {
        let dy = y as f32 - i as f32;
        let gx = g[i * nx + x] as f32;
        dy * dy + gx * gx
    };
    
    let intersection = |g: &[u32], x: usize, y0: usize, y1: usize| -> f32 {
        let g0 = g[y0 * nx + x] as f32;
        let g1 = g[y1 * nx + x] as f32;
        let y0_f = y0 as f32;
        let y1_f = y1 as f32;
        (g0 * g0 - g1 * g1 + y0_f * y0_f - y1_f * y1_f) / (2.0 * (y0_f - y1_f))
    };
    
    let mut output = vec![0.0f32; nx * ny];
    let mut starts = vec![0usize; ny];
    let mut minimums = vec![0usize; ny];
    
    for x in 0..nx {
        let mut segment = 0isize;
        starts[0] = 0;
        minimums[0] = 0;
        
        for y in 1..ny {
            while segment >= 0 && distance(&g, x, starts[segment as usize], minimums[segment as usize]) > distance(&g, x, starts[segment as usize], y) {
                segment -= 1;
            }
            if segment < 0 {
                segment = 0;
                minimums[0] = y;
            } else {
                let newstart = 1.0 + intersection(&g, x, minimums[segment as usize], y);
                if newstart < ny as f32 {
                    segment += 1;
                    minimums[segment as usize] = y;
                    starts[segment as usize] = newstart as usize;
                }
            }
        }
        
        for y in (0..ny).rev() {
            let d = distance(&g, x, y, minimums[segment as usize]).sqrt();
            output[y * nx + x] = d;
            if y == starts[segment as usize] {
                segment -= 1;
            }
        }
    }
    
    output
}

fn threshold_distances(distances: &[f32], offset: f32, _nx: usize, _ny: usize) -> Vec<bool> {
    distances.iter().map(|&d| d <= offset).collect()
}

fn detect_edges(binary_image: &[bool], nx: usize, ny: usize) -> Vec<u8> {
    let mut states = vec![0u8; nx * ny];
    for y in 0..ny {
        for x in 0..nx {
            let val = binary_image[y * nx + x];
            let mut is_boundary = false;
            
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx_coord = x as isize + dx;
                    let ny_coord = y as isize + dy;
                    if nx_coord >= 0 && nx_coord < nx as isize && ny_coord >= 0 && ny_coord < ny as isize {
                        let neighbor_val = binary_image[ny_coord as usize * nx + nx_coord as usize];
                        if neighbor_val != val {
                            is_boundary = true;
                            break;
                        }
                    }
                }
                if is_boundary {
                    break;
                }
            }
            
            states[y * nx + x] = if is_boundary {
                STATE_BOUNDARY
            } else if val {
                STATE_INTERIOR
            } else {
                STATE_EXTERIOR
            };
        }
    }
    states
}

fn orient_edges(states: &[u8], nx: usize, ny: usize) -> Vec<OrientedPixel> {
    let mut oriented = vec![OrientedPixel::default(); nx * ny];
    
    let is_boundary = |x: usize, y: usize| -> bool {
        states[y * nx + x] == STATE_BOUNDARY
    };
    let is_interior = |x: usize, y: usize| -> bool {
        states[y * nx + x] == STATE_INTERIOR
    };
    
    let north = 128u8;
    let south = 64u8;
    let east = 128u8;
    let west = 64u8;
    let start = 128u8;
    let stop = 64u8;
    
    for y in 1..(ny - 1) {
        for x in 1..(nx - 1) {
            if is_boundary(x, y) {
                let mut ns = 0u8;
                let mut ew = 0u8;
                
                if is_boundary(x, y - 1) && (is_interior(x + 1, y) || is_interior(x + 1, y - 1)) {
                    ns |= north;
                }
                if is_boundary(x, y + 1) && (is_interior(x - 1, y) || is_interior(x - 1, y + 1)) {
                    ns |= south;
                }
                if is_boundary(x + 1, y) && (is_interior(x, y + 1) || is_interior(x + 1, y + 1)) {
                    ew |= east;
                }
                if is_boundary(x - 1, y) && (is_interior(x, y - 1) || is_interior(x - 1, y - 1)) {
                    ew |= west;
                }
                
                oriented[y * nx + x].northsouth = ns;
                oriented[y * nx + x].eastwest = ew;
            }
        }
    }
    
    {
        let y = ny - 1;
        for x in 1..(nx - 1) {
            if is_boundary(x, y) {
                if is_boundary(x, y - 1) && is_interior(x + 1, y) {
                    oriented[y * nx + x].northsouth |= north;
                    oriented[y * nx + x].startstop |= start;
                }
                if is_interior(x - 1, y) {
                    oriented[y * nx + x].startstop |= stop;
                }
            }
        }
    }
    
    {
        let y = 0;
        for x in 1..(nx - 1) {
            if is_boundary(x, y) {
                if is_interior(x + 1, y) {
                    oriented[y * nx + x].startstop |= stop;
                }
                if is_boundary(x, y + 1) && is_interior(x - 1, y) {
                    oriented[y * nx + x].northsouth |= south;
                    oriented[y * nx + x].startstop |= start;
                }
            }
        }
    }
    
    {
        let x = 0;
        for y in 1..(ny - 1) {
            if is_boundary(x, y) {
                if is_boundary(x + 1, y) && is_interior(x, y + 1) {
                    oriented[y * nx + x].eastwest |= east;
                    oriented[y * nx + x].startstop |= start;
                }
                if is_interior(x, y - 1) {
                    oriented[y * nx + x].startstop |= stop;
                }
            }
        }
    }
    
    {
        let x = nx - 1;
        for y in 1..(ny - 1) {
            if is_boundary(x, y) {
                if is_interior(x, y + 1) {
                    oriented[y * nx + x].startstop |= stop;
                }
                if is_boundary(x - 1, y) && is_interior(x, y - 1) {
                    oriented[y * nx + x].eastwest |= west;
                    oriented[y * nx + x].startstop |= start;
                }
            }
        }
    }
    
    oriented
}

fn vectorize_oriented_edges(
    oriented: &mut [OrientedPixel],
    nx: usize,
    ny: usize,
    error: f32,
    sort: bool,
) -> Vec<Vec<[f32; 2]>> {
    let mut raw_paths = Vec::new();
    
    let north = 128u8;
    let south = 64u8;
    let east = 128u8;
    let west = 64u8;
    
    let mut follow_edges = |x_start: usize, y_start: usize, oriented_buf: &mut [OrientedPixel]| {
        let mut x = x_start;
        let mut y = y_start;
        if oriented_buf[y * nx + x].northsouth != 0 || oriented_buf[y * nx + x].eastwest != 0 {
            let mut segment = vec![[x as f32, y as f32]];
            loop {
                let idx = y * nx + x;
                if oriented_buf[idx].northsouth & north != 0 {
                    oriented_buf[idx].northsouth &= !north;
                    y += 1;
                    segment.push([x as f32, y as f32]);
                } else if oriented_buf[idx].northsouth & south != 0 {
                    oriented_buf[idx].northsouth &= !south;
                    y -= 1;
                    segment.push([x as f32, y as f32]);
                } else if oriented_buf[idx].eastwest & east != 0 {
                    oriented_buf[idx].eastwest &= !east;
                    x += 1;
                    segment.push([x as f32, y as f32]);
                } else if oriented_buf[idx].eastwest & west != 0 {
                    oriented_buf[idx].eastwest &= !west;
                    x -= 1;
                    segment.push([x as f32, y as f32]);
                } else {
                    break;
                }
            }
            raw_paths.push(segment);
        }
    };
    
    for y in 1..(ny - 1) {
        follow_edges(0, y, oriented);
        follow_edges(nx - 1, y, oriented);
    }
    for x in 1..(nx - 1) {
        follow_edges(x, ny - 1, oriented);
        follow_edges(x, 0, oriented);
    }
    
    for y in 1..(ny - 1) {
        for x in 1..(nx - 1) {
            follow_edges(x, y, oriented);
        }
    }
    
    let mut vec_paths = Vec::new();
    for path in &raw_paths {
        if path.len() < 2 {
            continue;
        }
        let x0 = path[0][0];
        let y0 = path[0][1];
        let mut vec_seg = vec![[x0, y0]];
        
        let mut cur_x0 = x0;
        let mut cur_y0 = y0;
        let mut xsum = x0;
        let mut ysum = y0;
        let mut sum = 1;
        
        for pt in 1..path.len() {
            let xold = path[pt - 1][0];
            let yold = path[pt - 1][1];
            let x = path[pt][0];
            let y = path[pt][1];
            
            if sum == 1 {
                xsum += x;
                ysum += y;
                sum += 1;
            } else {
                let xmean = xsum / sum as f32;
                let ymean = ysum / sum as f32;
                let dx = xmean - cur_x0;
                let dy = ymean - cur_y0;
                let d = (dx * dx + dy * dy).sqrt();
                
                if d < 1e-6 {
                    xsum += x;
                    ysum += y;
                    sum += 1;
                } else {
                    let nx_v = dy / d;
                    let ny_v = -dx / d;
                    let l = (nx_v * (x - cur_x0) + ny_v * (y - cur_y0)).abs();
                    
                    if l < error {
                        xsum += x;
                        ysum += y;
                        sum += 1;
                    } else {
                        vec_seg.push([xold, yold]);
                        cur_x0 = xold;
                        cur_y0 = yold;
                        xsum = xold;
                        ysum = yold;
                        sum = 1;
                    }
                }
            }
            if pt == path.len() - 1 {
                vec_seg.push([x, y]);
            }
        }
        vec_paths.push(vec_seg);
    }
    
    if vec_paths.len() > 1 && sort {
        let mut sorted = Vec::new();
        let mut remaining = vec_paths;
        
        let mut dmin = f32::MAX;
        let mut min_idx = 0;
        for (i, p) in remaining.iter().enumerate() {
            let x = p[0][0];
            let y = p[0][1];
            let d = x * x + y * y;
            if d < dmin {
                dmin = d;
                min_idx = i;
            }
        }
        
        let first = remaining.remove(min_idx);
        sorted.push(first);
        
        while !remaining.is_empty() {
            let last_idx = sorted.len() - 1;
            let last_pt_idx = sorted[last_idx].len() - 1;
            let x0 = sorted[last_idx][last_pt_idx][0];
            let y0 = sorted[last_idx][last_pt_idx][1];
            
            let mut dmin = f32::MAX;
            let mut min_idx = 0;
            for (i, p) in remaining.iter().enumerate() {
                let x = p[0][0];
                let y = p[0][1];
                let d = (x - x0) * (x - x0) + (y - y0) * (y - y0);
                if d < dmin {
                    dmin = d;
                    min_idx = i;
                }
            }
            let next_seg = remaining.remove(min_idx);
            sorted.push(next_seg);
        }
        sorted
    } else {
        vec_paths
    }
}

fn accumulate_paths(
    accumulated: &mut Vec<Vec<[f32; 2]>>,
    new_paths: Vec<Vec<[f32; 2]>>,
    conventional: bool,
    sort: bool,
    forward: bool,
) {
    for mut seg in new_paths {
        if conventional {
            seg.reverse();
        }
        if accumulated.is_empty() {
            accumulated.push(seg);
        } else if sort {
            let xnew = seg[0][0];
            let ynew = seg[0][1];
            let mut dmin = f32::MAX;
            let mut segmin = 0;
            for (segold, old_seg) in accumulated.iter().enumerate() {
                let xold = old_seg[0][0];
                let yold = old_seg[0][1];
                let dx = xnew - xold;
                let dy = ynew - yold;
                let d = (dx * dx + dy * dy).sqrt();
                if d < dmin {
                    dmin = d;
                    segmin = segold;
                }
            }
            if forward {
                accumulated.insert(segmin + 1, seg);
            } else {
                accumulated.insert(segmin, seg);
            }
        } else {
            if forward {
                accumulated.push(seg);
            } else {
                accumulated.insert(0, seg);
            }
        }
    }
}

fn merge_paths(path: &mut Vec<Vec<[f32; 2]>>, dmerge: f32) {
    let mut seg = 0;
    while seg < path.len().saturating_sub(1) {
        let last_idx = path[seg].len() - 1;
        let xold = path[seg][last_idx][0];
        let yold = path[seg][last_idx][1];
        let xnew = path[seg + 1][0][0];
        let ynew = path[seg + 1][0][1];
        let dx = xnew - xold;
        let dy = ynew - yold;
        let d = (dx * dx + dy * dy).sqrt();
        if d < dmerge {
            let next_seg = path.remove(seg + 1);
            path[seg].extend(next_seg);
        } else {
            seg += 1;
        }
    }
}

fn add_depth(
    path: &[Vec<[f32; 2]>],
    cut_mm: f32,
    max_mm: f32,
    pixels_per_mm: f32,
) -> Vec<Vec<[f32; 3]>> {
    let mut newpath = Vec::new();
    
    for seg in path {
        if seg.len() < 2 {
            continue;
        }
        let last = seg.len() - 1;
        let is_closed = (seg[0][0] - seg[last][0]).abs() < 1e-3 && (seg[0][1] - seg[last][1]).abs() < 1e-3;
        
        let mut depth = cut_mm;
        if is_closed {
            let mut newseg = Vec::new();
            while depth <= max_mm {
                let idepth = -(pixels_per_mm * depth).round();
                for pt in seg {
                    newseg.push([pt[0], pt[1], idepth]);
                }
                if (depth - max_mm).abs() < 1e-5 {
                    break;
                }
                depth += cut_mm;
                if depth > max_mm {
                    depth = max_mm;
                }
            }
            newpath.push(newseg);
        } else {
            while depth <= max_mm {
                let idepth = -(pixels_per_mm * depth).round();
                let mut newseg = Vec::new();
                for pt in seg {
                    newseg.push([pt[0], pt[1], idepth]);
                }
                newpath.push(newseg);
                if (depth - max_mm).abs() < 1e-5 {
                    break;
                }
                depth += cut_mm;
                if depth > max_mm {
                    depth = max_mm;
                }
            }
        }
    }
    
    newpath
}

struct GcodeStats {
    gcode: String,
    estimated_time_secs: f32,
    cut_distance_mm: f32,
}

fn generate_gcode(
    path: &[Vec<[f32; 3]>],
    width: u32,
    _height: u32,
    settings: &ConversionSettings,
) -> GcodeStats {
    let scale = width as f32 / (settings.pixels_per_mm * (width as f32 - 1.0));
    
    let cut_speed = settings.feed_rate_mm_min;
    let plunge_speed = settings.plunge_rate_mm_min;
    let jog_height = settings.safe_z_mm;
    let finish_height = 20.0f32;
    
    let feed_mm_sec = cut_speed / 60.0;
    let plunge_mm_sec = plunge_speed / 60.0;
    let rapid_mm_sec = 30.0f32; // Assume 1800 mm/min rapid travel
    
    let mut total_time_secs = 0.0f32;
    let mut cut_distance_mm = 0.0f32;
    
    struct ToolXyz {
        xp: Option<f32>,
        yp: Option<f32>,
        zp: Option<f32>,
    }
    impl ToolXyz {
        fn move_to(&mut self, xn: f32, yn: f32, zn: f32) -> f32 {
            let dist = match (self.xp, self.yp, self.zp) {
                (Some(xp), Some(yp), Some(zp)) => {
                    ((xn - xp).powi(2) + (yn - yp).powi(2) + (zn - zp).powi(2)).sqrt()
                }
                _ => 0.0,
            };
            self.xp = Some(xn);
            self.yp = Some(yn);
            self.zp = Some(zn);
            dist
        }
    }
    let mut tool = ToolXyz { xp: None, yp: None, zp: None };

    let mut str = String::new();
    str.push_str("%\n");
    str.push_str("G17\n"); // xy plane
    str.push_str("G21\n"); // mm format
    str.push_str("G40\n"); // cancel tool radius compensation
    str.push_str("G49\n"); // cancel tool length compensation
    str.push_str("G54\n"); // coordinate system 1
    str.push_str("G80\n"); // cancel canned cycles
    str.push_str("G90\n"); // absolute coordinates
    str.push_str("G94\n"); // feed/minute units
    
    str.push_str(&format!("F{:.4}\n", cut_speed));
    str.push_str(&format!("S{}\n", settings.spindle_speed_rpm));
    str.push_str(&format!("G00Z{:.4}\n", jog_height));
    str.push_str("M03\n"); // spindle on clockwise
    
    for seg in path {
        if seg.is_empty() {
            continue;
        }
        let x = scale * seg[0][0];
        let y = scale * seg[0][1];
        
        str.push_str(&format!("G00Z{:.4}\n", jog_height));
        total_time_secs += tool.move_to(tool.xp.unwrap_or(x), tool.yp.unwrap_or(y), jog_height) / rapid_mm_sec;
        
        str.push_str(&format!("G00X{:.4}Y{:.4}Z{:.4}\n", x, y, jog_height));
        total_time_secs += tool.move_to(x, y, jog_height) / rapid_mm_sec;
        
        let z = scale * seg[0][2];
        str.push_str(&format!("G01Z{:.4} F{:.4}\n", z, plunge_speed));
        total_time_secs += tool.move_to(x, y, z) / plunge_mm_sec;
        str.push_str(&format!("F{:.4}\n", cut_speed));
        
        for pt in &seg[1..] {
            let px = scale * pt[0];
            let py = scale * pt[1];
            let pz = scale * pt[2];
            str.push_str(&format!("G01X{:.4}Y{:.4}Z{:.4}\n", px, py, pz));
            let d = tool.move_to(px, py, pz);
            total_time_secs += d / feed_mm_sec;
            cut_distance_mm += d;
        }
    }
    
    str.push_str(&format!("G00Z{:.4}\n", jog_height));
    total_time_secs += tool.move_to(tool.xp.unwrap_or(0.0), tool.yp.unwrap_or(0.0), jog_height) / rapid_mm_sec;
    
    str.push_str(&format!("G00X0.0000Y0.0000Z{:.4}\n", finish_height));
    total_time_secs += tool.move_to(0.0, 0.0, finish_height) / rapid_mm_sec;
    
    str.push_str("M05\n"); // spindle stop
    str.push_str("M30\n"); // program end
    str.push_str("%\n");
    
    GcodeStats {
        gcode: str,
        estimated_time_secs: total_time_secs,
        cut_distance_mm,
    }
}

fn read_zip_gerbers(path: &Path) -> Result<Vec<String>, ConversionError> {
    let file = fs::File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut sources = Vec::new();

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if !entry.is_file() || !is_gerber_name(entry.name()) {
            continue;
        }

        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        sources.push(String::from_utf8_lossy(&bytes).into_owned());
    }

    Ok(sources)
}

fn parse_gerber_or_drill(source: &str) -> Vec<Primitive> {
    let has_m48 = source.contains("M48");
    let has_metric = source.contains("METRIC");
    let has_inch = source.contains("INCH");
    let asterisks = source.chars().filter(|&c| c == '*').count();
    
    if (has_m48 || has_metric || has_inch) && asterisks < 5 {
        parse_excellon(source)
    } else {
        parse_gerber(source)
    }
}

fn parse_gerber(source: &str) -> Vec<Primitive> {
    let mut format_int = 2usize;
    let mut format_dec = 4usize;
    let mut unit_scale = 1.0f32;
    let mut apertures = HashMap::<u32, Aperture>::new();
    let mut active_aperture = None::<u32>;
    let mut current = Point { x: 0.0, y: 0.0 };
    let mut last_operation = 2u8;
    let mut primitives = Vec::new();
    
    let mut in_region = false;
    let mut region_points = Vec::new();

    for raw in source.split('*') {
        // RS-274X extended-code blocks use "%...CMD...*%" syntax.  When two such blocks
        // appear on consecutive lines (e.g. "%TF...*%\r\n%FSLAX46Y46*%"), splitting the
        // whole source by '*' yields a chunk like "%\r\n%FSLAX46Y46".  The leading
        // "%" + whitespace is the closing delimiter of the previous block; strip it so
        // the actual command content ("%FSLAX46Y46") is recognized correctly.
        let raw_trimmed = raw.trim();
        let command = if let Some(rest) = raw_trimmed.strip_prefix('%') {
            let after_ws = rest.trim_start();
            if after_ws.starts_with('%') {
                after_ws   // e.g. "%\r\n%FSLAX46Y46" → "%FSLAX46Y46"
            } else {
                raw_trimmed
            }
        } else {
            raw_trimmed
        };
        if command.is_empty() {
            continue;
        }

        if command.starts_with("%FS") {
            if let Some((int_digits, dec_digits)) = parse_format(command) {
                format_int = int_digits;
                format_dec = dec_digits;
            }
            continue;
        }

        if command.contains("MOIN") {
            unit_scale = 25.4;
            continue;
        }

        if command.contains("MOMM") {
            unit_scale = 1.0;
            continue;
        }

        if command.starts_with("G36") {
            in_region = true;
            region_points.clear();
            continue;
        }
        if command.starts_with("G37") {
            in_region = false;
            if region_points.len() >= 3 {
                primitives.push(Primitive::Polygon {
                    points: region_points.clone(),
                });
            }
            region_points.clear();
            continue;
        }

        if let Some((code, aperture)) = parse_aperture(command, unit_scale) {
            apertures.insert(code, aperture);
            continue;
        }

        if let Some(code) = command
            .strip_prefix('D')
            .and_then(|value| value.parse::<u32>().ok())
        {
            if code >= 10 {
                active_aperture = Some(code);
            }
            continue;
        }

        if !command.contains('X') && !command.contains('Y') {
            continue;
        }

        let operation = parse_operation(command).unwrap_or(last_operation);
        
        let next = Point {
            x: parse_axis(command, 'X', format_int, format_dec)
                .map(|value| value * unit_scale)
                .unwrap_or(current.x),
            y: parse_axis(command, 'Y', format_int, format_dec)
                .map(|value| value * unit_scale)
                .unwrap_or(current.y),
        };

        if in_region {
            if operation == 1 || operation == 2 {
                region_points.push(next);
            }
        } else {
            let ap = active_aperture.and_then(|code| apertures.get(&code));
            let diameter_mm = ap.map(|aperture| {
                if aperture.shape == ApertureShape::Circle {
                    aperture.dimensions[0]
                } else {
                    aperture.dimensions.iter().cloned().fold(f32::MIN, f32::max)
                }
            }).unwrap_or(0.15);

            match operation {
                1 => primitives.push(Primitive::Segment {
                    start: current,
                    end: next,
                    diameter_mm,
                }),
                3 => {
                    if let Some(aperture) = ap {
                        primitives.push(Primitive::Flash {
                            center: next,
                            shape: aperture.shape,
                            dimensions: aperture.dimensions.clone(),
                        });
                    } else {
                        primitives.push(Primitive::Flash {
                            center: next,
                            shape: ApertureShape::Circle,
                            dimensions: vec![diameter_mm],
                        });
                    }
                }
                _ => {}
            }
        }

        current = next;
        last_operation = operation;
    }

    primitives
}

fn parse_excellon(source: &str) -> Vec<Primitive> {
    let mut unit_scale = 1.0f32; // mm
    let mut tools = HashMap::<u32, f32>::new();
    let mut active_tool = None::<u32>;
    let mut current = Point { x: 0.0, y: 0.0 };
    let mut primitives = Vec::new();
    let mut in_header = false;

    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }

        if line == "M48" {
            in_header = true;
            continue;
        }
        if line == "M30" || line == "M00" || line == "M02" {
            break;
        }

        if line.contains("METRIC") || line.contains("G71") {
            unit_scale = 1.0;
            continue;
        }
        if line.contains("INCH") || line.contains("G70") {
            unit_scale = 25.4;
            continue;
        }

        if in_header && line.starts_with('T') && line.contains('C') {
            if let Some(c_idx) = line.find('C') {
                let tool_str = &line[1..c_idx];
                let dia_str = &line[c_idx + 1..];
                
                let dia_val_str = dia_str.chars()
                    .take_while(|&c| c.is_ascii_digit() || c == '.' || c == '-' || c == '+')
                    .collect::<String>();

                if let (Ok(id), Ok(dia)) = (tool_str.parse::<u32>(), dia_val_str.parse::<f32>()) {
                    tools.insert(id, dia * unit_scale);
                }
            }
            continue;
        }

        if line == "%" || line == "DETECT" {
            in_header = false;
            continue;
        }

        if line.starts_with('T') && !line.contains('C') {
            let tool_str = line.trim_start_matches('T').chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>();
            if let Ok(id) = tool_str.parse::<u32>() {
                active_tool = Some(id);
            }
            continue;
        }

        if line.starts_with('X') || line.starts_with('Y') {
            let mut x = None;
            let mut y = None;
            
            let x_idx = line.find('X');
            let y_idx = line.find('Y');
            
            let get_val = |sub: &str| -> Option<f32> {
                let val_str = sub.chars()
                    .take_while(|&c| c.is_ascii_digit() || c == '.' || c == '-' || c == '+')
                    .collect::<String>();
                if val_str.contains('.') {
                    val_str.parse::<f32>().ok()
                } else {
                    let val = val_str.parse::<f32>().ok()?;
                    let dec = if unit_scale > 2.0 { 10000.0 } else { 1000.0 };
                    Some(val / dec)
                }
            };
            
            if let Some(idx) = x_idx {
                x = get_val(&line[idx + 1..]);
            }
            if let Some(idx) = y_idx {
                y = get_val(&line[idx + 1..]);
            }
            
            let next = Point {
                x: x.map(|v| v * unit_scale).unwrap_or(current.x),
                y: y.map(|v| v * unit_scale).unwrap_or(current.y),
            };

            let diameter_mm = active_tool
                .and_then(|id| tools.get(&id))
                .cloned()
                .unwrap_or(0.8);

            primitives.push(Primitive::Flash {
                center: next,
                shape: ApertureShape::Circle,
                dimensions: vec![diameter_mm],
            });

            current = next;
        }
    }

    primitives
}

fn render_primitives_to_png(
    primitives: &[Primitive],
    output_path: &Path,
    settings: ConversionSettings,
) -> Result<PngRenderResult, ConversionError> {
    let bounds = primitive_bounds(primitives).ok_or(ConversionError::NoRenderableGerber)?;
    let margin_mm = 0.82f32; // Matching OUTLINE_EXPORT_PADDING_MM in gerber2png
    let scale = settings.pixels_per_mm.max(1.0);
    
    let board_w = bounds.2 - bounds.0;
    let board_h = bounds.3 - bounds.1;
    
    let width = ((board_w + margin_mm * 2.0) * scale).round() as u32;
    let height = ((board_h + margin_mm * 2.0) * scale).round() as u32;
    let width = width.max(1);
    let height = height.max(1);
    let pixels = u64::from(width) * u64::from(height);
    if pixels > settings.max_render_pixels {
        return Err(ConversionError::RenderTooLarge {
            width,
            height,
            pixels,
            max_pixels: settings.max_render_pixels,
        });
    }
    
    // Default background is black (0 represents black, 255 represents white)
    let mut image = GrayImage::from_pixel(width, height, Luma([0]));

    let to_pixel = |point: Point| -> Point {
        let x = (point.x - bounds.0 + margin_mm) * scale;
        let y = (bounds.3 - point.y + margin_mm) * scale;
        Point { x, y }
    };

    let draw_color = 255u8;

    for primitive in primitives {
        match primitive {
            Primitive::Segment { start, end, diameter_mm } => {
                let p0 = to_pixel(*start);
                let p1 = to_pixel(*end);
                let radius_px = (diameter_mm * scale) / 2.0;
                draw_thick_line(&mut image, p0, p1, radius_px, draw_color);
            }
            Primitive::Flash { center, shape, dimensions } => {
                let c = to_pixel(*center);
                match shape {
                    ApertureShape::Circle => {
                        let radius_px = (dimensions[0] * scale) / 2.0;
                        draw_filled_circle(&mut image, c.x, c.y, radius_px, draw_color);
                    }
                    ApertureShape::Rectangle => {
                        let w_px = dimensions[0] * scale;
                        let h_px = dimensions[1] * scale;
                        draw_filled_rect(&mut image, c.x, c.y, w_px, h_px, draw_color);
                    }
                    ApertureShape::Oval => {
                        let w_px = dimensions[0] * scale;
                        let h_px = dimensions[1] * scale;
                        draw_filled_oval(&mut image, c.x, c.y, w_px, h_px, draw_color);
                    }
                    ApertureShape::Polygon => {
                        let dia_px = dimensions[0] * scale;
                        let vertices = dimensions.get(1).cloned().unwrap_or(5.0) as usize;
                        let rotation = dimensions.get(2).cloned().unwrap_or(0.0);
                        draw_filled_polygon_aperture(&mut image, c.x, c.y, dia_px, vertices, rotation, draw_color);
                    }
                }
            }
            Primitive::Polygon { points } => {
                let mapped_points = points.iter().map(|&p| to_pixel(p)).collect::<Vec<_>>();
                fill_polygon(&mut image, &mapped_points, draw_color);
            }
        }
    }

    let dark_pixels = image
        .pixels()
        .filter(|pixel| pixel[0] < settings.threshold)
        .count();
    image.save(output_path)?;

    Ok(PngRenderResult {
        path: output_path.to_path_buf(),
        width,
        height,
        dark_pixels,
    })
}

fn raster_to_gcode(image: &GrayImage, settings: ConversionSettings) -> String {
    let mut lines = vec![
        "; EasyMill raster G-code".to_owned(),
        "G21".to_owned(),
        "G90".to_owned(),
        format!("G0 Z{:.3}", settings.safe_z_mm),
        format!("M3 S{}", settings.spindle_speed_rpm),
    ];

    let mut left_to_right = true;
    for y in 0..image.height() {
        let mut x = 0;
        while x < image.width() {
            while x < image.width() && image.get_pixel(x, y)[0] >= settings.threshold {
                x += 1;
            }

            if x >= image.width() {
                break;
            }

            let start = x;
            while x < image.width() && image.get_pixel(x, y)[0] < settings.threshold {
                x += 1;
            }
            let end = x.saturating_sub(1);

            let (start_x, end_x) = if left_to_right {
                (start, end)
            } else {
                (end, start)
            };
            let y_mm = y as f32 / settings.pixels_per_mm;
            let start_mm = start_x as f32 / settings.pixels_per_mm;
            let end_mm = end_x as f32 / settings.pixels_per_mm;

            lines.push(format!("G0 X{start_mm:.3} Y{y_mm:.3}"));
            lines.push(format!(
                "G1 Z{:.3} F{:.1}",
                settings.cut_z_mm, settings.plunge_rate_mm_min
            ));
            lines.push(format!(
                "G1 X{end_mm:.3} Y{y_mm:.3} F{:.1}",
                settings.feed_rate_mm_min
            ));
            lines.push(format!("G0 Z{:.3}", settings.safe_z_mm));
            left_to_right = !left_to_right;
        }
    }

    lines.extend([
        "M5".to_owned(),
        format!("G0 Z{:.3}", settings.safe_z_mm),
        "M30".to_owned(),
    ]);
    lines.join("\n")
}

fn primitive_bounds(primitives: &[Primitive]) -> Option<(f32, f32, f32, f32)> {
    let mut bounds = None::<(f32, f32, f32, f32)>;
    
    let update_bounds = |x: f32, y: f32, bounds_ref: &mut Option<(f32, f32, f32, f32)>| {
        *bounds_ref = Some(match bounds_ref {
            Some((min_x, min_y, max_x, max_y)) => (
                min_x.min(x),
                min_y.min(y),
                max_x.max(x),
                max_y.max(y),
            ),
            None => (x, y, x, y),
        });
    };

    for primitive in primitives {
        match primitive {
            Primitive::Segment { start, end, diameter_mm } => {
                let half_d = diameter_mm / 2.0;
                update_bounds(start.x - half_d, start.y - half_d, &mut bounds);
                update_bounds(start.x + half_d, start.y + half_d, &mut bounds);
                update_bounds(end.x - half_d, end.y - half_d, &mut bounds);
                update_bounds(end.x + half_d, end.y + half_d, &mut bounds);
            }
            Primitive::Flash { center, shape, dimensions } => {
                match shape {
                    ApertureShape::Circle => {
                        let radius = dimensions[0] / 2.0;
                        update_bounds(center.x - radius, center.y - radius, &mut bounds);
                        update_bounds(center.x + radius, center.y + radius, &mut bounds);
                    }
                    ApertureShape::Rectangle | ApertureShape::Oval => {
                        let half_w = dimensions[0] / 2.0;
                        let half_h = dimensions[1] / 2.0;
                        update_bounds(center.x - half_w, center.y - half_h, &mut bounds);
                        update_bounds(center.x + half_w, center.y + half_h, &mut bounds);
                    }
                    ApertureShape::Polygon => {
                        let radius = dimensions[0] / 2.0;
                        update_bounds(center.x - radius, center.y - radius, &mut bounds);
                        update_bounds(center.x + radius, center.y + radius, &mut bounds);
                    }
                }
            }
            Primitive::Polygon { points } => {
                for pt in points {
                    update_bounds(pt.x, pt.y, &mut bounds);
                }
            }
        }
    }
    bounds
}

fn draw_thick_line(image: &mut GrayImage, p0: Point, p1: Point, radius: f32, color: u8) {
    let xmin = p0.x.min(p1.x) - radius;
    let xmax = p0.x.max(p1.x) + radius;
    let ymin = p0.y.min(p1.y) - radius;
    let ymax = p0.y.max(p1.y) + radius;
    
    let x_start = (xmin.floor() as i32).max(0);
    let x_end = (xmax.ceil() as i32).min(image.width() as i32 - 1);
    let y_start = (ymin.floor() as i32).max(0);
    let y_end = (ymax.ceil() as i32).min(image.height() as i32 - 1);
    
    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;
    let len_sq = dx * dx + dy * dy;
    let radius_sq = radius * radius;
    
    for y in y_start..=y_end {
        let py = y as f32 + 0.5;
        for x in x_start..=x_end {
            let px = x as f32 + 0.5;
            
            let dpx = px - p0.x;
            let dpy = py - p0.y;
            
            let dist_sq = if len_sq < 1e-6 {
                dpx * dpx + dpy * dpy
            } else {
                let t = (dpx * dx + dpy * dy) / len_sq;
                let t_clamped = t.max(0.0).min(1.0);
                let proj_x = p0.x + t_clamped * dx;
                let proj_y = p0.y + t_clamped * dy;
                (px - proj_x).powi(2) + (py - proj_y).powi(2)
            };
            
            if dist_sq <= radius_sq {
                image.put_pixel(x as u32, y as u32, Luma([color]));
            }
        }
    }
}

fn draw_filled_circle(image: &mut GrayImage, cx: f32, cy: f32, radius: f32, color: u8) {
    let xmin = cx - radius;
    let xmax = cx + radius;
    let ymin = cy - radius;
    let ymax = cy + radius;
    
    let x_start = (xmin.floor() as i32).max(0);
    let x_end = (xmax.ceil() as i32).min(image.width() as i32 - 1);
    let y_start = (ymin.floor() as i32).max(0);
    let y_end = (ymax.ceil() as i32).min(image.height() as i32 - 1);
    
    let radius_sq = radius * radius;
    for y in y_start..=y_end {
        let dy = y as f32 + 0.5 - cy;
        for x in x_start..=x_end {
            let dx = x as f32 + 0.5 - cx;
            if dx * dx + dy * dy <= radius_sq {
                image.put_pixel(x as u32, y as u32, Luma([color]));
            }
        }
    }
}

fn draw_filled_rect(image: &mut GrayImage, cx: f32, cy: f32, width: f32, height: f32, color: u8) {
    let half_w = width / 2.0;
    let half_h = height / 2.0;
    let xmin = cx - half_w;
    let xmax = cx + half_w;
    let ymin = cy - half_h;
    let ymax = cy + half_h;
    
    let x_start = (xmin.floor() as i32).max(0);
    let x_end = (xmax.ceil() as i32).min(image.width() as i32 - 1);
    let y_start = (ymin.floor() as i32).max(0);
    let y_end = (ymax.ceil() as i32).min(image.height() as i32 - 1);
    
    for y in y_start..=y_end {
        for x in x_start..=x_end {
            image.put_pixel(x as u32, y as u32, Luma([color]));
        }
    }
}

fn draw_filled_oval(image: &mut GrayImage, cx: f32, cy: f32, w: f32, h: f32, color: u8) {
    if w > h {
        let cap_radius = h / 2.0;
        let offset = (w - h) / 2.0;
        let p0 = Point { x: cx - offset, y: cy };
        let p1 = Point { x: cx + offset, y: cy };
        draw_thick_line(image, p0, p1, cap_radius, color);
    } else if h > w {
        let cap_radius = w / 2.0;
        let offset = (h - w) / 2.0;
        let p0 = Point { x: cx, y: cy - offset };
        let p1 = Point { x: cx, y: cy + offset };
        draw_thick_line(image, p0, p1, cap_radius, color);
    } else {
        draw_filled_circle(image, cx, cy, w / 2.0, color);
    }
}

fn draw_filled_polygon_aperture(image: &mut GrayImage, cx: f32, cy: f32, diameter: f32, vertices: usize, rotation_deg: f32, color: u8) {
    let radius = diameter / 2.0;
    let mut points = Vec::with_capacity(vertices);
    let rotation_rad = rotation_deg.to_radians();
    for i in 0..vertices {
        let angle = 2.0 * std::f32::consts::PI * (i as f32) / (vertices as f32) + rotation_rad;
        let px = cx + radius * angle.cos();
        let py = cy + radius * angle.sin();
        points.push(Point { x: px, y: py });
    }
    fill_polygon(image, &points, color);
}

fn fill_polygon(image: &mut GrayImage, points: &[Point], color: u8) {
    if points.len() < 3 {
        return;
    }
    
    let mut ymin = f32::MAX;
    let mut ymax = f32::MIN;
    for pt in points {
        ymin = ymin.min(pt.y);
        ymax = ymax.max(pt.y);
    }
    
    let y_start = (ymin.floor() as i32).max(0);
    let y_end = (ymax.ceil() as i32).min(image.height() as i32 - 1);
    
    for y in y_start..=y_end {
        let mut intersections = Vec::new();
        let y_f = y as f32 + 0.5;
        
        for i in 0..points.len() {
            let p0 = points[i];
            let p1 = points[(i + 1) % points.len()];
            
            if (p0.y <= y_f && p1.y > y_f) || (p1.y <= y_f && p0.y > y_f) {
                let t = (y_f - p0.y) / (p1.y - p0.y);
                let x = p0.x + t * (p1.x - p0.x);
                intersections.push(x);
            }
        }
        
        intersections.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        
        for chunk in intersections.chunks_exact(2) {
            let x0 = chunk[0].round() as i32;
            let x1 = chunk[1].round() as i32;
            
            let x_start = x0.max(0);
            let x_end = x1.min(image.width() as i32 - 1);
            
            for x in x_start..=x_end {
                image.put_pixel(x as u32, y as u32, Luma([color]));
            }
        }
    }
}

fn parse_format(command: &str) -> Option<(usize, usize)> {
    let index = command.find('X')?;
    let digits = command[index + 1..]
        .chars()
        .filter(|character| character.is_ascii_digit())
        .take(2)
        .collect::<String>();
    let mut chars = digits.chars();
    Some((
        chars.next()?.to_digit(10)? as usize,
        chars.next()?.to_digit(10)? as usize,
    ))
}

fn parse_aperture(command: &str, unit_scale: f32) -> Option<(u32, Aperture)> {
    let value = command
        .strip_prefix("%ADD")
        .or_else(|| command.strip_prefix("ADD"))?;
    let code_end = value.find(|character: char| !character.is_ascii_digit())?;
    let code = value[..code_end].parse::<u32>().ok()?;
    
    let rest = &value[code_end..];
    if rest.is_empty() {
        return None;
    }
    
    let shape_char = rest.chars().next()?;
    let shape = match shape_char {
        'C' | 'c' => ApertureShape::Circle,
        'R' | 'r' => ApertureShape::Rectangle,
        'O' | 'o' => ApertureShape::Oval,
        'P' | 'p' => ApertureShape::Polygon,
        _ => return None,
    };
    
    let comma = rest.find(',')?;
    let dims_str = rest[comma + 1..].trim_end_matches('%');
    
    let mut dimensions = Vec::new();
    for token in dims_str.split(['X', 'x']) {
        if let Ok(val) = token.parse::<f32>() {
            dimensions.push(val);
        }
    }
    
    if dimensions.is_empty() {
        return None;
    }
    
    // Scale appropriate dimensions
    let mut dims = dimensions.clone();
    match shape {
        ApertureShape::Circle => {
            dims[0] *= unit_scale;
        }
        ApertureShape::Rectangle | ApertureShape::Oval => {
            for dim in &mut dims {
                *dim *= unit_scale;
            }
        }
        ApertureShape::Polygon => {
            dims[0] *= unit_scale;
        }
    }
    
    Some((
        code,
        Aperture {
            shape,
            dimensions: dims,
        },
    ))
}

fn parse_operation(command: &str) -> Option<u8> {
    if command.contains("D01") {
        Some(1)
    } else if command.contains("D02") {
        Some(2)
    } else if command.contains("D03") {
        Some(3)
    } else {
        None
    }
}

fn parse_axis(command: &str, axis: char, int_digits: usize, dec_digits: usize) -> Option<f32> {
    let start = command.find(axis)? + 1;
    let value = command[start..]
        .chars()
        .take_while(|character| {
            character.is_ascii_digit() || *character == '-' || *character == '+' || *character == '.'
        })
        .collect::<String>();
    if value.is_empty() {
        return None;
    }

    if value.contains('.') {
        return value.parse::<f32>().ok();
    }

    let sign = if value.starts_with('-') { -1.0 } else { 1.0 };
    let digits = value.trim_start_matches(['-', '+']);
    let padded = if digits.len() < int_digits + dec_digits {
        format!("{digits:0>width$}", width = int_digits + dec_digits)
    } else {
        digits.to_owned()
    };
    let whole_len = padded.len().saturating_sub(dec_digits);
    let whole = padded[..whole_len].parse::<f32>().ok()?;
    let frac = padded[whole_len..].parse::<f32>().unwrap_or(0.0) / 10f32.powi(dec_digits as i32);
    Some(sign * (whole + frac))
}

fn is_zip(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
}

fn is_gerber_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "gbr" | "grb" | "gtl" | "gbl" | "gko" | "gto" | "gbo" | "drl"
            )
        })
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write};

    use image::{ImageBuffer, Luma};
    use tempfile::tempdir;
    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::{
        ConversionError, ConversionSettings, DEFAULT_PIXELS_PER_MM, gerber_inputs_to_png,
        png_to_gcode,
    };

    const SIMPLE_GERBER: &str = "%FSLAX24Y24*%\n%MOMM*%\n%ADD10C,0.300*%\nD10*\nX000000Y000000D02*\nX010000Y000000D01*\nM02*\n";

    #[test]
    fn gerber_stroke_renders_to_png() {
        let dir = tempdir().unwrap();
        let gerber_path = dir.path().join("top.gtl");
        let png_path = dir.path().join("preview.png");
        fs::write(&gerber_path, SIMPLE_GERBER).unwrap();

        let result = gerber_inputs_to_png(
            &[gerber_path],
            &png_path,
            ConversionSettings {
                pixels_per_mm: 20.0,
                ..ConversionSettings::default()
            },
        )
        .unwrap();

        assert!(png_path.exists());
        assert!(result.width > 0);
        assert!(result.height > 0);

        let rendered = image::open(&png_path).unwrap().to_luma8();
        let dark_pixels = rendered.pixels().filter(|pixel| pixel[0] < 128).count();
        assert!(dark_pixels > 0);
    }

    #[test]
    fn default_gerber_render_uses_1000_dpi() {
        let dir = tempdir().unwrap();
        let gerber_path = dir.path().join("top.gtl");
        let default_png_path = dir.path().join("default-preview.png");
        let explicit_png_path = dir.path().join("explicit-preview.png");
        fs::write(&gerber_path, SIMPLE_GERBER).unwrap();

        let default_result = gerber_inputs_to_png(
            std::slice::from_ref(&gerber_path),
            &default_png_path,
            ConversionSettings::default(),
        )
        .unwrap();
        let explicit_result = gerber_inputs_to_png(
            &[gerber_path],
            &explicit_png_path,
            ConversionSettings {
                pixels_per_mm: DEFAULT_PIXELS_PER_MM,
                ..ConversionSettings::default()
            },
        )
        .unwrap();

        assert!((DEFAULT_PIXELS_PER_MM - 1000.0 / 25.4).abs() < f32::EPSILON);
        assert_eq!(default_result.width, explicit_result.width);
        assert_eq!(default_result.height, explicit_result.height);
    }

    #[test]
    fn zipped_gerber_package_renders_to_png() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("board.zip");
        let png_path = dir.path().join("preview.png");

        let zip_file = fs::File::create(&zip_path).unwrap();
        let mut zip = ZipWriter::new(zip_file);
        zip.start_file("top.gtl", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(SIMPLE_GERBER.as_bytes()).unwrap();
        zip.finish().unwrap();

        let result = gerber_inputs_to_png(
            &[zip_path],
            &png_path,
            ConversionSettings {
                pixels_per_mm: 20.0,
                ..ConversionSettings::default()
            },
        )
        .unwrap();

        assert!(png_path.exists());
        assert!(result.dark_pixels > 0);
    }

    #[test]
    fn black_raster_generates_cutting_moves() {
        let dir = tempdir().unwrap();
        let png_path = dir.path().join("trace.png");

        let mut image = ImageBuffer::<Luma<u8>, Vec<u8>>::from_pixel(8, 5, Luma([255]));
        for x in 2..6 {
            image.put_pixel(x, 2, Luma([0]));
        }
        image.save(&png_path).unwrap();

        let gcode = png_to_gcode(
            &png_path,
            ConversionSettings {
                pixels_per_mm: 20.0,
                feed_rate_mm_min: 420.0,
                spindle_speed_rpm: 9000,
                cut_z_mm: -0.15,
                ..ConversionSettings::default()
            },
        )
        .unwrap();

        assert!(gcode.gcode.contains("G21"));
        assert!(gcode.gcode.contains("M03"));
        assert!(gcode.gcode.contains("S9000"));
        assert!(gcode.gcode.contains("F420.0000"));
        assert!(gcode.gcode.contains("M30"));
    }

    #[test]
    fn oversized_gerber_render_fails_before_allocation() {
        let dir = tempdir().unwrap();
        let gerber_path = dir.path().join("large-board.gtl");
        let png_path = dir.path().join("preview.png");
        fs::write(
            &gerber_path,
            "%FSLAX24Y24*%\n%MOMM*%\n%ADD10C,0.300*%\nD10*\nX000000Y000000D02*\nX2000000Y000000D01*\nM02*\n",
        )
        .unwrap();

        let err = gerber_inputs_to_png(
            &[gerber_path],
            &png_path,
            ConversionSettings {
                pixels_per_mm: 100.0,
                max_render_pixels: 10_000,
                ..ConversionSettings::default()
            },
        )
        .unwrap_err();

        match err {
            ConversionError::RenderTooLarge {
                pixels,
                max_pixels,
                ..
            } => {
                assert!(pixels > max_pixels);
                assert_eq!(max_pixels, 10_000);
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn gcode_result_includes_toolpaths() {
        let dir = tempdir().unwrap();
        let png_path = dir.path().join("trace.png");

        let mut image = ImageBuffer::<Luma<u8>, Vec<u8>>::from_pixel(40, 30, Luma([255]));
        // Create a black rectangle in the center
        for y in 10..20 {
            for x in 10..30 {
                image.put_pixel(x, y, Luma([0]));
            }
        }
        image.save(&png_path).unwrap();

        let result = png_to_gcode(
            &png_path,
            ConversionSettings {
                pixels_per_mm: 5.0,
                tool_diameter_mm: 0.5,
                offset_number: 2,
                offset_stepover: 0.2,
                ..ConversionSettings::default()
            },
        )
        .unwrap();

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

    #[test]
    fn real_gerber_zip_renders_to_expected_size() {
        // Board outline (Edge_Cuts) is ~31.85 × 17.15 mm.
        // At 1000 DPI (39.37 px/mm) + 0.82 mm margin on each side → ~1319 × 740 px.
        let zip = std::path::Path::new("test_files/inputs/gerber.zip");
        if !zip.exists() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let png_path = dir.path().join("out.png");
        let result = gerber_inputs_to_png(
            &[zip.to_path_buf()],
            &png_path,
            ConversionSettings::default(),
        )
        .unwrap();
        // Previously broken: format spec was not parsed, yielding ~450k × 390k pixels.
        assert!(
            result.width < 2000,
            "render width {} is too large — coordinate format likely misread (was {}×{})",
            result.width, result.width, result.height
        );
        assert!(
            result.height < 2000,
            "render height {} is too large — coordinate format likely misread",
            result.height
        );
        assert!(result.dark_pixels > 0, "no dark pixels rendered");
    }
}
