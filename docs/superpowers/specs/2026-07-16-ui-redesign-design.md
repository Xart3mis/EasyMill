# EasyMill UI Redesign — Design Spec
**Date:** 2026-07-16  
**Status:** Approved

---

## 1. Overview

Replace the current 3-tab layout (Source / Settings / Pipeline) with a hybrid design combining:

- **Concept 3** — Vertical onboarding-style step flow: four sequential step cards in the main canvas that collapse to summaries when complete and expand on click.
- **Concept 4** — Glassy left sidebar that serves as both a navigation menu and a file manager.

The result is a single-screen, scroll-to-focus layout. No tab switching. The entire job state is visible at once.

---

## 2. Layout

```
┌──────────────┬──────────────────────────────────────────────────┐
│  SIDEBAR     │  MAIN CANVAS                                     │
│  200px fixed │  scrollable, max-width 720px, centered in pane   │
│              │                                                  │
│  logo        │  step 1: FILES                                   │
│  nav+files   │  step 2: SETTINGS                                │
│  [Run All]   │  step 3: RASTERIZE                               │
│  status dot  │  step 4: G-CODE                                  │
└──────────────┴──────────────────────────────────────────────────┘
```

The sidebar is fixed. The main canvas scrolls independently. The window minimum width is ~860px.

---

## 3. Sidebar

### 3.1 Structure

```
EM  EasyMill
────────────────
① FILES         ✓
  Cu  board.gtl
  Out edge.ger
  Drl holes.drl

② SETTINGS      ✓
  1200dpi · -0.1mm

③ RASTERIZE     ⚠
  stale

④ G-CODE        ○
  waiting

────────────────
[▶ Run All]
────────────────
● Idle
```

### 3.2 Behaviour

- Each nav item has a **state badge**: `✓` complete, `⚠` stale/invalidated, `●` running (animated pulse), `○` waiting, `✗` error.
- Clicking a nav item scrolls the main canvas to that step card and expands it. If the step is complete (collapsed), clicking re-opens it for editing.
- The FILES nav item shows loaded layer files inline beneath it. Each file shows a type tag (`Cu` / `Out` / `Drl`) and filename. Clicking the filename removes it.
- The SETTINGS nav item shows the one-line token summary beneath it when settings are saved.
- `[▶ Run All]` chains Stage 1 (Rasterize) then Stage 2 (G-code) sequentially. While running it becomes `[■ Cancel]`.
- The status dot at the very bottom reflects global pipeline state: `● Idle` (muted) / `● Running` (accent, pulsing) / `● Error` (red).

---

## 4. Step Cards

### 4.1 Visual States

Every step card has exactly four states:

| State | Left border | Background | Behaviour |
|---|---|---|---|
| **Active** | 2px accent blue | `rgba(accent, 0.06)` | Expanded, interactive |
| **Complete** | 2px signal green | `rgba(green, 0.08)` | Collapsed to summary, click to re-edit |
| **Stale** | 2px signal gold | `rgba(gold, 0.08)` | Collapsed, shows warning, has re-run button |
| **Waiting** | none | transparent, muted | Collapsed, dim text; can be expanded but run button is disabled |

### 4.2 Invalidation Cascade

When the user edits a completed step, downstream steps are invalidated:

- **FILES changed** → Rasterize → `⚠ Stale`, G-code → `⚠ Stale`
- **SETTINGS changed (DPI)** → Rasterize → `⚠ Stale`, G-code → `⚠ Stale`
- **SETTINGS changed (any other field)** → G-code → `⚠ Stale` only
- **RASTERIZE re-run** → G-code → `⚠ Stale`

Stale state shows the message: `"[Step name] changed — results outdated"` with a `[↻ Re-run]` button.

Importantly: stale results are **not deleted** — they remain usable until explicitly re-run. The user can ignore the stale warning and save existing output.

### 4.3 Collapsed Summary Format

| Step | Collapsed summary |
|---|---|
| FILES | `3 files loaded · Cu, Out, Drl` |
| SETTINGS | `1200 dpi · cut -0.1mm · feed 800 mm/min · Ø 0.1mm` |
| RASTERIZE | `3 layers · traces.png, drills.png, outline.png · 1.4s` |
| G-CODE | `board.nc · 6.4 kb · 14m 22s est.` |

---

## 5. Step 1 — FILES

### 5.1 Expanded State

```
① FILES
┌─────────────────────────────────────────────────┐
│  Drop Gerber files here, or click to browse     │
│  .GTL  .GBL  .GKO  .DRL  ...                   │
└─────────────────────────────────────────────────┘

Cu   board.gtl     [✕]
Out  edge.ger      [✕]
Drl  holes.drl     [✕]

─────────── or ───────────

[↑ Load existing PNG instead]
```

### 5.2 Behaviour

- Drop zone accepts multiple files in one drag. Auto-detects layer type from extension:
  - `.GTL`, `.GBL`, `.GTB` → Copper (Cu)
  - `.GKO`, `.GM1`, `.GBO` → Outline (Out)
  - `.DRL`, `.TXT`, `.XLN` → Drill (Drl)
  - Unknown extensions get tagged `?` and are accepted but shown with an amber warning.
- Each loaded file row shows: type tag (colored) + filename + remove `[✕]` button.
- Multiple files per layer type are allowed (e.g., two copper layers).
- `[↑ Load existing PNG instead]` is a ghost-style secondary action. Selecting it:
  - Opens a file picker for a `.png` file.
  - Hides the Gerber rows (they are cleared).
  - Marks the RASTERIZE step as `✓ Skipped` (with a distinct "skipped" visual — muted green, italic label).
  - The loaded PNG path is shown as a single file row in FILES.
- Changing any file immediately cascades invalidation to downstream steps (see §4.2).

---

## 6. Step 2 — SETTINGS

### 6.1 Expanded State

```
② SETTINGS

▾ GEOMETRY
  Resolution (DPI)   [  1200  ]

▾ DEPTHS
  Cut Z (mm)   [ -0.1 ]    Safe Z (mm)  [  3.0 ]

▸ MOTION     (feed 800 · plunge 300 · spindle 12000)
▸ TOOLING    (dia 0.1mm · offsets 3 · stepover 0.05)
```

### 6.2 Behaviour

- Four accordion groups. **Geometry** and **Depths** open by default on first use. **Motion** and **Tooling** collapsed by default.
- Collapsed accordion headers show their values inline so the user can scan without expanding.
- Changing DPI triggers invalidation of Rasterize + G-code. Changing any other field triggers invalidation of G-code only.
- All fields are text inputs (matching current implementation). No validation UI in this iteration — invalid values silently fall back to defaults at parse time (existing behaviour preserved).
- Settings persist across sessions (existing behaviour preserved).

### 6.3 Field Groups

| Group | Fields |
|---|---|
| Geometry | DPI |
| Depths | Cut Z (mm), Safe Z (mm) |
| Motion | Feed rate (mm/min), Plunge rate (mm/min), Spindle (RPM) |
| Tooling | Tool diameter (mm), Offsets (0=fill), Stepover |

---

## 7. Step 3 — RASTERIZE

### 7.1 Expanded State — Running

```
③ RASTERIZE                              [▶ Run]

Cu   ████████████████████████  100%  ✓  traces.png
Out  ████████████░░░░░░░░░░░░   52%  ···
Drl  ░░░░░░░░░░░░░░░░░░░░░░░░    0%  waiting

┌──────────────────────────────────────────────┐
│  [traces thumb]  [outline thumb]  [drills]   │
│   fades in per-layer as each completes       │
└──────────────────────────────────────────────┘

[↓ Save Traces]  [↓ Save Drills]  [↓ Save Outline]  [↓ Save All]
```

### 7.2 Behaviour

- `[▶ Run]` in the step header is enabled when FILES has at least one Gerber file loaded and the stage is not already running. If the user loaded a PNG directly, this step shows as `✓ Skipped` and has no run button.
- Each layer gets its own progress bar. Layers process in parallel (current async behaviour preserved).
- PNG thumbnails render in place at fixed height (120px) as each layer finishes — they fade in individually, not all-at-once.
- Save buttons appear only after their respective layer is complete. "Save All" appears only after all three layers are done.
- When all layers complete, the step auto-collapses after a 1-second delay to its summary line.
- Re-running (via `[▶ Run]` or `[↻ Re-run]` in stale state) clears existing thumbnails and restarts progress bars from zero. G-code is immediately marked stale.

---

## 8. Step 4 — G-CODE

### 8.1 Expanded State — Complete

```
④ G-CODE                                 [▶ Run]

████████████████████████  100%  ✓  board.nc

┌────────────────────────────────────────┐
│  Est. cut time    14 min 22s           │
│  Cut distance     2.34 m               │
│  Board size       40 × 32 mm           │
└────────────────────────────────────────┘

[↓ Save G-code]
```

### 8.2 Behaviour

- `[▶ Run]` is enabled when Rasterize is `✓ Complete` or `✓ Skipped` (PNG loaded directly), and the stage is not already running.
- Single progress bar (existing behaviour).
- Stats card appears after completion.
- Save button appears after completion.

---

## 9. Glass Aesthetic

All colour values extend the existing palette in `src/ui/palette.rs`. No existing colours are changed — new tokens are added.

| Token | Value | Usage |
|---|---|---|
| `app_bg` | `#0D0F1A` | Root background (slightly deeper than current) |
| `sidebar_bg` | `rgba(255,255,255, 0.04)` | Sidebar panel |
| `sidebar_border` | `rgba(255,255,255, 0.08)` | Sidebar right edge |
| `card_bg` | `rgba(255,255,255, 0.03)` | Default step card |
| `card_active_bg` | `rgba(125,206,255, 0.06)` | Active step card background |
| `card_active_border` | `accent` (existing) | Active step left border, 2px |
| `card_complete_bg` | `rgba(98,129,66, 0.08)` | Complete step background |
| `card_complete_border` | `signal_green` (existing) | Complete step left border, 2px |
| `card_stale_bg` | `rgba(224,176,104, 0.08)` | Stale step background |
| `card_stale_border` | `signal_gold` (existing) | Stale step left border, 2px |
| `input_bg` | `rgba(255,255,255, 0.06)` | Text input background |
| `input_border_focus` | `accent` | Text input border when focused |
| `drop_zone_bg` | `rgba(255,255,255, 0.03)` | File drop zone |
| `drop_zone_border` | `rgba(255,255,255, 0.10)` dashed | Drop zone border |
| `drop_zone_active_bg` | `rgba(125,206,255, 0.08)` | Drop zone when file is dragged over |

Progress bars: rounded caps (`border_radius = height / 2`). Active bar has a subtle glow: `box_shadow` equivalent via a slightly wider, lower-opacity duplicate bar behind it in iced (approximated with a 2px taller container with `clip: false`).

---

## 10. New State Fields Required

The following additions to `AppState` in `src/main.rs` are needed:

```rust
// Track which steps are stale and why
pub(crate) rasterize_stale: bool,
pub(crate) gcode_stale: bool,

// Track which step card is currently expanded in the UI
pub(crate) expanded_step: Option<u8>,  // 1..=4, None = all collapsed
```

The existing `gerber_to_png` and `png_to_gcode` `StepState` fields remain and continue to drive progress/running state. The new `_stale` booleans are separate because a step can be `Complete` AND `Stale` simultaneously.

---

## 11. New Messages Required

```rust
// Step card expand/collapse
StepToggled(u8),

// Stale re-run triggers (distinct from initial run)
ReRunRasterize,
ReRunGcode,

// File removal from sidebar
RemoveFile { layer: LayerKind, index: usize },
```

`LayerKind` is a new enum: `Copper | Outline | Drill`.

---

## 12. Widget Inventory

### Widgets to remove
- `tab_bar` — replaced by sidebar navigation
- `source_panel` — replaced by FILES step card
- `config_panel` — replaced by SETTINGS step card
- `pipeline_panel` — replaced by the full step canvas
- `generated_output` — folded into RASTERIZE and G-CODE step cards
- `save_png_buttons`, `save_gcode_button` — folded into their respective step cards

### Widgets to add
- `sidebar(state)` → `Element`
- `step_canvas(state)` → `Element` (root of main area, contains all four step cards)
- `files_step(state)` → `Element`
- `settings_step(state)` → `Element`
- `rasterize_step(state)` → `Element`
- `gcode_step(state)` → `Element`
- `step_shell(label, step_num, state, content)` → `Element` (shared wrapper handling collapse/expand, left border, state badge)
- `drop_zone()` → `Element`
- `accordion(label, summary, content, is_open)` → `Element`
- `layer_row(kind, filename)` → `Element`

### Widgets to keep (unchanged)
- `header` — removed entirely (branding moves to sidebar top)
- `status_line` — replaced by sidebar bottom status dot
- `step_card` — replaced by the new step widgets above
- `progress_chip` — removed (progress now lives inside each step card)
- `setting_field` — kept as-is, used inside `settings_step`
- `pill_color`, `pill_bg` — kept for state badge styling

---

## 13. Out of Scope

- Drag-and-drop file reordering within a layer group
- Keyboard navigation / ⌘K command palette
- Job history / recent files
- G-code preview / syntax highlighting
- Zoomable PNG previews
- Animated toolpath visualization
- Actual Gerber thumbnail rendering (files shown by name only)
- Auto-detection of unknown Gerber extensions beyond the list in §5.2
