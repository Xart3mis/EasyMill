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
}
