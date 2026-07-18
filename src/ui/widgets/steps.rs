use iced::{
    Alignment, Color, Element, Length, Theme,
    widget::{button, column, container, image, progress_bar, row, text},
};
use easymill::conversion::PngRenderResult;
use crate::StepState;
use crate::ui::{palette, styles};
use super::components::{accordion, drop_zone, layer_row, setting_field};

/// Visual state of a step card — drives border/background color.
pub(crate) enum CardVisualState {
    Active,
    Complete,
    Stale,
    Waiting,
}

fn visual_state_colors(vs: &CardVisualState) -> (Color, Color) {
    // (border_color, bg_color)
    match vs {
        CardVisualState::Active => (palette::accent(), palette::card_active_bg()),
        CardVisualState::Complete => (palette::signal_green(), palette::card_complete_bg()),
        CardVisualState::Stale => (palette::signal_gold(), palette::card_stale_bg()),
        CardVisualState::Waiting => (
            Color::from_rgba(1.0, 1.0, 1.0, 0.06),
            palette::card_bg(),
        ),
    }
}

fn badge_for(vs: &CardVisualState) -> (&'static str, Color) {
    match vs {
        CardVisualState::Active => ("ACTIVE", palette::accent()),
        CardVisualState::Complete => ("DONE", palette::signal_green()),
        CardVisualState::Stale => ("STALE", palette::signal_gold()),
        CardVisualState::Waiting => ("WAITING", palette::text_muted()),
    }
}

/// Shared card wrapper. Handles collapse/expand, colored border, state badge.
pub(crate) fn step_shell<'a>(
    step_num: u8,
    label: &'a str,
    vs: CardVisualState,
    is_expanded: bool,
    summary: String,
    header_action: Option<Element<'a, crate::Message>>,
    content: Element<'a, crate::Message>,
) -> Element<'a, crate::Message> {
    let (border_color, bg_color) = visual_state_colors(&vs);
    let (badge_text, badge_color) = badge_for(&vs);

    let badge = container(
        text(badge_text)
            .font(palette::MONO)
            .size(10)
            .color(badge_color),
    )
    .padding([3, 7])
    .style(move |_: &Theme| {
        container::Style::default()
            .background(iced::Background::Color(Color::from_rgba(
                badge_color.r, badge_color.g, badge_color.b, 0.12,
            )))
            .border(iced::border::rounded(4.0).color(badge_color).width(1.0))
    });

    let mut header_elems: Vec<Element<'_, crate::Message>> = vec![
        text(format!("{step_num:02}"))
            .font(palette::MONO)
            .size(11)
            .color(palette::text_muted())
            .into(),
        text(label)
            .font(palette::mono_bold())
            .size(14)
            .color(palette::text_primary())
            .into(),
        container("").width(Length::Fill).into(),
        badge.into(),
    ];
    if let Some(action) = header_action {
        header_elems.push(action);
    }

    let header_row = iced::widget::Row::with_children(header_elems)
        .spacing(10)
        .align_y(Alignment::Center)
        .width(Length::Fill);

    let toggle_btn = button(header_row)
        .style(styles::transparent_button_style)
        .width(Length::Fill)
        .padding(0)
        .on_press(crate::Message::StepToggled(step_num));

    let card_body: Element<'_, crate::Message> = if is_expanded {
        column![
            toggle_btn,
            container(content).padding([12u16, 0]),
        ]
        .spacing(0)
        .into()
    } else if !summary.is_empty() {
        column![
            toggle_btn,
            text(summary)
                .font(palette::MONO)
                .size(12)
                .color(palette::text_muted()),
        ]
        .spacing(8)
        .into()
    } else {
        column![toggle_btn].into()
    };

    container(card_body)
        .width(Length::Fill)
        .padding(16)
        .style(move |_: &Theme| {
            container::Style::default()
                .background(iced::Background::Color(bg_color))
                .border(iced::border::rounded(10.0).color(border_color).width(1.5))
        })
        .into()
}

/// Root of the main scrollable area. Stacks all four step cards.
pub fn step_canvas<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    container(
        column![
            files_step(state),
            settings_step(state),
            rasterize_step(state),
            gcode_step(state),
        ]
        .spacing(12)
        .max_width(720),
    )
    .width(Length::Fill)
    .padding([24, 32])
    .center_x(Length::Fill)
    .into()
}

// --- Step card stubs (filled in Tasks 7–10) ---

pub fn files_step<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    let (copper, outline, drill) = state.stackup.milling_paths();
    let has_gerbers = !copper.is_empty() || !outline.is_empty() || !drill.is_empty();
    let is_skipped = state.loaded_png_path.is_some() && state.stackup.layers.is_empty();
    let has_input = has_gerbers || state.loaded_png_path.is_some();

    let vs = if has_input { CardVisualState::Complete } else { CardVisualState::Active };
    let is_expanded = state.expanded_step == Some(1);

    let n_files = state.stackup.layers.len();
    let summary_str: String = if is_skipped {
        let name = state.loaded_png_path.as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        format!("PNG: {name}")
    } else if has_gerbers {
        let mut parts = Vec::new();
        if !copper.is_empty() { parts.push("Cu"); }
        if !outline.is_empty() { parts.push("Out"); }
        if !drill.is_empty() { parts.push("Drl"); }
        format!("{n_files} file(s) · {}", parts.join(", "))
    } else {
        "No files loaded".to_owned()
    };

    let mut file_rows: Vec<Element<'_, crate::Message>> = Vec::new();
    for (i, layer) in state.stackup.layers.iter().enumerate() {
        let cat = layer.effective_category();
        let side = layer.effective_side();
        let is_overridden = layer.user_category.is_some() || layer.user_side.is_some();
        let name = layer.filename();
        let is_editing = state.editing_layer == Some(i);
        file_rows.push(layer_row(i, cat, side, is_overridden, name, is_editing));
    }
    if is_skipped {
        let name = state.loaded_png_path.as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        file_rows.push(
            row![
                text("PNG").font(palette::MONO).size(11).color(palette::signal_green()),
                text(name)
                    .font(palette::MONO)
                    .size(13)
                    .color(palette::text_secondary())
                    .width(Length::Fill),
                button(text("✕").font(palette::MONO).size(11).color(palette::text_muted()))
                    .style(styles::transparent_button_style)
                    .padding([2, 6])
                    .on_press(crate::Message::ClearPng),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into(),
        );
    }

    let files_col = iced::widget::Column::with_children(file_rows).spacing(6);

    let or_row = row![
        container("").width(Length::Fill).height(Length::Fixed(1.0))
            .style(|_: &Theme| container::Style::default()
                .background(iced::Background::Color(Color::from_rgba(1.0,1.0,1.0,0.06)))),
        text("or").font(palette::MONO).size(11).color(palette::text_muted()),
        container("").width(Length::Fill).height(Length::Fixed(1.0))
            .style(|_: &Theme| container::Style::default()
                .background(iced::Background::Color(Color::from_rgba(1.0,1.0,1.0,0.06)))),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let load_png_btn = button(
        text("↑  Load existing PNG instead")
            .font(palette::MONO)
            .size(13)
            .color(palette::text_secondary()),
    )
    .style(styles::ghost_action_style)
    .width(Length::Fill)
    .padding([8, 12])
    .on_press(crate::Message::LoadPng);

    let content = column![
        drop_zone(crate::Message::SelectGerberFiles),
        files_col,
        or_row,
        load_png_btn,
    ]
    .spacing(12);

    step_shell(1, "FILES", vs, is_expanded, summary_str, None, content.into())
}

pub fn settings_step<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    let is_expanded = state.expanded_step == Some(2);
    let summary = format!(
        "{}dpi · {}mm cut · {} mm/min · Ø{}mm",
        state.dpi_input, state.cut_z_mm_input, state.feed_rate_input, state.tool_diameter_mm_input
    );

    let geometry_content = column![
        setting_field("Resolution (DPI)", &state.dpi_input, crate::Message::DpiChanged),
    ]
    .spacing(12);

    let depths_content = row![
        setting_field("Cut Z (mm)", &state.cut_z_mm_input, crate::Message::CutZChanged),
        setting_field("Safe Z (mm)", &state.safe_z_mm_input, crate::Message::SafeZChanged),
    ]
    .spacing(20);

    let motion_summary = format!(
        "feed {} · plunge {} · spindle {}",
        state.feed_rate_input, state.plunge_rate_input, state.spindle_speed_input
    );
    let motion_content = column![
        row![
            setting_field("Feed rate (mm/min)", &state.feed_rate_input, crate::Message::FeedRateChanged),
            setting_field("Plunge (mm/min)", &state.plunge_rate_input, crate::Message::PlungeRateChanged),
        ]
        .spacing(20),
        setting_field("Spindle (RPM)", &state.spindle_speed_input, crate::Message::SpindleSpeedChanged),
    ]
    .spacing(12);

    let tooling_summary = format!(
        "dia {} · offsets {} · stepover {}",
        state.tool_diameter_mm_input, state.offset_number_input, state.offset_stepover_input
    );
    let tooling_content = column![
        row![
            setting_field("Tool dia. (mm)", &state.tool_diameter_mm_input, crate::Message::ToolDiameterChanged),
            setting_field("Offsets (0=fill)", &state.offset_number_input, crate::Message::OffsetNumberChanged),
        ]
        .spacing(20),
        setting_field("Stepover", &state.offset_stepover_input, crate::Message::OffsetStepoverChanged),
    ]
    .spacing(12);

    let [geo_open, dep_open, mot_open, tool_open] = state.settings_groups_open;

    let content = column![
        accordion("GEOMETRY", String::new(), geo_open,
            crate::Message::SettingsGroupToggled(0), geometry_content.into()),
        accordion("DEPTHS", String::new(), dep_open,
            crate::Message::SettingsGroupToggled(1), depths_content.into()),
        accordion("MOTION", motion_summary, mot_open,
            crate::Message::SettingsGroupToggled(2), motion_content.into()),
        accordion("TOOLING", tooling_summary, tool_open,
            crate::Message::SettingsGroupToggled(3), tooling_content.into()),
    ]
    .spacing(4);

    step_shell(2, "SETTINGS", CardVisualState::Complete, is_expanded, summary, None, content.into())
}

pub fn rasterize_step<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    let (has_copper, has_outline, has_drill) = {
        let (c, o, d) = state.stackup.milling_paths();
        (!c.is_empty(), !o.is_empty(), !d.is_empty())
    };
    let has_gerbers = has_copper || has_outline || has_drill;
    let is_skipped = state.loaded_png_path.is_some() && !has_gerbers;

    let vs = if is_skipped {
        CardVisualState::Complete
    } else if state.rasterize_stale {
        CardVisualState::Stale
    } else {
        match state.gerber_to_png {
            StepState::Complete => CardVisualState::Complete,
            StepState::Running => CardVisualState::Active,
            _ => if has_gerbers { CardVisualState::Active } else { CardVisualState::Waiting },
        }
    };

    let summary = if is_skipped {
        "Skipped — PNG loaded directly".to_owned()
    } else {
        match state.gerber_to_png {
            StepState::Complete => "3 layers rendered".to_owned(),
            _ => "Not yet run".to_owned(),
        }
    };

    let is_expanded = state.expanded_step == Some(3);

    // --- Run / Re-run button (goes in header) ---
    let can_run = has_gerbers && state.gerber_to_png != StepState::Running;
    let run_msg = if state.rasterize_stale {
        crate::Message::ReRunRasterize
    } else {
        crate::Message::ConvertToPng
    };
    let run_btn: Element<'_, crate::Message> = if can_run {
        button(text("▶  Run").font(palette::mono_bold()).size(12))
            .style(styles::primary_action_style)
            .padding([5, 12])
            .on_press(run_msg)
            .into()
    } else {
        button(text("▶  Run").font(palette::MONO).size(12))
            .style(styles::primary_action_style)
            .padding([5, 12])
            .into()
    };

    // --- Per-layer progress rows ---
    let layer_progress = |label: &'a str, progress: f32| -> Element<'a, crate::Message> {
        let bar_color = if progress >= 1.0 { palette::signal_green() } else { palette::accent() };
        row![
            container(text(label).font(palette::MONO).size(12).color(palette::text_secondary()))
                .width(Length::Fixed(40.0)),
            container(
                progress_bar(0.0..=1.0, progress)
                    .style(move |_| iced::widget::progress_bar::Style {
                        background: palette::surface_inset().into(),
                        bar: bar_color.into(),
                        border: iced::Border::default(),
                    }),
            )
            .width(Length::Fill)
            .height(Length::Fixed(6.0)),
            text(format!("{:.0}%", progress * 100.0))
                .font(palette::MONO)
                .size(11)
                .color(palette::text_muted())
                .width(Length::Fixed(36.0)),
            text(if progress >= 1.0 { "✓" } else if progress > 0.0 { "···" } else { "" })
                .font(palette::MONO)
                .size(11)
                .color(if progress >= 1.0 { palette::signal_green() } else { palette::text_muted() }),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
    };

    let p = state.gerber_to_png_progress;
    let progress_rows = column![
        layer_progress("Cu", if state.gerber_to_png == StepState::Complete { 1.0 } else { p }),
        layer_progress("Out", if state.gerber_to_png == StepState::Complete { 1.0 } else { (p - 0.33).max(0.0) }),
        layer_progress("Drl", if state.gerber_to_png == StepState::Complete { 1.0 } else { (p - 0.66).max(0.0) }),
    ]
    .spacing(8);

    // --- PNG thumbnails ---
    let thumbnails: Element<'_, crate::Message> = if let Some(pngs) = &state.generated_pngs {
        let thumb = |result: &'a PngRenderResult, label: &'static str| -> Element<'a, crate::Message> {
            column![
                text(label).font(palette::MONO).size(10).color(palette::text_accent()),
                container(
                    image(iced::widget::image::Handle::from_path(&result.path))
                        .width(Length::Fill)
                        .height(Length::Fixed(100.0)),
                )
                .width(Length::Fill)
                .height(Length::Fixed(100.0))
                .clip(true)
                .style(styles::inset_style()),
            ]
            .spacing(4)
            .width(Length::FillPortion(1))
            .into()
        };
        row![
            thumb(&pngs.copper, "Traces"),
            thumb(&pngs.drills, "Drills"),
            thumb(&pngs.outline, "Outline"),
        ]
        .spacing(8)
        .into()
    } else {
        container("").into()
    };

    // --- Save buttons (shown only when PNGs exist) ---
    let save_btns: Element<'_, crate::Message> = if state.generated_pngs.is_some() {
        row![
            button(text("↓ Traces").font(palette::MONO).size(12))
                .style(styles::secondary_action_style)
                .width(Length::FillPortion(1))
                .padding([7, 10])
                .on_press(crate::Message::SaveCopperPng),
            button(text("↓ Drills").font(palette::MONO).size(12))
                .style(styles::secondary_action_style)
                .width(Length::FillPortion(1))
                .padding([7, 10])
                .on_press(crate::Message::SaveDrillPng),
            button(text("↓ Outline").font(palette::MONO).size(12))
                .style(styles::secondary_action_style)
                .width(Length::FillPortion(1))
                .padding([7, 10])
                .on_press(crate::Message::SaveOutlinePng),
            button(text("↓ Save All").font(palette::mono_bold()).size(12))
                .style(styles::primary_action_style)
                .width(Length::FillPortion(1))
                .padding([7, 10])
                .on_press(crate::Message::SaveAllPngs),
        ]
        .spacing(8)
        .into()
    } else {
        container("").into()
    };

    // --- Stale warning row ---
    let stale_warning: Element<'_, crate::Message> = if state.rasterize_stale {
        container(
            row![
                text("⚠  Input changed — results outdated")
                    .font(palette::MONO)
                    .size(12)
                    .color(palette::signal_gold())
                    .width(Length::Fill),
                button(text("↻  Re-run").font(palette::mono_bold()).size(12))
                    .style(styles::secondary_action_style)
                    .padding([5, 10])
                    .on_press(crate::Message::ReRunRasterize),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([8, 12])
        .style(|_: &Theme| container::Style::default()
            .background(iced::Background::Color(palette::card_stale_bg()))
            .border(iced::border::rounded(6.0).color(palette::signal_gold()).width(1.0)))
        .into()
    } else {
        container("").into()
    };

    let content = column![
        stale_warning,
        progress_rows,
        thumbnails,
        save_btns,
    ]
    .spacing(12);

    step_shell(3, "RASTERIZE", vs, is_expanded, summary,
        if !is_skipped { Some(run_btn) } else { None },
        content.into())
}

pub fn gcode_step<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    let has_png = state.generated_pngs.is_some() || state.loaded_png_path.is_some();

    let vs = if state.gcode_stale {
        CardVisualState::Stale
    } else {
        match state.png_to_gcode {
            StepState::Complete => CardVisualState::Complete,
            StepState::Running => CardVisualState::Active,
            _ => if has_png { CardVisualState::Active } else { CardVisualState::Waiting },
        }
    };

    let summary = match state.png_to_gcode {
        StepState::Complete => {
            format!("board.nc · est. {}", state.estimated_time)
        }
        _ => "Not yet run".to_owned(),
    };

    let is_expanded = state.expanded_step == Some(4);

    // Header run button
    let can_run = has_png && state.png_to_gcode != StepState::Running;
    let run_msg = if state.gcode_stale {
        crate::Message::ReRunGcode
    } else {
        crate::Message::GenerateGcode
    };
    let run_btn: Element<'_, crate::Message> = if can_run {
        button(text("▶  Run").font(palette::mono_bold()).size(12))
            .style(styles::primary_action_style)
            .padding([5, 12])
            .on_press(run_msg)
            .into()
    } else {
        button(text("▶  Run").font(palette::MONO).size(12))
            .style(styles::primary_action_style)
            .padding([5, 12])
            .into()
    };

    // Progress bar
    let bar_color = if state.png_to_gcode == StepState::Complete { palette::signal_green() } else { palette::accent() };
    let progress = progress_bar(0.0..=1.0, state.png_to_gcode_progress)
        .style(move |_| iced::widget::progress_bar::Style {
            background: palette::surface_inset().into(),
            bar: bar_color.into(),
            border: iced::Border::default(),
        });

    // Wrap progress bar in a container for width/height (iced 0.14 limitation)
    let progress = container(progress)
        .width(Length::Fill)
        .height(Length::Fixed(6.0));

    // Stats card
    let stats: Element<'_, crate::Message> = if state.png_to_gcode == StepState::Complete {
        container(
            column![
                row![
                    text("Est. cut time").font(palette::MONO).size(13).color(palette::text_secondary()),
                    container("").width(Length::Fill),
                    text(&state.estimated_time).font(palette::MONO).size(13).color(palette::text_primary()),
                ],
                row![
                    text("Cut distance").font(palette::MONO).size(13).color(palette::text_secondary()),
                    container("").width(Length::Fill),
                    text(&state.cut_distance).font(palette::MONO).size(13).color(palette::text_primary()),
                ],
                row![
                    text("Board size").font(palette::MONO).size(13).color(palette::text_secondary()),
                    container("").width(Length::Fill),
                    text(&state.board_dimensions).font(palette::MONO).size(13).color(palette::text_primary()),
                ],
            ]
            .spacing(8),
        )
        .width(Length::Fill)
        .padding(12)
        .style(styles::inset_style())
        .into()
    } else {
        container("").into()
    };

    // Save button
    let save_btn: Element<'_, crate::Message> = if state.generated_gcode.is_some() {
        button(text("↓  Save G-code").font(palette::mono_bold()).size(13))
            .style(styles::primary_action_style)
            .width(Length::Fill)
            .padding([10, 14])
            .on_press(crate::Message::SaveGcode)
            .into()
    } else {
        container("").into()
    };

    // Stale warning
    let stale_warning: Element<'_, crate::Message> = if state.gcode_stale {
        container(
            row![
                text("⚠  Settings changed — G-code outdated")
                    .font(palette::MONO)
                    .size(12)
                    .color(palette::signal_gold())
                    .width(Length::Fill),
                button(text("↻  Re-run").font(palette::mono_bold()).size(12))
                    .style(styles::secondary_action_style)
                    .padding([5, 10])
                    .on_press(crate::Message::ReRunGcode),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([8, 12])
        .style(|_: &Theme| container::Style::default()
            .background(iced::Background::Color(palette::card_stale_bg()))
            .border(iced::border::rounded(6.0).color(palette::signal_gold()).width(1.0)))
        .into()
    } else {
        container("").into()
    };

    let content = column![stale_warning, progress, stats, save_btn].spacing(12);

    step_shell(4, "G-CODE", vs, is_expanded, summary, Some(run_btn), content.into())
}
