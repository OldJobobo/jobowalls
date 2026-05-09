use crate::media::MediaKind;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Backend {
    Hyprpaper,
    Mpvpaper,
    Awww,
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Backend::Hyprpaper => write!(f, "hyprpaper"),
            Backend::Mpvpaper => write!(f, "mpvpaper"),
            Backend::Awww => write!(f, "awww"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendOverride {
    Auto,
    Backend(Backend),
}

pub trait WallpaperBackend {
    fn backend(&self) -> Backend;

    fn name(&self) -> &'static str;

    fn is_available(&self) -> bool;

    fn supports(&self, media: MediaKind) -> bool;
}
