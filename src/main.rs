use easymill::conversion::{
    ConversionSettings, DEFAULT_DPI, MM_PER_INCH, GcodeResult, PngRenderResult,
    gerber_inputs_to_png, png_to_gcode,
};
use easymill::logging::init_logging;
use iced::{
    self, Alignment, Background, Color, Element, Font, Length, Shadow, Task, Theme, Vector, border,
    widget::{button, column, container, progress_bar, row, text, text_input, image},
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum StepState {
    #[default]
    Idle,
    Ready,
    Running,
    Complete,
}

impl StepState {
    fn label(self) -> &'static str {
        match self {
            Self::Idle => "WAITING",
            Self::Ready => "READY",
            Self::Running => "RUNNING",
            Self::Complete => "DONE",
        }
    }

    fn progress(self) -> f32 {
        match self {
            Self::Idle => 0.0,
            Self::Ready => 0.45,
            Self::Running => 0.70,
            Self::Complete => 1.0,
        }
    }
}

struct AppState {
    selected_paths: Vec<PathBuf>,
    selected_inputs: Vec<String>,
    gerber_to_png: StepState,
    png_to_gcode: StepState,
    generated_png: Option<String>,
    generated_gcode: Option<String>,
    status: String,
    dpi_input: String,
    cut_z_mm_input: String,
    safe_z_mm_input: String,
    feed_rate_input: String,
    plunge_rate_input: String,
    spindle_speed_input: String,
    tool_diameter_mm_input: String,
    offset_number_input: String,
    offset_stepover_input: String,
    estimated_time: String,
    cut_distance: String,
    board_dimensions: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            selected_paths: Vec::new(),
            selected_inputs: Vec::new(),
            gerber_to_png: StepState::default(),
            png_to_gcode: StepState::default(),
            generated_png: None,
            generated_gcode: None,
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
enum Message {
    SelectGerberFiles,
    SelectZipArchive,
    GerberFilesPicked(Option<Vec<PathBuf>>),
    ZipArchivePicked(Option<PathBuf>),
    RunPipeline,
    PipelineFinished(Result<PipelineOutput, String>),
    SaveGcode,
    GcodeSavePathPicked(Option<PathBuf>),
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

#[derive(Debug, Clone)]
struct PipelineOutput {
    png: PngRenderResult,
    gcode: GcodeResult,
}

fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::SelectGerberFiles => {
            info!("opening gerber file picker");
            state.status = "Waiting for Gerber file selection...".to_owned();

            return Task::perform(
                async {
                    rfd::FileDialog::new()
                        .add_filter("Gerber", &["gbr", "grb", "gtl", "gbl", "gko", "gto", "gbo"])
                        .add_filter("Drill", &["drl"])
                        .set_title("Select Gerber and drill files")
                        .pick_files()
                },
                Message::GerberFilesPicked,
            );
        }
        Message::SelectZipArchive => {
            info!("opening zip archive picker");
            state.status = "Waiting for archive selection...".to_owned();

            return Task::perform(
                async {
                    rfd::FileDialog::new()
                        .add_filter("ZIP archive", &["zip"])
                        .set_title("Select zipped Gerber package")
                        .pick_file()
                },
                Message::ZipArchivePicked,
            );
        }
        Message::GerberFilesPicked(files) => {
            if let Some(files) = files {
                if files.is_empty() {
                    warn!("gerber file picker returned an empty selection");
                    state.status = "No files selected.".to_owned();
                    return Task::none();
                }

                state.selected_paths = files;
                state.selected_inputs = state
                    .selected_paths
                    .iter()
                    .cloned()
                    .map(path_to_label)
                    .collect();
                state.gerber_to_png = StepState::Ready;
                state.png_to_gcode = StepState::Idle;
                state.generated_png = None;
                state.generated_gcode = None;
                info!(
                    input_count = state.selected_paths.len(),
                    "selected gerber input files"
                );
                state.status = "Gerber inputs selected. Pipeline primed.".to_owned();
            } else {
                info!("gerber file selection canceled");
                state.status = "Gerber selection canceled.".to_owned();
            }
        }
        Message::ZipArchivePicked(file) => {
            if let Some(file) = file {
                state.selected_paths = vec![file.clone()];
                state.selected_inputs = vec![path_to_label(file)];
                state.gerber_to_png = StepState::Ready;
                state.png_to_gcode = StepState::Idle;
                state.generated_png = None;
                state.generated_gcode = None;
                info!(path = %state.selected_paths[0].display(), "selected gerber zip archive");
                state.status = "Archive loaded. Pipeline primed.".to_owned();
            } else {
                info!("zip archive selection canceled");
                state.status = "Archive selection canceled.".to_owned();
            }
        }
        Message::RunPipeline => {
            if state.selected_paths.is_empty() {
                warn!("pipeline requested without selected inputs");
                state.status = "Select Gerber files or a .zip archive first.".to_owned();
                return Task::none();
            }

            state.gerber_to_png = StepState::Running;
            state.png_to_gcode = StepState::Running;
            state.generated_png = None;
            state.generated_gcode = None;
            state.status = "Running Gerber -> PNG -> G-code conversion...".to_owned();

            let inputs = state.selected_paths.clone();
            let settings = state.get_settings();
            info!(
                input_count = inputs.len(),
                pixels_per_mm = settings.pixels_per_mm,
                cut_z_mm = settings.cut_z_mm,
                safe_z_mm = settings.safe_z_mm,
                feed_rate_mm_min = settings.feed_rate_mm_min,
                plunge_rate_mm_min = settings.plunge_rate_mm_min,
                spindle_speed_rpm = settings.spindle_speed_rpm,
                tool_diameter_mm = settings.tool_diameter_mm,
                offset_number = settings.offset_number,
                offset_stepover = settings.offset_stepover,
                "starting ui pipeline task"
            );
            return Task::perform(run_conversion_pipeline(inputs, settings), Message::PipelineFinished);
        }
        Message::PipelineFinished(result) => match result {
            Ok(output) => {
                let png_label = path_to_label(output.png.path.clone());
                let dark_pixels = output.png.dark_pixels;
                state.gerber_to_png = StepState::Complete;
                state.png_to_gcode = StepState::Complete;
                state.generated_png = Some(output.png.path.to_string_lossy().into_owned());
                state.generated_gcode = Some(output.gcode.gcode);
                
                // Format statistics
                let hours = (output.gcode.estimated_time_secs / 3600.0) as i32;
                let minutes = ((output.gcode.estimated_time_secs % 3600.0) / 60.0) as i32;
                let seconds = (output.gcode.estimated_time_secs % 60.0) as i32;
                state.estimated_time = format!("{hours:02}:{minutes:02}:{seconds:02}");
                state.cut_distance = format!("{:.1} mm", output.gcode.cut_distance_mm);
                state.board_dimensions = format!("{:.1} x {:.1} mm", output.gcode.width_mm, output.gcode.height_mm);
                
                info!(
                    png = %png_label,
                    dark_pixels,
                    gcode_bytes = state.generated_gcode.as_ref().map(|value| value.len()).unwrap_or_default(),
                    estimated_time = %state.estimated_time,
                    cut_distance = %state.cut_distance,
                    board_dimensions = %state.board_dimensions,
                    "ui pipeline task completed"
                );
                state.status = format!(
                    "Run complete. PNG saved to {png_label}; {dark_pixels} cut pixels converted to G-code."
                );
            }
            Err(err) => {
                state.gerber_to_png = StepState::Ready;
                state.png_to_gcode = StepState::Idle;
                state.generated_png = None;
                state.generated_gcode = None;
                state.estimated_time = "--:--:--".to_owned();
                state.cut_distance = "0.0 mm".to_owned();
                state.board_dimensions = "0.0 x 0.0 mm".to_owned();
                error!(error = %err, "ui pipeline task failed");
                state.status = format!("Pipeline failed: {err}");
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

fn setting_field<'a>(
    label: &'a str,
    value: &'a str,
    on_change: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    column![
        text(label)
            .size(12)
            .font(Font::MONOSPACE)
            .color(Color::from_rgb(0.57, 0.62, 0.68)),
        text_input("", value)
            .on_input(on_change)
            .padding([7, 10])
            .size(13)
            .width(Length::Fill),
    ]
    .spacing(4)
    .into()
}

fn view(state: &AppState) -> Element<'_, Message> {
    let source_actions = column![
        button("Load Gerber set")
            .style(primary_action_style)
            .width(Length::Fill)
            .padding([11, 16])
            .on_press(Message::SelectGerberFiles),
        row![
            button("Load .zip")
                .style(secondary_action_style)
                .width(Length::Fill)
                .padding([10, 14])
                .on_press(Message::SelectZipArchive),
            button("Reset")
                .style(ghost_action_style)
                .width(Length::Fill)
                .padding([10, 14])
                .on_press(Message::Reset),
        ]
        .spacing(10),
    ]
    .spacing(10);

    let selected_items: Element<'_, Message> = if state.selected_inputs.is_empty() {
        container(
            text("No input selected")
                .size(14)
                .font(Font::MONOSPACE)
                .color(Color::from_rgb(0.48, 0.55, 0.60)),
        )
        .width(Length::Fill)
        .height(132)
        .padding([12, 14])
        .style(inset_style())
        .into()
    } else {
        let list = state
            .selected_inputs
            .iter()
            .fold(column![].spacing(7), |col, item| {
                col.push(
                    row![
                        text(">")
                            .size(14)
                            .font(Font::MONOSPACE)
                            .color(Color::from_rgb(0.37, 0.89, 0.68,)),
                        text(item)
                            .size(14)
                            .font(Font::MONOSPACE)
                            .color(Color::from_rgb(0.79, 0.84, 0.88)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
            });

        container(list)
            .width(Length::Fill)
            .height(132)
            .padding([12, 14])
            .style(inset_style())
            .into()
    };

    let source_panel = container(
        column![
            text("INPUT SOURCE")
                .size(13)
                .font(Font::MONOSPACE)
                .color(Color::from_rgb(0.84, 0.70, 0.43)),
            text("Select Gerber files or a zipped Gerber package")
                .size(15)
                .color(Color::from_rgb(0.62, 0.67, 0.72)),
            source_actions,
            text("Loaded assets")
                .size(12)
                .font(Font::MONOSPACE)
                .color(Color::from_rgb(0.42, 0.47, 0.53)),
            selected_items,
        ]
        .spacing(12),
    )
    .width(Length::Fill)
    .padding(18)
    .style(panel_style());

    let settings_panel = container(
        column![
            text("PIPELINE CONFIG")
                .size(13)
                .font(Font::MONOSPACE)
                .color(Color::from_rgb(0.84, 0.70, 0.43)),
            text("Adjust CAM and G-code generation parameters")
                .size(15)
                .color(Color::from_rgb(0.62, 0.67, 0.72)),
            column![
                setting_field("Resolution (DPI)", &state.dpi_input, Message::DpiChanged),
                setting_field("Safe Z height (mm)", &state.safe_z_mm_input, Message::SafeZChanged),
                setting_field("Cut Z depth (mm)", &state.cut_z_mm_input, Message::CutZChanged),
                setting_field("Feed rate (mm/min)", &state.feed_rate_input, Message::FeedRateChanged),
                setting_field("Plunge rate (mm/min)", &state.plunge_rate_input, Message::PlungeRateChanged),
                setting_field("Spindle speed (RPM)", &state.spindle_speed_input, Message::SpindleSpeedChanged),
                setting_field("Tool diameter (mm)", &state.tool_diameter_mm_input, Message::ToolDiameterChanged),
                setting_field("Offset count (0=fill)", &state.offset_number_input, Message::OffsetNumberChanged),
                setting_field("Offset stepover", &state.offset_stepover_input, Message::OffsetStepoverChanged),
            ]
            .spacing(10)
        ]
        .spacing(12),
    )
    .width(Length::Fill)
    .padding(18)
    .style(panel_style());

    let pipeline_panel = container(
        column![
            row![
                text("PIPELINE")
                    .size(13)
                    .font(Font::MONOSPACE)
                    .color(Color::from_rgb(0.84, 0.70, 0.43)),
                container("").width(Length::Fill),
                progress_chip(state),
            ]
            .width(Length::Fill)
            .align_y(Alignment::Center),
            text("Automated conversion chain")
                .size(16)
                .color(Color::from_rgb(0.74, 0.79, 0.84)),
            step_card(
                "STAGE 01",
                "GERBER -> PNG",
                "Rasterization",
                state.gerber_to_png,
                Color::from_rgb(0.39, 0.89, 0.64),
            ),
            step_card(
                "STAGE 02",
                "PNG -> GCODE",
                "Toolpath generation",
                state.png_to_gcode,
                Color::from_rgb(0.91, 0.74, 0.38),
            ),
            generated_output(state),
            button("Run Pipeline")
                .style(primary_action_style)
                .width(Length::Fill)
                .padding([12, 16])
                .on_press(Message::RunPipeline),
            save_gcode_button(state),
        ]
        .spacing(14),
    )
    .width(Length::Fill)
    .padding(20)
    .style(panel_style());

    let status_line = container(
        row![
            text("SYSTEM STATUS")
                .size(12)
                .font(Font::MONOSPACE)
                .color(Color::from_rgb(0.84, 0.70, 0.43)),
            text(if state.status.is_empty() {
                "Idle"
            } else {
                &state.status
            })
            .size(14)
            .font(Font::MONOSPACE)
            .color(Color::from_rgb(0.77, 0.83, 0.87)),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding([10, 14])
    .style(inset_style());

    let stats_panel = container(
        column![
            text("JOB STATISTICS")
                .size(13)
                .font(Font::MONOSPACE)
                .color(Color::from_rgb(0.84, 0.70, 0.43)),
            column![
                row![
                    text("Est. Cut Time:").size(14).color(Color::from_rgb(0.57, 0.62, 0.68)),
                    container("").width(Length::Fill),
                    text(&state.estimated_time).size(14).font(Font::MONOSPACE).color(Color::from_rgb(0.89, 0.91, 0.93)),
                ],
                row![
                    text("Cut Distance:").size(14).color(Color::from_rgb(0.57, 0.62, 0.68)),
                    container("").width(Length::Fill),
                    text(&state.cut_distance).size(14).font(Font::MONOSPACE).color(Color::from_rgb(0.89, 0.91, 0.93)),
                ],
                row![
                    text("Board Size:").size(14).color(Color::from_rgb(0.57, 0.62, 0.68)),
                    container("").width(Length::Fill),
                    text(&state.board_dimensions).size(14).font(Font::MONOSPACE).color(Color::from_rgb(0.89, 0.91, 0.93)),
                ],
            ]
            .spacing(10)
        ]
        .spacing(12)
    )
    .width(Length::Fill)
    .padding(18)
    .style(panel_style());

    let left_column = column![
        source_panel,
        stats_panel,
    ]
    .spacing(16);

    let shell = column![
        header(),
        row![
            container(left_column)
                .width(Length::FillPortion(3))
                .height(Length::Fill),
            container(settings_panel)
                .width(Length::FillPortion(3))
                .height(Length::Fill),
            container(pipeline_panel)
                .width(Length::FillPortion(4))
                .height(Length::Fill),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Start)
        .spacing(16),
        status_line,
    ]
    .spacing(16)
    .height(Length::Fill)
    .max_width(1320);

    container(shell)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .padding([20, 24])
        .style(app_style())
        .into()
}

fn header<'a>() -> Element<'a, Message> {
    container(
        row![
            container(text("EM").size(18).color(Color::from_rgb(0.06, 0.10, 0.14)))
                .padding([8, 11])
                .style(|_| {
                    container::Style::default()
                        .background(Background::Color(Color::from_rgb(0.91, 0.74, 0.38)))
                        .border(border::rounded(8.0))
                }),
            container(
                column![
                    text("EasyMill")
                        .size(36)
                        .color(Color::from_rgb(0.89, 0.91, 0.93)),
                    text("Gerber to G-code pipeline")
                        .size(16)
                        .font(Font::MONOSPACE)
                        .color(Color::from_rgb(0.57, 0.62, 0.68)),
                ]
                .spacing(2),
            )
            .width(Length::Fill),
            container(
                text("TECH PREVIEW")
                    .size(11)
                    .font(Font::MONOSPACE)
                    .color(Color::from_rgb(0.39, 0.89, 0.64)),
            )
            .padding([7, 10])
            .style(inset_style()),
        ]
        .spacing(14)
        .align_y(Alignment::Center),
    )
    .padding(16)
    .style(panel_style())
    .into()
}

fn progress_chip<'a>(state: &AppState) -> Element<'a, Message> {
    let percent = ((state.gerber_to_png.progress() + state.png_to_gcode.progress()) * 50.0) as i32;

    container(
        text(format!("{percent}% COMPLETE"))
            .size(11)
            .font(Font::MONOSPACE)
            .color(Color::from_rgb(0.93, 0.79, 0.51)),
    )
    .padding([6, 9])
    .style(|_| {
        container::Style::default()
            .background(Background::Color(Color::from_rgba(0.74, 0.54, 0.24, 0.16)))
            .border(
                border::rounded(8.0)
                    .width(1.0)
                    .color(Color::from_rgba(0.90, 0.72, 0.40, 0.42)),
            )
    })
    .into()
}

fn step_card<'a>(
    stage: &'a str,
    title: &'a str,
    subtitle: &'a str,
    state: StepState,
    accent: Color,
) -> Element<'a, Message> {
    let status_pill = container(
        text(state.label())
            .size(11)
            .font(Font::MONOSPACE)
            .color(Color::from_rgb(0.86, 0.90, 0.92)),
    )
    .padding([5, 8])
    .style(move |_| {
        container::Style::default()
            .background(Background::Color(Color { a: 0.20, ..accent }))
            .border(
                border::rounded(8.0)
                    .width(1.0)
                    .color(Color { a: 0.46, ..accent }),
            )
    });

    container(
        column![
            row![
                text(stage)
                    .size(11)
                    .font(Font::MONOSPACE)
                    .color(Color::from_rgb(0.47, 0.53, 0.60)),
                container("").width(Length::Fill),
                status_pill,
            ]
            .width(Length::Fill)
            .align_y(Alignment::Center),
            text(title)
                .size(21)
                .font(Font::MONOSPACE)
                .color(Color::from_rgb(0.90, 0.92, 0.93)),
            text(subtitle)
                .size(14)
                .color(Color::from_rgb(0.58, 0.64, 0.70)),
            progress_bar(0.0..=1.0, state.progress()),
        ]
        .spacing(10),
    )
    .padding(14)
    .style(inset_style())
    .into()
}

fn app_style() -> impl Fn(&Theme) -> container::Style {
    |_| container::Style::default().background(Background::Color(Color::from_rgb(0.03, 0.04, 0.05)))
}

fn panel_style() -> impl Fn(&Theme) -> container::Style {
    |_| {
        container::Style::default()
            .background(Background::Color(Color::from_rgb(0.07, 0.08, 0.10)))
            .border(
                border::rounded(20.0)
                    .width(1.0)
                    .color(Color::from_rgba(0.44, 0.48, 0.54, 0.38)),
            )
            .shadow(Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.62),
                offset: Vector::new(0.0, 10.0),
                blur_radius: 30.0,
            })
    }
}

fn inset_style() -> impl Fn(&Theme) -> container::Style {
    |_| {
        container::Style::default()
            .background(Background::Color(Color::from_rgb(0.10, 0.11, 0.13)))
            .border(
                border::rounded(16.0)
                    .width(1.0)
                    .color(Color::from_rgba(0.36, 0.40, 0.46, 0.33)),
            )
    }
}

fn primary_action_style(_theme: &Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: Some(Background::Color(Color::from_rgb(0.37, 0.89, 0.68))),
        text_color: Color::from_rgb(0.06, 0.10, 0.12),
        border: border::rounded(12.0),
        shadow: Shadow::default(),
        ..button::Style::default()
    };

    match status {
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(Color::from_rgb(0.45, 0.93, 0.74))),
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(Color::from_rgb(0.30, 0.78, 0.58))),
            ..base
        },
        button::Status::Disabled => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.37, 0.89, 0.68, 0.35))),
            text_color: Color::from_rgba(0.08, 0.11, 0.14, 0.60),
            ..base
        },
        button::Status::Active => base,
    }
}

fn secondary_action_style(_theme: &Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: Some(Background::Color(Color::from_rgb(0.90, 0.72, 0.39))),
        text_color: Color::from_rgb(0.10, 0.08, 0.06),
        border: border::rounded(12.0),
        shadow: Shadow::default(),
        ..button::Style::default()
    };

    match status {
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(Color::from_rgb(0.94, 0.77, 0.46))),
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(Color::from_rgb(0.77, 0.61, 0.31))),
            ..base
        },
        button::Status::Disabled => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.90, 0.72, 0.39, 0.40))),
            text_color: Color::from_rgba(0.10, 0.08, 0.06, 0.60),
            ..base
        },
        button::Status::Active => base,
    }
}

fn ghost_action_style(_theme: &Theme, status: button::Status) -> button::Style {
    let base = button::Style {
        background: Some(Background::Color(Color::from_rgba(0.16, 0.18, 0.22, 0.0))),
        text_color: Color::from_rgb(0.61, 0.67, 0.73),
        border: border::rounded(12.0)
            .width(1.0)
            .color(Color::from_rgba(0.38, 0.42, 0.48, 0.35)),
        shadow: Shadow::default(),
        ..button::Style::default()
    };

    match status {
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.20, 0.22, 0.27, 0.65))),
            text_color: Color::from_rgb(0.74, 0.79, 0.84),
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.15, 0.17, 0.21, 0.78))),
            ..base
        },
        button::Status::Disabled => button::Style {
            text_color: Color::from_rgba(0.61, 0.67, 0.73, 0.45),
            ..base
        },
        button::Status::Active => base,
    }
}

pub fn main() -> iced::Result {
    if let Err(err) = init_logging() {
        eprintln!("failed to initialize logging: {err}");
    }
    info!("starting easymill ui");

    iced::application(AppState::default, update, view)
        .theme(theme)
        .title("EasyMill")
        .run()
}

fn path_to_label(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

fn save_gcode_button<'a>(state: &AppState) -> Element<'a, Message> {
    let button = button("Save generated G-code")
        .style(secondary_action_style)
        .width(Length::Fill)
        .padding([12, 16]);

    if state.generated_gcode.is_some() {
        button.on_press(Message::SaveGcode).into()
    } else {
        button.into()
    }
}

fn generated_output<'a>(state: &'a AppState) -> Element<'a, Message> {
    let header = text("OUTPUT PREVIEW")
        .size(11)
        .font(Font::MONOSPACE)
        .color(Color::from_rgb(0.47, 0.53, 0.60));

    if let Some(png_path) = &state.generated_png {
        let png_label = path_to_label(PathBuf::from(png_path));
        let img_widget = image(iced::widget::image::Handle::from_path(png_path))
            .width(Length::Fill)
            .height(Length::Fixed(180.0));

        container(
            column![
                header,
                text(format!("Render: {png_label}"))
                    .size(12)
                    .font(Font::MONOSPACE)
                    .color(Color::from_rgb(0.75, 0.81, 0.85)),
                container(img_widget)
                    .width(Length::Fill)
                    .height(Length::Fixed(180.0))
                    .style(inset_style()),
            ]
            .spacing(8)
        )
        .width(Length::Fill)
        .padding(14)
        .style(inset_style())
        .into()
    } else {
        container(
            column![
                header,
                text("No preview available. Run pipeline to generate.")
                    .size(13)
                    .font(Font::MONOSPACE)
                    .color(Color::from_rgb(0.48, 0.55, 0.60)),
            ]
            .spacing(8)
        )
        .width(Length::Fill)
        .padding(14)
        .style(inset_style())
        .into()
    }
}

async fn run_conversion_pipeline(inputs: Vec<PathBuf>, settings: ConversionSettings) -> Result<PipelineOutput, String> {
    let png_path = temporary_png_path();
    info!(
        input_count = inputs.len(),
        png = %png_path.display(),
        "running conversion pipeline"
    );

    // Run blocking PNG render on the blocking thread pool so the async
    // executor (and the UI event loop) stay responsive.
    let inputs_clone = inputs.clone();
    let settings_clone = settings.clone();
    let png_path_clone = png_path.clone();
    let png = tokio::task::spawn_blocking(move || {
        gerber_inputs_to_png(&inputs_clone, &png_path_clone, settings_clone)
    })
    .await
    .map_err(|e| format!("PNG render thread panicked: {e}"))?
    .map_err(|e| e.to_string())?;

    // Same for G-code generation — also CPU-intensive.
    let png_path_for_gcode = png.path.clone();
    let gcode = tokio::task::spawn_blocking(move || {
        png_to_gcode(&png_path_for_gcode, settings)
    })
    .await
    .map_err(|e| format!("G-code thread panicked: {e}"))?
    .map_err(|e| e.to_string())?;

    info!(
        png = %png.path.display(),
        width = png.width,
        height = png.height,
        dark_pixels = png.dark_pixels,
        gcode_bytes = gcode.gcode.len(),
        "conversion pipeline finished"
    );
    Ok(PipelineOutput { png, gcode })
}

fn temporary_png_path() -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("easymill-{timestamp}.png"))
}
