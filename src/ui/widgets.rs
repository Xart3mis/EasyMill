use iced::{
    Alignment, Color, Element, Length,
    widget::{button, column, container, progress_bar, row, scrollable, text, text_input, image},
};
use std::path::PathBuf;

use super::palette;
use super::styles;
use easymill::conversion::PngRenderResult;

pub fn header<'a>() -> Element<'a, crate::Message> {
    container(
        row![
            container(
                text("EM")
                    .font(palette::MONO)
                    .size(18)
                    .color(palette::accent()),
            )
            .padding([8, 11])
            .style(|_| {
                container::Style::default()
                    .background(iced::Background::Color(palette::surface_inset()))
                    .border(iced::border::rounded(10.0))
            }),
            container(
                column![
                    text("EasyMill")
                        .font(palette::MONO)
                        .size(32)
                        .color(palette::text_primary()),
                    text("Gerber to G-code pipeline")
                        .font(palette::MONO)
                        .size(16)
                        .color(palette::text_secondary()),
                ]
                .spacing(2),
            )
            .width(Length::Fill),
            container(
                text("TECH PREVIEW")
                    .font(palette::MONO)
                    .size(11)
                    .color(palette::signal_green()),
            )
            .padding([7, 10])
            .style(styles::inset_style()),
        ]
        .spacing(14)
        .align_y(Alignment::Center),
    )
    .padding(16)
    .style(styles::panel_style())
    .into()
}

pub fn setting_field<'a>(
    label: &'a str,
    value: &'a str,
    on_change: impl Fn(String) -> crate::Message + 'a,
) -> Element<'a, crate::Message> {
    column![
        text(label)
            .font(palette::MONO)
            .size(12)
            .color(palette::text_secondary()),
        text_input("", value)
            .on_input(on_change)
            .padding([7, 10])
            .size(13)
            .width(Length::Fill),
    ]
    .spacing(4)
    .into()
}

fn pill_color(state: crate::StepState) -> Color {
    match state {
        crate::StepState::Idle => palette::text_muted(),
        crate::StepState::Ready => palette::signal_gold(),
        crate::StepState::Running => palette::accent(),
        crate::StepState::Complete => palette::signal_green(),
    }
}

fn pill_bg(state: crate::StepState) -> Color {
    match state {
        crate::StepState::Idle => Color::from_rgba(0.0, 0.0, 0.0, 0.0),
        crate::StepState::Ready => palette::signal_gold_muted(),
        crate::StepState::Running => palette::accent_muted(),
        crate::StepState::Complete => palette::signal_green_muted(),
    }
}

pub fn step_card<'a>(
    stage: &'a str,
    title: &'a str,
    subtitle: &'a str,
    state: crate::StepState,
    progress_value: f32,
) -> Element<'a, crate::Message> {
    let status_pill = container(
        text(state.label())
            .font(palette::MONO)
            .size(11)
            .color(palette::text_primary()),
    )
    .padding([5, 9])
    .style(move |_| {
        container::Style::default()
            .background(iced::Background::Color(pill_bg(state)))
            .border(
                iced::border::rounded(6.0)
                    .width(1.0)
                    .color(pill_color(state)),
            )
    });

    let bar_color = match state {
        crate::StepState::Complete => palette::signal_green(),
        _ => palette::accent(),
    };

    container(
        column![
            row![
                text(stage)
                    .font(palette::MONO)
                    .size(11)
                    .color(palette::text_muted()),
                container("").width(Length::Fill),
                status_pill,
            ]
            .width(Length::Fill)
            .align_y(Alignment::Center),
            text(title)
                .font(palette::MONO)
                .size(21)
                .color(palette::text_primary()),
            text(subtitle)
                .font(palette::MONO)
                .size(14)
                .color(palette::text_secondary()),
            container(
                progress_bar(0.0..=1.0, progress_value)
                    .style(move |_| iced::widget::progress_bar::Style {
                        background: palette::surface_inset().into(),
                        bar: bar_color.into(),
                        border: iced::Border::default(),
                    }),
            )
            .width(Length::Fill)
            .height(Length::Fixed(6.0)),
        ]
        .spacing(10),
    )
    .padding(14)
    .style(styles::inset_style())
    .into()
}

pub fn progress_chip(state: &crate::AppState) -> Element<'static, crate::Message> {
    let pct = ((state.gerber_to_png_progress + state.png_to_gcode_progress) * 50.0) as i32;

    container(
        text(format!("{pct}% COMPLETE"))
            .font(palette::MONO)
            .size(11)
            .color(palette::text_accent()),
    )
    .padding([6, 9])
    .style(|_| {
        container::Style::default()
            .background(iced::Background::Color(palette::accent_muted()))
            .border(
                iced::border::rounded(6.0)
                    .width(1.0)
                    .color(palette::accent()),
            )
    })
    .into()
}

pub fn status_line<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    container(
        row![
            text("SYSTEM STATUS")
                .font(palette::MONO)
                .size(12)
                .color(palette::text_accent()),
            text(if state.status.is_empty() {
                "Idle"
            } else {
                &state.status
            })
            .font(palette::MONO)
            .size(14)
            .color(palette::text_primary()),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding([10, 14])
    .style(styles::inset_style())
    .into()
}

pub fn source_panel<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    let source_actions = column![
        button(text("Select Copper").font(palette::mono_bold()).size(14))
            .style(styles::secondary_action_style)
            .width(Length::Fill)
            .padding([11, 16])
            .on_press(crate::Message::SelectCopperFiles),
        button(text("Select Outline").font(palette::mono_bold()).size(14))
            .style(styles::secondary_action_style)
            .width(Length::Fill)
            .padding([11, 16])
            .on_press(crate::Message::SelectOutlineFiles),
        button(text("Select Drill").font(palette::mono_bold()).size(14))
            .style(styles::secondary_action_style)
            .width(Length::Fill)
            .padding([11, 16])
            .on_press(crate::Message::SelectDrillFiles),
        button(text("Load existing PNG").font(palette::mono_bold()).size(14))
            .style(styles::secondary_action_style)
            .width(Length::Fill)
            .padding([11, 16])
            .on_press(crate::Message::LoadPng),
        button(text("Reset").font(palette::mono_bold()).size(14))
            .style(styles::ghost_action_style)
            .width(Length::Fill)
            .padding([10, 14])
            .on_press(crate::Message::Reset),
    ]
    .spacing(10);

    let selected_items: Element<'_, crate::Message> = if state.loaded_inputs.is_empty() {
        container(
            text("No input selected")
                .font(palette::MONO)
                .size(14)
                .color(palette::text_muted()),
        )
        .width(Length::Fill)
        .height(132)
        .padding([12, 14])
        .style(styles::inset_style())
        .into()
    } else {
        let list = state
            .loaded_inputs
            .iter()
            .fold(column![].spacing(7), |col, item| {
                col.push(
                    row![
                        text(">")
                            .font(palette::MONO)
                            .size(14)
                            .color(palette::accent()),
                        text(item)
                            .font(palette::MONO)
                            .size(14)
                            .color(palette::text_primary()),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
            });

        container(list)
            .width(Length::Fill)
            .height(132)
            .padding([12, 14])
            .style(styles::inset_style())
            .into()
    };

    container(
        column![
            text("INPUT SOURCE")
                .font(palette::MONO)
                .size(13)
                .color(palette::text_accent()),
            text("Select Gerber and drill files to convert")
                .font(palette::MONO)
                .size(15)
                .color(palette::text_secondary()),
            source_actions,
            text("Loaded assets")
                .font(palette::MONO)
                .size(12)
                .color(palette::text_secondary()),
            selected_items,
        ]
        .spacing(12),
    )
    .width(Length::Fill)
    .padding(18)
    .style(styles::panel_style())
    .into()
}

pub fn config_panel<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    container(
        column![
            text("CONFIGURATION")
                .font(palette::MONO)
                .size(13)
                .color(palette::text_accent()),
            text("Adjust CAM and G-code generation parameters")
                .font(palette::MONO)
                .size(15)
                .color(palette::text_secondary()),
            column![
                setting_field("Resolution (DPI)", &state.dpi_input, crate::Message::DpiChanged),
                setting_field("Safe Z (mm)", &state.safe_z_mm_input, crate::Message::SafeZChanged),
                setting_field("Cut Z (mm)", &state.cut_z_mm_input, crate::Message::CutZChanged),
                setting_field("Feed rate (mm/min)", &state.feed_rate_input, crate::Message::FeedRateChanged),
                setting_field("Plunge (mm/min)", &state.plunge_rate_input, crate::Message::PlungeRateChanged),
                setting_field("Spindle (RPM)", &state.spindle_speed_input, crate::Message::SpindleSpeedChanged),
                setting_field("Tool dia. (mm)", &state.tool_diameter_mm_input, crate::Message::ToolDiameterChanged),
                setting_field("Offsets (0=fill)", &state.offset_number_input, crate::Message::OffsetNumberChanged),
                setting_field("Stepover", &state.offset_stepover_input, crate::Message::OffsetStepoverChanged),
            ]
            .spacing(10)
        ]
        .spacing(12),
    )
    .width(Length::Fill)
    .padding(18)
    .style(styles::panel_style())
    .into()
}

pub fn pipeline_panel<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    let has_input_files = !state.copper_paths.is_empty()
        || !state.outline_paths.is_empty()
        || !state.drill_paths.is_empty();
    let has_png = state.generated_pngs.is_some() || state.loaded_png_path.is_some();

    let convert_btn = button(
        text("Convert to PNG")
            .font(palette::mono_bold()).size(14),
    )
    .style(styles::primary_action_style)
    .width(Length::Fill)
    .padding([10, 16]);
    let convert_btn = if has_input_files && state.gerber_to_png != crate::StepState::Running {
        convert_btn.on_press(crate::Message::ConvertToPng)
    } else {
        convert_btn
    };

    let generate_btn = button(
        text("Generate G-code")
            .font(palette::mono_bold()).size(14),
    )
    .style(styles::primary_action_style)
    .width(Length::Fill)
    .padding([10, 16]);
    let generate_btn = if has_png && state.png_to_gcode != crate::StepState::Running {
        generate_btn.on_press(crate::Message::GenerateGcode)
    } else {
        generate_btn
    };

    scrollable(
        container(
            column![
                row![
                    text("PIPELINE")
                        .font(palette::MONO)
                        .size(13)
                        .color(palette::text_accent()),
                    container("").width(Length::Fill),
                    progress_chip(state),
                ]
                .width(Length::Fill)
                .align_y(Alignment::Center),
                text("Stage 1: Rasterization")
                    .font(palette::MONO)
                    .size(12)
                    .color(palette::text_secondary()),
                step_card("STAGE 01", "GERBER -> PNG", "Rasterization", state.gerber_to_png, state.gerber_to_png_progress),
                convert_btn,
                text("Stage 2: Toolpath generation")
                    .font(palette::MONO)
                    .size(12)
                    .color(palette::text_secondary()),
                step_card("STAGE 02", "PNG -> GCODE", "Toolpath generation", state.png_to_gcode, state.png_to_gcode_progress),
                generate_btn,
                generated_output(state),
                save_png_buttons(state),
                save_gcode_button(state),
            ]
            .spacing(12),
        )
        .width(Length::Fill)
        .padding(20)
        .style(styles::panel_style()),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

pub fn stats_panel<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    container(
        column![
            text("JOB STATISTICS")
                .font(palette::MONO)
                .size(13)
                .color(palette::text_accent()),
            column![
                row![
                    text("Est. Cut Time:")
                        .font(palette::MONO)
                        .size(14)
                        .color(palette::text_secondary()),
                    container("").width(Length::Fill),
                    text(&state.estimated_time)
                        .font(palette::MONO)
                        .size(14)
                        .color(palette::text_primary()),
                ],
                row![
                    text("Cut Distance:")
                        .font(palette::MONO)
                        .size(14)
                        .color(palette::text_secondary()),
                    container("").width(Length::Fill),
                    text(&state.cut_distance)
                        .font(palette::MONO)
                        .size(14)
                        .color(palette::text_primary()),
                ],
                row![
                    text("Board Size:")
                        .font(palette::MONO)
                        .size(14)
                        .color(palette::text_secondary()),
                    container("").width(Length::Fill),
                    text(&state.board_dimensions)
                        .font(palette::MONO)
                        .size(14)
                        .color(palette::text_primary()),
                ],
            ]
            .spacing(10)
        ]
        .spacing(12),
    )
    .width(Length::Fill)
    .padding(18)
    .style(styles::panel_style())
    .into()
}

pub fn generated_output<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    let heading = text("OUTPUT PREVIEW")
        .font(palette::MONO)
        .size(11)
        .color(palette::text_muted());

    if let Some(pngs) = &state.generated_pngs {
        let preview = |result: &'a PngRenderResult, label: &'a str| -> Element<'a, crate::Message> {
            let img_widget = image(iced::widget::image::Handle::from_path(&result.path))
                .width(Length::FillPortion(1))
                .height(Length::Fixed(120.0));
            container(
                column![
                    text(label)
                        .font(palette::MONO)
                        .size(11)
                        .color(palette::text_accent()),
                    container(img_widget)
                        .width(Length::Fill)
                        .height(Length::Fixed(120.0))
                        .clip(true)
                        .style(styles::inset_style()),
                ]
                .spacing(4),
            )
            .width(Length::FillPortion(1))
            .into()
        };

        container(
            column![
                heading,
                row![
                    preview(&pngs.copper, "Traces"),
                    preview(&pngs.drills, "Drills"),
                    preview(&pngs.outline, "Outline"),
                ]
                .spacing(8)
                .width(Length::Fill),
            ]
            .spacing(8),
        )
        .width(Length::Fill)
        .padding(14)
        .style(styles::inset_style())
        .into()
    } else if state.loaded_png_path.is_some() {
        let png_path = state.loaded_png_path.as_ref().unwrap();
        let img_widget = image(iced::widget::image::Handle::from_path(png_path.as_path()))
            .width(Length::Fill)
            .height(Length::Fixed(180.0));
        container(
            column![
                heading,
                text("Loaded external PNG")
                    .font(palette::MONO)
                    .size(12)
                    .color(palette::text_secondary()),
                container(img_widget)
                    .width(Length::Fill)
                    .height(Length::Fixed(180.0))
                    .clip(true)
                    .style(styles::inset_style()),
            ]
            .spacing(8),
        )
        .width(Length::Fill)
        .padding(14)
        .style(styles::inset_style())
        .into()
    } else {
        container(
            column![
                heading,
                text("No preview available. Run pipeline to generate.")
                    .font(palette::MONO)
                    .size(13)
                    .color(palette::text_muted()),
            ]
            .spacing(8),
        )
        .width(Length::Fill)
        .padding(14)
        .style(styles::inset_style())
        .into()
    }
}

pub fn save_png_buttons<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    let has_pngs = state.generated_pngs.is_some();

    let traces_btn = button(
        text("Save Traces").font(palette::mono_bold()).size(13),
    )
    .style(styles::secondary_action_style)
    .width(Length::Fill)
    .padding([8, 12]);
    let traces_btn = if has_pngs { traces_btn.on_press(crate::Message::SaveCopperPng) } else { traces_btn };

    let drills_btn = button(
        text("Save Drills").font(palette::mono_bold()).size(13),
    )
    .style(styles::secondary_action_style)
    .width(Length::Fill)
    .padding([8, 12]);
    let drills_btn = if has_pngs { drills_btn.on_press(crate::Message::SaveDrillPng) } else { drills_btn };

    let outline_btn = button(
        text("Save Outline").font(palette::mono_bold()).size(13),
    )
    .style(styles::secondary_action_style)
    .width(Length::Fill)
    .padding([8, 12]);
    let outline_btn = if has_pngs { outline_btn.on_press(crate::Message::SaveOutlinePng) } else { outline_btn };

    let save_all_btn = button(
        text("Save All PNGs").font(palette::mono_bold()).size(14),
    )
    .style(styles::primary_action_style)
    .width(Length::Fill)
    .padding([10, 16]);
    let save_all_btn = if has_pngs { save_all_btn.on_press(crate::Message::SaveAllPngs) } else { save_all_btn };

    container(
        column![
            row![traces_btn, drills_btn, outline_btn].spacing(8),
            save_all_btn,
        ]
        .spacing(8),
    )
    .width(Length::Fill)
    .into()
}

pub fn save_gcode_button<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    let btn = button(
        text("Save G-code")
            .font(palette::mono_bold()).size(14),
    )
    .style(styles::secondary_action_style)
    .width(Length::Fill)
    .padding([10, 16]);

    if state.generated_gcode.is_some() {
        btn.on_press(crate::Message::SaveGcode).into()
    } else {
        btn.into()
    }
}

fn path_to_label(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}
