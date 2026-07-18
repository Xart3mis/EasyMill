use iced::{
    Alignment, Color, Element, Length, Theme,
    widget::{button, column, container, row, text, text_input},
};
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
    kind_label: &'static str,
    kind_color: Color,
    filename: &'a str,
    remove_msg: crate::Message,
) -> Element<'a, crate::Message> {
    row![
        container(
            text(kind_label)
                .font(palette::MONO)
                .size(11)
                .color(kind_color),
        )
        .width(Length::Fixed(32.0)),
        text(filename)
            .font(palette::MONO)
            .size(13)
            .color(palette::text_secondary())
            .width(Length::Fill),
        button(
            text("✕")
                .font(palette::MONO)
                .size(11)
                .color(palette::text_muted()),
        )
        .style(styles::transparent_button_style)
        .padding([2, 6])
        .on_press(remove_msg),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}
