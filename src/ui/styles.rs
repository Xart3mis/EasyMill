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
