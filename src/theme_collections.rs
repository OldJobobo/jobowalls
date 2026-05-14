use crate::{
    collections_store::CollectionWallpaper,
    config::{ThemeCollectionAddTargetConfig, ThemeCollectionsConfig},
    media::{classify_path, has_supported_extension},
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeCollectionSource {
    UserTheme,
    StockTheme,
    AuthorTheme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeCollectionAddTarget {
    UserBackgrounds,
    ThemeRepo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThemeCollectionSummary {
    pub id: String,
    pub name: String,
    pub theme_name: String,
    pub count: usize,
    pub source: ThemeCollectionSource,
    pub installed_path: String,
    pub real_path: String,
    pub theme_backgrounds_path: String,
    pub user_backgrounds_path: String,
    pub can_write_user_backgrounds: bool,
    pub can_write_theme_repo: bool,
    pub add_requires_choice: bool,
    pub default_add_target: ThemeCollectionAddTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThemeCollectionDetail {
    #[serde(flatten)]
    pub summary: ThemeCollectionSummary,
    pub items: Vec<CollectionWallpaper>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThemeCollectionImport {
    pub collection: ThemeCollectionDetail,
    pub copied_path: String,
    pub target: ThemeCollectionAddTarget,
}

#[derive(Debug, Clone)]
struct ThemeCollection {
    id: String,
    name: String,
    theme_name: String,
    source: ThemeCollectionSource,
    installed_path: PathBuf,
    real_path: PathBuf,
    theme_backgrounds_path: PathBuf,
    user_backgrounds_path: PathBuf,
    can_write_user_backgrounds: bool,
    can_write_theme_repo: bool,
    add_requires_choice: bool,
    default_add_target: ThemeCollectionAddTarget,
}

pub fn list_theme_collections(
    config: &ThemeCollectionsConfig,
) -> Result<Vec<ThemeCollectionSummary>> {
    Ok(discover_theme_collections(config)?
        .into_values()
        .map(|collection| collection.summary())
        .collect())
}

pub fn get_theme_collection(
    config: &ThemeCollectionsConfig,
    id: &str,
) -> Result<ThemeCollectionDetail> {
    let collections = discover_theme_collections(config)?;
    let collection = collections
        .get(id)
        .or_else(|| collections.get(&normalize_theme_id(id)))
        .with_context(|| format!("theme collection not found: {id}"))?;
    collection.detail()
}

pub fn add_to_theme_collection(
    config: &ThemeCollectionsConfig,
    id: &str,
    source: &Path,
    target: Option<ThemeCollectionAddTarget>,
) -> Result<ThemeCollectionImport> {
    let collections = discover_theme_collections(config)?;
    let collection = collections
        .get(id)
        .or_else(|| collections.get(&normalize_theme_id(id)))
        .with_context(|| format!("theme collection not found: {id}"))?;
    let source = canonicalize_wallpaper(source)?;
    let target = target.unwrap_or(collection.default_add_target);
    let target_dir = collection.target_dir(target)?;
    fs::create_dir_all(&target_dir)
        .with_context(|| format!("failed to create {}", target_dir.display()))?;
    let destination = non_colliding_destination(&target_dir, &source)?;
    fs::copy(&source, &destination).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;

    Ok(ThemeCollectionImport {
        collection: collection.detail()?,
        copied_path: destination.display().to_string(),
        target,
    })
}

fn discover_theme_collections(
    config: &ThemeCollectionsConfig,
) -> Result<BTreeMap<String, ThemeCollection>> {
    let mut collections = BTreeMap::new();
    if !config.enabled {
        return Ok(collections);
    }

    let author_roots = config
        .author
        .theme_roots
        .iter()
        .map(|path| expand_tilde(path))
        .map(|path| fs::canonicalize(&path).unwrap_or(path))
        .collect::<Vec<_>>();
    let user_themes_dir = expand_tilde(&config.user_themes_dir);
    let stock_themes_dir = expand_tilde(&config.stock_themes_dir);
    let user_backgrounds_dir = expand_tilde(&config.user_backgrounds_dir);

    scan_theme_root(
        &mut collections,
        &user_themes_dir,
        &user_backgrounds_dir,
        ThemeCollectionSource::UserTheme,
        config,
        &author_roots,
    )?;

    if config.include_stock_themes {
        scan_theme_root(
            &mut collections,
            &stock_themes_dir,
            &user_backgrounds_dir,
            ThemeCollectionSource::StockTheme,
            config,
            &author_roots,
        )?;
    }

    Ok(collections)
}

fn scan_theme_root(
    collections: &mut BTreeMap<String, ThemeCollection>,
    root: &Path,
    user_backgrounds_dir: &Path,
    source: ThemeCollectionSource,
    config: &ThemeCollectionsConfig,
    author_roots: &[PathBuf],
) -> Result<()> {
    let Ok(entries) = fs::read_dir(root) else {
        return Ok(());
    };

    for entry in entries {
        let entry = entry.with_context(|| format!("failed to read entry in {}", root.display()))?;
        let installed_path = entry.path();
        let Some(theme_name) = installed_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if should_ignore_theme(theme_name) {
            continue;
        }
        let Ok(metadata) = fs::metadata(&installed_path) else {
            continue;
        };
        if !metadata.is_dir() {
            continue;
        }

        let real_path = fs::canonicalize(&installed_path).unwrap_or(installed_path.clone());
        let theme_backgrounds_path = real_path.join("backgrounds");
        let user_backgrounds_path = user_backgrounds_dir.join(theme_name);
        if !theme_backgrounds_path.is_dir() && !user_backgrounds_path.is_dir() {
            continue;
        }

        let id = normalize_theme_id(theme_name);
        if collections.contains_key(&id) {
            continue;
        }

        let author_capable =
            config.author.enabled && author_roots.iter().any(|root| real_path.starts_with(root));
        let source = if author_capable {
            ThemeCollectionSource::AuthorTheme
        } else {
            source
        };
        let can_write_theme_repo =
            author_capable && source != ThemeCollectionSource::StockTheme && real_path.is_dir();
        let add_requires_choice =
            config.add_target == ThemeCollectionAddTargetConfig::Ask && can_write_theme_repo;
        let default_add_target = default_add_target(config.add_target, can_write_theme_repo);

        collections.insert(
            id.clone(),
            ThemeCollection {
                id,
                name: theme_name.to_string(),
                theme_name: theme_name.to_string(),
                source,
                installed_path,
                real_path: real_path.clone(),
                theme_backgrounds_path,
                user_backgrounds_path,
                can_write_user_backgrounds: true,
                can_write_theme_repo,
                add_requires_choice,
                default_add_target,
            },
        );
    }

    Ok(())
}

impl ThemeCollection {
    fn summary(&self) -> ThemeCollectionSummary {
        ThemeCollectionSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            theme_name: self.theme_name.clone(),
            count: self.wallpaper_paths().len(),
            source: self.source,
            installed_path: self.installed_path.display().to_string(),
            real_path: self.real_path.display().to_string(),
            theme_backgrounds_path: self.theme_backgrounds_path.display().to_string(),
            user_backgrounds_path: self.user_backgrounds_path.display().to_string(),
            can_write_user_backgrounds: self.can_write_user_backgrounds,
            can_write_theme_repo: self.can_write_theme_repo,
            add_requires_choice: self.add_requires_choice,
            default_add_target: self.default_add_target,
        }
    }

    fn detail(&self) -> Result<ThemeCollectionDetail> {
        let mut items = self
            .wallpaper_paths()
            .into_iter()
            .filter_map(|path| wallpaper_item(&path))
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        items.dedup_by(|a, b| a.path == b.path);

        Ok(ThemeCollectionDetail {
            summary: self.summary(),
            items,
        })
    }

    fn wallpaper_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        paths.extend(scan_folder_allow_empty(&self.user_backgrounds_path));
        paths.extend(scan_folder_allow_empty(&self.theme_backgrounds_path));
        paths.sort();
        paths.dedup();
        paths
    }

    fn target_dir(&self, target: ThemeCollectionAddTarget) -> Result<PathBuf> {
        match target {
            ThemeCollectionAddTarget::UserBackgrounds => {
                if self.can_write_user_backgrounds {
                    Ok(self.user_backgrounds_path.clone())
                } else {
                    bail!(
                        "theme collection cannot write user backgrounds: {}",
                        self.id
                    )
                }
            }
            ThemeCollectionAddTarget::ThemeRepo => {
                if self.can_write_theme_repo {
                    Ok(self.theme_backgrounds_path.clone())
                } else {
                    bail!("theme repo writes are not enabled for {}", self.id)
                }
            }
        }
    }
}

fn default_add_target(
    configured: ThemeCollectionAddTargetConfig,
    can_write_theme_repo: bool,
) -> ThemeCollectionAddTarget {
    match configured {
        ThemeCollectionAddTargetConfig::ThemeRepo if can_write_theme_repo => {
            ThemeCollectionAddTarget::ThemeRepo
        }
        ThemeCollectionAddTargetConfig::UserBackgrounds
        | ThemeCollectionAddTargetConfig::ThemeRepo
        | ThemeCollectionAddTargetConfig::Ask => ThemeCollectionAddTarget::UserBackgrounds,
    }
}

fn normalize_theme_id(value: &str) -> String {
    if value.starts_with("theme:") {
        value.to_string()
    } else {
        format!("theme:{value}")
    }
}

fn should_ignore_theme(name: &str) -> bool {
    name.starts_with('.')
        || name == "current"
        || name.ends_with(".bak")
        || name.contains(".backup-")
        || name.contains(".disabled.")
}

fn expand_tilde(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if raw == "~" {
        return dirs::home_dir().unwrap_or_else(|| path.to_path_buf());
    }
    if let Some(rest) = raw.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    path.to_path_buf()
}

fn scan_folder_allow_empty(path: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(path) else {
        return Vec::new();
    };
    let mut wallpapers = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && has_supported_extension(&path) && classify_path(&path).is_ok() {
            wallpapers.push(fs::canonicalize(&path).unwrap_or(path));
        }
    }
    wallpapers
}

fn canonicalize_wallpaper(path: &Path) -> Result<PathBuf> {
    let path = fs::canonicalize(path)
        .with_context(|| format!("failed to resolve wallpaper {}", path.display()))?;
    if !path.is_file() {
        bail!("theme collection item is not a file: {}", path.display());
    }
    if !has_supported_extension(&path) || classify_path(&path).is_err() {
        bail!("unsupported wallpaper type: {}", path.display());
    }
    Ok(path)
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

fn non_colliding_destination(target_dir: &Path, source: &Path) -> Result<PathBuf> {
    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("wallpaper");
    let extension = source.extension().and_then(|extension| extension.to_str());

    for index in 0..10_000 {
        let name = if index == 0 {
            file_name(stem, extension)
        } else {
            file_name(&format!("{stem}-{index}"), extension)
        };
        let candidate = target_dir.join(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    bail!(
        "failed to find available filename in {}",
        target_dir.display()
    )
}

fn file_name(stem: &str, extension: Option<&str>) -> String {
    match extension {
        Some(extension) if !extension.is_empty() => format!("{stem}.{extension}"),
        _ => stem.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ThemeCollectionsAuthorConfig;
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    fn write_png(path: &Path) {
        fs::write(path, b"\x89PNG\r\n\x1a\nrest").unwrap();
    }

    fn config(root: &Path) -> ThemeCollectionsConfig {
        ThemeCollectionsConfig {
            enabled: true,
            include_stock_themes: false,
            user_themes_dir: root.join("themes"),
            stock_themes_dir: root.join("stock"),
            user_backgrounds_dir: root.join("backgrounds"),
            add_target: ThemeCollectionAddTargetConfig::UserBackgrounds,
            author: ThemeCollectionsAuthorConfig {
                enabled: false,
                theme_roots: vec![root.join("repos")],
            },
        }
    }

    #[test]
    fn disabled_config_returns_no_theme_collections() {
        let root = tempdir().unwrap();
        let mut config = config(root.path());
        config.enabled = false;

        assert!(list_theme_collections(&config).unwrap().is_empty());
    }

    #[test]
    fn discovers_user_theme_backgrounds_and_user_backgrounds() {
        let root = tempdir().unwrap();
        let config = config(root.path());
        let theme_backgrounds = root.path().join("themes/dune/backgrounds");
        let user_backgrounds = root.path().join("backgrounds/dune");
        fs::create_dir_all(&theme_backgrounds).unwrap();
        fs::create_dir_all(&user_backgrounds).unwrap();
        write_png(&theme_backgrounds.join("theme.png"));
        write_png(&user_backgrounds.join("user.png"));

        let detail = get_theme_collection(&config, "theme:dune").unwrap();

        assert_eq!(detail.summary.count, 2);
        assert_eq!(detail.items.len(), 2);
    }

    #[test]
    fn skips_stock_themes_unless_enabled() {
        let root = tempdir().unwrap();
        let mut config = config(root.path());
        let stock_backgrounds = root.path().join("stock/catppuccin/backgrounds");
        fs::create_dir_all(&stock_backgrounds).unwrap();
        write_png(&stock_backgrounds.join("wall.png"));

        assert!(list_theme_collections(&config).unwrap().is_empty());

        config.include_stock_themes = true;
        assert_eq!(list_theme_collections(&config).unwrap().len(), 1);
    }

    #[test]
    fn normal_add_copies_to_user_backgrounds_without_overwrite() {
        let root = tempdir().unwrap();
        let config = config(root.path());
        let theme_backgrounds = root.path().join("themes/dune/backgrounds");
        let user_backgrounds = root.path().join("backgrounds/dune");
        fs::create_dir_all(&theme_backgrounds).unwrap();
        fs::create_dir_all(&user_backgrounds).unwrap();
        write_png(&theme_backgrounds.join("theme.png"));
        write_png(&user_backgrounds.join("wall.png"));
        let source = root.path().join("wall.png");
        write_png(&source);

        let import = add_to_theme_collection(
            &config,
            "theme:dune",
            &source,
            Some(ThemeCollectionAddTarget::UserBackgrounds),
        )
        .unwrap();

        assert!(import.copied_path.ends_with("wall-1.png"));
        assert!(Path::new(&import.copied_path).exists());
    }

    #[test]
    fn author_add_copies_to_resolved_repo_backgrounds_when_enabled() {
        let root = tempdir().unwrap();
        let repo_backgrounds = root.path().join("repos/omarchy-dune-theme/backgrounds");
        let installed_parent = root.path().join("themes");
        fs::create_dir_all(&repo_backgrounds).unwrap();
        fs::create_dir_all(&installed_parent).unwrap();
        symlink(
            root.path().join("repos/omarchy-dune-theme"),
            installed_parent.join("dune"),
        )
        .unwrap();
        write_png(&repo_backgrounds.join("theme.png"));
        let source = root.path().join("new.png");
        write_png(&source);
        let mut config = config(root.path());
        config.author.enabled = true;
        config.add_target = ThemeCollectionAddTargetConfig::ThemeRepo;

        let import = add_to_theme_collection(
            &config,
            "theme:dune",
            &source,
            Some(ThemeCollectionAddTarget::ThemeRepo),
        )
        .unwrap();

        assert!(
            import
                .copied_path
                .contains("repos/omarchy-dune-theme/backgrounds/new.png")
        );
    }

    #[test]
    fn rejects_theme_repo_add_without_author_mode() {
        let root = tempdir().unwrap();
        let config = config(root.path());
        let theme_backgrounds = root.path().join("themes/dune/backgrounds");
        fs::create_dir_all(&theme_backgrounds).unwrap();
        write_png(&theme_backgrounds.join("theme.png"));
        let source = root.path().join("new.png");
        write_png(&source);

        let error = add_to_theme_collection(
            &config,
            "theme:dune",
            &source,
            Some(ThemeCollectionAddTarget::ThemeRepo),
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("theme repo writes are not enabled")
        );
    }
}
