use easymill::conversion::{
    ConversionSettings, DEFAULT_DPI, MM_PER_INCH, GcodeResult, PngLayerResults,
    gerber_inputs_to_png, png_to_gcode,
};
use easymill::logging::init_logging;
use webbrowser;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GcodeSide {
    Top,
    Bottom,
}


pub(crate) struct AppState {
    pub(crate) stackup: Stackup,
    pub(crate) editing_layer: Option<usize>,
    pub(crate) loaded_top_png_path: Option<PathBuf>,
    pub(crate) loaded_bottom_png_path: Option<PathBuf>,
    pub(crate) loaded_inputs: Vec<String>,
    pub(crate) gerber_to_png: StepState,
    pub(crate) png_to_gcode: StepState,
    pub(crate) generated_pngs: Option<PngLayerResults>,
    pub(crate) generated_top_gcode: Option<GcodeResult>,
    pub(crate) generated_bottom_gcode: Option<GcodeResult>,
    pub(crate) active_gcode_side: GcodeSide,
    pub(crate) gcode_side_indicator: String,
    pub(crate) mirror_bottom: bool,
    pub(crate) mirror_top: bool,
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
    pub(crate) mods_server_port: Option<u16>,
    pub(crate) pending_mods_open: Option<std::path::PathBuf>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            stackup: Stackup::new(),
            editing_layer: None,
            loaded_top_png_path: None,
            loaded_bottom_png_path: None,
            loaded_inputs: Vec::new(),
            gerber_to_png: StepState::default(),
            png_to_gcode: StepState::default(),
            generated_pngs: None,
            generated_top_gcode: None,
            generated_bottom_gcode: None,
            active_gcode_side: GcodeSide::Top,
            gcode_side_indicator: "Top".to_owned(),
            mirror_bottom: true,
            mirror_top: false,
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
            mods_server_port: None,
            pending_mods_open: None,
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
        settings.mirror_bottom = self.mirror_bottom;
        settings.mirror_top = self.mirror_top;
        settings
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    SelectGerberFiles,
    GerberFilesPicked(Option<Vec<PathBuf>>),
    OverrideLayer { index: usize, category: LayerCategory, side: Side },
    LoadPng,
    LoadTopPngPicked(Option<PathBuf>),
    LoadBottomPng,
    LoadBottomPngPicked(Option<PathBuf>),
    ConvertToPng,
    ConvertToPngFinished(Result<PngLayerResults, String>),
    SaveCopperPng,
    SaveCopperBottomPng,
    SaveDrillPng,
    SaveOutlinePng,
    SaveAllPngs,
    SaveAllPngsFinished((Option<PathBuf>, PngLayerResults)),
    SavePngPathPicked((Option<PathBuf>, PathBuf)),
    GenerateGcode,
    GenerateGcodeFinished(Result<GcodeResult, String>),
    GenerateBothGcode,
    GenerateBothGcodeFinished(Result<(GcodeResult, GcodeResult), String>),
    SaveGcode,
    GcodeSavePathPicked(Option<PathBuf>),
    SwitchGcodeSide(GcodeSide),
    MirrorBottomToggled(bool),
    MirrorTopToggled(bool),
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
    EditLayerToggle(usize),
    SetLayerCategory { index: usize, category: LayerCategory },
    SetLayerSide { index: usize, side: Side },
    ResetLayerOverride(usize),
    FileDropped(PathBuf),
    OpenInMods(PathBuf),
    ModsServerStarted(u16),
    SettingsGroupToggled(usize),
    ReRunRasterize,
    ReRunGcode,
    RunAll,
    ToggleLayerExclude(usize),
}

async fn start_mods_server() -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind mods server");
    let port = listener.local_addr().expect("no local addr").port();
    let render_dir = std::env::temp_dir().join("easymill-render");

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { continue };
            let dir = render_dir.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let raw_path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                let filename = raw_path.trim_start_matches('/');
                let is_safe = !filename.is_empty()
                    && !filename.contains("..")
                    && filename.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.');
                if !is_safe {
                    let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n").await;
                    return;
                }
                match tokio::fs::read(dir.join(filename)).await {
                    Ok(data) => {
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
                            data.len()
                        );
                        let _ = stream.write_all(header.as_bytes()).await;
                        let _ = stream.write_all(&data).await;
                    }
                    Err(_) => {
                        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\n\r\n").await;
                    }
                }
            });
        }
    });

    port
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
        Message::LoadPng => {
            return Task::perform(
                async {
                    rfd::FileDialog::new()
                        .add_filter("PNG image", &["png"])
                        .set_title("Load top traces PNG")
                        .pick_file()
                },
                Message::LoadTopPngPicked,
            );
        }
        Message::LoadTopPngPicked(file) => {
            if let Some(path) = file {
                state.loaded_top_png_path = Some(path.clone());
                state.generated_pngs = None;
                state.gerber_to_png = StepState::Idle;
                state.gcode_stale = state.png_to_gcode == StepState::Complete;
                state.png_to_gcode = StepState::Ready;
                state.status = "Top PNG loaded. Ready to generate G-code.".into();
            }
        }
        Message::LoadBottomPng => {
            return Task::perform(
                async {
                    rfd::FileDialog::new()
                        .add_filter("PNG image", &["png"])
                        .set_title("Load bottom traces PNG")
                        .pick_file()
                },
                Message::LoadBottomPngPicked,
            );
        }
        Message::LoadBottomPngPicked(file) => {
            if let Some(path) = file {
                state.loaded_bottom_png_path = Some(path.clone());
                state.generated_pngs = None;
                state.gerber_to_png = StepState::Idle;
                state.gcode_stale = state.png_to_gcode == StepState::Complete;
                state.png_to_gcode = StepState::Ready;
                state.status = "Bottom PNG loaded. Ready to generate G-code.".into();
            }
        }
        Message::ConvertToPng => {
            let (copper_top, copper_bottom, outline, drill) = state.stackup.milling_paths();

            if copper_top.is_empty() && copper_bottom.is_empty() && outline.is_empty() && drill.is_empty() {
                warn!("convert to png requested with no inputs loaded");
                state.status = "Load Gerber files before converting.".to_owned();
                return Task::none();
            }

            for layer in &state.stackup.layers {
                if layer.effective_category() == LayerCategory::Copper && layer.effective_side() == Side::All {
                    warn!("Layer {} has Side::All — treating as top for copper", layer.filename());
                }
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
                        gerber_inputs_to_png(
                            &copper_top, &copper_bottom, &outline, &drill,
                            &output_dir, &output_stem, settings, on_progress,
                        )
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
                state.loaded_top_png_path = Some(layers.copper_top.path.clone());
                state.loaded_bottom_png_path = Some(layers.copper_bottom.path.clone());
                state.status = format!(
                    "Rendered 4 PNGs ({}x{}). Top: {} dark px, Bottom: {} dark px. Ready for G-code.",
                    layers.copper_top.width,
                    layers.copper_top.height,
                    layers.copper_top.dark_pixels,
                    layers.copper_bottom.dark_pixels,
                );
            }
            Err(err) => {
                state.gerber_to_png = StepState::Ready;
                state.status = format!("Rendering failed: {err}");
                error!("gerber to 4-png conversion failed: {err}");
            }
        },
        Message::SaveCopperPng => {
            let src = match &state.generated_pngs {
                Some(p) => p.copper_top.path.clone(),
                None => {
                    state.status = "Run the conversion before saving PNGs.".to_owned();
                    return Task::none();
                }
            };
            return Task::perform(
                async move {
                    let dest = rfd::FileDialog::new()
                        .add_filter("PNG", &["png"])
                        .set_file_name("traces_top.png")
                        .set_title("Save top traces PNG")
                        .save_file();
                    (dest, src)
                },
                Message::SavePngPathPicked,
            );
        }
        Message::SaveCopperBottomPng => {
            let src = match &state.generated_pngs {
                Some(p) => p.copper_bottom.path.clone(),
                None => {
                    state.status = "Run the conversion before saving PNGs.".to_owned();
                    return Task::none();
                }
            };
            return Task::perform(
                async move {
                    let dest = rfd::FileDialog::new()
                        .add_filter("PNG", &["png"])
                        .set_file_name("traces_bot.png")
                        .set_title("Save bottom traces PNG")
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
                        .set_title("Select directory to save all 4 PNGs")
                        .pick_folder();
                    (dir, pngs)
                },
                Message::SaveAllPngsFinished,
            );
        }
        Message::SaveAllPngsFinished((dir_opt, pngs)) => {
            if let Some(dir) = dir_opt {
                for (result, name) in [
                    (&pngs.copper_top, "traces_top.png"),
                    (&pngs.copper_bottom, "traces_bot.png"),
                    (&pngs.drills, "drills.png"),
                    (&pngs.outline, "outline.png"),
                ] {
                    let dest = dir.join(name);
                    if let Err(err) = fs::copy(&result.path, &dest) {
                        error!("failed to save {name}: {err}");
                    }
                }
                state.status = format!("Saved 4 PNGs to {}.", dir.display());
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
            let png_path = match state.active_gcode_side {
                GcodeSide::Top => &state.loaded_top_png_path,
                GcodeSide::Bottom => &state.loaded_bottom_png_path,
            };
            let png_path = match png_path {
                Some(p) => p.to_string_lossy().into_owned(),
                None => {
                    let side_label = match state.active_gcode_side {
                        GcodeSide::Top => "top",
                        GcodeSide::Bottom => "bottom",
                    };
                    warn!("generate gcode requested with no {side_label} png loaded");
                    state.status = format!("Generate a {side_label} PNG or load one first.");
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
                let time_secs = gcode_result.estimated_time_secs;
                let cut_dist = gcode_result.cut_distance_mm;
                let width = gcode_result.width_mm;
                let height = gcode_result.height_mm;
                let path_count = gcode_result.paths.len();

                match state.active_gcode_side {
                    GcodeSide::Top => state.generated_top_gcode = Some(gcode_result),
                    GcodeSide::Bottom => state.generated_bottom_gcode = Some(gcode_result),
                }

                state.png_to_gcode = StepState::Complete;
                state.gcode_stale = false;
                state.png_to_gcode_progress = 1.0;
                state.gcode_progress.store(1000, Ordering::Relaxed);

                let hours = (time_secs / 3600.0) as u32;
                let mins = ((time_secs % 3600.0) / 60.0) as u32;
                let secs = (time_secs % 60.0) as u32;
                state.estimated_time = format!("{hours:02}:{mins:02}:{secs:02}");
                state.cut_distance = format!("{:.1} mm", cut_dist);
                state.board_dimensions = format!("{:.1} x {:.1} mm", width, height);

                let side_label = match state.active_gcode_side {
                    GcodeSide::Top => "top",
                    GcodeSide::Bottom => "bottom",
                };

                state.status = format!(
                    "Generated {side_label} G-code: {} paths, {:.1}m cut, est. {:02}:{:02}:{:02}.",
                    path_count,
                    cut_dist / 1000.0,
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
        Message::GenerateBothGcode => {
            let top_path = match &state.loaded_top_png_path {
                Some(p) => p.clone(),
                None => {
                    state.status = "Generate a top PNG first.".to_owned();
                    return Task::none();
                }
            };
            let bottom_path = match &state.loaded_bottom_png_path {
                Some(p) => p.clone(),
                None => {
                    state.status = "Generate a bottom PNG first.".to_owned();
                    return Task::none();
                }
            };

            let settings = state.get_settings();

            state.png_to_gcode = StepState::Running;
            state.status = "Generating G-code for both sides...".to_owned();

            let gcode_progress = state.gcode_progress.clone();
            gcode_progress.store(0, Ordering::Relaxed);

            let on_progress: Option<easymill::conversion::ProgressFn> = Some(Box::new(move |p: f32| {
                gcode_progress.store((p * 1000.0) as u32, Ordering::Relaxed);
            }));

            return Task::perform(
                async move {
                    spawn_blocking(move || {
                        let top_result = png_to_gcode(&top_path, settings.clone(), on_progress)
                            .map_err(|e| e.to_string())?;
                        let bottom_result = png_to_gcode(&bottom_path, settings, None)
                            .map_err(|e| e.to_string())?;
                        Ok((top_result, bottom_result))
                    })
                    .await
                    .unwrap_or_else(|join_err| {
                        Err(format!("Blocking task failed: {join_err}"))
                    })
                },
                |result| match result {
                    Ok((top, bottom)) => Message::GenerateBothGcodeFinished(Ok((top, bottom))),
                    Err(e) => Message::GenerateBothGcodeFinished(Err(e)),
                },
            );
        }
        Message::GenerateBothGcodeFinished(result) => match result {
            Ok((top_result, bottom_result)) => {
                state.generated_top_gcode = Some(top_result);
                state.generated_bottom_gcode = Some(bottom_result);
                state.png_to_gcode = StepState::Complete;
                state.gcode_stale = false;
                state.png_to_gcode_progress = 1.0;
                state.gcode_progress.store(1000, Ordering::Relaxed);

                let gcode_result = match state.active_gcode_side {
                    GcodeSide::Top => state.generated_top_gcode.as_ref().unwrap(),
                    GcodeSide::Bottom => state.generated_bottom_gcode.as_ref().unwrap(),
                };
                let time_secs = gcode_result.estimated_time_secs;
                let hours = (time_secs / 3600.0) as u32;
                let mins = ((time_secs % 3600.0) / 60.0) as u32;
                let secs = (time_secs % 60.0) as u32;
                state.estimated_time = format!("{hours:02}:{mins:02}:{secs:02}");
                state.cut_distance = format!("{:.1} mm", gcode_result.cut_distance_mm);
                state.board_dimensions = format!("{:.1} x {:.1} mm", gcode_result.width_mm, gcode_result.height_mm);

                state.status = "Generated G-code for both sides.".to_owned();
            }
            Err(err) => {
                state.png_to_gcode = StepState::Ready;
                state.status = format!("G-code generation failed: {err}");
                error!("png to gcode conversion failed: {err}");
            }
        },
        Message::SaveGcode => {
            let has_gcode = match state.active_gcode_side {
                GcodeSide::Top => state.generated_top_gcode.is_some(),
                GcodeSide::Bottom => state.generated_bottom_gcode.is_some(),
            };
            if !has_gcode {
                warn!("save requested before gcode was generated");
                state.status = "Run the pipeline before saving G-code.".to_owned();
                return Task::none();
            }

            info!("opening gcode save dialog");
            let filename = match state.active_gcode_side {
                GcodeSide::Top => "output_top.gcode",
                GcodeSide::Bottom => "output_bot.gcode",
            };
            return Task::perform(
                async move {
                    rfd::FileDialog::new()
                        .add_filter("G-code", &["gcode", "nc", "tap"])
                        .set_file_name(filename)
                        .set_title("Save generated G-code")
                        .save_file()
                },
                Message::GcodeSavePathPicked,
            );
        }
        Message::GcodeSavePathPicked(path) => {
            if let Some(path) = path {
                let gcode = match state.active_gcode_side {
                    GcodeSide::Top => state.generated_top_gcode.as_ref().map(|r| r.gcode.clone()),
                    GcodeSide::Bottom => state.generated_bottom_gcode.as_ref().map(|r| r.gcode.clone()),
                };
                if let Some(gcode) = gcode {
                    match fs::write(&path, &gcode) {
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
        Message::SwitchGcodeSide(side) => {
            state.active_gcode_side = side;
            state.gcode_side_indicator = match side {
                GcodeSide::Top => "Top",
                GcodeSide::Bottom => "Bottom",
            }.to_owned();
            if let Some(gcode_result) = match side {
                GcodeSide::Top => state.generated_top_gcode.as_ref(),
                GcodeSide::Bottom => state.generated_bottom_gcode.as_ref(),
            } {
                let time_secs = gcode_result.estimated_time_secs;
                let hours = (time_secs / 3600.0) as u32;
                let mins = ((time_secs % 3600.0) / 60.0) as u32;
                let secs = (time_secs % 60.0) as u32;
                state.estimated_time = format!("{hours:02}:{mins:02}:{secs:02}");
                state.cut_distance = format!("{:.1} mm", gcode_result.cut_distance_mm);
                state.board_dimensions = format!("{:.1} x {:.1} mm", gcode_result.width_mm, gcode_result.height_mm);
            }
        }
        Message::MirrorBottomToggled(val) => {
            state.mirror_bottom = val;
            state.rasterize_stale = state.gerber_to_png == StepState::Complete;
            state.gcode_stale = state.png_to_gcode == StepState::Complete;
        }
        Message::MirrorTopToggled(val) => {
            state.mirror_top = val;
            state.rasterize_stale = state.gerber_to_png == StepState::Complete;
            state.gcode_stale = state.png_to_gcode == StepState::Complete;
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
            state.loaded_top_png_path = None;
            state.loaded_bottom_png_path = None;
            state.generated_pngs = None;
            state.generated_top_gcode = None;
            state.generated_bottom_gcode = None;
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
                state.editing_layer = match state.editing_layer {
                    Some(i) if i == index => None,
                    Some(i) if i > index => Some(i - 1),
                    other => other,
                };
                state.rasterize_stale = state.gerber_to_png == StepState::Complete;
                state.gcode_stale = state.png_to_gcode == StepState::Complete;
                state.loaded_inputs = derive_loaded_inputs(state);
            }
        }
        Message::ToggleLayerExclude(index) => {
            if let Some(layer) = state.stackup.layers.get_mut(index) {
                layer.excluded = !layer.excluded;
                state.rasterize_stale = state.gerber_to_png == StepState::Complete;
                state.gcode_stale = state.png_to_gcode == StepState::Complete;
                state.loaded_inputs = derive_loaded_inputs(state);
            }
        }
        Message::EditLayerToggle(i) => {
            state.editing_layer = if state.editing_layer == Some(i) { None } else { Some(i) };
        }
        Message::SetLayerCategory { index, category } => {
            if let Some(layer) = state.stackup.layers.get_mut(index) {
                layer.user_category = if layer.auto_category == category { None } else { Some(category) };
                state.rasterize_stale = state.gerber_to_png == StepState::Complete;
                state.gcode_stale = state.png_to_gcode == StepState::Complete;
                state.loaded_inputs = derive_loaded_inputs(state);
            }
        }
        Message::SetLayerSide { index, side } => {
            if let Some(layer) = state.stackup.layers.get_mut(index) {
                layer.user_side = if layer.auto_side == side { None } else { Some(side) };
                state.rasterize_stale = state.gerber_to_png == StepState::Complete;
                state.gcode_stale = state.png_to_gcode == StepState::Complete;
                state.loaded_inputs = derive_loaded_inputs(state);
            }
        }
        Message::ResetLayerOverride(index) => {
            if let Some(layer) = state.stackup.layers.get_mut(index) {
                layer.user_category = None;
                layer.user_side = None;
                state.rasterize_stale = state.gerber_to_png == StepState::Complete;
                state.gcode_stale = state.png_to_gcode == StepState::Complete;
                state.loaded_inputs = derive_loaded_inputs(state);
            }
        }
        Message::OpenInMods(path) => {
            if let Some(port) = state.mods_server_port {
                let filename = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let url = format!(
                    "https://modsproject.org/?program=programs/machines/G-code/mill+2D+PCB&src=http://127.0.0.1:{}/{}",
                    port, filename
                );
                let _ = webbrowser::open(&url);
            } else {
                state.pending_mods_open = Some(path);
                return Task::perform(start_mods_server(), Message::ModsServerStarted);
            }
        }
        Message::ModsServerStarted(port) => {
            state.mods_server_port = Some(port);
            if let Some(path) = state.pending_mods_open.take() {
                let filename = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let url = format!(
                    "https://modsproject.org/?program=programs/machines/G-code/mill+2D+PCB&src=http://127.0.0.1:{}/{}",
                    port, filename
                );
                let _ = webbrowser::open(&url);
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
            let (copper_top, copper_bottom, outline, drill) = state.stackup.milling_paths();
            let has_gerbers = !copper_top.is_empty() || !copper_bottom.is_empty() || !outline.is_empty() || !drill.is_empty();
            let rasterize_needed = has_gerbers
                && (state.gerber_to_png != StepState::Complete || state.rasterize_stale);
            let has_both_pngs = state.loaded_top_png_path.is_some() && state.loaded_bottom_png_path.is_some();
            let has_any_png = state.loaded_top_png_path.is_some() || state.loaded_bottom_png_path.is_some();
            let gcode_needed = has_any_png
                && (state.png_to_gcode != StepState::Complete || state.gcode_stale);
            if rasterize_needed {
                state.rasterize_stale = false;
                return update(state, Message::ConvertToPng);
            } else if gcode_needed && has_both_pngs {
                state.gcode_stale = false;
                return update(state, Message::GenerateBothGcode);
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
