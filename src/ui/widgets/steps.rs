use iced::{
    Alignment, Color, Element, Length, Theme,
    widget::{button, column, container, progress_bar, row, text},
};
use crate::StepState;
use crate::ui::{palette, styles};

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
    let has_input = !state.copper_paths.is_empty()
        || !state.outline_paths.is_empty()
        || !state.drill_paths.is_empty()
        || state.loaded_png_path.is_some();
    let vs = if has_input { CardVisualState::Complete } else { CardVisualState::Active };
    let is_expanded = state.expanded_step == Some(1);
    let summary = if has_input {
        let n = state.copper_paths.len() + state.outline_paths.len() + state.drill_paths.len();
        format!("{n} file(s) loaded")
    } else {
        "No files loaded".to_owned()
    };
    step_shell(
        1, "FILES", vs, is_expanded, summary, None,
        text("(files content — Task 7)").font(palette::MONO).size(13).color(palette::text_muted()).into(),
    )
}

pub fn settings_step<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    let is_expanded = state.expanded_step == Some(2);
    let summary = format!(
        "{}dpi · {}mm cut · {} mm/min",
        state.dpi_input, state.cut_z_mm_input, state.feed_rate_input
    );
    step_shell(
        2, "SETTINGS", CardVisualState::Complete, is_expanded, summary, None,
        text("(settings content — Task 8)").font(palette::MONO).size(13).color(palette::text_muted()).into(),
    )
}

pub fn rasterize_step<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    let has_gerbers = !state.copper_paths.is_empty()
        || !state.outline_paths.is_empty()
        || !state.drill_paths.is_empty();
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

    let summary = match state.gerber_to_png {
        StepState::Complete => "3 layers rendered".to_owned(),
        _ => if is_skipped { "Skipped — PNG loaded directly".to_owned() } else { "Not yet run".to_owned() },
    };

    let is_expanded = state.expanded_step == Some(3);
    step_shell(
        3, "RASTERIZE", vs, is_expanded, summary, None,
        text("(rasterize content — Task 9)").font(palette::MONO).size(13).color(palette::text_muted()).into(),
    )
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
        StepState::Complete => "G-code ready".to_owned(),
        _ => "Not yet run".to_owned(),
    };

    let is_expanded = state.expanded_step == Some(4);
    step_shell(
        4, "G-CODE", vs, is_expanded, summary, None,
        text("(gcode content — Task 10)").font(palette::MONO).size(13).color(palette::text_muted()).into(),
    )
}
