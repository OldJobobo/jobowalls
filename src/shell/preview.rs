use crate::shell::model::WallpaperItem;
use anyhow::{Context, Result, bail};
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewProfile {
    pub static_width: u32,
    pub animated_width: u32,
    pub animated_fps: u32,
    pub animated_duration_secs: u32,
}

impl Default for PreviewProfile {
    fn default() -> Self {
        Self {
            static_width: 480,
            animated_width: 480,
            animated_fps: 10,
            animated_duration_secs: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewJob {
    pub source: PathBuf,
    pub output: PathBuf,
    pub kind: PreviewKind,
    pub profile: PreviewProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    Poster,
    Animated,
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".cache")
        })
        .join("jobowalls")
        .join("gui-thumbnails")
}

pub fn poster_path(source: &Path, profile: PreviewProfile) -> PathBuf {
    cache_dir().join(format!(
        "{}-shell-poster-v1.jpg",
        cache_key(source, profile)
    ))
}

pub fn animated_path(source: &Path, profile: PreviewProfile) -> PathBuf {
    cache_dir().join(format!(
        "{}-shell-selected-live-v1.webp",
        cache_key(source, profile)
    ))
}

pub fn prioritized_jobs(
    items: &[WallpaperItem],
    selected: usize,
    profile: PreviewProfile,
    animate_live: bool,
) -> Vec<PreviewJob> {
    if items.is_empty() {
        return Vec::new();
    }

    let mut jobs = Vec::new();
    let selected_item = &items[selected % items.len()];
    jobs.push(PreviewJob {
        source: selected_item.path.clone(),
        output: poster_path(&selected_item.path, profile),
        kind: PreviewKind::Poster,
        profile,
    });
    if animate_live && selected_item.is_live() {
        jobs.push(PreviewJob {
            source: selected_item.path.clone(),
            output: animated_path(&selected_item.path, profile),
            kind: PreviewKind::Animated,
            profile,
        });
    }

    for index in nearby_indexes(items.len(), selected, 3) {
        let item = &items[index];
        jobs.push(PreviewJob {
            source: item.path.clone(),
            output: poster_path(&item.path, profile),
            kind: PreviewKind::Poster,
            profile,
        });
    }

    if animate_live {
        for index in nearby_indexes(items.len(), selected, 3) {
            let item = &items[index];
            if item.is_live() {
                jobs.push(PreviewJob {
                    source: item.path.clone(),
                    output: animated_path(&item.path, profile),
                    kind: PreviewKind::Animated,
                    profile,
                });
            }
        }
    }

    jobs
}

pub fn display_path(item: &WallpaperItem, selected: bool, animate_live: bool) -> Option<PathBuf> {
    let profile = PreviewProfile::default();
    if selected && animate_live && item.is_live() {
        let animated = animated_path(&item.path, profile);
        if animated.exists() {
            return Some(animated);
        }
    }

    let poster = poster_path(&item.path, profile);
    if poster.exists() {
        return Some(poster);
    }

    None
}

pub fn generate(job: &PreviewJob) -> Result<()> {
    if job.output.exists() {
        return Ok(());
    }

    if let Some(parent) = job.output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create preview cache {}", parent.display()))?;
    }

    let mut command = Command::new("ffmpeg");
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(&job.source);

    match job.kind {
        PreviewKind::Poster => {
            command.arg("-frames:v").arg("1").arg("-vf").arg(format!(
                "scale={}:-2:force_original_aspect_ratio=decrease",
                job.profile.static_width
            ));
        }
        PreviewKind::Animated => {
            command
                .arg("-t")
                .arg(job.profile.animated_duration_secs.to_string())
                .arg("-vf")
                .arg(format!(
                    "fps={},scale={}:-2:force_original_aspect_ratio=decrease",
                    job.profile.animated_fps, job.profile.animated_width
                ))
                .arg("-loop")
                .arg("0");
        }
    }

    let output = command
        .arg(&job.output)
        .output()
        .with_context(|| "failed to run ffmpeg for wallpaper preview")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("ffmpeg preview generation failed: {}", stderr.trim());
    }

    Ok(())
}

fn nearby_indexes(len: usize, selected: usize, radius: usize) -> Vec<usize> {
    if len < 2 || radius == 0 {
        return Vec::new();
    }

    let selected = selected % len;
    let mut indexes = Vec::new();
    for distance in 1..=radius.min(len - 1) {
        for index in [
            (selected + len - distance) % len,
            (selected + distance) % len,
        ] {
            if !indexes.contains(&index) {
                indexes.push(index);
            }
        }
    }

    indexes
}

fn cache_key(source: &Path, profile: PreviewProfile) -> String {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    if let Ok(metadata) = source.metadata() {
        metadata.len().hash(&mut hasher);
        if let Ok(modified) = metadata.modified() {
            modified.hash(&mut hasher);
        }
    }
    profile.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

impl Hash for PreviewProfile {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.static_width.hash(state);
        self.animated_width.hash(state);
        self.animated_fps.hash(state);
        self.animated_duration_secs.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{media::MediaKind, shell::model::WallpaperItem};

    #[test]
    fn queues_only_static_posters_when_animation_is_disabled() {
        let items = vec![
            WallpaperItem {
                path: "/tmp/a.mp4".into(),
                kind: MediaKind::Live,
            },
            WallpaperItem {
                path: "/tmp/b.mp4".into(),
                kind: MediaKind::Live,
            },
            WallpaperItem {
                path: "/tmp/c.png".into(),
                kind: MediaKind::Static,
            },
        ];

        let jobs = prioritized_jobs(&items, 0, PreviewProfile::default(), false);
        assert!(jobs.iter().all(|job| job.kind == PreviewKind::Poster));
    }

    #[test]
    fn queues_full_visible_carousel_for_prewarming() {
        let items: Vec<_> = (0..8)
            .map(|index| WallpaperItem {
                path: format!("/tmp/{index}.png").into(),
                kind: MediaKind::Static,
            })
            .collect();

        let jobs = prioritized_jobs(&items, 0, PreviewProfile::default(), false);
        let sources: Vec<_> = jobs.iter().map(|job| job.source.as_path()).collect();

        assert_eq!(jobs.len(), 7);
        assert!(sources.contains(&Path::new("/tmp/0.png")));
        assert!(sources.contains(&Path::new("/tmp/1.png")));
        assert!(sources.contains(&Path::new("/tmp/2.png")));
        assert!(sources.contains(&Path::new("/tmp/3.png")));
        assert!(sources.contains(&Path::new("/tmp/5.png")));
        assert!(sources.contains(&Path::new("/tmp/6.png")));
        assert!(sources.contains(&Path::new("/tmp/7.png")));
    }
}
