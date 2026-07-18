use image::{GrayImage, Luma};
use nalgebra::Point2;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use tracing::{debug, info};

use crate::gerber;

/// Progress callback: receives progress as f32 in [0.0, 1.0].
pub type ProgressFn = Box<dyn Fn(f32) + Send>;

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
    pub mirror_bottom: bool,
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
            mirror_bottom: true,
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

#[derive(Debug, Clone)]
pub struct PngLayerResults {
    pub copper_top: PngRenderResult,
    pub copper_bottom: PngRenderResult,
    pub drills: PngRenderResult,
    pub outline: PngRenderResult,
}

#[derive(Debug)]
pub enum ConversionError {
    Io(std::io::Error),
    Image(image::ImageError),
    EmptyInput,
    NoRenderableGerber,
    GerberParseError(String),
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
            Self::EmptyInput => write!(formatter, "no input files were provided"),
            Self::NoRenderableGerber => write!(formatter, "no supported Gerber geometry was found"),
            Self::GerberParseError(err) => write!(formatter, "gerber parse error: {err}"),
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

#[allow(dead_code)]
fn gerber_output_stem(paths: &[PathBuf]) -> String {
    paths.first()
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .map(|s| s.to_owned())
        .unwrap_or_else(|| "output".to_owned())
}

pub fn gerber_inputs_to_png(
    copper_top_inputs: &[PathBuf],
    copper_bottom_inputs: &[PathBuf],
    outline_inputs: &[PathBuf],
    drill_inputs: &[PathBuf],
    output_dir: &Path,
    output_stem: &str,
    settings: ConversionSettings,
    on_progress: Option<ProgressFn>,
) -> Result<PngLayerResults, ConversionError> {
    info!(
        copper_top = copper_top_inputs.len(),
        copper_bottom = copper_bottom_inputs.len(),
        outline = outline_inputs.len(),
        drill = drill_inputs.len(),
        "starting gerber to 4-png conversion"
    );

    if copper_top_inputs.is_empty() && copper_bottom_inputs.is_empty()
        && outline_inputs.is_empty() && drill_inputs.is_empty()
    {
        return Err(ConversionError::EmptyInput);
    }

    let mut all_tagged: Vec<gerber::TaggedTriangle> = Vec::new();
    let total_files = copper_top_inputs.len() + copper_bottom_inputs.len()
        + outline_inputs.len() + drill_inputs.len();
    let mut file_count = 0u32;

    let mut parse_and_tag = |paths: &[PathBuf], layer: gerber::LayerType| -> Result<(), ConversionError> {
        for input in paths {
            let source = fs::read_to_string(input)?;
            let triangles: Vec<gerber::Triangle> = if is_excellon(&source) {
                parse_excellon(&source)
            } else {
                gerber::parse_to_shapes(&source)?
            };
            for tri in triangles {
                all_tagged.push(gerber::TaggedTriangle { triangle: tri, layer });
            }
            file_count += 1;
            if let Some(ref cb) = on_progress {
                cb(file_count as f32 / total_files as f32 * 0.5);
            }
        }
        Ok(())
    };

    parse_and_tag(copper_top_inputs, gerber::LayerType::CopperTop)?;
    parse_and_tag(copper_bottom_inputs, gerber::LayerType::CopperBottom)?;
    parse_and_tag(outline_inputs, gerber::LayerType::Profile)?;
    parse_and_tag(drill_inputs, gerber::LayerType::Drill)?;

    if all_tagged.is_empty() {
        return Err(ConversionError::NoRenderableGerber);
    }

    let all_tris: Vec<gerber::Triangle> = all_tagged.iter().map(|t| t.triangle.clone()).collect();
    let bbox = gerber::compute_bounding_box(&all_tris);
    let layout = gerber::compute_render_layout(bbox, &settings)?;

    if let Some(ref cb) = on_progress { cb(0.6); }

    let render_layer = |tagged: &[gerber::TaggedTriangle],
                         layer: gerber::LayerType,
                         suffix: &str,
                         invert: bool,
                         flood_fill: bool,
                         mirror: bool,
                         progress: f32| -> Result<PngRenderResult, ConversionError>
    {
        let layer_tris: Vec<gerber::Triangle> = tagged.iter()
            .filter(|t| t.layer == layer)
            .map(|t| t.triangle.clone())
            .collect();

        let output_path = output_dir.join(format!("{output_stem}{suffix}"));
        let mut image = GrayImage::from_pixel(layout.width, layout.height, Luma([0]));
        gerber::render_triangles_to_image(&mut image, &layer_tris, &layout, 255);

        if mirror {
            image = image::imageops::flip_horizontal(&image);
        }
        if invert {
            gerber::invert_image_gray(&mut image);
        }
        if flood_fill {
            image = gerber::flood_fill_holes(&image);
        }

        let dark_pixels = image.pixels().filter(|p| p[0] < settings.threshold).count();
        image.save(&output_path)?;

        if let Some(ref cb) = on_progress { cb(progress); }

        Ok(PngRenderResult {
            path: output_path,
            width: layout.width,
            height: layout.height,
            dark_pixels,
        })
    };

    let copper_top = render_layer(&all_tagged, gerber::LayerType::CopperTop, "_traces_top.png", false, false, false, 0.60)?;
    let copper_bottom = render_layer(&all_tagged, gerber::LayerType::CopperBottom, "_traces_bot.png", false, false, settings.mirror_bottom, 0.70)?;
    let drills = render_layer(&all_tagged, gerber::LayerType::Drill, "_drills.png", true, false, false, 0.85)?;
    let outline = render_layer(&all_tagged, gerber::LayerType::Profile, "_outline.png", false, true, false, 0.95)?;

    info!("4-png conversion complete: copper_top={}, copper_bottom={}, drills={}, outline={}",
        copper_top.path.display(), copper_bottom.path.display(), drills.path.display(), outline.path.display());

    Ok(PngLayerResults { copper_top, copper_bottom, drills, outline })
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
    on_progress: Option<ProgressFn>,
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
    if let Some(ref cb) = on_progress { cb(0.1); }

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
    if let Some(ref cb) = on_progress { cb(0.5); }

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
    if let Some(ref cb) = on_progress { cb(0.8); }

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
    if let Some(ref cb) = on_progress { cb(0.95); }

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

    if let Some(ref cb) = on_progress { cb(1.0); }

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

fn is_excellon(source: &str) -> bool {
    let has_m48 = source.contains("M48");
    let has_metric = source.contains("METRIC");
    let has_inch = source.contains("INCH");
    let asterisks = source.chars().filter(|&c| c == '*').count();
    (has_m48 || has_metric || has_inch) && asterisks < 5
}



fn parse_excellon(source: &str) -> Vec<gerber::Triangle> {
    let mut unit_scale = 1.0;
    let mut tools = HashMap::<u32, f64>::new();
    let mut active_tool = None::<u32>;
    let mut current = Point2::new(0.0, 0.0);
    let mut triangles = Vec::new();
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

                if let (Ok(id), Ok(dia)) = (tool_str.parse::<u32>(), dia_val_str.parse::<f64>()) {
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

            let get_val = |sub: &str| -> Option<f64> {
                let val_str = sub.chars()
                    .take_while(|&c| c.is_ascii_digit() || c == '.' || c == '-' || c == '+')
                    .collect::<String>();
                if val_str.contains('.') {
                    val_str.parse::<f64>().ok()
                } else {
                    let val = val_str.parse::<f64>().ok()?;
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

            let next = Point2::new(
                x.map(|v| v * unit_scale).unwrap_or(current.x),
                y.map(|v| v * unit_scale).unwrap_or(current.y),
            );

            let diameter_mm = active_tool
                .and_then(|id| tools.get(&id))
                .cloned()
                .unwrap_or(0.8);

            triangles.extend(gerber::tessellate_aperture_circle(next, diameter_mm));

            current = next;
        }
    }

    triangles
}



#[allow(dead_code)]
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



#[cfg(test)]
mod tests {
    use std::fs;

    use image::{ImageBuffer, Luma};
    use tempfile::tempdir;

    use super::{
        ConversionError, ConversionSettings, DEFAULT_PIXELS_PER_MM, gerber_inputs_to_png,
        png_to_gcode,
    };

    const SIMPLE_GERBER: &str = "%FSLAX24Y24*%\n%MOMM*%\n%ADD10C,0.300*%\nD10*\nX000000Y000000D02*\nX010000Y000000D01*\nM02*\n";

    #[test]
    fn gerber_stroke_renders_to_png() {
        let dir = tempdir().unwrap();
        let gerber_path = dir.path().join("top.gtl");
        let output_dir = dir.path().to_path_buf();
        fs::write(&gerber_path, SIMPLE_GERBER).unwrap();

        let result = gerber_inputs_to_png(
            &[gerber_path.clone()],
            &[],
            &[],
            &[],
            &output_dir,
            "preview",
            ConversionSettings {
                pixels_per_mm: 20.0,
                ..ConversionSettings::default()
            },
            None,
        )
        .unwrap();

        assert!(result.copper_top.path.exists());
        assert!(result.copper_top.width > 0);
        assert!(result.copper_top.height > 0);
        assert!(result.copper_top.path.to_string_lossy().contains("preview_traces_top.png"));
        assert!(result.drills.path.to_string_lossy().contains("preview_drills.png"));
        assert!(result.outline.path.to_string_lossy().contains("preview_outline.png"));

        let rendered = image::open(&result.copper_top.path).unwrap().to_luma8();
        let dark_pixels = rendered.pixels().filter(|pixel| pixel[0] < 128).count();
        assert!(dark_pixels > 0);
    }

    #[test]
    fn default_gerber_render_uses_1000_dpi() {
        let dir = tempdir().unwrap();
        let gerber_path = dir.path().join("top.gtl");
        fs::write(&gerber_path, SIMPLE_GERBER).unwrap();

        let default_result = gerber_inputs_to_png(
            &[gerber_path.clone()], &[], &[], &[],
            dir.path(), "default-preview",
            ConversionSettings::default(), None,
        ).unwrap();
        let explicit_result = gerber_inputs_to_png(
            &[gerber_path], &[], &[], &[],
            dir.path(), "explicit-preview",
            ConversionSettings {
                pixels_per_mm: DEFAULT_PIXELS_PER_MM,
                ..ConversionSettings::default()
            }, None,
        ).unwrap();

        assert_eq!(default_result.copper_top.width, explicit_result.copper_top.width);
        assert_eq!(default_result.copper_top.height, explicit_result.copper_top.height);
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
            None,
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
        fs::write(&gerber_path,
            "%FSLAX24Y24*%\n%MOMM*%\n%ADD10C,0.300*%\nD10*\nX000000Y000000D02*\nX999999Y999999D01*\nM02*\n",
        ).unwrap();

        let err = gerber_inputs_to_png(
            &[gerber_path], &[], &[], &[],
            dir.path(), "large",
            ConversionSettings {
                pixels_per_mm: 100.0,
                max_render_pixels: 10_000,
                ..ConversionSettings::default()
            }, None,
        ).unwrap_err();

        match err {
            ConversionError::RenderTooLarge { pixels, max_pixels, .. } => {
                assert!(pixels > max_pixels);
                assert_eq!(max_pixels, 10_000);
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn three_pngs_have_identical_dimensions() {
        let dir = tempdir().unwrap();
        let gerber_path = dir.path().join("top.gtl");
        fs::write(&gerber_path, SIMPLE_GERBER).unwrap();

        let result = gerber_inputs_to_png(
            &[gerber_path], &[], &[], &[],
            dir.path(), "test",
            ConversionSettings {
                pixels_per_mm: 20.0,
                ..ConversionSettings::default()
            }, None,
        ).unwrap();

        assert_eq!(result.copper_top.width, result.drills.width);
        assert_eq!(result.copper_top.height, result.drills.height);
        assert_eq!(result.copper_top.width, result.outline.width);
        assert_eq!(result.copper_top.height, result.outline.height);
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
            None,
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

}
