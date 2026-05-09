use crate::media::MediaKind;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WallpaperItem {
    pub path: PathBuf,
    pub kind: MediaKind,
}

impl WallpaperItem {
    pub fn display_name(&self) -> String {
        self.path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("wallpaper")
            .to_string()
    }

    pub fn is_live(&self) -> bool {
        self.kind == MediaKind::Live
    }
}
