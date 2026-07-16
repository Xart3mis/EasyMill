use iced::{
    Alignment, Color, Element, Length, Theme,
    widget::{button, column, container, row, text},
};
use crate::StepState;
use crate::ui::{palette, styles};

pub fn sidebar<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    let logo = row![
        container(
            text("EM").font(palette::MONO).size(15).color(palette::accent()),
        )
        .padding([5, 8])
        .style(styles::inset_style()),
        text("EasyMill")
            .font(palette::mono_bold())
            .size(16)
            .color(palette::text_primary()),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let make_divider = || -> Element<'_, crate::Message> {
        container("")
            .width(Length::Fill)
            .height(Length::Fixed(1.0))
            .style(|_: &Theme| {
                container::Style::default()
                    .background(iced::Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.06)))
            })
            .into()
    };

    // Nav items
    let nav1 = nav_item(1, "FILES", files_badge(state), state.expanded_step == Some(1));
    let nav2 = nav_item(2, "SETTINGS", settings_badge(), state.expanded_step == Some(2));
    let nav3 = nav_item(3, "RASTERIZE", rasterize_badge(state), state.expanded_step == Some(3));
    let nav4 = nav_item(4, "G-CODE", gcode_badge(state), state.expanded_step == Some(4));

    // Files sub-list
    let mut file_items: Vec<Element<'_, crate::Message>> = Vec::new();
    for (i, path) in state.copper_paths.iter().enumerate() {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        file_items.push(sidebar_file_row("Cu", palette::layer_copper(), name, crate::LayerKind::Copper, i));
    }
    for (i, path) in state.outline_paths.iter().enumerate() {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        file_items.push(sidebar_file_row("Out", palette::layer_outline(), name, crate::LayerKind::Outline, i));
    }
    for (i, path) in state.drill_paths.iter().enumerate() {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        file_items.push(sidebar_file_row("Drl", palette::layer_drill(), name, crate::LayerKind::Drill, i));
    }
    if state.generated_pngs.is_none() {
        if let Some(png) = &state.loaded_png_path {
            let name = png.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            file_items.push(
                row![
                    container(
                        text("PNG").font(palette::MONO).size(10).color(palette::signal_green()),
                    )
                    .width(Length::Fixed(32.0))
                    .padding([0, 8]),
                    text(name).font(palette::MONO).size(10).color(palette::text_muted()),
                ]
                .spacing(4)
                .padding([1, 8])
                .into(),
            );
        }
    }
    let file_list = iced::widget::Column::with_children(file_items)
        .spacing(1)
        .padding(iced::Padding { top: 0.0, right: 0.0, bottom: 4.0, left: 8.0 });

    // Settings summary
    let settings_summary = container(
        text(format!(
            "{}dpi · {}mm · {}mm/min",
            state.dpi_input, state.cut_z_mm_input, state.feed_rate_input
        ))
        .font(palette::MONO)
        .size(10)
        .color(palette::text_muted()),
    )
    .padding(iced::Padding { top: 0.0, right: 8.0, bottom: 4.0, left: 8.0 });

    // Run All / Cancel
    let is_running = state.gerber_to_png == StepState::Running
        || state.png_to_gcode == StepState::Running;

    let run_btn: Element<'_, crate::Message> = if is_running {
        button(text("■  Cancel").font(palette::MONO).size(13))
            .style(styles::ghost_action_style)
            .width(Length::Fill)
            .padding([10, 14])
            .into()
    } else {
        button(text("▶  Run All").font(palette::mono_bold()).size(13))
            .style(styles::primary_action_style)
            .width(Length::Fill)
            .padding([10, 14])
            .on_press(crate::Message::RunAll)
            .into()
    };

    // Status dot
    let (dot_color, status_label): (Color, &str) = if is_running {
        (palette::accent(), "Running…")
    } else if state.status.to_lowercase().contains("fail")
        || state.status.to_lowercase().contains("error")
    {
        (Color::from_rgb(1.0, 0.4, 0.4), "Error")
    } else {
        (palette::text_muted(), "Idle")
    };

    let status_dot = container("")
        .width(Length::Fixed(7.0))
        .height(Length::Fixed(7.0))
        .style(move |_: &Theme| {
            container::Style::default()
                .background(iced::Background::Color(dot_color))
                .border(iced::border::rounded(4.0))
        });

    let status_row = row![
        status_dot,
        text(status_label).font(palette::MONO).size(11).color(palette::text_muted()),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    container(
        column![
            logo,
            make_divider(),
            nav1,
            file_list,
            nav2,
            settings_summary,
            nav3,
            nav4,
            container("").height(Length::Fill),
            make_divider(),
            run_btn,
            make_divider(),
            status_row,
        ]
        .spacing(2),
    )
    .width(Length::Fixed(200.0))
    .height(Length::Fill)
    .padding([16, 12])
    .style(styles::sidebar_style())
    .into()
}

// --- Private helpers ---

fn nav_item<'a>(
    step: u8,
    label: &'a str,
    (badge_text, badge_color): (&'a str, Color),
    is_active: bool,
) -> Element<'a, crate::Message> {
    button(
        row![
            text(format!("{step}"))
                .font(palette::MONO)
                .size(10)
                .color(palette::text_muted()),
            text(label)
                .font(palette::MONO)
                .size(12)
                .color(if is_active { palette::accent() } else { palette::text_secondary() }),
            container("").width(Length::Fill),
            text(badge_text)
                .font(palette::MONO)
                .size(11)
                .color(badge_color),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .width(Length::Fill),
    )
    .style(if is_active { styles::nav_item_active_style } else { styles::nav_item_style })
    .width(Length::Fill)
    .padding([5, 8])
    .on_press(crate::Message::StepToggled(step))
    .into()
}

fn sidebar_file_row<'a>(
    kind_label: &'static str,
    kind_color: Color,
    filename: &'a str,
    kind: crate::LayerKind,
    index: usize,
) -> Element<'a, crate::Message> {
    row![
        container(
            text(kind_label).font(palette::MONO).size(10).color(kind_color),
        )
        .width(Length::Fixed(28.0))
        .padding([0, 8]),
        text(filename)
            .font(palette::MONO)
            .size(10)
            .color(palette::text_muted())
            .width(Length::Fill),
        button(
            text("✕").font(palette::MONO).size(9).color(palette::text_muted()),
        )
        .style(styles::transparent_button_style)
        .padding([1, 4])
        .on_press(crate::Message::RemoveFile { layer: kind, index }),
    ]
    .spacing(2)
    .align_y(Alignment::Center)
    .into()
}

fn files_badge(state: &crate::AppState) -> (&'static str, Color) {
    let has_any = !state.copper_paths.is_empty()
        || !state.outline_paths.is_empty()
        || !state.drill_paths.is_empty()
        || state.loaded_png_path.is_some();
    if has_any {
        ("✓", palette::signal_green())
    } else {
        ("○", palette::text_muted())
    }
}

fn settings_badge() -> (&'static str, Color) {
    ("✓", palette::signal_green())
}

fn rasterize_badge(state: &crate::AppState) -> (&'static str, Color) {
    if state.rasterize_stale {
        return ("⚠", palette::signal_gold());
    }
    match state.gerber_to_png {
        StepState::Complete => ("✓", palette::signal_green()),
        StepState::Running => ("●", palette::accent()),
        _ => ("○", palette::text_muted()),
    }
}

fn gcode_badge(state: &crate::AppState) -> (&'static str, Color) {
    if state.gcode_stale {
        return ("⚠", palette::signal_gold());
    }
    match state.png_to_gcode {
        StepState::Complete => ("✓", palette::signal_green()),
        StepState::Running => ("●", palette::accent()),
        _ => ("○", palette::text_muted()),
    }
}
