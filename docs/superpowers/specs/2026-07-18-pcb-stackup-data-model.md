# PCB Stackup Data Model

## Problem

EasyMill tracks input Gerber files as three flat `Vec<PathBuf>` lists (copper,
outline, drill). There is no per-file metadata, no layer classification system,
and no way to handle real PCB stackups that include soldermask, silkscreen,
solderpaste, inner copper layers, or unknown file types.

## Solution

Introduce a `Stackup` data model with JSON-driven auto-detection of layer type
from filenames, user override for misclassified or unknown files, and minimal
changes to the existing rendering pipeline.

## Data Types — `src/stackup.rs` (new module)

```rust
enum LayerCategory {
    Copper, Soldermask, Silkscreen, Solderpaste,
    Outline, Drill, Drawing, Unknown,
}

enum Side {
    Top, Bottom, Inner(u8), All,
}

struct LayerFile {
    path: PathBuf,
    auto_category: LayerCategory,
    auto_side: Side,
    user_category: Option<LayerCategory>,
    user_side: Option<Side>,
    user_label: Option<String>,
}

impl LayerFile {
    fn effective_category(&self) -> LayerCategory;
    fn effective_side(&self) -> Side;
    fn is_resolved(&self) -> bool;  // has usable category
}

struct Stackup {
    layers: Vec<LayerFile>,
}
```

## Auto-Detection — JSON-Driven

`gerber-filenames.json` is embedded at compile time via `include_str!` and
parsed with `serde_json` at startup. Two rule tables are built:

1. **Extension rules**: Group all entries by extension; if every entry for a
   given extension maps to the same (category, side), emit a rule. Examples:
   - `.gtl` → Copper+Top (KiCad + Altium agree)
   - `.gts` → Soldermask+Top
   - `.gko` → Outline+All
   - `.drl` → Drill+All
   - `.xln` → Drill+All

2. **Pattern rules**: For ambiguous extensions (`.gbr`, `.txt`, `.ger`),
   normalize each entry name by stripping common prefixes (`board`, `board-`,
   project name) and extract distinguishing substrings. Match against the
   filename stem at detection time.

Detection priority: extension rule → pattern rule → Excellon content sniff →
Unknown.

Confidence is tracked implicitly: extension match = high, pattern match =
medium, Unknown = low (user must override).

## AppState Changes — `src/main.rs`

**Replace:**
- `copper_paths: Vec<PathBuf>`
- `outline_paths: Vec<PathBuf>`
- `drill_paths: Vec<PathBuf>`

**With:**
- `stackup: Stackup`

`loaded_inputs` is still derived but shows `[Category+Side] filename`.

## Pipeline Integration — No Signature Change

The existing `gerber_inputs_to_png(copper, outline, drill, ...)` keeps its
signature. `Stackup` gains a helper:

```rust
impl Stackup {
    fn milling_paths(&self) -> (Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>);
}
```

It extracts files where:
- `effective_category() == Copper` (any side, any inner) → copper paths
- `effective_category() == Outline` → outline paths
- `effective_category() == Drill` → drill paths

Files with unresolved categories are excluded and shown as warnings in the UI.

## UI Changes

- **File list rows**: Show `[Category+Side] filename` with color-coded badges
  (one color per LayerCategory, reusing existing palette patterns).
- **Unknown files**: Red badge, "?" tooltip, excluded from pipeline.
- **Override popover**: Clicking the badge opens an inline dropdown to change
  category and side. Stored in `LayerFile.user_category` / `.user_side`.
- **File pickers**: Keep dedicated copper/outline/drill buttons (convenience).
  Add a generic "Add Gerber" button that auto-detects from the filename.
- **Remove**: `RemoveFile { index: usize }` → flat index into `stackup.layers`.
- **Derived display**: `loaded_inputs` shows `"[Copper+Top] board-F_Cu.gbr"`.

## Dependencies Added

- `serde` (with `derive` feature)
- `serde_json`

## Files Changed

| File | Change |
|------|--------|
| `Cargo.toml` | Add `serde`, `serde_json` |
| `gerber-filenames.json` | Used as embedded data source (already exists) |
| `src/stackup.rs` | New file: types, parse JSON, detection, Stackup |
| `src/lib.rs` | Add `pub mod stackup` |
| `src/main.rs` | Replace 3 Vecs with Stackup, update messages, update UI |
| `src/ui/widgets/*.rs` | Update file row rendering, add override popover |

## Exclusions

- No extra PNG renders for non-milling layers
- No drag-to-reorder layers
- No multi-board designs
- The JSON is baked in (no hot-reload)
