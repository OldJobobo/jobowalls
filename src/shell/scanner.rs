use crate::{
    media::{classify_path, has_supported_extension},
    shell::{model::WallpaperItem, state::ShellState},
};
use anyhow::{Context, Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn resolve_folder(explicit: Option<PathBuf>, state: Option<&ShellState>) -> Result<PathBuf> {
    let candidates = explicit
        .into_iter()
        .chain(state.and_then(|state| state.last_folder.clone()))
        .chain(omarchy_backgrounds_dir())
        .chain(fallback_wallpaper_dir());

    for path in candidates {
        let expanded = expand_home(path);
        if expanded.is_dir() {
            return Ok(expanded);
        }
    }

    bail!(
        "no wallpaper folder found; pass a folder, create ~/.config/omarchy/current/theme/backgrounds, or create ~/Pictures/Wallpapers"
    );
}

pub fn scan_folder(folder: &Path) -> Result<Vec<WallpaperItem>> {
    let entries =
        fs::read_dir(folder).with_context(|| format!("failed to read {}", folder.display()))?;
    let mut items = Vec::new();

    for entry in entries {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", folder.display()))?;
        let path = entry.path();
        if !path.is_file() || !has_supported_extension(&path) {
            continue;
        }
        let kind = classify_path(&path)?;
        items.push(WallpaperItem { path, kind });
    }

    items.sort_by_key(|item| {
        item.path
            .file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    });
    Ok(items)
}

fn omarchy_backgrounds_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| {
        home.join(".config")
            .join("omarchy")
            .join("current")
            .join("theme")
            .join("backgrounds")
    })
}

fn fallback_wallpaper_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join("Pictures").join("Wallpapers"))
}

fn expand_home(path: PathBuf) -> PathBuf {
    let Some(raw) = path.to_str() else {
        return path;
    };
    if raw == "~" {
        return dirs::home_dir().unwrap_or(path);
    }
    if let Some(rest) = raw.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_supported_wallpapers_sorted() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("b.mp4"), b"").unwrap();
        fs::write(dir.path().join("a.png"), b"").unwrap();
        fs::write(dir.path().join("notes.txt"), b"").unwrap();

        let items = scan_folder(dir.path()).unwrap();
        let names: Vec<_> = items.iter().map(WallpaperItem::display_name).collect();
        assert_eq!(names, ["a.png", "b.mp4"]);
    }
}
