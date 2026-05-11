use crate::media::MediaKind;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Backend {
    Mpvpaper,
    Awww,
    Swaybg,
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Backend::Mpvpaper => write!(f, "mpvpaper"),
            Backend::Awww => write!(f, "awww"),
            Backend::Swaybg => write!(f, "swaybg"),
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
