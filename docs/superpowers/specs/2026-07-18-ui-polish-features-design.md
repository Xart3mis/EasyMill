# EasyMill UI Polish & Feature Additions — Design Spec
Date: 2026-07-18

## Overview

Ten targeted improvements to EasyMill's iced-based UI, grouped into three tracks:
- **Track A — Simplification**: Hide G-code UI elements (under development)
- **Track B — New Features**: Mirror-top toggle, per-file exclude, open-in-mods
- **Track C — Polish**: Fix drag & drop, increase spacing, improve progress bars, layout consistency

Files primarily affected: `src/main.rs`, `src/ui/widgets/steps.rs`, `src/ui/widgets/components.rs`, `src/stackup.rs`, `Cargo.toml`.

---

## Track A — Simplification

### A1. Hide G-Code Generation step (item 4)

Remove the `gcode_step(state)` call from the `step_canvas` column in `steps.rs`. The function, all state fields (`generated_top_gcode`, `generated_bottom_gcode`, `png_to_gcode`, etc.), and all message handlers remain intact — just not rendered.

Mark with `// TODO: unhide when G-code is ready` at the call site.

### A2. Hide non-rasterization settings (item 3)

In `settings_step`, remove the DEPTHS, MOTION, and TOOLING accordion groups. Keep only the GEOMETRY group (DPI + mirror toggles). The corresponding `AppState` fields and message handlers are preserved — just not exposed in UI.

The `settings_step` function becomes simpler: no accordion wrapper needed if only one group remains; render GEOMETRY content directly in the step body.

### A3. Hide "Load PNG" buttons (item 2)

Remove `or_row`, `load_top_btn`, and `load_bot_btn` from the `files_step` content column. The `LoadPng`, `LoadTopPngPicked`, `LoadBottomPng`, `LoadBottomPngPicked` messages and handlers stay.

---

## Track B — New Features

### B1. Mirror top traces toggle (item 5)

**State:** Add `mirror_top: bool` to `AppState`, default `false`.

**Message:** Add `MirrorTopToggled(bool)` variant. Handler: set `state.mirror_top`, mark `rasterize_stale` and `gcode_stale` if respective steps are complete (same pattern as `MirrorBottomToggled`).

**Settings:** Add `mirror_top` field to `ConversionSettings` in `conversion.rs` (check if it exists; add if not). Wire in `get_settings()`: `settings.mirror_top = self.mirror_top`.

**UI:** In the GEOMETRY settings section, add a second checkbox below the existing mirror-bottom toggle:
```
☑ Mirror bottom traces
☐ Mirror top traces
```
Both use the same `ghost_action_style` button pattern.

### B2. Manual file exclude (item 9)

**Data model:** Add `excluded: bool` to `LayerFile` in `stackup.rs`, default `false`. Update `LayerFile::new()` accordingly.

**Filtering:** In `Stackup::milling_paths()`, skip any layer where `layer.excluded` is true (add `if layer.excluded { continue; }` before the category match).

**Message:** Add `ToggleLayerExclude(usize)` variant. Handler: toggle `state.stackup.layers[index].excluded`, mark stale states, re-derive `loaded_inputs`.

**UI in `layer_row`:** Add a small "⊘" toggle button to the right of the filename (before the remove "✕" button). When `excluded`:
- Filename text color changes to `palette::text_muted()` (dimmed)
- The "⊘" button color changes to `palette::signal_gold()` to show active state

The layer type badge and reset/remove buttons remain functional when excluded. Excluded layers are never auto-removed — the user must explicitly remove them with "✕".

### B3. Open in mods (item 10)

**Dependency:** Add `webbrowser` crate to `Cargo.toml` for opening URLs in the system browser. Add a minimal HTTP server capability using Tokio (already a dependency) — no new async HTTP crate needed.

**Local HTTP server:**
- `AppState` gains: `mods_server_port: Option<u16>` (None until first use).
- On first "Open in mods" click, spawn a Tokio background task: a minimal raw TCP listener on `127.0.0.1:0`. It handles `GET /<filename>` requests by reading the file from the EasyMill temp render dir (`std::env::temp_dir().join("easymill-render")/`) and responding with `HTTP/1.1 200 OK\r\nContent-Type: image/png\r\n\r\n<bytes>`. Error responses use 404. The task runs indefinitely until the app exits.
- The bound port is sent back via a new `Message::ModsServerStarted(u16)` message and stored in `state.mods_server_port`.

**Message:** Add `OpenInMods(PathBuf)` variant. Handler logic:
1. If `mods_server_port` is `None`: spawn the server task, then open the URL once `ModsServerStarted` fires. To avoid a second click being needed, queue the path temporarily in a new `AppState` field `pending_mods_open: Option<PathBuf>`, and open the URL in the `ModsServerStarted` handler.
2. If `mods_server_port` is `Some(port)`: immediately open `https://modsproject.org/?program=programs/machines/G-code/mill+2D+PCB&src=http://127.0.0.1:{port}/{filename}` using `webbrowser::open(url)`.

**UI in `rasterize_step`:** The existing thumbnail builder (`thumb` closure) gains an "Open in mods ↗" button below each image. Only shown when `state.generated_pngs.is_some()`. Four thumbnails → four buttons, each fires `Message::OpenInMods(result.path.clone())`.

Button style: `secondary_action_style`, full width of its thumbnail column, label `"↗ Open in mods"`, size 11, mono font.

---

## Track C — Polish

### C1. Fix drag & drop (item 1)

**Investigation approach:** The subscription in `main.rs` uses `event::listen_with` with `iced::window::Event::FileDropped(path)`. Most likely failure mode on Linux is one of:
- (a) **Wayland**: iced's DnD support on Wayland may require the `xdg-shell` window protocol with explicit DnD handling. Mitigation: check if running under XWayland or Wayland native; may need `WINIT_UNIX_BACKEND=x11` workaround documented in-app, or iced window flags.
- (b) **Event consumption**: the `drop_zone` button widget may be consuming pointer events before the window-level handler sees them. Mitigation: confirm iced processes `FileDropped` at window level regardless of widget focus.
- (c) **API change**: verify `iced::window::Event::FileDropped(PathBuf)` is the correct variant for iced 0.14 (not `FilesHoveredLeft` / `FilesHovered`).

Fix will be targeted once root cause confirmed during implementation. The subscription architecture (window-level event, `Subscription::batch`) is otherwise correct.

### C2. Increase spacing (item 6)

| Location | Current | New |
|---|---|---|
| `step_canvas` column spacing | 12 | 20 |
| `step_shell` card padding | 16 | `[20, 20]` |
| `files_col` spacing | 6 | 10 |
| `layer_row` internal spacing | 6 | 8 |
| Progress rows spacing | 8 | 12 |
| Settings content spacing | 12 | 16 |

### C3. Progress bar improvements (item 7)

Both the per-layer progress rows in `rasterize_step` and the single bar in `gcode_step`:
- Height: `6.0` → `10.0` px
- Add `border: iced::border::rounded(5.0)` to the `progress_bar::Style` (both bar and background get rounded ends)
- The container height constraint also updates to `Length::Fixed(10.0)`

### C4. Layout consistency (item 8)

- **Button padding**: all action buttons use `[8, 14]` (currently a mix of `[7, 10]`, `[7, 12]`, `[10, 14]`)
- **Body text size**: uniform `size(13)` for content text, `size(12)` for labels/section headers, `size(11)` for meta/badges/chips
- **Thumbnail label text**: all thumbnail labels unified to `size(11)`
- **Step header**: step number column fixed width `Length::Fixed(28.0)` for consistent alignment across all step cards
- **Save buttons row**: all save buttons uniform `[8, 10]` padding and `size(12)` text

---

## Data Model Changes Summary

| File | Change |
|---|---|
| `src/stackup.rs` | Add `excluded: bool` to `LayerFile` |
| `src/main.rs` | Add `mirror_top`, `mods_server_port`, `pending_mods_open` to `AppState`; add 5 new `Message` variants |
| `src/conversion.rs` | Add/verify `mirror_top` in `ConversionSettings`; wire in `get_settings()` |
| `Cargo.toml` | Add `webbrowser` crate |

## Constraints & Non-Goals

- G-code pipeline code is **not removed** — hidden only. All handlers, state, and step functions stay.
- The local HTTP server is **read-only**, serves only from the fixed temp dir, and only responds to GET requests for filenames (no directory traversal).
- No new UI state persistence (settings already don't persist across launches).
- No changes to the rasterization or G-code conversion logic except `mirror_top` wiring.
