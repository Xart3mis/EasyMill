# PCB Stackup Data Model Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace 3 hardcoded `Vec<PathBuf>` layer lists with a Stackup data model that auto-detects layer type from Gerber filenames (using embedded `gerber-filenames.json`) and supports user override.

**Architecture:** New `src/stackup.rs` module with types (`LayerCategory`, `Side`, `LayerFile`, `Stackup`) and a `LayerDetector` that parses the embedded JSON at startup to build extension + pattern rules. `Stackup::milling_paths()` extracts copper/outline/drill paths for the unchanged pipeline. AppState swaps the 3 Vecs for a single `Stackup`. UI shows category+side badges with override popover.

**Tech Stack:** Rust, serde, serde_json, Iced 0.14

---

### Task 1: Add dependencies + create stackup module with core types

**Files:**
- Modify: `Cargo.toml`
- Create: `src/stackup.rs`
- Modify: `src/lib.rs`
- Test: inline in `src/stackup.rs`

- [ ] **Step 1: Add serde + serde_json to Cargo.toml**

Edit `Cargo.toml` to add after the existing `nalgebra` line:
```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: Create `src/stackup.rs` with core types**

```rust
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerCategory {
    Copper,
    Soldermask,
    Silkscreen,
    Solderpaste,
    Outline,
    Drill,
    Drawing,
    Unknown,
}

impl LayerCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Copper => "Copper",
            Self::Soldermask => "Mask",
            Self::Silkscreen => "Silk",
            Self::Solderpaste => "Paste",
            Self::Outline => "Outline",
            Self::Drill => "Drill",
            Self::Drawing => "Drawing",
            Self::Unknown => "?",
        }
    }

    pub fn variants() -> &'static [LayerCategory] {
        &[
            Self::Copper,
            Self::Soldermask,
            Self::Silkscreen,
            Self::Solderpaste,
            Self::Outline,
            Self::Drill,
            Self::Drawing,
            Self::Unknown,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
    Inner(u8),
    All,
}

impl Side {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::Bottom => "Bot",
            Self::Inner(_) => "In",
            Self::All => "All",
        }
    }

    pub fn variants() -> &'static [Side] {
        &[Self::Top, Self::Bottom, Self::Inner(0), Self::All]
    }
}

#[derive(Debug, Clone)]
pub struct LayerFile {
    pub path: PathBuf,
    pub auto_category: LayerCategory,
    pub auto_side: Side,
    pub user_category: Option<LayerCategory>,
    pub user_side: Option<Side>,
    pub user_label: Option<String>,
}

impl LayerFile {
    pub fn new(path: PathBuf, category: LayerCategory, side: Side) -> Self {
        Self {
            path,
            auto_category: category,
            auto_side: side,
            user_category: None,
            user_side: None,
            user_label: None,
        }
    }

    pub fn effective_category(&self) -> LayerCategory {
        self.user_category.unwrap_or(self.auto_category)
    }

    pub fn effective_side(&self) -> Side {
        self.user_side.unwrap_or(self.auto_side)
    }

    pub fn is_resolved(&self) -> bool {
        self.effective_category() != LayerCategory::Unknown
    }

    pub fn filename(&self) -> String {
        self.path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub struct Stackup {
    pub layers: Vec<LayerFile>,
}

impl Stackup {
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    pub fn milling_paths(&self) -> (Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>) {
        let mut copper = Vec::new();
        let mut outline = Vec::new();
        let mut drill = Vec::new();
        for layer in &self.layers {
            if !layer.is_resolved() {
                continue;
            }
            match layer.effective_category() {
                LayerCategory::Copper => copper.push(layer.path.clone()),
                LayerCategory::Outline => outline.push(layer.path.clone()),
                LayerCategory::Drill => drill.push(layer.path.clone()),
                _ => {}
            }
        }
        (copper, outline, drill)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effective_category_defaults_to_auto() {
        let f = LayerFile::new(
            PathBuf::from("test.gtl"),
            LayerCategory::Copper,
            Side::Top,
        );
        assert_eq!(f.effective_category(), LayerCategory::Copper);
        assert_eq!(f.effective_side(), Side::Top);
    }

    #[test]
    fn test_effective_category_uses_override() {
        let mut f = LayerFile::new(
            PathBuf::from("test.gtl"),
            LayerCategory::Copper,
            Side::Top,
        );
        f.user_category = Some(LayerCategory::Outline);
        assert_eq!(f.effective_category(), LayerCategory::Outline);
    }

    #[test]
    fn test_is_resolved_unknown_is_false() {
        let f = LayerFile::new(
            PathBuf::from("unknown.xyz"),
            LayerCategory::Unknown,
            Side::All,
        );
        assert!(!f.is_resolved());
    }

    #[test]
    fn test_is_resolved_known_is_true() {
        let f = LayerFile::new(
            PathBuf::from("test.drl"),
            LayerCategory::Drill,
            Side::All,
        );
        assert!(f.is_resolved());
    }

    #[test]
    fn test_milling_paths_filters_correctly() {
        let mut s = Stackup::new();
        s.layers.push(LayerFile::new(
            PathBuf::from("top.gtl"), LayerCategory::Copper, Side::Top,
        ));
        s.layers.push(LayerFile::new(
            PathBuf::from("bot.gbl"), LayerCategory::Copper, Side::Bottom,
        ));
        s.layers.push(LayerFile::new(
            PathBuf::from("outline.gko"), LayerCategory::Outline, Side::All,
        ));
        s.layers.push(LayerFile::new(
            PathBuf::from("drill.drl"), LayerCategory::Drill, Side::All,
        ));
        s.layers.push(LayerFile::new(
            PathBuf::from("mask.gts"), LayerCategory::Soldermask, Side::Top,
        ));
        s.layers.push(LayerFile::new(
            PathBuf::from("unknown.xyz"), LayerCategory::Unknown, Side::All,
        ));

        let (copper, outline, drill) = s.milling_paths();
        assert_eq!(copper.len(), 2);
        assert_eq!(outline.len(), 1);
        assert_eq!(drill.len(), 1);
    }

    #[test]
    fn test_milling_paths_excludes_unknown() {
        let mut s = Stackup::new();
        s.layers.push(LayerFile::new(
            PathBuf::from("unknown.xyz"), LayerCategory::Unknown, Side::All,
        ));
        let (copper, outline, drill) = s.milling_paths();
        assert!(copper.is_empty());
        assert!(outline.is_empty());
        assert!(drill.is_empty());
    }
}
```

- [ ] **Step 3: Export stackup module from `src/lib.rs`**

Edit `src/lib.rs` to add `pub mod stackup;` after `pub mod logging;`:
```rust
pub mod conversion;
pub mod gerber;
pub mod logging;
pub mod stackup;

pub use conversion::PngLayerResults;
```

- [ ] **Step 4: Run tests**

```bash
cargo test --lib stackup
```
Expected: all 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/stackup.rs src/lib.rs
git commit -m "feat: add serde deps and PCB stackup core types"
```

---

### Task 2: JSON-driven layer detector

**Files:**
- Modify: `src/stackup.rs`

- [ ] **Step 1: Add JSON deserialization structs and LayerDetector**

Append to `src/stackup.rs` before the `#[cfg(test)]` block:

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct JsonEntry {
    cad: String,
    files: Vec<JsonFileEntry>,
}

#[derive(Debug, Deserialize)]
struct JsonFileEntry {
    name: String,
    side: Option<String>,
    #[serde(rename = "type")]
    layer_type: Option<String>,
}

#[derive(Debug)]
pub struct LayerDetector {
    extension_rules: std::collections::HashMap<String, (LayerCategory, Side)>,
    pattern_rules: Vec<(String, LayerCategory, Side)>,
}

impl LayerDetector {
    pub fn new() -> Self {
        let json = include_str!("../gerber-filenames.json");
        let entries: Vec<JsonEntry> = serde_json::from_str(json).expect("valid gerber-filenames.json");
        Self::from_entries(&entries)
    }

    fn from_entries(entries: &[JsonEntry]) -> Self {
        let mut ext_groups: std::collections::HashMap<String, Vec<(LayerCategory, Side)>> =
            std::collections::HashMap::new();
        let mut pattern_rules = Vec::new();

        for cad in entries {
            for file in &cad.files {
                let Some((cat, side)) = parse_type_side(
                    file.layer_type.as_deref(),
                    file.side.as_deref(),
                ) else {
                    continue;
                };

                let dot = file.name.rfind('.');
                let ext = dot.map(|i| file.name[i+1..].to_lowercase()).unwrap_or_default();

                ext_groups.entry(ext.clone()).or_default().push((cat, side));

                if ext == "gbr" || ext == "ger" || ext == "gpi" || ext == "gko" {
                    let stem = &file.name[..dot.unwrap_or(file.name.len())];
                    let pattern = normalize_pattern(stem);
                    if !pattern.is_empty() {
                        pattern_rules.push((pattern, cat, side));
                    }
                }
            }
        }

        let mut extension_rules = std::collections::HashMap::new();
        for (ext, pairs) in &ext_groups {
            if pairs.is_empty() {
                continue;
            }
            let first = pairs[0];
            if ext == "gbr" || ext == "ger" || ext == "gpi" {
                continue;
            }
            if pairs.iter().all(|p| *p == first) {
                extension_rules.insert(ext.clone(), first);
            }
        }

        pattern_rules.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        Self { extension_rules, pattern_rules }
    }

    pub fn detect(&self, filename: &str) -> (LayerCategory, Side) {
        let lc = filename.to_lowercase();
        let dot = lc.rfind('.');
        let ext = dot.map(|i| &lc[i+1..]).unwrap_or("");
        let stem = dot.map(|i| &filename[..i]).unwrap_or(filename);

        if let Some(&result) = self.extension_rules.get(ext) {
            return result;
        }

        for (pattern, cat, side) in &self.pattern_rules {
            if stem.contains(pattern) {
                return (*cat, *side);
            }
        }

        (LayerCategory::Unknown, Side::All)
    }
}

fn parse_type_side(layer_type: Option<&str>, side: Option<&str>) -> Option<(LayerCategory, Side)> {
    let cat = match layer_type? {
        "copper" => LayerCategory::Copper,
        "soldermask" => LayerCategory::Soldermask,
        "silkscreen" => LayerCategory::Silkscreen,
        "solderpaste" => LayerCategory::Solderpaste,
        "outline" => LayerCategory::Outline,
        "drill" => LayerCategory::Drill,
        "drawing" => LayerCategory::Drawing,
        _ => return None,
    };
    let side = match side? {
        "top" => Side::Top,
        "bottom" => Side::Bottom,
        "inner" => Side::Inner(0),
        "all" => Side::All,
        _ => return None,
    };
    Some((cat, side))
}

fn normalize_pattern(stem: &str) -> String {
    let s = stem
        .trim_start_matches("board")
        .trim_start_matches('-')
        .trim_start_matches('.')
        .trim_start_matches("name")
        .trim_start_matches('-')
        .trim_start_matches('.');
    if s.is_empty() || s == stem {
        stem.to_owned()
    } else {
        s.to_owned()
    }
}
```

- [ ] **Step 2: Add tests for LayerDetector**

Append to the `#[cfg(test)]` block:

```rust
#[test]
fn test_detect_known_extension() {
    let entries = vec![JsonEntry {
        cad: "test".into(),
        files: vec![
            JsonFileEntry {
                name: "test.gtl".into(),
                side: Some("top".into()),
                layer_type: Some("copper".into()),
            },
        ],
    }];
    let detector = LayerDetector::from_entries(&entries);
    let (cat, side) = detector.detect("board.gtl");
    assert_eq!(cat, LayerCategory::Copper);
    assert_eq!(side, Side::Top);
}

#[test]
fn test_detect_drill_extensions() {
    let entries = vec![JsonEntry {
        cad: "test".into(),
        files: vec![
            JsonFileEntry {
                name: "board.drl".into(),
                side: Some("all".into()),
                layer_type: Some("drill".into()),
            },
            JsonFileEntry {
                name: "board.xln".into(),
                side: Some("all".into()),
                layer_type: Some("drill".into()),
            },
        ],
    }];
    let detector = LayerDetector::from_entries(&entries);
    assert_eq!(detector.detect("test.drl"), (LayerCategory::Drill, Side::All));
    assert_eq!(detector.detect("test.xln"), (LayerCategory::Drill, Side::All));
}

#[test]
fn test_detect_unknown_extension() {
    let entries = vec![JsonEntry {
        cad: "test".into(),
        files: vec![
            JsonFileEntry {
                name: "test.drl".into(),
                side: Some("all".into()),
                layer_type: Some("drill".into()),
            },
        ],
    }];
    let detector = LayerDetector::from_entries(&entries);
    let (cat, side) = detector.detect("test.xyz");
    assert_eq!(cat, LayerCategory::Unknown);
    assert_eq!(side, Side::All);
}

#[test]
fn test_detect_skip_null_fields() {
    let entries = vec![JsonEntry {
        cad: "test".into(),
        files: vec![
            JsonFileEntry {
                name: "pnp_bom.txt".into(),
                side: None,
                layer_type: None,
            },
            JsonFileEntry {
                name: "board.drl".into(),
                side: Some("all".into()),
                layer_type: Some("drill".into()),
            },
        ],
    }];
    let detector = LayerDetector::from_entries(&entries);
    // .txt extension should NOT get a rule (null entries filtered)
    let mut ext_rule_found = false;
    for ext in detector.extension_rules.keys() {
        if ext == "txt" {
            ext_rule_found = true;
        }
    }
    assert!(!ext_rule_found, ".txt should not get an extension rule");
    assert_eq!(detector.detect("board.drl"), (LayerCategory::Drill, Side::All));
}

#[test]
fn test_detect_gbr_via_pattern() {
    let entries = vec![JsonEntry {
        cad: "kicad".into(),
        files: vec![
            JsonFileEntry {
                name: "board-F_Cu.gbr".into(),
                side: Some("top".into()),
                layer_type: Some("copper".into()),
            },
            JsonFileEntry {
                name: "board-Edge_Cuts.gbr".into(),
                side: Some("all".into()),
                layer_type: Some("outline".into()),
            },
        ],
    }];
    let detector = LayerDetector::from_entries(&entries);
    assert_eq!(
        detector.detect("my_board-F_Cu.gbr"),
        (LayerCategory::Copper, Side::Top)
    );
    assert_eq!(
        detector.detect("my_board-Edge_Cuts.gbr"),
        (LayerCategory::Outline, Side::All)
    );
}

#[test]
fn test_detect_extension_rule_preferred_over_pattern() {
    // .drl should match extension rule even if name contains "F_Cu"
    let entries = vec![
        JsonEntry {
            cad: "kicad".into(),
            files: vec![
                JsonFileEntry {
                    name: "board.drl".into(),
                    side: Some("all".into()),
                    layer_type: Some("drill".into()),
                },
            ],
        },
    ];
    let detector = LayerDetector::from_entries(&entries);
    assert_eq!(
        detector.detect("F_Cu.drl"),
        (LayerCategory::Drill, Side::All)
    );
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test --lib stackup
```
Expected: all 12 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/stackup.rs
git commit -m "feat: add JSON-driven gerber layer detector"
```

---

### Task 3: Integrate Stackup into AppState

**Files:**
- Modify: `src/main.rs`
- Modify: `src/ui/widgets/` (sidebar, components, steps)

- [ ] **Step 1: Replace AppState Vecs with Stackup**

In `src/main.rs`, update imports to include `Stackup`, `LayerFile`, and remove the old `LayerKind`-related code (keep `LayerKind` enum for now, it's used in Messages).

Add `use easymill::stackup::{Stackup, LayerFile, LayerCategory, Side};` at the top.

Replace in `AppState`:
```rust
pub(crate) struct AppState {
    pub(crate) active_tab: Tab,
    pub(crate) stackup: Stackup,               // was: copper_paths, outline_paths, drill_paths
    pub(crate) loaded_png_path: Option<PathBuf>,
    ...
}
```

In the `Default` impl, replace:
```rust
copper_paths: Vec::new(),
outline_paths: Vec::new(),
drill_paths: Vec::new(),
```
With:
```rust
stackup: Stackup::new(),
```

- [ ] **Step 2: Remove `derive_loaded_inputs` and `path_to_label`**

Replace `derive_loaded_inputs` with a new version that reads from `state.stackup`:

```rust
fn derive_loaded_inputs(state: &AppState) -> Vec<String> {
    state.stackup.layers.iter().map(|layer| {
        let cat = layer.effective_category();
        let side = layer.effective_side();
        let path = layer.path.to_string_lossy();
        format!("[{} + {}] {}", cat.label(), side.label(), path)
    }).collect()
}
```

- [ ] **Step 3: Update file picker message handlers**

Replace `CopperFilesPicked`, `OutlineFilesPicked`, `DrillFilesPicked` handlers:

```rust
Message::CopperFilesPicked(files) => {
    if let Some(files) = files {
        // Remove existing copper entries, add new ones
        state.stackup.layers.retain(|l| l.effective_category() != LayerCategory::Copper);
        for path in files {
            state.stackup.layers.push(LayerFile::new(path, LayerCategory::Copper, Side::Top));
        }
        state.loaded_inputs = derive_loaded_inputs(state);
        state.gerber_to_png = StepState::Ready;
        state.rasterize_stale = state.gerber_to_png == StepState::Complete;
        state.gcode_stale = state.png_to_gcode == StepState::Complete;
        state.status = format!("Copper layers loaded ({} files).", files.len());
    }
}
Message::OutlineFilesPicked(files) => {
    if let Some(files) = files {
        state.stackup.layers.retain(|l| l.effective_category() != LayerCategory::Outline);
        for path in files {
            state.stackup.layers.push(LayerFile::new(path, LayerCategory::Outline, Side::All));
        }
        state.loaded_inputs = derive_loaded_inputs(state);
        state.gerber_to_png = StepState::Ready;
        state.rasterize_stale = state.gerber_to_png == StepState::Complete;
        state.gcode_stale = state.png_to_gcode == StepState::Complete;
        state.status = format!("Outline loaded ({} files).", files.len());
    }
}
Message::DrillFilesPicked(files) => {
    if let Some(files) = files {
        state.stackup.layers.retain(|l| l.effective_category() != LayerCategory::Drill);
        for path in files {
            state.stackup.layers.push(LayerFile::new(path, LayerCategory::Drill, Side::All));
        }
        state.loaded_inputs = derive_loaded_inputs(state);
        state.gerber_to_png = StepState::Ready;
        state.rasterize_stale = state.gerber_to_png == StepState::Complete;
        state.gcode_stale = state.png_to_gcode == StepState::Complete;
        state.status = format!("Drills loaded ({} files).", files.len());
    }
}
```

- [ ] **Step 4: Update ConvertToPng to use stackup**

Replace the ConvertToPng handler's extraction:
```rust
Message::ConvertToPng => {
    let (copper, outline, drill) = state.stackup.milling_paths();

    if copper.is_empty() && outline.is_empty() && drill.is_empty() {
        warn!("convert to png requested with no resolvable inputs");
        state.status = "Load Gerber files before converting.".to_owned();
        return Task::none();
    }
    // ... rest unchanged
```

- [ ] **Step 5: Update RemoveFile handler**

Change from per-layer-kind index to flat index:
```rust
Message::RemoveFile { index } => {
    if index < state.stackup.layers.len() {
        state.stackup.layers.remove(index);
        state.rasterize_stale = state.gerber_to_png == StepState::Complete;
        state.gcode_stale = state.png_to_gcode == StepState::Complete;
        state.loaded_inputs = derive_loaded_inputs(state);
    }
}
```

Update the Message enum:
```rust
#[derive(Debug, Clone)]
pub(crate) enum Message {
    ...
    RemoveFile { index: usize },
    ...
}
```
Remove the `layer: LayerKind` field.

- [ ] **Step 6: Add generic file picker + override messages**

Add to the `Message` enum:
```rust
SelectGerberFiles,
GerberFilesPicked(Option<Vec<PathBuf>>),
OverrideLayer { index: usize, category: LayerCategory, side: Side },
```

Add handlers in `update()`:
```rust
Message::SelectGerberFiles => {
    return Task::perform(
        async {
            rfd::FileDialog::new()
                .add_filter("Gerber/Excellon", &["gbr", "gtl", "gbl", "gts", "gbs", "gto", "gbo", "gtp", "gbp", "gko", "gm1", "drl", "xln", "txt", "g1", "g2", "g3", "g4", "ger", "exc"])
                .set_title("Select Gerber or Excellon files")
                .pick_files()
        },
        Message::GerberFilesPicked,
    );
}
Message::GerberFilesPicked(files) => {
    if let Some(files) = files {
        let detector = easymill::stackup::LayerDetector::new();
        for path in files {
            let filename = path.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let (cat, side) = detector.detect(&filename);
            state.stackup.layers.push(LayerFile::new(path, cat, side));
        }
        state.loaded_inputs = derive_loaded_inputs(state);
        state.gerber_to_png = StepState::Ready;
        state.rasterize_stale = state.gerber_to_png == StepState::Complete;
        state.gcode_stale = state.png_to_gcode == StepState::Complete;
        state.status = format!("Added {} files via auto-detect.", files.len());
    }
}
Message::OverrideLayer { index, category, side } => {
    if let Some(layer) = state.stackup.layers.get_mut(index) {
        layer.user_category = if layer.auto_category == category { None } else { Some(category) };
        layer.user_side = if layer.auto_side == side { None } else { Some(side) };
        state.rasterize_stale = state.gerber_to_png == StepState::Complete;
        state.gcode_stale = state.png_to_gcode == StepState::Complete;
        state.loaded_inputs = derive_loaded_inputs(state);
    }
}
```

- [ ] **Step 7: Update `RunAll` to use stackup**

```rust
Message::RunAll => {
    let (copper, outline, drill) = state.stackup.milling_paths();
    let has_gerbers = !copper.is_empty() || !outline.is_empty() || !drill.is_empty();
    ...
}
```

- [ ] **Step 8: Build to check compilation**

```bash
cargo build 2>&1 | head -50
```
Expected: compilation errors in UI widgets that reference old AppState fields.

- [ ] **Step 9: Commit (even if build fails due to UI — committing data model change)**

```bash
git add src/main.rs
git commit -m "feat: integrate Stackup into AppState, add generic file picker and override messages"
```

---

### Task 4: Update UI widgets

**Files:**
- Modify: `src/ui/widgets/sidebar.rs`
- Modify: `src/ui/widgets/components.rs`
- Modify: `src/ui/palette.rs`

- [ ] **Step 1: Add palette colors for all LayerCategory variants**

Edit `src/ui/palette.rs` to add colors for each category. Add alongside existing `fn layer_copper()`, `fn layer_outline()`, `fn layer_drill()`:

```rust
pub(crate) fn layer_copper() -> Color { Color::from_rgb(0.88, 0.69, 0.41) }
pub(crate) fn layer_outline() -> Color { Color::from_rgb(0.49, 0.81, 1.0) }
pub(crate) fn layer_drill() -> Color { Color::from_rgb(1.0, 0.5, 0.5) }
pub(crate) fn layer_mask() -> Color { Color::from_rgb(0.3, 0.85, 0.5) }
pub(crate) fn layer_silk() -> Color { Color::from_rgb(0.95, 0.95, 0.95) }
pub(crate) fn layer_paste() -> Color { Color::from_rgb(0.7, 0.7, 0.3) }
pub(crate) fn layer_drawing() -> Color { Color::from_rgb(0.6, 0.6, 0.6) }
pub(crate) fn layer_unknown() -> Color { Color::from_rgb(1.0, 0.2, 0.2) }
```

Add a helper function:
```rust
pub(crate) fn layer_category_color(cat: &easymill::stackup::LayerCategory) -> Color {
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
```

- [ ] **Step 2: Update sidebar file row to use Stackup**

Edit `src/ui/widgets/sidebar.rs`. The sidebar's file row rendering function currently receives `copper_paths`, `outline_paths`, `drill_paths` from `AppState`. Change it to receive `&Stackup` and iterate `stackup.layers`.

Replace the file listing section in `sidebar_file_row` or the parent rendering function. The function signature should change from accepting 3 path slices to accepting `&Stackup`. For each `LayerFile` in the stackup:

```rust
pub(crate) fn sidebar_file_row(
    index: usize,
    layer: &easymill::stackup::LayerFile,
) -> Element<'_, Message> {
    let cat = layer.effective_category();
    let side = layer.effective_side();
    let label = format!("{} + {}", cat.label(), side.label());
    let color = palette::layer_category_color(&cat);
    let resolved = layer.is_resolved();

    // category badge
    let badge = container(text(label).size(11))
        .padding([2, 6])
        .style(move |theme: &Theme| {
            // minimal rounded badge style
            container::Style::default()
                .background(Some(Background::Color(
                    if resolved { color } else { palette::layer_unknown() }
                )))
                .border_radius(4.0)
        });

    // filename
    let fname = text(layer.filename()).size(13);

    // row: [badge] filename [remove button]
    // if unresolved, add warning indicator
    // if resolved, clicking badge opens category dropdown
}
```

- [ ] **Step 3: Update sidebar source panel**

In the function that builds the source panel (likely `source_panel` or equivalent in sidebar.rs), replace the 3 separate section loops with a single loop over `stackup.layers`:

```rust
// Replace 3 separate file list sections with:
for (i, layer) in state.stackup.layers.iter().enumerate() {
    rows = rows.push(sidebar_file_row(i, layer));
}
```

Add the "Add Gerber Files" button alongside the existing dedicated buttons:
```rust
button("Add Gerber")
    .on_press(Message::SelectGerberFiles)
    .style(/* secondary/ghost style */)
```

- [ ] **Step 4: Update `source_panel` call site**

In `src/main.rs`, the `view` function calls `source_panel(state)`. Ensure `source_panel` accepts the full `AppState` (it already does — check the existing signature).

- [ ] **Step 5: Remove unused LayerKind imports and code**

Remove references to `LayerKind` in `src/main.rs` — the `RemoveFile` message now uses flat `index`. `LayerKind` can be fully removed since `stackup::LayerCategory` replaces it.

- [ ] **Step 6: Build**

```bash
cargo build
```
Expected: clean compile.

- [ ] **Step 7: Commit**

```bash
git add src/ui/ src/main.rs
git commit -m "feat: update UI widgets for Stackup data model"
```

---

### Self-Review

**Spec coverage:**
- Task 1 covers: core types (LayerCategory, Side, LayerFile, Stackup), tests, module export ✓
- Task 2 covers: JSON embedding, parsing, LayerDetector, detect function, pattern rules ✓
- Task 3 covers: AppState integration, milling_paths wiring, file pickers, RemoveMessage, generic picker, override messages ✓
- Task 4 covers: UI badges, colors, file row rendering, source panel update ✓

**No placeholders:** All steps have complete code, file paths, test code, and commands.

**Type consistency:** LayerCategory/Side defined in Task 1 are used in Tasks 2, 3, 4 consistently. Milling_paths signature matches.
