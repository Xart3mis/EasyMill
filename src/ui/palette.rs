use iced::Color;
use iced::Font;

pub const MONO: Font = Font::MONOSPACE;

pub fn mono_bold() -> Font {
    Font::MONOSPACE
}

pub fn accent() -> Color {
    Color::from_rgb(0.49, 0.81, 1.0)
}

pub fn accent_muted() -> Color {
    Color::from_rgba(0.49, 0.81, 1.0, 0.15)
}

pub fn surface_inset() -> Color {
    Color::from_rgb(0.12, 0.14, 0.21)
}

pub fn text_primary() -> Color {
    Color::from_rgb(0.75, 0.79, 0.96)
}

pub fn text_secondary() -> Color {
    Color::from_rgb(0.66, 0.69, 0.84)
}

pub fn text_muted() -> Color {
    Color::from_rgb(0.34, 0.37, 0.54)
}

pub fn text_accent() -> Color {
    accent()
}

pub fn signal_green() -> Color {
    Color::from_rgb(0.62, 0.81, 0.42)
}

pub fn signal_green_muted() -> Color {
    Color::from_rgba(0.62, 0.81, 0.42, 0.15)
}

pub fn signal_gold() -> Color {
    Color::from_rgb(0.88, 0.69, 0.41)
}

pub fn signal_gold_muted() -> Color {
    Color::from_rgba(0.88, 0.69, 0.41, 0.15)
}

pub fn sidebar_bg() -> Color {
    Color::from_rgb(0.094, 0.106, 0.172)
}

pub fn sidebar_border() -> Color {
    Color::from_rgba(1.0, 1.0, 1.0, 0.08)
}

pub fn card_bg() -> Color {
    Color::from_rgba(1.0, 1.0, 1.0, 0.03)
}

pub fn card_active_bg() -> Color {
    Color::from_rgba(0.49, 0.81, 1.0, 0.06)
}

pub fn card_complete_bg() -> Color {
    Color::from_rgba(0.62, 0.81, 0.42, 0.08)
}

pub fn card_stale_bg() -> Color {
    Color::from_rgba(0.88, 0.69, 0.41, 0.08)
}

pub fn input_bg() -> Color {
    Color::from_rgba(1.0, 1.0, 1.0, 0.06)
}

pub fn drop_zone_bg() -> Color {
    Color::from_rgba(1.0, 1.0, 1.0, 0.02)
}

pub fn drop_zone_border() -> Color {
    Color::from_rgba(1.0, 1.0, 1.0, 0.10)
}

pub fn drop_zone_active_bg() -> Color {
    Color::from_rgba(0.49, 0.81, 1.0, 0.08)
}

pub fn layer_copper() -> Color {
    Color::from_rgb(0.88, 0.69, 0.41)
}

pub fn layer_outline() -> Color {
    accent()
}

pub fn layer_drill() -> Color {
    Color::from_rgb(1.0, 0.5, 0.5)
}

pub fn layer_mask() -> Color {
    Color::from_rgb(0.3, 0.85, 0.5)
}

pub fn layer_silk() -> Color {
    Color::from_rgb(0.85, 0.85, 0.90)
}

pub fn layer_paste() -> Color {
    Color::from_rgb(0.7, 0.7, 0.3)
}

pub fn layer_drawing() -> Color {
    Color::from_rgb(0.6, 0.6, 0.6)
}

pub fn layer_unknown() -> Color {
    Color::from_rgb(1.0, 0.2, 0.2)
}

pub fn layer_category_color(cat: &easymill::stackup::LayerCategory) -> Color {
    match cat {
        easymill::stackup::LayerCategory::Copper => layer_copper(),
        easymill::stackup::LayerCategory::Soldermask => layer_mask(),
        easymill::stackup::LayerCategory::Silkscreen => layer_silk(),
        easymill::stackup::LayerCategory::Solderpaste => layer_paste(),
        easymill::stackup::LayerCategory::Outline => layer_outline(),
        easymill::stackup::LayerCategory::Drill => layer_drill(),
        easymill::stackup::LayerCategory::Drawing => layer_drawing(),
        easymill::stackup::LayerCategory::Unknown => layer_unknown(),
    }
}
