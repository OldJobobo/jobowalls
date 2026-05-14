use crate::media::{MediaKind, classify_path, has_supported_extension};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CollectionRegistry {
    pub collections: BTreeMap<String, NamedCollection>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NamedCollection {
    pub name: String,
    pub source_folder: Option<PathBuf>,
    pub items: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CollectionSummary {
    pub id: String,
    pub name: String,
    pub count: usize,
    pub source_folder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CollectionDetail {
    pub id: String,
    pub name: String,
    pub count: usize,
    pub source_folder: Option<String>,
    pub items: Vec<CollectionWallpaper>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CollectionWallpaper {
    pub path: String,
    pub name: String,
    pub kind: MediaKind,
}

impl CollectionRegistry {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read collections {}", path.display()))?;
        toml::from_str(&raw)
            .with_context(|| format!("failed to parse collections {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create collections dir {}", parent.display())
            })?;
        }

        let raw = toml::to_string_pretty(self).context("failed to serialize collections")?;
        fs::write(path, raw)
            .with_context(|| format!("failed to write collections {}", path.display()))
    }

    pub fn list(&self) -> Result<Vec<CollectionSummary>> {
        self.collections
            .iter()
            .map(|(id, collection)| collection.summary(id))
            .collect()
    }

    pub fn detail(&self, id: &str) -> Result<CollectionDetail> {
        let collection = self
            .collections
            .get(id)
            .with_context(|| format!("collection not found: {id}"))?;
        collection.detail(id)
    }

    pub fn create(
        &mut self,
        name: &str,
        requested_id: Option<&str>,
        source_folder: Option<PathBuf>,
    ) -> Result<CollectionSummary> {
        let id = requested_id
            .map(validate_id)
            .transpose()?
            .unwrap_or_else(|| slugify(name));
        if id.is_empty() {
            bail!("collection name must contain at least one letter or number");
        }
        if self.collections.contains_key(&id) {
            bail!("collection already exists: {id}");
        }

        let source_folder = source_folder.map(canonicalize_dir).transpose()?;
        self.collections.insert(
            id.clone(),
            NamedCollection {
                name: name.trim().to_string(),
                source_folder,
                items: Vec::new(),
            },
        );

        self.collections[&id].summary(&id)
    }

    pub fn delete(&mut self, id: &str) -> Result<CollectionSummary> {
        let collection = self
            .collections
            .remove(id)
            .with_context(|| format!("collection not found: {id}"))?;
        collection.summary(id)
    }

    pub fn add_items(&mut self, id: &str, paths: &[PathBuf]) -> Result<CollectionDetail> {
        let collection = self
            .collections
            .get_mut(id)
            .with_context(|| format!("collection not found: {id}"))?;

        for path in paths {
            let path = canonicalize_wallpaper(path)?;
            if !collection.items.iter().any(|item| item == &path) {
                collection.items.push(path);
            }
        }
        collection.items.sort();
        collection.detail(id)
    }

    pub fn remove_item(&mut self, id: &str, path: &Path) -> Result<CollectionDetail> {
        let collection = self
            .collections
            .get_mut(id)
            .with_context(|| format!("collection not found: {id}"))?;
        let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        collection.items.retain(|item| item != &path);
        collection.detail(id)
    }
}

impl NamedCollection {
    fn summary(&self, id: &str) -> Result<CollectionSummary> {
        Ok(CollectionSummary {
            id: id.to_string(),
            name: self.display_name(id),
            count: self.wallpaper_paths()?.len(),
            source_folder: self
                .source_folder
                .as_ref()
                .map(|path| path.display().to_string()),
        })
    }

    fn detail(&self, id: &str) -> Result<CollectionDetail> {
        let mut items = self
            .wallpaper_paths()?
            .into_iter()
            .filter_map(|path| wallpaper_item(&path))
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        items.dedup_by(|a, b| a.path == b.path);

        Ok(CollectionDetail {
            id: id.to_string(),
            name: self.display_name(id),
            count: items.len(),
            source_folder: self
                .source_folder
                .as_ref()
                .map(|path| path.display().to_string()),
            items,
        })
    }

    fn display_name(&self, id: &str) -> String {
        if self.name.trim().is_empty() {
            id.to_string()
        } else {
            self.name.clone()
        }
    }

    fn wallpaper_paths(&self) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();

        if let Some(folder) = &self.source_folder {
            paths.extend(scan_folder_allow_empty(folder)?);
        }
        paths.extend(self.items.iter().cloned().filter(|path| {
            path.is_file() && has_supported_extension(path) && classify_path(path).is_ok()
        }));

        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

fn canonicalize_dir(path: PathBuf) -> Result<PathBuf> {
    let path = fs::canonicalize(&path)
        .with_context(|| format!("failed to resolve folder {}", path.display()))?;
    if !path.is_dir() {
        bail!("collection source is not a folder: {}", path.display());
    }
    Ok(path)
}

fn canonicalize_wallpaper(path: &Path) -> Result<PathBuf> {
    let path = fs::canonicalize(path)
        .with_context(|| format!("failed to resolve wallpaper {}", path.display()))?;
    if !path.is_file() {
        bail!("collection item is not a file: {}", path.display());
    }
    if !has_supported_extension(&path) || classify_path(&path).is_err() {
        bail!("unsupported wallpaper type: {}", path.display());
    }
    Ok(path)
}

fn scan_folder_allow_empty(path: &Path) -> Result<Vec<PathBuf>> {
    if !path.is_dir() {
        bail!("collection source is not a folder: {}", path.display());
    }

    let mut wallpapers = Vec::new();
    for entry in
        fs::read_dir(path).with_context(|| format!("failed to read folder {}", path.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in folder {}", path.display()))?;
        let path = entry.path();
        if path.is_file() && has_supported_extension(&path) && classify_path(&path).is_ok() {
            wallpapers.push(fs::canonicalize(&path).unwrap_or(path));
        }
    }

    wallpapers.sort();
    wallpapers.dedup();
    Ok(wallpapers)
}

fn wallpaper_item(path: &Path) -> Option<CollectionWallpaper> {
    let kind = classify_path(path).ok()?;
    let name = path.file_name()?.to_str()?.to_string();
    Some(CollectionWallpaper {
        path: path.display().to_string(),
        name,
        kind,
    })
}

fn validate_id(id: &str) -> Result<String> {
    let id = id.trim();
    if id.is_empty() {
        bail!("collection id cannot be empty");
    }
    if !id
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        bail!("collection id may only contain lowercase letters, numbers, and hyphens");
    }
    Ok(id.to_string())
}

fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;

    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() {
            slug.push(byte.to_ascii_lowercase() as char);
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

pub fn default_collections_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("jobowalls")
        .join("collections.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn creates_collection_with_slug_id() {
        let mut registry = CollectionRegistry::default();

        let summary = registry
            .create("Favorite Walls", None, None)
            .expect("collection");

        assert_eq!(summary.id, "favorite-walls");
        assert_eq!(summary.name, "Favorite Walls");
        assert_eq!(summary.count, 0);
    }

    #[test]
    fn adds_supported_wallpapers_once() {
        let dir = tempdir().unwrap();
        let wallpaper = dir.path().join("wall.png");
        fs::write(&wallpaper, "").unwrap();
        let mut registry = CollectionRegistry::default();
        registry.create("Favorites", None, None).unwrap();

        let detail = registry
            .add_items("favorites", &[wallpaper.clone(), wallpaper])
            .unwrap();

        assert_eq!(detail.items.len(), 1);
        assert_eq!(registry.collections["favorites"].items.len(), 1);
    }

    #[test]
    fn rejects_unsupported_wallpaper() {
        let dir = tempdir().unwrap();
        let note = dir.path().join("note.txt");
        fs::write(&note, "").unwrap();
        let mut registry = CollectionRegistry::default();
        registry.create("Favorites", None, None).unwrap();

        let error = registry.add_items("favorites", &[note]).unwrap_err();

        assert!(error.to_string().contains("unsupported wallpaper type"));
    }

    #[test]
    fn removes_wallpaper_without_deleting_file() {
        let dir = tempdir().unwrap();
        let wallpaper = dir.path().join("wall.jpg");
        fs::write(&wallpaper, "").unwrap();
        let mut registry = CollectionRegistry::default();
        registry.create("Favorites", None, None).unwrap();
        registry
            .add_items("favorites", std::slice::from_ref(&wallpaper))
            .unwrap();

        let detail = registry.remove_item("favorites", &wallpaper).unwrap();

        assert!(detail.items.is_empty());
        assert!(wallpaper.exists());
    }

    #[test]
    fn persists_registry_as_toml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("collections.toml");
        let wallpaper = dir.path().join("wall.mp4");
        fs::write(&wallpaper, "").unwrap();
        let mut registry = CollectionRegistry::default();
        registry.create("Live", None, None).unwrap();
        registry.add_items("live", &[wallpaper]).unwrap();

        registry.save(&path).unwrap();
        let loaded = CollectionRegistry::load(&path).unwrap();

        assert_eq!(loaded.collections["live"].items.len(), 1);
    }
}
