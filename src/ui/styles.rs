use iced::widget::{button, container};
use iced::{Color, Theme};
use super::palette;

pub fn panel_style() -> impl Fn(&Theme) -> container::Style {
    |_| {
        container::Style::default()
            .background(iced::Background::Color(Color::from_rgb(0.14, 0.16, 0.24)))
            .border(iced::border::rounded(12.0).color(Color::from_rgba(1.0, 1.0, 1.0, 0.06)).width(1.0))
    }
}

pub fn inset_style() -> impl Fn(&Theme) -> container::Style {
    |_| {
        container::Style::default()
            .background(iced::Background::Color(palette::surface_inset()))
            .border(iced::border::rounded(8.0).color(Color::from_rgba(1.0, 1.0, 1.0, 0.04)).width(1.0))
    }
}

pub fn app_style() -> impl Fn(&Theme) -> container::Style {
    |_| {
        container::Style::default()
            .background(iced::Background::Color(Color::from_rgb(0.10, 0.11, 0.18)))
    }
}

pub fn primary_action_style(
    _theme: &Theme,
    status: button::Status,
) -> button::Style {
    let mut style = button::Style::default();
    style.background = Some(iced::Background::Color(palette::accent()));
    style.border = iced::border::rounded(8.0);
    style.text_color = Color::from_rgb(0.06, 0.07, 0.12);
    match status {
        button::Status::Hovered => {
            style.background = Some(iced::Background::Color(palette::text_accent()));
        }
        button::Status::Pressed => {
            style.background = Some(iced::Background::Color(palette::signal_green()));
        }
        _ => {}
    }
    style
}

pub fn secondary_action_style(
    _theme: &Theme,
    status: button::Status,
) -> button::Style {
    let mut style = button::Style::default();
    style.background = Some(iced::Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.06)));
    style.border = iced::border::rounded(8.0).color(Color::from_rgba(1.0, 1.0, 1.0, 0.08)).width(1.0);
    style.text_color = palette::text_primary();
    match status {
        button::Status::Hovered => {
            style.background = Some(iced::Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.10)));
        }
        button::Status::Pressed => {
            style.background = Some(iced::Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.14)));
        }
        _ => {}
    }
    style
}

pub fn ghost_action_style(
    _theme: &Theme,
    status: button::Status,
) -> button::Style {
    let mut style = button::Style::default();
    style.background = None;
    style.border = iced::border::rounded(8.0).color(Color::from_rgba(1.0, 1.0, 1.0, 0.06)).width(1.0);
    style.text_color = palette::text_muted();
    match status {
        button::Status::Hovered => {
            style.background = Some(iced::Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.04)));
        }
        button::Status::Pressed => {
            style.background = Some(iced::Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.08)));
        }
        _ => {}
    }
    style
}

pub fn sidebar_style() -> impl Fn(&Theme) -> container::Style {
    |_| {
        container::Style::default()
            .background(iced::Background::Color(palette::sidebar_bg()))
            .border(
                iced::border::rounded(0.0)
                    .color(palette::sidebar_border())
                    .width(1.0),
            )
    }
}

pub fn card_active_style() -> impl Fn(&Theme) -> container::Style {
    |_| {
        container::Style::default()
            .background(iced::Background::Color(palette::card_active_bg()))
            .border(
                iced::border::rounded(10.0)
                    .color(palette::accent())
                    .width(1.5),
            )
    }
}

pub fn card_complete_style() -> impl Fn(&Theme) -> container::Style {
    |_| {
        container::Style::default()
            .background(iced::Background::Color(palette::card_complete_bg()))
            .border(
                iced::border::rounded(10.0)
                    .color(palette::signal_green())
                    .width(1.5),
            )
    }
}

pub fn card_stale_style() -> impl Fn(&Theme) -> container::Style {
    |_| {
        container::Style::default()
            .background(iced::Background::Color(palette::card_stale_bg()))
            .border(
                iced::border::rounded(10.0)
                    .color(palette::signal_gold())
                    .width(1.5),
            )
    }
}

pub fn card_waiting_style() -> impl Fn(&Theme) -> container::Style {
    |_| {
        container::Style::default()
            .background(iced::Background::Color(palette::card_bg()))
            .border(
                iced::border::rounded(10.0)
                    .color(Color::from_rgba(1.0, 1.0, 1.0, 0.05))
                    .width(1.0),
            )
    }
}

pub fn drop_zone_style() -> impl Fn(&Theme) -> container::Style {
    |_| {
        container::Style::default()
            .background(iced::Background::Color(palette::drop_zone_bg()))
            .border(
                iced::border::rounded(8.0)
                    .color(palette::drop_zone_border())
                    .width(1.0),
            )
    }
}

pub fn text_input_style(
    _theme: &Theme,
    _status: iced::widget::text_input::Status,
) -> iced::widget::text_input::Style {
    iced::widget::text_input::Style {
        background: iced::Background::Color(palette::input_bg()),
        border: iced::border::rounded(6.0)
            .color(Color::from_rgba(1.0, 1.0, 1.0, 0.08))
            .width(1.0),
        icon: palette::text_muted(),
        placeholder: palette::text_muted(),
        value: palette::text_primary(),
        selection: palette::accent_muted(),
    }
}

pub fn nav_item_active_style(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(iced::Background::Color(palette::accent_muted())),
        border: iced::border::rounded(6.0),
        text_color: palette::accent(),
        ..Default::default()
    }
}

pub fn nav_item_style(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: if matches!(status, button::Status::Hovered) {
            Some(iced::Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.04)))
        } else {
            None
        },
        border: iced::border::rounded(6.0),
        text_color: palette::text_secondary(),
        ..Default::default()
    }
}

pub fn transparent_button_style(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: None,
        border: iced::border::rounded(0.0),
        text_color: palette::text_primary(),
        ..Default::default()
    }
}
