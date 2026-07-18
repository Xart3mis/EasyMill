use std::collections::HashMap;
use std::path::PathBuf;
use serde::Deserialize;

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
    extension_rules: HashMap<String, (LayerCategory, Side)>,
    pattern_rules: Vec<(String, LayerCategory, Side)>,
}

impl LayerDetector {
    pub fn new() -> Self {
        let json = include_str!("../gerber-filenames.json");
        let entries: Vec<JsonEntry> = serde_json::from_str(json).expect("valid gerber-filenames.json");
        Self::from_entries(&entries)
    }

    fn from_entries(entries: &[JsonEntry]) -> Self {
        let mut ext_groups: HashMap<String, Vec<(LayerCategory, Side)>> = HashMap::new();
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

        let mut extension_rules = HashMap::new();
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
            if stem.contains(pattern.as_str()) {
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
    fn test_filename_returns_file_name() {
        let f = LayerFile::new(
            PathBuf::from("/some/dir/test-file.gtl"),
            LayerCategory::Copper,
            Side::Top,
        );
        assert_eq!(f.filename(), "test-file.gtl");
    }

    #[test]
    fn test_filename_returns_empty_for_root() {
        let f = LayerFile::new(
            PathBuf::from("/"),
            LayerCategory::Copper,
            Side::Top,
        );
        assert_eq!(f.filename(), "");
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
}
