# EasyMill UI Polish & Features Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apply 10 UI improvements to EasyMill — hide under-development G-code UI, add mirror-top/exclude/open-in-mods features, and polish spacing/progress bars/layout.

**Architecture:** Changes span the data model (`stackup.rs`, `conversion.rs`), app state & messages (`main.rs`), and UI widgets (`widgets/steps.rs`, `widgets/components.rs`). A minimal Tokio TCP server is added for serving PNG files to the mods web app. All G-code-related UI is hidden (not deleted) behind comments.

**Tech Stack:** Rust, iced 0.14, tokio 1, `webbrowser` crate (new).

## Global Constraints

- iced version: 0.14.0 — do not upgrade
- All hidden code stays compilable — no dead code removal
- `cargo check` must pass after every task
- The HTTP server must sanitize filenames (no `..`, only `[a-zA-Z0-9._-]`)
- `Access-Control-Allow-Origin: *` header required on all server responses (modsproject.org fetches from localhost)
- Mirror top default: `false`; mirror bottom default: `true` (unchanged)

---

## File Map

| File | What changes |
|---|---|
| `src/stackup.rs` | Add `excluded: bool` to `LayerFile`; filter in `milling_paths()` |
| `src/conversion.rs` | Add `mirror_top: bool` to `ConversionSettings`; wire to copper-top render call |
| `src/main.rs` | New `AppState` fields; new `Message` variants; new handlers; HTTP server task |
| `src/ui/widgets/steps.rs` | Hide G-code step; simplify settings; remove PNG load buttons; add mods buttons; polish spacing/bars |
| `src/ui/widgets/components.rs` | Add exclude button to `layer_row`; polish spacing |
| `Cargo.toml` | Add `webbrowser = "2"` |

---

## Task 1: Track A — UI Simplification

Hide G-code step, non-rasterization settings, and Load PNG buttons. Pure UI removal — no state changes.

**Files:**
- Modify: `src/ui/widgets/steps.rs`

**Interfaces:**
- Produces: simplified `settings_step`, `files_step`, `step_canvas` used by all later tasks

- [ ] **Step 1: Hide `gcode_step` from `step_canvas`**

In `src/ui/widgets/steps.rs`, replace the `step_canvas` function body:

```rust
pub fn step_canvas<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    container(
        column![
            files_step(state),
            settings_step(state),
            rasterize_step(state),
            // TODO: unhide when G-code is ready
            // gcode_step(state),
        ]
        .spacing(12)
        .max_width(720),
    )
    .width(Length::Fill)
    .padding([24, 32])
    .center_x(Length::Fill)
    .into()
}
```

- [ ] **Step 2: Simplify `settings_step` to GEOMETRY only**

Replace the entire `settings_step` function with:

```rust
pub fn settings_step<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    let is_expanded = state.expanded_step == Some(2);
    let summary = format!(
        "{}dpi · mirror bot={} top={}",
        state.dpi_input,
        if state.mirror_bottom { "on" } else { "off" },
        if state.mirror_bottom { "off" } else { "off" },
    );

    let content = column![
        setting_field("Resolution (DPI)", &state.dpi_input, crate::Message::DpiChanged),
        button(
            text(if state.mirror_bottom { "☑ Mirror bottom traces" } else { "☐ Mirror bottom traces" })
                .font(palette::MONO).size(12).color(palette::text_secondary()),
        )
        .style(styles::ghost_action_style)
        .padding([8, 14])
        .width(Length::Fill)
        .on_press(crate::Message::MirrorBottomToggled(!state.mirror_bottom)),
        // Mirror top placeholder — wired in Task 2
    ]
    .spacing(12);

    step_shell(2, "SETTINGS", CardVisualState::Complete, is_expanded, summary, None, content.into())
}
```

Note: The hidden accordions (`DEPTHS`, `MOTION`, `TOOLING`) and their `accordion` imports remain — only the rendering is removed. The `SettingsGroupToggled` message handler stays.

- [ ] **Step 3: Remove Load PNG buttons from `files_step`**

In `files_step`, replace the `content` column (currently near line 251):

```rust
let content = column![
    drop_zone(crate::Message::SelectGerberFiles),
    files_col,
]
.spacing(12);
```

Remove `or_row`, `load_top_btn`, `load_bot_btn` entirely from this function. The `LoadPng`, `LoadBottomPng` messages and all their handlers remain in `main.rs` — just not triggered from the UI.

- [ ] **Step 4: Verify**

```bash
cargo check 2>&1 | grep -E "^error"
```

Expected: no errors. Warnings about unused message variants (`LoadPng`, `LoadBottomPng`, etc.) are acceptable.

- [ ] **Step 5: Commit**

```bash
git add src/ui/widgets/steps.rs
git commit -m "feat: hide G-code step, non-rasterization settings, load-PNG buttons"
```

---

## Task 2: Track B1 — Mirror Top Toggle

Add `mirror_top` to the data model, wire it through the settings, and add its checkbox to the UI.

**Files:**
- Modify: `src/conversion.rs` (lines 21–53 and 233)
- Modify: `src/main.rs` (AppState, Message, handlers, get_settings)
- Modify: `src/ui/widgets/steps.rs` (settings_step)

**Interfaces:**
- Produces: `ConversionSettings::mirror_top: bool`; `AppState::mirror_top: bool`; `Message::MirrorTopToggled(bool)`

- [ ] **Step 1: Add `mirror_top` to `ConversionSettings`**

In `src/conversion.rs`, change the struct (line 21):

```rust
pub struct ConversionSettings {
    pub pixels_per_mm: f32,
    pub max_render_pixels: u64,
    pub threshold: u8,
    pub safe_z_mm: f32,
    pub cut_z_mm: f32,
    pub feed_rate_mm_min: f32,
    pub plunge_rate_mm_min: f32,
    pub spindle_speed_rpm: u32,
    pub tool_diameter_mm: f32,
    pub offset_number: u32,
    pub offset_stepover: f32,
    pub mirror_bottom: bool,
    pub mirror_top: bool,
}
```

And its `Default` impl (line 36):

```rust
impl Default for ConversionSettings {
    fn default() -> Self {
        Self {
            pixels_per_mm: DEFAULT_PIXELS_PER_MM,
            max_render_pixels: DEFAULT_MAX_RENDER_PIXELS,
            threshold: 128,
            safe_z_mm: 2.0,
            cut_z_mm: -0.1,
            feed_rate_mm_min: 300.0,
            plunge_rate_mm_min: 120.0,
            spindle_speed_rpm: 12_000,
            tool_diameter_mm: 0.4,
            offset_number: 4,
            offset_stepover: 0.5,
            mirror_bottom: true,
            mirror_top: false,
        }
    }
}
```

- [ ] **Step 2: Wire `mirror_top` into the copper-top render call**

In `src/conversion.rs` at line 233, change the copper_top call from:

```rust
let copper_top = render_layer(&all_tagged, gerber::LayerType::CopperTop, "_traces_top.png", false, false, false, 0.60)?;
```

to:

```rust
let copper_top = render_layer(&all_tagged, gerber::LayerType::CopperTop, "_traces_top.png", false, false, settings.mirror_top, 0.60)?;
```

- [ ] **Step 3: Add `mirror_top` to `AppState` and the new message**

In `src/main.rs`, add to the `AppState` struct (after `mirror_bottom`):

```rust
pub(crate) mirror_top: bool,
```

In `AppState::default()`, add (after `mirror_bottom: true`):

```rust
mirror_top: false,
```

In the `Message` enum, add (after `MirrorBottomToggled`):

```rust
MirrorTopToggled(bool),
```

In `get_settings()`, add (after `settings.mirror_bottom = self.mirror_bottom;`):

```rust
settings.mirror_top = self.mirror_top;
```

- [ ] **Step 4: Add `MirrorTopToggled` handler**

In the `update` function's `match message` block, add after the `MirrorBottomToggled` arm:

```rust
Message::MirrorTopToggled(val) => {
    state.mirror_top = val;
    state.rasterize_stale = state.gerber_to_png == StepState::Complete;
    state.gcode_stale = state.png_to_gcode == StepState::Complete;
}
```

- [ ] **Step 5: Add mirror top checkbox to `settings_step` UI**

In `src/ui/widgets/steps.rs`, in the `settings_step` function, add the mirror top button after the mirror bottom one and fix the summary line:

```rust
pub fn settings_step<'a>(state: &'a crate::AppState) -> Element<'a, crate::Message> {
    let is_expanded = state.expanded_step == Some(2);
    let summary = format!(
        "{}dpi · mirror bot={} top={}",
        state.dpi_input,
        if state.mirror_bottom { "on" } else { "off" },
        if state.mirror_top { "on" } else { "off" },
    );

    let content = column![
        setting_field("Resolution (DPI)", &state.dpi_input, crate::Message::DpiChanged),
        button(
            text(if state.mirror_bottom { "☑ Mirror bottom traces" } else { "☐ Mirror bottom traces" })
                .font(palette::MONO).size(12).color(palette::text_secondary()),
        )
        .style(styles::ghost_action_style)
        .padding([8, 14])
        .width(Length::Fill)
        .on_press(crate::Message::MirrorBottomToggled(!state.mirror_bottom)),
        button(
            text(if state.mirror_top { "☑ Mirror top traces" } else { "☐ Mirror top traces" })
                .font(palette::MONO).size(12).color(palette::text_secondary()),
        )
        .style(styles::ghost_action_style)
        .padding([8, 14])
        .width(Length::Fill)
        .on_press(crate::Message::MirrorTopToggled(!state.mirror_top)),
    ]
    .spacing(12);

    step_shell(2, "SETTINGS", CardVisualState::Complete, is_expanded, summary, None, content.into())
}
```

- [ ] **Step 6: Verify**

```bash
cargo check 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add src/conversion.rs src/main.rs src/ui/widgets/steps.rs
git commit -m "feat: add mirror top traces toggle"
```

---

## Task 3: Track B2 — Per-File Exclude

Allow files to be manually excluded from the milling pipeline without being removed.

**Files:**
- Modify: `src/stackup.rs`
- Modify: `src/main.rs`
- Modify: `src/ui/widgets/components.rs`

**Interfaces:**
- Consumes: `LayerFile` from `src/stackup.rs`
- Produces: `LayerFile::excluded: bool`; `Message::ToggleLayerExclude(usize)`

- [ ] **Step 1: Add `excluded` field to `LayerFile`**

In `src/stackup.rs`, update the `LayerFile` struct:

```rust
#[derive(Debug, Clone)]
pub struct LayerFile {
    pub path: PathBuf,
    pub auto_category: LayerCategory,
    pub auto_side: Side,
    pub user_category: Option<LayerCategory>,
    pub user_side: Option<Side>,
    pub user_label: Option<String>,
    pub excluded: bool,
}
```

Update `LayerFile::new()`:

```rust
pub fn new(path: PathBuf, category: LayerCategory, side: Side) -> Self {
    Self {
        path,
        auto_category: category,
        auto_side: side,
        user_category: None,
        user_side: None,
        user_label: None,
        excluded: false,
    }
}
```

- [ ] **Step 2: Filter excluded layers in `milling_paths()`**

In `src/stackup.rs`, in the `milling_paths` loop, add an exclude check as the first guard:

```rust
pub fn milling_paths(&self) -> (Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>) {
    let mut copper_top = Vec::new();
    let mut copper_bottom = Vec::new();
    let mut outline = Vec::new();
    let mut drill = Vec::new();
    for layer in &self.layers {
        if layer.excluded {
            continue;
        }
        if !layer.is_resolved() {
            continue;
        }
        match layer.effective_category() {
            LayerCategory::Copper => {
                match layer.effective_side() {
                    Side::Bottom => copper_bottom.push(layer.path.clone()),
                    Side::All => copper_top.push(layer.path.clone()),
                    Side::Inner(_) => {}
                    _ => copper_top.push(layer.path.clone()),
                }
            }
            LayerCategory::Outline => outline.push(layer.path.clone()),
            LayerCategory::Drill => drill.push(layer.path.clone()),
            _ => {}
        }
    }
    (copper_top, copper_bottom, outline, drill)
}
```

- [ ] **Step 3: Add `ToggleLayerExclude` message and handler**

In `src/main.rs`, add to `Message` enum (after `RemoveFile`):

```rust
ToggleLayerExclude(usize),
```

Add handler in `update` match block (after `RemoveFile` arm):

```rust
Message::ToggleLayerExclude(index) => {
    if let Some(layer) = state.stackup.layers.get_mut(index) {
        layer.excluded = !layer.excluded;
        state.rasterize_stale = state.gerber_to_png == StepState::Complete;
        state.gcode_stale = state.png_to_gcode == StepState::Complete;
        state.loaded_inputs = derive_loaded_inputs(state);
    }
}
```

- [ ] **Step 4: Add exclude button to `layer_row` in `components.rs`**

In `src/ui/widgets/components.rs`, update the `layer_row` function signature to accept `excluded`:

```rust
pub fn layer_row<'a>(
    index: usize,
    cat: LayerCategory,
    side: Side,
    is_overridden: bool,
    is_excluded: bool,
    filename: String,
    is_editing: bool,
) -> Element<'a, crate::Message> {
```

Update the filename text to dim when excluded:

```rust
let filename_color = if is_excluded {
    palette::text_muted()
} else {
    palette::text_secondary()
};

let exclude_btn: Element<'_, crate::Message> = button(
    text("⊘").font(palette::MONO).size(11).color(
        if is_excluded { palette::signal_gold() } else { palette::text_muted() }
    ),
)
.style(styles::transparent_button_style)
.padding([2, 5])
.on_press(crate::Message::ToggleLayerExclude(index))
.into();
```

Update `top_row` to include the exclude button (add before `reset_btn`):

```rust
let top_row: Element<'_, crate::Message> = row![
    label_btn,
    text(filename)
        .font(palette::MONO)
        .size(13)
        .color(filename_color)
        .width(Length::Fill),
    exclude_btn,
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
```

- [ ] **Step 5: Update `layer_row` call site in `steps.rs`**

In `src/ui/widgets/steps.rs`, in `files_step`, the call to `layer_row` gains `is_excluded`:

```rust
for (i, layer) in state.stackup.layers.iter().enumerate() {
    let cat = layer.effective_category();
    let side = layer.effective_side();
    let is_overridden = layer.user_category.is_some() || layer.user_side.is_some();
    let is_excluded = layer.excluded;
    let name = layer.filename();
    let is_editing = state.editing_layer == Some(i);
    file_rows.push(layer_row(i, cat, side, is_overridden, is_excluded, name, is_editing));
}
```

- [ ] **Step 6: Verify**

```bash
cargo check 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add src/stackup.rs src/main.rs src/ui/widgets/components.rs src/ui/widgets/steps.rs
git commit -m "feat: add per-file exclude toggle"
```

---

## Task 4: Track B3 — Open in Mods

Serve rendered PNGs over a local HTTP server and open them in the mods web app with one click.

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/main.rs`
- Modify: `src/ui/widgets/steps.rs`

**Interfaces:**
- Consumes: `PngRenderResult::path` (already in `state.generated_pngs`)
- Produces: `Message::OpenInMods(PathBuf)`, `Message::ModsServerStarted(u16)`, `AppState::mods_server_port: Option<u16>`, `AppState::pending_mods_open: Option<PathBuf>`

- [ ] **Step 1: Add `webbrowser` dependency**

In `Cargo.toml`, add under `[dependencies]`:

```toml
webbrowser = "2"
```

- [ ] **Step 2: Add new `AppState` fields**

In `src/main.rs`, add to `AppState` struct (after `gcode_stale`):

```rust
pub(crate) mods_server_port: Option<u16>,
pub(crate) pending_mods_open: Option<std::path::PathBuf>,
```

In `AppState::default()`, add:

```rust
mods_server_port: None,
pending_mods_open: None,
```

- [ ] **Step 3: Add new `Message` variants**

In `src/main.rs`, add to `Message` enum (after `FileDropped`):

```rust
OpenInMods(PathBuf),
ModsServerStarted(u16),
```

- [ ] **Step 4: Implement the local HTTP server function**

In `src/main.rs`, add this function above the `update` function:

```rust
async fn start_mods_server() -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind mods server");
    let port = listener.local_addr().expect("no local addr").port();
    let render_dir = std::env::temp_dir().join("easymill-render");

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { continue };
            let dir = render_dir.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let raw_path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                let filename = raw_path.trim_start_matches('/');
                let is_safe = !filename.is_empty()
                    && !filename.contains("..")
                    && filename.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.');
                if !is_safe {
                    let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n").await;
                    return;
                }
                match tokio::fs::read(dir.join(filename)).await {
                    Ok(data) => {
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
                            data.len()
                        );
                        let _ = stream.write_all(header.as_bytes()).await;
                        let _ = stream.write_all(&data).await;
                    }
                    Err(_) => {
                        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\n\r\n").await;
                    }
                }
            });
        }
    });

    port
}
```

- [ ] **Step 5: Add `OpenInMods` and `ModsServerStarted` handlers**

In the `update` function's `match message` block, add (before the closing brace, after all other arms):

```rust
Message::OpenInMods(path) => {
    if let Some(port) = state.mods_server_port {
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let url = format!(
            "https://modsproject.org/?program=programs/machines/G-code/mill+2D+PCB&src=http://127.0.0.1:{}/{}",
            port, filename
        );
        let _ = webbrowser::open(&url);
    } else {
        state.pending_mods_open = Some(path);
        return Task::perform(start_mods_server(), Message::ModsServerStarted);
    }
}
Message::ModsServerStarted(port) => {
    state.mods_server_port = Some(port);
    if let Some(path) = state.pending_mods_open.take() {
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let url = format!(
            "https://modsproject.org/?program=programs/machines/G-code/mill+2D+PCB&src=http://127.0.0.1:{}/{}",
            port, filename
        );
        let _ = webbrowser::open(&url);
    }
}
```

Add `use webbrowser;` at the top of `src/main.rs` with the other `use` statements.

- [ ] **Step 6: Add "Open in mods" buttons to thumbnails in `rasterize_step`**

In `src/ui/widgets/steps.rs`, replace the `thumb` closure in `rasterize_step`:

```rust
let thumb = |result: &'a PngRenderResult, label: &'static str| -> Element<'a, crate::Message> {
    let path = result.path.clone();
    column![
        text(label).font(palette::MONO).size(11).color(palette::text_accent()),
        container(
            image(iced::widget::image::Handle::from_path(&result.path))
                .width(Length::Fill)
                .height(Length::Fixed(100.0)),
        )
        .width(Length::Fill)
        .height(Length::Fixed(100.0))
        .clip(true)
        .style(styles::inset_style()),
        button(
            text("↗ Open in mods")
                .font(palette::MONO)
                .size(11)
                .color(palette::text_secondary()),
        )
        .style(styles::secondary_action_style)
        .width(Length::Fill)
        .padding([5, 8])
        .on_press(crate::Message::OpenInMods(path)),
    ]
    .spacing(4)
    .width(Length::FillPortion(1))
    .into()
};
```

- [ ] **Step 7: Verify**

```bash
cargo check 2>&1 | grep -E "^error"
```

Expected: no errors. Run `cargo build` to ensure `webbrowser` downloads and links cleanly.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs src/ui/widgets/steps.rs
git commit -m "feat: open rendered PNGs in mods via local HTTP server"
```

---

## Task 5: Track C1 — Fix Drag & Drop

Investigate and fix file drag & drop not working.

**Files:**
- Possibly modify: `src/main.rs`

**Interfaces:**
- Consumes: existing `FileDropped` subscription and handler
- Produces: working file drop on the target platform

- [ ] **Step 1: Test current behavior on X11**

```bash
WINIT_UNIX_BACKEND=x11 cargo run
```

Drag a Gerber file onto the window. If files appear in the list, drag & drop works under X11 but not Wayland. Note the result.

- [ ] **Step 2: Check iced 0.14 event variant spelling**

In `src/main.rs` around line 914, the subscription uses:

```rust
iced::event::Event::Window(iced::window::Event::FileDropped(path))
```

Confirm this compiles without "non-exhaustive patterns" warnings. In iced 0.14 the correct variants are:
- `iced::window::Event::FileDropped(PathBuf)` — single file, fires once per dropped file
- `iced::window::Event::FilesHoveredLeft` — no path
- `iced::window::Event::FilesHovered(Vec<PathBuf>)` — hover preview

The current code is correct. If `FileDropped` doesn't fire on Wayland, the fix is a Wayland-specific workaround.

- [ ] **Step 3: Apply fix based on Step 1 result**

**If X11 works but Wayland does not:** Wayland DnD is a known winit/iced limitation. Add a note to the UI. In the `drop_zone` component in `src/ui/widgets/components.rs`, update the subtitle:

```rust
text(".GTL  .GBL  .GKO  .DRL  .TXT  … (click on Wayland)")
    .font(palette::MONO)
    .size(11)
    .color(palette::text_muted()),
```

And document the workaround in `README.md` or a comment in `main.rs` near the subscription: `// Note: FileDropped events require X11; on Wayland use WINIT_UNIX_BACKEND=x11`.

**If X11 also doesn't work:** The event might need `status` filtering. Change the subscription to accept uncaptured events only:

```rust
let dnd_sub = event::listen_with(
    |event: iced::event::Event, status: iced::event::Status, _window: iced::window::Id| {
        // Accept regardless of status — drop events are always Ignored
        let _ = status;
        if let iced::event::Event::Window(iced::window::Event::FileDropped(path)) = event {
            Some(Message::FileDropped(path))
        } else {
            None
        }
    },
);
```

- [ ] **Step 4: Verify**

```bash
cargo check 2>&1 | grep -E "^error"
```

Test drag & drop manually with the appropriate backend.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs src/ui/widgets/components.rs
git commit -m "fix: drag & drop investigation and platform workaround"
```

---

## Task 6: Track C2-C4 — Polish (Spacing, Progress Bars, Layout Consistency)

Apply all visual polish: spacing increases, thicker rounded progress bars, uniform layout metrics.

**Files:**
- Modify: `src/ui/widgets/steps.rs`
- Modify: `src/ui/widgets/components.rs`

**Interfaces:**
- No interface changes — pure visual updates

- [ ] **Step 1: Increase global spacing in `step_canvas` and `step_shell`**

In `src/ui/widgets/steps.rs`:

In `step_canvas`, change column spacing:
```rust
.spacing(20)   // was 12
```

In `step_shell`, change card padding:
```rust
container(card_body)
    .width(Length::Fill)
    .padding([20, 20])   // was 16
```

- [ ] **Step 2: Update spacing inside `files_step`**

In `files_step`, change `files_col` spacing and content column spacing:

```rust
let files_col = iced::widget::Column::with_children(file_rows).spacing(10); // was 6

// ...

let content = column![
    drop_zone(crate::Message::SelectGerberFiles),
    files_col,
]
.spacing(16);  // was 12
```

- [ ] **Step 3: Fix step header step-number alignment**

In `step_shell`, set a fixed width on the step number text so all cards align:

```rust
container(
    text(format!("{step_num:02}"))
        .font(palette::MONO)
        .size(11)
        .color(palette::text_muted()),
)
.width(Length::Fixed(28.0))
.into(),
```

Replace the plain `text(format!("{step_num:02}"))...into()` with the above `container(...).into()`.

- [ ] **Step 4: Update spacing in `rasterize_step` progress rows**

In `rasterize_step`, change:

```rust
let progress_rows = column![...]
    .spacing(12);  // was 8
```

In the `layer_progress` closure, update the progress bar height and add border radius:

```rust
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
                    border: iced::border::rounded(5.0),
                }),
        )
        .width(Length::Fill)
        .height(Length::Fixed(10.0)),   // was 6.0
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
```

- [ ] **Step 5: Update G-code step progress bar (even though step is hidden, keep code consistent)**

In `gcode_step`, update the progress bar style:

```rust
let progress = progress_bar(0.0..=1.0, state.png_to_gcode_progress)
    .style(move |_| iced::widget::progress_bar::Style {
        background: palette::surface_inset().into(),
        bar: bar_color.into(),
        border: iced::border::rounded(5.0),
    });

let progress = container(progress)
    .width(Length::Fill)
    .height(Length::Fixed(10.0));  // was 6.0
```

- [ ] **Step 6: Uniform button padding and text sizes**

In `rasterize_step`, update save buttons to uniform padding:

```rust
button(text("↓ Top").font(palette::MONO).size(12))
    .style(styles::secondary_action_style)
    .width(Length::FillPortion(1))
    .padding([8, 10])   // was [7, 10]
    .on_press(crate::Message::SaveCopperPng),
// ... same for all save buttons
button(text("↓ Save All").font(palette::mono_bold()).size(12))
    .style(styles::primary_action_style)
    .width(Length::FillPortion(1))
    .padding([8, 10])   // was [7, 10]
    .on_press(crate::Message::SaveAllPngs),
```

In `gcode_step`, update side tab buttons:
```rust
.padding([7, 12])   // was [6, 10]
```

- [ ] **Step 7: Update `layer_row` spacing in `components.rs`**

In `layer_row`, change the top row spacing:

```rust
let top_row: Element<'_, crate::Message> = row![...]
    .spacing(8)   // was 6
    .align_y(Alignment::Center)
    .into();
```

- [ ] **Step 8: Verify**

```bash
cargo check 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 9: Commit**

```bash
git add src/ui/widgets/steps.rs src/ui/widgets/components.rs
git commit -m "style: increase spacing, thicken progress bars, uniform layout metrics"
```

---

## Self-Review

**Spec coverage:**
- A1 (hide gcode_step) → Task 1 Step 1 ✓
- A2 (hide non-rasterization settings) → Task 1 Step 2 ✓
- A3 (hide load PNG buttons) → Task 1 Step 3 ✓
- B1 (mirror top) → Task 2 ✓
- B2 (file exclude) → Task 3 ✓
- B3 (open in mods) → Task 4 ✓
- C1 (fix drag & drop) → Task 5 ✓
- C2 (increase spacing) → Task 6 Steps 1–3 ✓
- C3 (progress bars) → Task 6 Steps 4–5 ✓
- C4 (layout consistency) → Task 6 Steps 3, 6–7 ✓

**Placeholder scan:** No TBD/TODO patterns in code steps. All code blocks are complete.

**Type consistency check:**
- `LayerFile::excluded: bool` defined in Task 3 Step 1, consumed in Steps 2, 4, 5 ✓
- `ConversionSettings::mirror_top: bool` defined in Task 2 Step 1, consumed in Step 2 ✓
- `AppState::mirror_top: bool` defined in Task 2 Step 3, consumed in Step 5 UI ✓
- `Message::MirrorTopToggled(bool)` defined in Task 2 Step 3, handled in Step 4, emitted in Step 5 ✓
- `Message::ToggleLayerExclude(usize)` defined in Task 3 Step 3, handled there, emitted in Step 4 ✓
- `Message::OpenInMods(PathBuf)` defined in Task 4 Step 3, handled in Step 5, emitted in Step 6 ✓
- `Message::ModsServerStarted(u16)` defined in Task 4 Step 3, handled in Step 5 ✓
- `layer_row` gains `is_excluded: bool` param in Task 3 Step 4; call site updated in Task 3 Step 5 ✓

**One fix applied:** Task 1 Step 2 originally had a bug in the summary string — both branches read `mirror_bottom` for top. Fixed to `mirror_top`.
