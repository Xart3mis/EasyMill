use easymill::conversion::{
    ConversionSettings, DEFAULT_DPI, MM_PER_INCH, GcodeResult, PngLayerResults,
    gerber_inputs_to_png, png_to_gcode,
};
use easymill::logging::init_logging;
use iced::{
    self, Element, Length, Subscription, Task, Theme, event,
    widget::{container, scrollable},
};
use easymill::stackup::{Stackup, LayerFile, LayerCategory, Side};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::task::spawn_blocking;
use tracing::{error, info, warn};

mod ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum StepState {
    #[default]
    Idle,
    Ready,
    Running,
    Complete,
}


pub(crate) struct AppState {
    pub(crate) stackup: Stackup,
    pub(crate) loaded_png_path: Option<PathBuf>,
    pub(crate) loaded_inputs: Vec<String>,
    pub(crate) gerber_to_png: StepState,
    pub(crate) png_to_gcode: StepState,
    pub(crate) generated_pngs: Option<PngLayerResults>,
    pub(crate) generated_gcode: Option<String>,
    pub(crate) gerber_to_png_progress: f32,
    pub(crate) png_to_gcode_progress: f32,
    pub(crate) status: String,
    pub(crate) dpi_input: String,
    pub(crate) cut_z_mm_input: String,
    pub(crate) safe_z_mm_input: String,
    pub(crate) feed_rate_input: String,
    pub(crate) plunge_rate_input: String,
    pub(crate) spindle_speed_input: String,
    pub(crate) tool_diameter_mm_input: String,
    pub(crate) offset_number_input: String,
    pub(crate) offset_stepover_input: String,
    pub(crate) estimated_time: String,
    pub(crate) cut_distance: String,
    pub(crate) board_dimensions: String,
    pub(crate) png_progress: Arc<AtomicU32>,
    pub(crate) gcode_progress: Arc<AtomicU32>,
    pub(crate) rasterize_stale: bool,
    pub(crate) gcode_stale: bool,
    pub(crate) expanded_step: Option<u8>,
    pub(crate) settings_groups_open: [bool; 4],
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            stackup: Stackup::new(),
            loaded_png_path: None,
            loaded_inputs: Vec::new(),
            gerber_to_png: StepState::default(),
            png_to_gcode: StepState::default(),
            generated_pngs: None,
            generated_gcode: None,
            gerber_to_png_progress: 0.0,
            png_to_gcode_progress: 0.0,
            status: String::new(),
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
            png_progress: Arc::new(AtomicU32::new(0)),
            gcode_progress: Arc::new(AtomicU32::new(0)),
            rasterize_stale: false,
            gcode_stale: false,
            expanded_step: Some(1),
            settings_groups_open: [true, true, false, false],
        }
    }
}

impl AppState {
    fn get_settings(&self) -> ConversionSettings {
        let dpi = self.dpi_input.parse::<f32>().unwrap_or(DEFAULT_DPI);
        let mut settings = ConversionSettings::default();
        settings.pixels_per_mm = dpi / MM_PER_INCH;
        settings.threshold = 128;
        settings.safe_z_mm = self.safe_z_mm_input.parse::<f32>().unwrap_or(2.0);
        settings.cut_z_mm = self.cut_z_mm_input.parse::<f32>().unwrap_or(-0.1);
        settings.feed_rate_mm_min = self.feed_rate_input.parse::<f32>().unwrap_or(300.0);
        settings.plunge_rate_mm_min = self.plunge_rate_input.parse::<f32>().unwrap_or(120.0);
        settings.spindle_speed_rpm = self.spindle_speed_input.parse::<u32>().unwrap_or(12000);
        settings.tool_diameter_mm = self.tool_diameter_mm_input.parse::<f32>().unwrap_or(0.4);
        settings.offset_number = self.offset_number_input.parse::<u32>().unwrap_or(4);
        settings.offset_stepover = self.offset_stepover_input.parse::<f32>().unwrap_or(0.5);
        settings
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    SelectCopperFiles,
    SelectOutlineFiles,
    SelectDrillFiles,
    CopperFilesPicked(Option<Vec<PathBuf>>),
    OutlineFilesPicked(Option<Vec<PathBuf>>),
    DrillFilesPicked(Option<Vec<PathBuf>>),
    SelectGerberFiles,
    GerberFilesPicked(Option<Vec<PathBuf>>),
    OverrideLayer { index: usize, category: LayerCategory, side: Side },
    LoadPng,
    LoadPngPicked(Option<PathBuf>),
    ConvertToPng,
    ConvertToPngFinished(Result<PngLayerResults, String>),
    SaveCopperPng,
    SaveDrillPng,
    SaveOutlinePng,
    SaveAllPngs,
    SaveAllPngsFinished((Option<PathBuf>, PngLayerResults)),
    SavePngPathPicked((Option<PathBuf>, PathBuf)),
    GenerateGcode,
    GenerateGcodeFinished(Result<GcodeResult, String>),
    SaveGcode,
    GcodeSavePathPicked(Option<PathBuf>),
    PollProgress,
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
    ClearPng,
    StepToggled(u8),
    RemoveFile { index: usize },
    FileDropped(PathBuf),
    SettingsGroupToggled(usize),
    ReRunRasterize,
    ReRunGcode,
    RunAll,
}

fn derive_loaded_inputs(state: &AppState) -> Vec<String> {
    state.stackup.layers.iter().map(|layer| {
        let cat = layer.effective_category();
        let side = layer.effective_side();
        format!("[{} + {}] {}", cat.label(), side.label(), layer.filename())
    }).collect()
}

fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::SelectCopperFiles => {
            return Task::perform(
                async {
                    rfd::FileDialog::new()
                        .set_title("Select Gerber / Drill files (Copper)")
                        .pick_files()
                },
                Message::CopperFilesPicked,
            );
        }
        Message::SelectOutlineFiles => {
            return Task::perform(
                async {
                    rfd::FileDialog::new()
                        .set_title("Select Gerber / Drill files (Outline)")
                        .pick_files()
                },
                Message::OutlineFilesPicked,
            );
        }
        Message::SelectDrillFiles => {
            return Task::perform(
                async {
                    rfd::FileDialog::new()
                        .set_title("Select Gerber / Drill files (Drill)")
                        .pick_files()
                },
                Message::DrillFilesPicked,
            );
        }
        Message::CopperFilesPicked(files) => {
            if let Some(files) = files {
                let count = files.len();
                state.stackup.layers.retain(|l| l.effective_category() != LayerCategory::Copper);
                for path in files {
                    state.stackup.layers.push(LayerFile::new(path, LayerCategory::Copper, Side::Top));
                }
                state.loaded_inputs = derive_loaded_inputs(state);
                state.gerber_to_png = StepState::Ready;
                state.status = format!("Copper layers loaded ({count} files).");
                state.rasterize_stale = state.gerber_to_png == StepState::Complete;
                state.gcode_stale = state.png_to_gcode == StepState::Complete;
            }
        }
        Message::OutlineFilesPicked(files) => {
            if let Some(files) = files {
                let count = files.len();
                state.stackup.layers.retain(|l| l.effective_category() != LayerCategory::Outline);
                for path in files {
                    state.stackup.layers.push(LayerFile::new(path, LayerCategory::Outline, Side::All));
                }
                state.loaded_inputs = derive_loaded_inputs(state);
                state.gerber_to_png = StepState::Ready;
                state.status = format!("Outline loaded ({count} files).");
                state.rasterize_stale = state.gerber_to_png == StepState::Complete;
                state.gcode_stale = state.png_to_gcode == StepState::Complete;
            }
        }
        Message::DrillFilesPicked(files) => {
            if let Some(files) = files {
                let count = files.len();
                state.stackup.layers.retain(|l| l.effective_category() != LayerCategory::Drill);
                for path in files {
                    state.stackup.layers.push(LayerFile::new(path, LayerCategory::Drill, Side::All));
                }
                state.loaded_inputs = derive_loaded_inputs(state);
                state.gerber_to_png = StepState::Ready;
                state.status = format!("Drills loaded ({count} files).");
                state.rasterize_stale = state.gerber_to_png == StepState::Complete;
                state.gcode_stale = state.png_to_gcode == StepState::Complete;
            }
        }
        Message::LoadPng => {
            return Task::perform(
                async {
                    rfd::FileDialog::new()
                        .add_filter("PNG image", &["png"])
                        .set_title("Load existing rendered PNG")
                        .pick_file()
                },
                Message::LoadPngPicked,
            );
        }
        Message::LoadPngPicked(file) => {
            if let Some(path) = file {
                state.loaded_png_path = Some(path.clone());
                state.generated_pngs = None;
                state.gerber_to_png = StepState::Idle;
                state.gcode_stale = state.png_to_gcode == StepState::Complete;
                state.png_to_gcode = StepState::Ready;
                state.status = "PNG loaded. Ready to generate G-code.".into();
            }
        }
        Message::ConvertToPng => {
            let (copper, outline, drill) = state.stackup.milling_paths();

            if copper.is_empty() && outline.is_empty() && drill.is_empty() {
                warn!("convert to png requested with no inputs loaded");
                state.status = "Load Gerber files before converting.".to_owned();
                return Task::none();
            }

            let settings = state.get_settings();
            let output_dir = std::env::temp_dir().join("easymill-render");
            let _ = std::fs::create_dir_all(&output_dir);
            let output_stem = "board".to_owned();

            state.gerber_to_png = StepState::Running;
            state.status = "Rendering Gerber to PNG...".to_owned();

            let png_progress = state.png_progress.clone();
            png_progress.store(0, Ordering::Relaxed);

            let on_progress: Option<easymill::conversion::ProgressFn> = Some(Box::new(move |p: f32| {
                png_progress.store((p * 1000.0) as u32, Ordering::Relaxed);
            }));

            return Task::perform(
                async move {
                    spawn_blocking(move || {
                        gerber_inputs_to_png(&copper, &outline, &drill, &output_dir, &output_stem, settings, on_progress)
                            .map_err(|e| e.to_string())
                    })
                    .await
                    .unwrap_or_else(|join_err| {
                        Err(format!("Blocking task failed: {join_err}"))
                    })
                },
                Message::ConvertToPngFinished,
            );
        }
        Message::ConvertToPngFinished(result) => match result {
            Ok(layers) => {
                state.generated_pngs = Some(layers.clone());
                state.gerber_to_png = StepState::Complete;
                state.rasterize_stale = false;
                state.gerber_to_png_progress = 1.0;
                state.png_progress.store(1000, Ordering::Relaxed);
                state.png_to_gcode = StepState::Ready;
                state.loaded_png_path = Some(layers.copper.path.clone());
                state.status = format!(
                    "Rendered 3 PNGs ({}x{}). Copper: {} dark px. Ready for G-code.",
                    layers.copper.width,
                    layers.copper.height,
                    layers.copper.dark_pixels,
                );
            }
            Err(err) => {
                state.gerber_to_png = StepState::Ready;
                state.status = format!("Rendering failed: {err}");
                error!("gerber to 3-png conversion failed: {err}");
            }
        },
        Message::SaveCopperPng => {
            let src = match &state.generated_pngs {
                Some(p) => p.copper.path.clone(),
                None => {
                    state.status = "Run the conversion before saving PNGs.".to_owned();
                    return Task::none();
                }
            };
            return Task::perform(
                async move {
                    let dest = rfd::FileDialog::new()
                        .add_filter("PNG", &["png"])
                        .set_file_name("traces.png")
                        .set_title("Save copper traces PNG")
                        .save_file();
                    (dest, src)
                },
                Message::SavePngPathPicked,
            );
        }
        Message::SaveDrillPng => {
            let src = match &state.generated_pngs {
                Some(p) => p.drills.path.clone(),
                None => {
                    state.status = "Run the conversion before saving PNGs.".to_owned();
                    return Task::none();
                }
            };
            return Task::perform(
                async move {
                    let dest = rfd::FileDialog::new()
                        .add_filter("PNG", &["png"])
                        .set_file_name("drills.png")
                        .set_title("Save drills PNG")
                        .save_file();
                    (dest, src)
                },
                Message::SavePngPathPicked,
            );
        }
        Message::SaveOutlinePng => {
            let src = match &state.generated_pngs {
                Some(p) => p.outline.path.clone(),
                None => {
                    state.status = "Run the conversion before saving PNGs.".to_owned();
                    return Task::none();
                }
            };
            return Task::perform(
                async move {
                    let dest = rfd::FileDialog::new()
                        .add_filter("PNG", &["png"])
                        .set_file_name("outline.png")
                        .set_title("Save board outline PNG")
                        .save_file();
                    (dest, src)
                },
                Message::SavePngPathPicked,
            );
        }
        Message::SaveAllPngs => {
            let pngs = match &state.generated_pngs {
                Some(p) => p.clone(),
                None => {
                    state.status = "Run the conversion before saving PNGs.".to_owned();
                    return Task::none();
                }
            };
            return Task::perform(
                async move {
                    let dir = rfd::FileDialog::new()
                        .set_title("Select directory to save all 3 PNGs")
                        .pick_folder();
                    (dir, pngs)
                },
                Message::SaveAllPngsFinished,
            );
        }
        Message::SaveAllPngsFinished((dir_opt, pngs)) => {
            if let Some(dir) = dir_opt {
                for (result, name) in [
                    (&pngs.copper, "traces.png"),
                    (&pngs.drills, "drills.png"),
                    (&pngs.outline, "outline.png"),
                ] {
                    let dest = dir.join(name);
                    if let Err(err) = fs::copy(&result.path, &dest) {
                        error!("failed to save {name}: {err}");
                    }
                }
                state.status = format!("Saved 3 PNGs to {}.", dir.display());
            }
        }
        Message::SavePngPathPicked((dest, src)) => {
            if let Some(dest) = dest {
                match fs::copy(&src, &dest) {
                    Ok(_) => {
                        info!(src = %src.display(), dest = %dest.display(), "saved rendered png");
                        state.status = format!("Saved PNG to {}.", path_to_label(dest));
                    }
                    Err(err) => {
                        error!(src = %src.display(), dest = %dest.display(), error = %err, "failed to save png");
                        state.status = format!("Failed to save PNG: {err}");
                    }
                }
            } else {
                state.status = "Save canceled.".to_owned();
            }
        }
        Message::GenerateGcode => {
            let png_path = match &state.loaded_png_path {
                Some(p) => p.to_string_lossy().into_owned(),
                None => {
                    warn!("generate gcode requested with no png loaded");
                    state.status = "Generate a PNG or load one first.".to_owned();
                    return Task::none();
                }
            };

            let settings = state.get_settings();

            state.png_to_gcode = StepState::Running;
            state.status = "Generating G-code from PNG...".to_owned();

            let gcode_progress = state.gcode_progress.clone();
            gcode_progress.store(0, Ordering::Relaxed);

            let on_progress: Option<easymill::conversion::ProgressFn> = Some(Box::new(move |p: f32| {
                gcode_progress.store((p * 1000.0) as u32, Ordering::Relaxed);
            }));

            return Task::perform(
                async move {
                    spawn_blocking(move || {
                        png_to_gcode(&PathBuf::from(&png_path), settings, on_progress)
                            .map_err(|e| e.to_string())
                    })
                    .await
                    .unwrap_or_else(|join_err| {
                        Err(format!("Blocking task failed: {join_err}"))
                    })
                },
                Message::GenerateGcodeFinished,
            );
        }
        Message::GenerateGcodeFinished(result) => match result {
            Ok(gcode_result) => {
                state.generated_gcode = Some(gcode_result.gcode);
                state.png_to_gcode = StepState::Complete;
                state.gcode_stale = false;
                state.png_to_gcode_progress = 1.0;
                state.gcode_progress.store(1000, Ordering::Relaxed);

                let time_secs = gcode_result.estimated_time_secs;
                let hours = (time_secs / 3600.0) as u32;
                let mins = ((time_secs % 3600.0) / 60.0) as u32;
                let secs = (time_secs % 60.0) as u32;
                state.estimated_time = format!("{hours:02}:{mins:02}:{secs:02}");
                state.cut_distance =
                    format!("{:.1} mm", gcode_result.cut_distance_mm);
                state.board_dimensions = format!(
                    "{:.1} x {:.1} mm",
                    gcode_result.width_mm, gcode_result.height_mm
                );

                state.status = format!(
                    "Generated G-code: {} paths, {:.1}m cut, est. {:02}:{:02}:{:02}.",
                    gcode_result.paths.len(),
                    gcode_result.cut_distance_mm / 1000.0,
                    hours,
                    mins,
                    secs,
                );
            }
            Err(err) => {
                state.png_to_gcode = StepState::Ready;
                state.status = format!("G-code generation failed: {err}");
                error!("png to gcode conversion failed: {err}");
            }
        },
        Message::SaveGcode => {
            if state.generated_gcode.is_none() {
                warn!("save requested before gcode was generated");
                state.status = "Run the pipeline before saving G-code.".to_owned();
                return Task::none();
            }

            info!("opening gcode save dialog");
            return Task::perform(
                async {
                    rfd::FileDialog::new()
                        .add_filter("G-code", &["gcode", "nc", "tap"])
                        .set_file_name("output.gcode")
                        .set_title("Save generated G-code")
                        .save_file()
                },
                Message::GcodeSavePathPicked,
            );
        }
        Message::GcodeSavePathPicked(path) => {
            if let Some(path) = path {
                if let Some(gcode) = &state.generated_gcode {
                    match fs::write(&path, gcode) {
                        Ok(()) => {
                            info!(
                                path = %path.display(),
                                bytes = gcode.len(),
                                "saved generated gcode"
                            );
                            state.status =
                                format!("Saved generated G-code to {}.", path_to_label(path));
                        }
                        Err(err) => {
                            error!(
                                path = %path.display(),
                                error = %err,
                                "failed to save generated gcode"
                            );
                            state.status = format!("Failed to save G-code: {err}");
                        }
                    }
                } else {
                    warn!("save path selected but generated gcode was unavailable");
                    state.status = "No generated G-code available to save.".to_owned();
                }
            } else {
                info!("gcode save canceled");
                state.status = "Save canceled.".to_owned();
            }
        }
        Message::PollProgress => {
            if state.gerber_to_png == StepState::Running {
                let p = state.png_progress.load(Ordering::Relaxed);
                state.gerber_to_png_progress = p as f32 / 1000.0;
            }
            if state.png_to_gcode == StepState::Running {
                let p = state.gcode_progress.load(Ordering::Relaxed);
                state.png_to_gcode_progress = p as f32 / 1000.0;
            }
        }
        Message::Reset => {
            info!("resetting application state");
            *state = AppState::default();
        }
        Message::DpiChanged(val) => {
            state.dpi_input = val;
            state.rasterize_stale = state.gerber_to_png == StepState::Complete;
            state.gcode_stale = state.png_to_gcode == StepState::Complete;
        }
        Message::CutZChanged(val) => {
            state.cut_z_mm_input = val;
            state.gcode_stale = state.png_to_gcode == StepState::Complete;
        }
        Message::SafeZChanged(val) => {
            state.safe_z_mm_input = val;
            state.gcode_stale = state.png_to_gcode == StepState::Complete;
        }
        Message::FeedRateChanged(val) => {
            state.feed_rate_input = val;
            state.gcode_stale = state.png_to_gcode == StepState::Complete;
        }
        Message::PlungeRateChanged(val) => {
            state.plunge_rate_input = val;
            state.gcode_stale = state.png_to_gcode == StepState::Complete;
        }
        Message::SpindleSpeedChanged(val) => {
            state.spindle_speed_input = val;
            state.gcode_stale = state.png_to_gcode == StepState::Complete;
        }
        Message::ToolDiameterChanged(val) => {
            state.tool_diameter_mm_input = val;
            state.gcode_stale = state.png_to_gcode == StepState::Complete;
        }
        Message::OffsetNumberChanged(val) => {
            state.offset_number_input = val;
            state.gcode_stale = state.png_to_gcode == StepState::Complete;
        }
        Message::OffsetStepoverChanged(val) => {
            state.offset_stepover_input = val;
            state.gcode_stale = state.png_to_gcode == StepState::Complete;
        }
        Message::ClearPng => {
            state.loaded_png_path = None;
            state.gerber_to_png = StepState::Idle;
            state.png_to_gcode = StepState::Ready;
            state.gcode_stale = state.png_to_gcode == StepState::Complete;
        }
        Message::StepToggled(n) => {
            state.expanded_step = if state.expanded_step == Some(n) { None } else { Some(n) };
        }
        Message::SelectGerberFiles => {
            return Task::perform(
                async {
                    rfd::FileDialog::new()
                        .set_title("Select Gerber or Excellon files")
                        .pick_files()
                },
                Message::GerberFilesPicked,
            );
        }
        Message::GerberFilesPicked(files) => {
            if let Some(files) = files {
                let count = files.len();
                let detector = easymill::stackup::LayerDetector::new();
                for path in files {
                    let filename = path.file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let (cat, side) = detector.detect(&filename);
                    state.stackup.layers.push(LayerFile::new(path, cat, side));
                }
                state.loaded_inputs = derive_loaded_inputs(state);
                state.gerber_to_png = StepState::Ready;
                state.status = format!("Added {count} files via auto-detect.");
                state.rasterize_stale = state.gerber_to_png == StepState::Complete;
                state.gcode_stale = state.png_to_gcode == StepState::Complete;
            }
        }
        Message::FileDropped(path) => {
            let detector = easymill::stackup::LayerDetector::new();
            let filename = path.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let (cat, side) = detector.detect(&filename);
            state.stackup.layers.push(LayerFile::new(path, cat, side));
            state.loaded_inputs = derive_loaded_inputs(state);
            state.gerber_to_png = StepState::Ready;
            state.status = format!("File dropped: {filename}");
            state.rasterize_stale = state.gerber_to_png == StepState::Complete;
            state.gcode_stale = state.png_to_gcode == StepState::Complete;
        }
        Message::OverrideLayer { index, category, side } => {
            if let Some(layer) = state.stackup.layers.get_mut(index) {
                layer.user_category = if layer.auto_category == category { None } else { Some(category) };
                layer.user_side = if layer.auto_side == side { None } else { Some(side) };
                state.rasterize_stale = state.gerber_to_png == StepState::Complete;
                state.gcode_stale = state.png_to_gcode == StepState::Complete;
                state.loaded_inputs = derive_loaded_inputs(state);
            }
        }
        Message::RemoveFile { index } => {
            if index < state.stackup.layers.len() {
                state.stackup.layers.remove(index);
                state.rasterize_stale = state.gerber_to_png == StepState::Complete;
                state.gcode_stale = state.png_to_gcode == StepState::Complete;
                state.loaded_inputs = derive_loaded_inputs(state);
            }
        }
        Message::SettingsGroupToggled(i) => {
            if i < 4 {
                state.settings_groups_open[i] = !state.settings_groups_open[i];
            }
        }
        Message::ReRunRasterize => {
            state.rasterize_stale = false;
            state.gcode_stale = state.png_to_gcode == StepState::Complete;
            return update(state, Message::ConvertToPng);
        }
        Message::ReRunGcode => {
            state.gcode_stale = false;
            return update(state, Message::GenerateGcode);
        }
        Message::RunAll => {
            let (copper, outline, drill) = state.stackup.milling_paths();
            let has_gerbers = !copper.is_empty() || !outline.is_empty() || !drill.is_empty();
            let rasterize_needed = has_gerbers
                && (state.gerber_to_png != StepState::Complete || state.rasterize_stale);
            let gcode_needed = state.loaded_png_path.is_some()
                && (state.png_to_gcode != StepState::Complete || state.gcode_stale);
            if rasterize_needed {
                state.rasterize_stale = false;
                return update(state, Message::ConvertToPng);
            } else if gcode_needed {
                state.gcode_stale = false;
                return update(state, Message::GenerateGcode);
            }
        }
    }

    Task::none()
}

fn theme(_state: &AppState) -> Theme {
    Theme::TokyoNight
}

fn view(state: &AppState) -> Element<'_, Message> {
    use ui::{sidebar, step_canvas};

    let layout = iced::widget::row![
        sidebar(state),
        scrollable(step_canvas(state))
            .height(Length::Fill)
            .width(Length::Fill),
    ]
    .height(Length::Fill);

    container(layout)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(ui::styles::app_style())
        .into()
}

fn subscription(state: &AppState) -> Subscription<Message> {
    let progress_sub = if state.gerber_to_png == StepState::Running || state.png_to_gcode == StepState::Running {
        iced::time::every(Duration::from_millis(100))
            .map(|_| Message::PollProgress)
    } else {
        Subscription::none()
    };

    let dnd_sub = event::listen_with(
        |event: iced::event::Event, _status: iced::event::Status, _window: iced::window::Id| {
            if let iced::event::Event::Window(iced::window::Event::FileDropped(path)) = event {
                Some(Message::FileDropped(path))
            } else {
                None
            }
        },
    );

    Subscription::batch(vec![progress_sub, dnd_sub])
}

pub fn main() -> iced::Result {
    if let Err(err) = init_logging() {
        eprintln!("failed to initialize logging: {err}");
    }
    info!("starting easymill ui");

    iced::application(AppState::default, update, view)
        .subscription(subscription)
        .theme(theme)
        .title("EasyMill")
        .run()
}

fn path_to_label(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}


