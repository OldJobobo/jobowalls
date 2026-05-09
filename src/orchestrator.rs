use crate::{
    backends::{
        awww::AwwwBackend,
        hyprpaper::HyprpaperBackend,
        model::{Backend, BackendOverride, WallpaperBackend},
        mpvpaper::MpvpaperBackend,
        swaybg::SwaybgBackend,
    },
    config::Config,
    media::{MediaKind, classify_path},
    state::State,
};
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetPlan {
    pub wallpaper: PathBuf,
    pub media_kind: MediaKind,
    pub backend: Backend,
    pub monitor: String,
}

impl SetPlan {
    pub fn for_monitor(&self, monitor: impl Into<String>) -> Self {
        Self {
            wallpaper: self.wallpaper.clone(),
            media_kind: self.media_kind,
            backend: self.backend,
            monitor: monitor.into(),
        }
    }

    pub fn from_state(state: &State) -> Result<Self> {
        let monitor = state
            .monitors
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "all".to_string());

        Ok(Self {
            wallpaper: PathBuf::from(&state.wallpaper),
            media_kind: state.mode,
            backend: state.active_backend,
            monitor,
        })
    }
}

pub fn plan_set(
    config: &Config,
    wallpaper: &Path,
    monitor: Option<String>,
    backend_override: BackendOverride,
) -> Result<SetPlan> {
    let media_kind = classify_path(wallpaper)?;
    let backend = match backend_override {
        BackendOverride::Auto => match media_kind {
            MediaKind::Static => config
                .configured_static_backend()
                .unwrap_or(Backend::Hyprpaper),
            MediaKind::Live => config.general.live_backend,
        },
        BackendOverride::Backend(backend) => {
            validate_backend_for_media(backend, media_kind)?;
            backend
        }
    };

    Ok(SetPlan {
        wallpaper: wallpaper.to_path_buf(),
        media_kind,
        backend,
        monitor: monitor.unwrap_or_else(|| config.monitors.default.clone()),
    })
}

fn validate_backend_for_media(backend: Backend, media_kind: MediaKind) -> Result<()> {
    let adapter = backend_adapter(backend);
    if adapter.supports(media_kind) {
        Ok(())
    } else {
        bail!("{backend} cannot handle {media_kind:?} wallpapers")
    }
}

fn backend_adapter(backend: Backend) -> &'static dyn WallpaperBackend {
    match backend {
        Backend::Hyprpaper => &HyprpaperBackend,
        Backend::Mpvpaper => &MpvpaperBackend,
        Backend::Awww => &AwwwBackend,
        Backend::Swaybg => &SwaybgBackend,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_static_wallpaper_with_hyprpaper_by_default() {
        let plan = plan_set(
            &Config::default(),
            Path::new("/tmp/wall.png"),
            None,
            BackendOverride::Auto,
        )
        .unwrap();

        assert_eq!(plan.media_kind, MediaKind::Static);
        assert_eq!(plan.backend, Backend::Hyprpaper);
        assert_eq!(plan.monitor, "all");
    }

    #[test]
    fn plans_live_wallpaper_with_mpvpaper_by_default() {
        let plan = plan_set(
            &Config::default(),
            Path::new("/tmp/rain.mp4"),
            Some("DP-1".to_string()),
            BackendOverride::Auto,
        )
        .unwrap();

        assert_eq!(plan.media_kind, MediaKind::Live);
        assert_eq!(plan.backend, Backend::Mpvpaper);
        assert_eq!(plan.monitor, "DP-1");
    }

    #[test]
    fn rejects_invalid_backend_override() {
        let result = plan_set(
            &Config::default(),
            Path::new("/tmp/rain.mp4"),
            None,
            BackendOverride::Backend(Backend::Hyprpaper),
        );

        assert!(result.is_err());
    }

    #[test]
    fn rebuilds_plan_from_state() {
        let state = State::from_set_plan(
            &SetPlan {
                wallpaper: PathBuf::from("/tmp/wall.png"),
                media_kind: MediaKind::Static,
                backend: Backend::Awww,
                monitor: "DP-1".to_string(),
            },
            None,
        );

        let plan = SetPlan::from_state(&state).unwrap();

        assert_eq!(plan.wallpaper, PathBuf::from("/tmp/wall.png"));
        assert_eq!(plan.media_kind, MediaKind::Static);
        assert_eq!(plan.backend, Backend::Awww);
        assert_eq!(plan.monitor, "DP-1");
    }

    #[test]
    fn clones_plan_for_monitor() {
        let plan = SetPlan {
            wallpaper: PathBuf::from("/tmp/wall.png"),
            media_kind: MediaKind::Static,
            backend: Backend::Awww,
            monitor: "all".to_string(),
        };

        let monitor_plan = plan.for_monitor("DP-3");

        assert_eq!(monitor_plan.wallpaper, plan.wallpaper);
        assert_eq!(monitor_plan.media_kind, plan.media_kind);
        assert_eq!(monitor_plan.backend, plan.backend);
        assert_eq!(monitor_plan.monitor, "DP-3");
    }
}
