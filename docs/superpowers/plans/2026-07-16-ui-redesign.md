# EasyMill UI Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the 3-tab layout with a glassy fixed sidebar + vertical step-flow canvas (Files → Settings → Rasterize → G-code) where completed steps collapse to summaries and stale steps show invalidation warnings.

**Architecture:** A 200px fixed sidebar doubles as file manager and navigation; clicking a nav item scrolls/expands the corresponding step card in the main canvas. Step cards compute their visual state (Active/Complete/Stale/Waiting) from AppState and cascade invalidation when upstream inputs change. The four step widgets replace all existing tab-specific panels.

**Tech Stack:** Rust, iced 0.14.0, rfd 0.15, tokio

## Global Constraints

- iced 0.14.0 — use `center_x(Length::Fill)` not `center_x()`, `on_press_maybe` available on buttons
- All widget functions follow the existing `pub fn name<'a>(state: &'a AppState) -> Element<'a, Message>` signature pattern
- Monospace font throughout — use `palette::MONO` and `palette::mono_bold()`
- No new dependencies — work within existing Cargo.toml
- `cargo check` must pass after every task before committing

---

## File Structure

**Modified:**
- `src/main.rs` — AppState fields, LayerKind enum, Message variants, update() handlers, view()
- `src/ui/palette.rs` — new glass-aesthetic color tokens
- `src/ui/styles.rs` — new card state style functions, sidebar style
- `src/ui/mod.rs` — updated pub use exports

**Deleted:**
- `src/ui/widgets.rs` — replaced by the directory below

**Created:**
- `src/ui/widgets/mod.rs` — submodule declarations + re-exports of all public widgets
- `src/ui/widgets/components.rs` — `setting_field`, `drop_zone`, `accordion`, `layer_row`
- `src/ui/widgets/sidebar.rs` — `sidebar()` + private helpers
- `src/ui/widgets/steps.rs` — `step_canvas`, `step_shell`, `files_step`, `settings_step`, `rasterize_step`, `gcode_step`, `CardVisualState`

---

## Task 1: Data Layer — new types, AppState fields, Messages, update() handlers

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Produces: `LayerKind` enum, `CardVisualState` (in steps.rs later), `AppState::rasterize_stale`, `AppState::gcode_stale`, `AppState::expanded_step`, `AppState::settings_groups_open`, `Message::StepToggled`, `Message::RemoveFile`, `Message::SettingsGroupToggled`, `Message::ReRunRasterize`, `Message::ReRunGcode`, `Message::RunAll`

- [ ] **Step 1: Add `LayerKind` enum after the `StepState` enum (line ~39 in main.rs)**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayerKind {
    Copper,
    Outline,
    Drill,
}
```

- [ ] **Step 2: Add four new fields to `AppState` struct (after `gcode_progress` field)**

```rust
pub(crate) rasterize_stale: bool,
pub(crate) gcode_stale: bool,
pub(crate) expanded_step: Option<u8>,
pub(crate) settings_groups_open: [bool; 4],
```

- [ ] **Step 3: Add the four new fields to `impl Default for AppState`**

Add inside the `Self { ... }` block:
```rust
rasterize_stale: false,
gcode_stale: false,
expanded_step: Some(1),
settings_groups_open: [true, true, false, false],
```

- [ ] **Step 4: Add six new variants to the `Message` enum**

```rust
StepToggled(u8),
RemoveFile { layer: LayerKind, index: usize },
SettingsGroupToggled(usize),
ReRunRasterize,
ReRunGcode,
RunAll,
```

- [ ] **Step 5: Add handlers for all six new messages in `update()`**

Add these arms to the `match message` block (before the closing brace):

```rust
Message::StepToggled(n) => {
    state.expanded_step = if state.expanded_step == Some(n) { None } else { Some(n) };
}
Message::RemoveFile { layer, index } => {
    match layer {
        LayerKind::Copper => {
            if index < state.copper_paths.len() {
                state.copper_paths.remove(index);
            }
        }
        LayerKind::Outline => {
            if index < state.outline_paths.len() {
                state.outline_paths.remove(index);
            }
        }
        LayerKind::Drill => {
            if index < state.drill_paths.len() {
                state.drill_paths.remove(index);
            }
        }
    }
    state.rasterize_stale = state.gerber_to_png == StepState::Complete;
    state.gcode_stale = state.png_to_gcode == StepState::Complete;
    state.loaded_inputs = derive_loaded_inputs(state);
}
Message::SettingsGroupToggled(i) => {
    if i < 4 {
        state.settings_groups_open[i] = !state.settings_groups_open[i];
    }
}
Message::ReRunRasterize => {
    state.rasterize_stale = false;
    state.gcode_stale = state.png_to_gcode == StepState::Complete;
    return update(state, Message::ConvertToPng);
}
Message::ReRunGcode => {
    state.gcode_stale = false;
    return update(state, Message::GenerateGcode);
}
Message::RunAll => {
    let has_gerbers = !state.copper_paths.is_empty()
        || !state.outline_paths.is_empty()
        || !state.drill_paths.is_empty();
    let rasterize_needed = has_gerbers
        && (state.gerber_to_png != StepState::Complete || state.rasterize_stale);
    let gcode_needed = state.loaded_png_path.is_some()
        && (state.png_to_gcode != StepState::Complete || state.gcode_stale);
    if rasterize_needed {
        state.rasterize_stale = false;
        return update(state, Message::ConvertToPng);
    } else if gcode_needed {
        state.gcode_stale = false;
        return update(state, Message::GenerateGcode);
    }
}
```

- [ ] **Step 6: Add stale cascades to existing file-pick handlers in `update()`**

In the `Message::CopperFilesPicked(files)` arm, after the existing assignments add:
```rust
state.rasterize_stale = state.gerber_to_png == StepState::Complete;
state.gcode_stale = state.png_to_gcode == StepState::Complete;
```

Repeat the same two lines in `Message::OutlineFilesPicked` and `Message::DrillFilesPicked`.

In `Message::LoadPngPicked`, after `state.loaded_png_path = Some(path.clone());` add:
```rust
state.generated_pngs = None;
state.gerber_to_png = StepState::Idle;
state.gcode_stale = state.png_to_gcode == StepState::Complete;
```

- [ ] **Step 7: Add stale cascades to settings change handlers**

In `Message::DpiChanged`, after `state.dpi_input = val;` add:
```rust
state.rasterize_stale = state.gerber_to_png == StepState::Complete;
state.gcode_stale = state.png_to_gcode == StepState::Complete;
```

For all other settings handlers (`CutZChanged`, `SafeZChanged`, `FeedRateChanged`, `PlungeRateChanged`, `SpindleSpeedChanged`, `ToolDiameterChanged`, `OffsetNumberChanged`, `OffsetStepoverChanged`), add after each existing assignment:
```rust
state.gcode_stale = state.png_to_gcode == StepState::Complete;
```

- [ ] **Step 8: Clear stale flags in completion handlers**

In `Message::ConvertToPngFinished(Ok(...))` arm, after `state.gerber_to_png = StepState::Complete;` add:
```rust
state.rasterize_stale = false;
```

In `Message::GenerateGcodeFinished(Ok(...))` arm, after `state.png_to_gcode = StepState::Complete;` add:
```rust
state.gcode_stale = false;
```

- [ ] **Step 9: Verify compilation**

```bash
cargo check 2>&1 | head -40
```

Expected: no errors. Warnings about unused `LayerKind` fields are fine at this stage.

- [ ] **Step 10: Commit**

```bash
git add src/main.rs
git commit -m "feat: add LayerKind, stale flags, step expand state, and new Messages to data layer"
```

---

## Task 2: Palette & Styles — glass aesthetic tokens

**Files:**
- Modify: `src/ui/palette.rs`
- Modify: `src/ui/styles.rs`

**Interfaces:**
- Produces: `palette::sidebar_bg`, `palette::sidebar_border`, `palette::card_bg`, `palette::card_active_bg`, `palette::card_complete_bg`, `palette::card_stale_bg`, `palette::input_bg`, `palette::drop_zone_bg`, `palette::drop_zone_border`, `palette::drop_zone_active_bg`
- Produces: `styles::sidebar_style`, `styles::card_active_style`, `styles::card_complete_style`, `styles::card_stale_style`, `styles::card_waiting_style`, `styles::drop_zone_style`, `styles::text_input_style`

- [ ] **Step 1: Add new color tokens to `src/ui/palette.rs`**

Append after the last function:

```rust
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
```

- [ ] **Step 2: Add new style functions to `src/ui/styles.rs`**

Append after the last function:

```rust
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
```

- [ ] **Step 3: Verify compilation**

```bash
cargo check 2>&1 | head -40
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/ui/palette.rs src/ui/styles.rs
git commit -m "feat: add glass-aesthetic palette tokens and card/sidebar style functions"
```

---

## Task 3: Widget file restructure

**Files:**
- Delete: `src/ui/widgets.rs`
- Create: `src/ui/widgets/mod.rs`, `src/ui/widgets/components.rs`, `src/ui/widgets/sidebar.rs`, `src/ui/widgets/steps.rs`

- [ ] **Step 1: Create the widgets directory and move the existing file**

```bash
mkdir -p /home/sico/Code/EasyMill/src/ui/widgets
cp /home/sico/Code/EasyMill/src/ui/widgets.rs /home/sico/Code/EasyMill/src/ui/widgets/mod.rs
rm /home/sico/Code/EasyMill/src/ui/widgets.rs
```

- [ ] **Step 2: Create empty `components.rs`, `sidebar.rs`, `steps.rs` with placeholder content**

Create `src/ui/widgets/components.rs`:
```rust
// Shared reusable components — populated in Tasks 4–9
```

Create `src/ui/widgets/sidebar.rs`:
```rust
// Sidebar widget — populated in Task 4
```

Create `src/ui/widgets/steps.rs`:
```rust
// Step canvas and step cards — populated in Tasks 5–9
```

- [ ] **Step 3: Add submodule declarations at the top of `src/ui/widgets/mod.rs`**

Add these three lines at the very top of the file (before existing `use` statements):
```rust
pub mod components;
pub mod sidebar;
pub mod steps;
```

- [ ] **Step 4: Verify compilation**

```bash
cargo check 2>&1 | head -40
```

Expected: no errors. The app still compiles with the old widgets in `mod.rs`.

- [ ] **Step 5: Commit**

```bash
git add src/ui/widgets/ src/ui/widgets.rs
git commit -m "refactor: convert widgets.rs to widgets/ directory module"
```

---

## Task 4: Shared components — `drop_zone`, `accordion`, `layer_row`, updated `setting_field`

**Files:**
- Modify: `src/ui/widgets/components.rs`

**Interfaces:**
- Produces:
  - `drop_zone<'a>(on_press: Message) -> Element<'a, Message>`
  - `accordion<'a>(label, summary, is_open, toggle_msg, content) -> Element<'a, Message>`
  - `layer_row<'a>(kind_label, kind_color, filename, remove_msg) -> Element<'a, Message>`
  - `setting_field<'a>(label, value, on_change) -> Element<'a, Message>` (moved from mod.rs)

- [ ] **Step 1: Write `src/ui/widgets/components.rs` in full**

```rust
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
    summary: &'a str,
    is_open: bool,
    toggle_msg: crate::Message,
    content: Element<'a, crate::Message>,
) -> Element<'a, crate::Message> {
    let chevron = text(if is_open { "▾" } else { "▸" })
        .font(palette::MONO)
        .size(12)
        .color(palette::text_muted());

    let summary_widget: Element<'_, crate::Message> = if !is_open && !summary.is_empty() {
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
        column![header, container(content).padding([4, 16, 8, 16])]
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
```

- [ ] **Step 2: Verify compilation**

```bash
cargo check 2>&1 | head -40
```

Expected: no errors. Note: `setting_field` still exists in `mod.rs` too — that's fine for now; it'll be removed in Task 9.

- [ ] **Step 3: Commit**

```bash
git add src/ui/widgets/components.rs
git commit -m "feat: add drop_zone, accordion, layer_row shared components"
```

---

## Task 5: Sidebar widget

**Files:**
- Modify: `src/ui/widgets/sidebar.rs`

**Interfaces:**
- Produces: `pub fn sidebar<'a>(state: &'a AppState) -> Element<'a, Message>`

- [ ] **Step 1: Write `src/ui/widgets/sidebar.rs` in full**

```rust
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

    let divider = container("").width(Length::Fill).height(Length::Fixed(1.0))
        .style(|_: &Theme| container::Style::default()
            .background(iced::Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.06))));

    // --- Nav items ---
    let nav1 = nav_item(1, "FILES", files_badge(state), state.expanded_step == Some(1));
    let nav2 = nav_item(2, "SETTINGS", settings_badge(), state.expanded_step == Some(2));
    let nav3 = nav_item(3, "RASTERIZE", rasterize_badge(state), state.expanded_step == Some(3));
    let nav4 = nav_item(4, "G-CODE", gcode_badge(state), state.expanded_step == Some(4));

    // --- Files sub-list ---
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
                    container(text("PNG").font(palette::MONO).size(10).color(palette::signal_green()))
                        .width(Length::Fixed(32.0)),
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
        .padding([0, 0, 4, 8]);

    // --- Settings summary ---
    let settings_summary = container(
        text(format!(
            "{}dpi · {}mm · {}mm/min",
            state.dpi_input, state.cut_z_mm_input, state.feed_rate_input
        ))
        .font(palette::MONO)
        .size(10)
        .color(palette::text_muted()),
    )
    .padding([0, 8, 4, 8]);

    // --- Run All / Cancel ---
    let is_running = state.gerber_to_png == StepState::Running
        || state.png_to_gcode == StepState::Running;

    let run_btn: Element<'_, crate::Message> = if is_running {
        button(
            text("■  Cancel").font(palette::MONO).size(13),
        )
        .style(styles::ghost_action_style)
        .width(Length::Fill)
        .padding([10, 14])
        .into()
    } else {
        button(
            text("▶  Run All").font(palette::mono_bold()).size(13),
        )
        .style(styles::primary_action_style)
        .width(Length::Fill)
        .padding([10, 14])
        .on_press(crate::Message::RunAll)
        .into()
    };

    // --- Status dot ---
    let (dot_color, status_label): (Color, &str) = if is_running {
        (palette::accent(), "Running…")
    } else if state.status.to_lowercase().contains("fail") || state.status.to_lowercase().contains("error") {
        (Color::from_rgb(1.0, 0.4, 0.4), "Error")
    } else {
        (palette::text_muted(), "Idle")
    };

    let status_dot = container("").width(Length::Fixed(7.0)).height(Length::Fixed(7.0))
        .style(move |_: &Theme| container::Style::default()
            .background(iced::Background::Color(dot_color))
            .border(iced::border::rounded(4.0)));

    let status_row = row![
        status_dot,
        text(status_label).font(palette::MONO).size(11).color(palette::text_muted()),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    container(
        column![
            logo,
            divider.clone(),
            nav1,
            file_list,
            nav2,
            settings_summary,
            nav3,
            nav4,
            container("").height(Length::Fill),
            divider.clone(),
            run_btn,
            divider,
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
        button(text("✕").font(palette::MONO).size(9).color(palette::text_muted()))
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
    if has_any { ("✓", palette::signal_green()) } else { ("○", palette::text_muted()) }
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
```

- [ ] **Step 2: Export `sidebar` from `src/ui/widgets/mod.rs`**

Add at the top of the existing `pub use` block in `mod.rs`:
```rust
pub use sidebar::sidebar;
```

- [ ] **Step 3: Verify compilation**

```bash
cargo check 2>&1 | head -40
```

- [ ] **Step 4: Commit**

```bash
git add src/ui/widgets/sidebar.rs src/ui/widgets/mod.rs
git commit -m "feat: add glassy sidebar with nav, file list, run-all button, and status dot"
```

---

## Task 6: step_shell + step_canvas scaffold + wire view()

**Files:**
- Modify: `src/ui/widgets/steps.rs`
- Modify: `src/ui/widgets/mod.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/main.rs` — `view()` function only

**Interfaces:**
- Produces: `pub fn step_canvas<'a>(state: &'a AppState) -> Element<'a, Message>`
- Produces (private): `CardVisualState`, `step_shell(...)`, `visual_state_colors(...)`

- [ ] **Step 1: Write the scaffold in `src/ui/widgets/steps.rs`**

```rust
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
///
/// - `step_num`: 1..=4
/// - `label`: e.g. "FILES"
/// - `vs`: visual state driving colors and badge
/// - `is_expanded`: whether the card is open
/// - `summary`: one-line text shown when collapsed (empty = no summary row)
/// - `header_action`: optional element shown right of badge in header (e.g. a Run button)
/// - `content`: full card content shown when expanded
pub(crate) fn step_shell<'a>(
    step_num: u8,
    label: &'a str,
    vs: CardVisualState,
    is_expanded: bool,
    summary: &'a str,
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

    // Build header elements into a Vec so we can conditionally append the action
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
            container(content).padding([12, 0, 0, 0]),
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
        // Use a static placeholder for now; will be dynamic in Task 7
        format!("{n} file(s) loaded")
    } else {
        "No files loaded".to_owned()
    };
    step_shell(
        1, "FILES", vs, is_expanded, &summary, None,
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
        2, "SETTINGS", CardVisualState::Complete, is_expanded, &summary, None,
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
        StepState::Complete => "3 layers rendered",
        _ => if is_skipped { "Skipped — PNG loaded directly" } else { "Not yet run" },
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
        StepState::Complete => "G-code ready",
        _ => "Not yet run",
    };

    let is_expanded = state.expanded_step == Some(4);
    step_shell(
        4, "G-CODE", vs, is_expanded, summary, None,
        text("(gcode content — Task 10)").font(palette::MONO).size(13).color(palette::text_muted()).into(),
    )
}
```

- [ ] **Step 2: Export `step_canvas` from `src/ui/widgets/mod.rs`**

Add to the `pub use` block:
```rust
pub use steps::step_canvas;
```

- [ ] **Step 3: Export `sidebar` and `step_canvas` from `src/ui/mod.rs`**

Replace the existing `pub use widgets::{...}` line with:
```rust
pub use widgets::{sidebar, step_canvas};
```

- [ ] **Step 4: Replace `view()` in `src/main.rs`**

Replace the entire `fn view(state: &AppState) -> Element<'_, Message>` function body:

```rust
fn view(state: &AppState) -> Element<'_, Message> {
    use ui::{sidebar, step_canvas};

    let layout = iced::widget::row![
        sidebar(state),
        iced::widget::scrollable(step_canvas(state))
            .height(Length::Fill)
            .width(Length::Fill),
    ]
    .height(Length::Fill);

    container(layout)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(ui::styles::app_style())
        .into()
}
```

- [ ] **Step 5: Build and run to visually confirm sidebar + placeholder steps appear**

```bash
cargo build 2>&1 | tail -20
cargo run
```

Expected: app launches showing the sidebar on the left with nav items, and four placeholder step cards stacked in the main area. Tabs are gone. Old panels are gone.

- [ ] **Step 6: Commit**

```bash
git add src/ui/widgets/steps.rs src/ui/widgets/mod.rs src/ui/mod.rs src/main.rs
git commit -m "feat: wire sidebar + step canvas into view(), replace tab layout"
```

---

## Task 7: FILES step content

**Files:**
- Modify: `src/ui/widgets/steps.rs` — replace `files_step` stub

- [ ] **Step 1: Replace the `files_step` function in `steps.rs`**

Replace the stub `files_step` with:

```rust
pub fn files_step<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    use super::components::{drop_zone, layer_row};

    let has_gerbers = !state.copper_paths.is_empty()
        || !state.outline_paths.is_empty()
        || !state.drill_paths.is_empty();
    let is_skipped = state.loaded_png_path.is_some() && !has_gerbers;
    let has_input = has_gerbers || state.loaded_png_path.is_some();

    let vs = if has_input { CardVisualState::Complete } else { CardVisualState::Active };
    let is_expanded = state.expanded_step == Some(1);

    // Summary line for collapsed state
    let n_files = state.copper_paths.len() + state.outline_paths.len() + state.drill_paths.len();
    let summary_str: String = if is_skipped {
        let name = state.loaded_png_path.as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        format!("PNG: {name}")
    } else if has_gerbers {
        let mut parts = Vec::new();
        if !state.copper_paths.is_empty() { parts.push("Cu"); }
        if !state.outline_paths.is_empty() { parts.push("Out"); }
        if !state.drill_paths.is_empty() { parts.push("Drl"); }
        format!("{n_files} file(s) · {}", parts.join(", "))
    } else {
        "No files loaded".to_owned()
    };

    // Layer rows
    let mut file_rows: Vec<Element<'_, crate::Message>> = Vec::new();
    for (i, path) in state.copper_paths.iter().enumerate() {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        file_rows.push(layer_row("Cu", palette::layer_copper(), name,
            crate::Message::RemoveFile { layer: crate::LayerKind::Copper, index: i }));
    }
    for (i, path) in state.outline_paths.iter().enumerate() {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        file_rows.push(layer_row("Out", palette::layer_outline(), name,
            crate::Message::RemoveFile { layer: crate::LayerKind::Outline, index: i }));
    }
    for (i, path) in state.drill_paths.iter().enumerate() {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        file_rows.push(layer_row("Drl", palette::layer_drill(), name,
            crate::Message::RemoveFile { layer: crate::LayerKind::Drill, index: i }));
    }
    if is_skipped {
        let name = state.loaded_png_path.as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        file_rows.push(layer_row("PNG", palette::signal_green(), name,
            // No remove for the PNG row in this view — reset clears it
            crate::Message::Reset));
    }

    let files_col = iced::widget::Column::with_children(file_rows).spacing(6);

    // "or load PNG" separator
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
        drop_zone(crate::Message::SelectCopperFiles),
        files_col,
        or_row,
        load_png_btn,
    ]
    .spacing(12);

    step_shell(1, "FILES", vs, is_expanded, &summary_str, None, content.into())
}
```

- [ ] **Step 2: Verify compilation**

```bash
cargo check 2>&1 | head -40
```

- [ ] **Step 3: Run and verify FILES step shows drop zone and file rows**

```bash
cargo run
```

- [ ] **Step 4: Commit**

```bash
git add src/ui/widgets/steps.rs
git commit -m "feat: implement FILES step card with drop zone and layer rows"
```

---

## Task 8: SETTINGS step content

**Files:**
- Modify: `src/ui/widgets/steps.rs` — replace `settings_step` stub

- [ ] **Step 1: Replace the `settings_step` function in `steps.rs`**

```rust
pub fn settings_step<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    use super::components::{accordion, setting_field};

    let is_expanded = state.expanded_step == Some(2);
    let summary = format!(
        "{}dpi · {}mm cut · {} mm/min · Ø{}mm",
        state.dpi_input, state.cut_z_mm_input, state.feed_rate_input, state.tool_diameter_mm_input
    );

    // Geometry group (index 0)
    let geometry_content = column![
        setting_field("Resolution (DPI)", &state.dpi_input, crate::Message::DpiChanged),
    ]
    .spacing(12);

    // Depths group (index 1)
    let depths_content = row![
        setting_field("Cut Z (mm)", &state.cut_z_mm_input, crate::Message::CutZChanged),
        setting_field("Safe Z (mm)", &state.safe_z_mm_input, crate::Message::SafeZChanged),
    ]
    .spacing(20);

    // Motion group (index 2)
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

    // Tooling group (index 3)
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
        accordion("GEOMETRY", "", geo_open,
            crate::Message::SettingsGroupToggled(0), geometry_content.into()),
        accordion("DEPTHS", "", dep_open,
            crate::Message::SettingsGroupToggled(1), depths_content.into()),
        accordion("MOTION", &motion_summary, mot_open,
            crate::Message::SettingsGroupToggled(2), motion_content.into()),
        accordion("TOOLING", &tooling_summary, tool_open,
            crate::Message::SettingsGroupToggled(3), tooling_content.into()),
    ]
    .spacing(4);

    step_shell(2, "SETTINGS", CardVisualState::Complete, is_expanded, &summary, None, content.into())
}
```

- [ ] **Step 2: Verify compilation**

```bash
cargo check 2>&1 | head -40
```

- [ ] **Step 3: Run and verify accordions expand/collapse**

```bash
cargo run
```

- [ ] **Step 4: Commit**

```bash
git add src/ui/widgets/steps.rs
git commit -m "feat: implement SETTINGS step card with collapsible accordion groups"
```

---

## Task 9: RASTERIZE step content

**Files:**
- Modify: `src/ui/widgets/steps.rs` — replace `rasterize_step` stub

- [ ] **Step 1: Replace the `rasterize_step` function in `steps.rs`**

```rust
pub fn rasterize_step<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    use easymill::conversion::PngRenderResult;
    use iced::widget::{image, progress_bar};

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

    let summary = if is_skipped {
        "Skipped — PNG loaded directly"
    } else {
        match state.gerber_to_png {
            StepState::Complete => "3 layers rendered",
            _ => "Not yet run",
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
            progress_bar(0.0..=1.0, progress)
                .style(move |_| iced::widget::progress_bar::Style {
                    background: palette::surface_inset().into(),
                    bar: bar_color.into(),
                    border: iced::Border::default(),
                })
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

    // Use overall progress for all three until per-layer progress is tracked separately
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

    // Stale warning row
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
```

- [ ] **Step 2: Verify compilation**

```bash
cargo check 2>&1 | head -40
```

- [ ] **Step 3: Run and check rasterize step**

```bash
cargo run
```

- [ ] **Step 4: Commit**

```bash
git add src/ui/widgets/steps.rs
git commit -m "feat: implement RASTERIZE step with per-layer progress, thumbnails, and stale warning"
```

---

## Task 10: G-CODE step + remove old widgets

**Files:**
- Modify: `src/ui/widgets/steps.rs` — replace `gcode_step` stub
- Modify: `src/ui/widgets/mod.rs` — remove old widget exports, remove old widget functions
- Modify: `src/ui/mod.rs` — clean up exports
- Modify: `src/main.rs` — remove `Tab` enum and `active_tab` field, remove `Message::TabChanged` handler

- [ ] **Step 1: Replace the `gcode_step` function in `steps.rs`**

```rust
pub fn gcode_step<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    use iced::widget::progress_bar;

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
        })
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

    step_shell(4, "G-CODE", vs, is_expanded, &summary, Some(run_btn), content.into())
}
```

- [ ] **Step 2: Remove `Tab` enum and `active_tab` from `src/main.rs`**

Delete the `Tab` enum definition (lines 19–24).

In `AppState`, remove the field:
```rust
pub(crate) active_tab: Tab,
```

In `impl Default for AppState`, remove:
```rust
active_tab: Tab::default(),
```

In `update()`, remove the `Message::TabChanged(tab)` arm entirely.

In `Message`, remove:
```rust
TabChanged(Tab),
```

- [ ] **Step 3: Clean up `src/ui/widgets/mod.rs`**

Delete the following functions entirely from `mod.rs` (they are replaced by the new widgets):
- `tab_bar`
- `header`
- `source_panel`
- `config_panel`
- `pipeline_panel`
- `generated_output`
- `save_png_buttons`
- `save_gcode_button`
- `step_card`
- `progress_chip`
- `status_line`
- `setting_field` (now in `components.rs`)

Keep only: `pill_color`, `pill_bg` (still used by `step_shell` badge styling).

Remove the `pub use` re-exports for those deleted functions.

Update the `pub use` block to:
```rust
pub use sidebar::sidebar;
pub use steps::step_canvas;
pub use components::setting_field;
```

- [ ] **Step 4: Update `src/ui/mod.rs`**

The file should now be:
```rust
pub mod palette;
pub mod styles;
pub mod widgets;

pub use widgets::{sidebar, step_canvas};
```

- [ ] **Step 5: Full build**

```bash
cargo build 2>&1 | tail -30
```

Expected: clean build, zero errors.

- [ ] **Step 6: Run end-to-end and test the full workflow**

```bash
cargo run
```

Manual checks:
1. Load copper `.gtl` files via the FILES step drop zone — sidebar file list updates, FILES badge turns green
2. Click SETTINGS in sidebar — SETTINGS step expands, accordion groups work, changing DPI shows RASTERIZE as stale in sidebar
3. Click RASTERIZE — step expands, click Run — progress bars animate, PNG thumbnails appear on completion, save buttons appear
4. Click G-CODE — step expands, click Run — progress animates, stats card and save button appear on completion
5. Click "▶ Run All" in sidebar — chains both stages correctly
6. Change a setting after G-code completes — G-CODE badge shows ⚠ in sidebar, stale warning appears in card

- [ ] **Step 7: Commit**

```bash
git add src/ui/widgets/steps.rs src/ui/widgets/mod.rs src/ui/mod.rs src/main.rs
git commit -m "feat: complete UI redesign — G-code step, remove old tab widgets and Tab enum"
```

---

## Implementation Notes for the Executing Agent

**PNG row remove button (Task 7):** The `layer_row` helper always takes a `remove_msg`. The loaded-PNG row in `files_step` passes `Message::Reset` as a stub — replace it with a dedicated `Message::ClearPng` message (add to enum + handler: sets `loaded_png_path = None`, `gerber_to_png = StepState::Idle`) or simply don't show a remove button for the PNG row by inlining it as a plain `row![...]` without a button.

**Per-layer progress (Task 9):** The three progress bars in `rasterize_step` use `state.gerber_to_png_progress` (a single value) with staggered offsets as a visual approximation. If true per-layer tracking is desired later, add `copper_progress: f32`, `outline_progress: f32`, `drill_progress: f32` to `AppState` and update them from the `ProgressFn` callback using layer indices.

**File drag-and-drop (Task 7):** The drop zone is currently a click-to-browse button. True OS drag-and-drop requires subscribing to `iced::event::listen()` and handling `Event::Window(iced::window::Event::FileDropped(path))` in `subscription()`. This is a natural follow-up task — the drop zone visual is already in place.

**Import structure in steps.rs:** The full `use` block at the top of `steps.rs` should include:
```rust
use easymill::conversion::PngRenderResult;
use iced::widget::{button, column, container, image, progress_bar, row, text};
```
The `use image` and `use progress_bar` shown inside function bodies in this plan are illustrative — move them to the file-level `use` block to avoid re-declarations.
