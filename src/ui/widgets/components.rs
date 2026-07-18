use iced::{
    Alignment, Color, Element, Length, Theme,
    widget::{button, column, container, row, text, text_input},
};
use easymill::stackup::{LayerCategory, Side};
use crate::ui::{palette, styles};

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
            .style(styles::text_input_style)
            .padding([7, 10])
            .size(13)
            .width(Length::Fill),
    ]
    .spacing(4)
    .into()
}

pub fn drop_zone<'a>(on_press: crate::Message) -> Element<'a, crate::Message> {
    button(
        container(
            column![
                text("Drop files here, or click to browse")
                    .font(palette::MONO)
                    .size(13)
                    .color(palette::text_secondary()),
                text(".GTL  .GBL  .GKO  .DRL  .TXT  …")
                    .font(palette::MONO)
                    .size(11)
                    .color(palette::text_muted()),
            ]
            .spacing(6)
            .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([24, 16])
        .style(styles::drop_zone_style()),
    )
    .style(styles::transparent_button_style)
    .width(Length::Fill)
    .on_press(on_press)
    .into()
}

pub fn accordion<'a>(
    label: &'a str,
    summary: String,
    is_open: bool,
    toggle_msg: crate::Message,
    content: Element<'a, crate::Message>,
) -> Element<'a, crate::Message> {
    let chevron = text(if is_open { "▾" } else { "▸" })
        .font(palette::MONO)
        .size(12)
        .color(palette::text_muted());

    let show_summary = !summary.is_empty();
    let summary_widget: Element<'_, crate::Message> = if !is_open && show_summary {
        text(summary)
            .font(palette::MONO)
            .size(11)
            .color(palette::text_muted())
            .into()
    } else {
        container("").into()
    };

    let header = button(
        row![
            chevron,
            text(label)
                .font(palette::MONO)
                .size(12)
                .color(palette::text_secondary()),
            container("").width(Length::Fill),
            summary_widget,
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill),
    )
    .style(styles::transparent_button_style)
    .width(Length::Fill)
    .padding([6, 0])
    .on_press(toggle_msg);

    if is_open {
        column![header, container(content).padding(iced::Padding { top: 4.0, right: 16.0, bottom: 8.0, left: 16.0 })]
            .spacing(0)
            .into()
    } else {
        column![header].into()
    }
}

pub fn layer_row<'a>(
    index: usize,
    cat: LayerCategory,
    side: Side,
    is_overridden: bool,
    filename: String,
    is_editing: bool,
) -> Element<'a, crate::Message> {
    let cat_color = palette::layer_category_color(&cat);
    let label_color = if is_overridden { palette::accent() } else { cat_color };
    let label_text = format!("{} · {}", cat.label(), side.label());

    let label_btn = button(
        row![
            text(label_text)
                .font(palette::MONO)
                .size(11)
                .color(label_color),
            text(if is_editing { " ▴" } else { " ▾" })
                .font(palette::MONO)
                .size(9)
                .color(palette::text_muted()),
        ]
        .align_y(Alignment::Center),
    )
    .style(|_: &Theme, status: button::Status| button::Style {
        background: if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            Some(iced::Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.05)))
        } else {
            None
        },
        border: iced::border::rounded(4.0),
        ..Default::default()
    })
    .padding([2, 5])
    .on_press(crate::Message::EditLayerToggle(index));

    let reset_btn: Element<'_, crate::Message> = if is_overridden {
        button(
            text("↺").font(palette::MONO).size(10).color(palette::accent()),
        )
        .style(styles::transparent_button_style)
        .padding([2, 4])
        .on_press(crate::Message::ResetLayerOverride(index))
        .into()
    } else {
        container("").width(Length::Fixed(0.0)).into()
    };

    let top_row: Element<'_, crate::Message> = row![
        label_btn,
        text(filename)
            .font(palette::MONO)
            .size(13)
            .color(palette::text_secondary())
            .width(Length::Fill),
        reset_btn,
        button(
            text("✕").font(palette::MONO).size(11).color(palette::text_muted()),
        )
        .style(styles::transparent_button_style)
        .padding([2, 6])
        .on_press(crate::Message::RemoveFile { index }),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into();

    if !is_editing {
        return top_row;
    }

    // --- Inline type/side picker ---

    let cat_chips: Vec<Element<'_, crate::Message>> = LayerCategory::variants()
        .iter()
        .map(|&c| {
            let is_active = c == cat;
            let color = palette::layer_category_color(&c);
            button(text(c.label()).font(palette::MONO).size(11))
                .style(move |_: &Theme, status: button::Status| button::Style {
                    background: if is_active {
                        Some(iced::Background::Color(Color::from_rgba(color.r, color.g, color.b, 0.18)))
                    } else if matches!(status, button::Status::Hovered) {
                        Some(iced::Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.05)))
                    } else {
                        None
                    },
                    border: if is_active {
                        iced::border::rounded(4.0).color(color).width(1.0)
                    } else {
                        iced::border::rounded(4.0).color(Color::from_rgba(1.0, 1.0, 1.0, 0.08)).width(1.0)
                    },
                    text_color: if is_active { color } else { palette::text_muted() },
                    ..Default::default()
                })
                .padding([3, 7])
                .on_press(crate::Message::SetLayerCategory { index, category: c })
                .into()
        })
        .collect();

    let side_chips: Vec<Element<'_, crate::Message>> = Side::variants()
        .iter()
        .map(|&s| {
            let is_active = match (s, side) {
                (Side::Top, Side::Top) => true,
                (Side::Bottom, Side::Bottom) => true,
                (Side::Inner(_), Side::Inner(_)) => true,
                (Side::All, Side::All) => true,
                _ => false,
            };
            let active_color = palette::text_primary();
            button(text(s.label()).font(palette::MONO).size(11))
                .style(move |_: &Theme, status: button::Status| button::Style {
                    background: if is_active {
                        Some(iced::Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.10)))
                    } else if matches!(status, button::Status::Hovered) {
                        Some(iced::Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.04)))
                    } else {
                        None
                    },
                    border: if is_active {
                        iced::border::rounded(4.0).color(Color::from_rgba(1.0, 1.0, 1.0, 0.30)).width(1.0)
                    } else {
                        iced::border::rounded(4.0).color(Color::from_rgba(1.0, 1.0, 1.0, 0.08)).width(1.0)
                    },
                    text_color: if is_active { active_color } else { palette::text_muted() },
                    ..Default::default()
                })
                .padding([3, 7])
                .on_press(crate::Message::SetLayerSide { index, side: s })
                .into()
        })
        .collect();

    let picker = container(
        column![
            row![
                text("Type").font(palette::MONO).size(10).color(palette::text_muted())
                    .width(Length::Fixed(36.0)),
                iced::widget::Row::with_children(cat_chips).spacing(4),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            row![
                text("Side").font(palette::MONO).size(10).color(palette::text_muted())
                    .width(Length::Fixed(36.0)),
                iced::widget::Row::with_children(side_chips).spacing(4),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(6),
    )
    .padding(iced::Padding { top: 8.0, right: 4.0, bottom: 4.0, left: 4.0 });

    column![top_row, picker]
        .spacing(2)
        .into()
}
