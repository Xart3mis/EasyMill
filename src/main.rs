use easymill::conversion::{
    ConversionSettings, DEFAULT_DPI, MM_PER_INCH, GcodeResult, PngLayerResults,
    gerber_inputs_to_png, png_to_gcode,
};
use easymill::logging::init_logging;
use iced::{
    self, Alignment, Element, Length, Subscription, Task, Theme,
    widget::{column, container, row},
};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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

impl StepState {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Idle => "WAITING",
            Self::Ready => "READY",
            Self::Running => "RUNNING",
            Self::Complete => "DONE",
        }
    }

    pub(crate) fn progress(self) -> f32 {
        match self {
            Self::Idle => 0.0,
            Self::Ready => 0.45,
            Self::Running => 0.70,
            Self::Complete => 1.0,
        }
    }
}

pub(crate) struct AppState {
    pub(crate) copper_paths: Vec<PathBuf>,
    pub(crate) outline_paths: Vec<PathBuf>,
    pub(crate) drill_paths: Vec<PathBuf>,
    pub(crate) loaded_png_path: Option<PathBuf>,
    pub(crate) loaded_inputs: Vec<String>,
    pub(crate) gerber_to_png: StepState,
    pub(crate) png_to_gcode: StepState,
    pub(crate) generated_png_path: Option<String>,
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
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            copper_paths: Vec::new(),
            outline_paths: Vec::new(),
            drill_paths: Vec::new(),
            loaded_png_path: None,
            loaded_inputs: Vec::new(),
            gerber_to_png: StepState::default(),
            png_to_gcode: StepState::default(),
            generated_png_path: None,
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
    LoadPng,
    LoadPngPicked(Option<PathBuf>),
    ConvertToPng,
    ConvertToPngFinished(Result<PngLayerResults, String>),
    SavePng,
    SavePngPathPicked(Option<PathBuf>),
    GenerateGcode,
    GenerateGcodeFinished(Result<GcodeResult, String>),
    SaveGcode,
    GcodeSavePathPicked(Option<PathBuf>),
    GerberProgress(f32),
    GcodeProgress(f32),
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
}

fn derive_loaded_inputs(state: &AppState) -> Vec<String> {
    let mut labels = Vec::new();
    for p in &state.copper_paths {
        labels.push(format!("[Copper] {}", path_to_label(p.clone())));
    }
    for p in &state.outline_paths {
        labels.push(format!("[Outline] {}", path_to_label(p.clone())));
    }
    for p in &state.drill_paths {
        labels.push(format!("[Drill] {}", path_to_label(p.clone())));
    }
    labels
}

fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::SelectCopperFiles => {
            return Task::perform(
                async {
                    rfd::FileDialog::new()
                        .add_filter("Copper", &["gbr", "grb", "gtl", "gbl"])
                        .set_title("Select Copper Layer files")
                        .pick_files()
                },
                Message::CopperFilesPicked,
            );
        }
        Message::SelectOutlineFiles => {
            return Task::perform(
                async {
                    rfd::FileDialog::new()
                        .add_filter("Board Outline", &["gko", "gbr", "ou"])
                        .set_title("Select Board Profile / Outline files")
                        .pick_files()
                },
                Message::OutlineFilesPicked,
            );
        }
        Message::SelectDrillFiles => {
            return Task::perform(
                async {
                    rfd::FileDialog::new()
                        .add_filter("Drill", &["drl", "txt", "xln"])
                        .set_title("Select Drill files")
                        .pick_files()
                },
                Message::DrillFilesPicked,
            );
        }
        Message::CopperFilesPicked(files) => {
            if let Some(files) = files {
                state.copper_paths = files;
                state.loaded_inputs = derive_loaded_inputs(state);
                state.gerber_to_png = StepState::Ready;
                state.status = "Copper layers loaded.".into();
            }
        }
        Message::OutlineFilesPicked(files) => {
            if let Some(files) = files {
                state.outline_paths = files;
                state.loaded_inputs = derive_loaded_inputs(state);
                state.status = "Outline loaded.".into();
            }
        }
        Message::DrillFilesPicked(files) => {
            if let Some(files) = files {
                state.drill_paths = files;
                state.loaded_inputs = derive_loaded_inputs(state);
                state.status = "Drills loaded.".into();
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
                state.generated_png_path = Some(path.to_string_lossy().into_owned());
                state.png_to_gcode = StepState::Ready;
                state.status = "PNG loaded. Ready to generate G-code.".into();
            }
        }
        Message::ConvertToPng => {
            let copper = state.copper_paths.clone();
            let outline = state.outline_paths.clone();
            let drill = state.drill_paths.clone();

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
                state.generated_png_path =
                    Some(layers.copper.path.to_string_lossy().into_owned());
                state.gerber_to_png = StepState::Complete;
                state.gerber_to_png_progress = 1.0;
                state.png_progress.store(1000, Ordering::Relaxed);
                state.png_to_gcode = StepState::Ready;
                state.status = format!(
                    "Rendered {}x{} traces ({} dark px). Ready to generate G-code.",
                    layers.copper.width,
                    layers.copper.height,
                    layers.copper.dark_pixels,
                );
            }
            Err(err) => {
                state.gerber_to_png = StepState::Ready;
                state.status = format!("Rendering failed: {err}");
                error!("gerber to png conversion failed: {err}");
            }
        },
        Message::SavePng => {
            if state.generated_png_path.is_none() {
                warn!("save png requested before png was generated");
                state.status = "Run the conversion before saving PNG.".to_owned();
                return Task::none();
            }

            return Task::perform(
                async {
                    rfd::FileDialog::new()
                        .add_filter("PNG", &["png"])
                        .set_file_name("output.png")
                        .set_title("Save rendered PNG")
                        .save_file()
                },
                Message::SavePngPathPicked,
            );
        }
        Message::SavePngPathPicked(path) => {
            if let Some(dest) = path {
                if let Some(src) = &state.generated_png_path {
                    match fs::copy(src, &dest) {
                        Ok(_) => {
                            info!(src = %src, dest = %dest.display(), "saved rendered png");
                            state.status =
                                format!("Saved PNG to {}.", path_to_label(dest));
                        }
                        Err(err) => {
                            error!(src = %src, dest = %dest.display(), error = %err, "failed to save png");
                            state.status = format!("Failed to save PNG: {err}");
                        }
                    }
                } else {
                    warn!("save path selected but generated png was unavailable");
                    state.status = "No generated PNG available to save.".to_owned();
                }
            } else {
                state.status = "Save canceled.".to_owned();
            }
        }
        Message::GenerateGcode => {
            let png_path = state
                .generated_png_path
                .clone()
                .or_else(|| {
                    state
                        .loaded_png_path
                        .as_ref()
                        .map(|p| p.to_string_lossy().into_owned())
                });

            let png_path = match png_path {
                Some(p) => p,
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
        Message::GerberProgress(p) => {
            state.gerber_to_png_progress = p;
        }
        Message::GcodeProgress(p) => {
            state.png_to_gcode_progress = p;
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
        }
        Message::CutZChanged(val) => {
            state.cut_z_mm_input = val;
        }
        Message::SafeZChanged(val) => {
            state.safe_z_mm_input = val;
        }
        Message::FeedRateChanged(val) => {
            state.feed_rate_input = val;
        }
        Message::PlungeRateChanged(val) => {
            state.plunge_rate_input = val;
        }
        Message::SpindleSpeedChanged(val) => {
            state.spindle_speed_input = val;
        }
        Message::ToolDiameterChanged(val) => {
            state.tool_diameter_mm_input = val;
        }
        Message::OffsetNumberChanged(val) => {
            state.offset_number_input = val;
        }
        Message::OffsetStepoverChanged(val) => {
            state.offset_stepover_input = val;
        }
    }

    Task::none()
}

fn theme(_state: &AppState) -> Theme {
    Theme::TokyoNight
}

fn view(state: &AppState) -> Element<'_, Message> {
    use ui::{header, source_panel, config_panel, pipeline_panel, stats_panel, status_line};

    let left_column = column![source_panel(state), config_panel(state), stats_panel(state)].spacing(16);

    let shell = column![
        header(),
        row![
            container(left_column)
                .width(Length::FillPortion(5))
                .height(Length::Fill),
            container(pipeline_panel(state))
                .width(Length::FillPortion(5))
                .height(Length::Fill),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Start)
        .spacing(16),
        status_line(state),
    ]
    .spacing(16)
    .height(Length::Fill)
    .max_width(1320);

    container(shell)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .padding([20, 24])
        .style(ui::styles::app_style())
        .into()
}

fn subscription(state: &AppState) -> Subscription<Message> {
    if state.gerber_to_png == StepState::Running || state.png_to_gcode == StepState::Running {
        iced::time::every(Duration::from_millis(100))
            .map(|_| Message::PollProgress)
    } else {
        Subscription::none()
    }
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

fn temporary_png_path() -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("easymill-{timestamp}.png"))
}
