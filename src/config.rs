use crate::backends::model::Backend;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum StaticBackendPreference {
    #[default]
    Auto,
    Hyprpaper,
    Awww,
    Swaybg,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub monitors: MonitorConfig,
    pub live: LiveConfig,
    pub hyprpaper: HyprpaperConfig,
    pub mpvpaper: MpvpaperConfig,
    pub awww: AwwwConfig,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("failed to parse config {}", path.display()))
    }

    pub fn save(&self, path: &Path, force: bool) -> Result<()> {
        if path.exists() && !force {
            anyhow::bail!("config already exists: {}", path.display());
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create config dir {}", parent.display()))?;
        }

        let raw = toml::to_string_pretty(self).context("failed to serialize config")?;
        fs::write(path, raw).with_context(|| format!("failed to write config {}", path.display()))
    }

    pub fn to_toml_string(&self) -> Result<String> {
        toml::to_string_pretty(self).context("failed to serialize config")
    }

    pub fn configured_static_backend(&self) -> Option<Backend> {
        match self.general.static_backend {
            StaticBackendPreference::Auto => {
                if self.awww.enabled {
                    Some(Backend::Awww)
                } else {
                    Some(Backend::Hyprpaper)
                }
            }
            StaticBackendPreference::Hyprpaper => Some(Backend::Hyprpaper),
            StaticBackendPreference::Awww => Some(Backend::Awww),
            StaticBackendPreference::Swaybg => Some(Backend::Swaybg),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub static_backend: StaticBackendPreference,
    pub live_backend: Backend,
    pub restore_on_startup: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            static_backend: StaticBackendPreference::Auto,
            live_backend: Backend::Mpvpaper,
            restore_on_startup: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MonitorConfig {
    pub default: String,
    pub profiles: BTreeMap<String, MonitorProfileConfig>,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            default: "all".to_string(),
            profiles: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MonitorProfileConfig {
    pub wallpaper: Option<PathBuf>,
    pub backend: BackendPreference,
}

impl Default for MonitorProfileConfig {
    fn default() -> Self {
        Self {
            wallpaper: None,
            backend: BackendPreference::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum BackendPreference {
    #[default]
    Auto,
    Hyprpaper,
    Mpvpaper,
    Awww,
    Swaybg,
}

impl BackendPreference {
    pub fn as_backend(self) -> Option<Backend> {
        match self {
            Self::Auto => None,
            Self::Hyprpaper => Some(Backend::Hyprpaper),
            Self::Mpvpaper => Some(Backend::Mpvpaper),
            Self::Awww => Some(Backend::Awww),
            Self::Swaybg => Some(Backend::Swaybg),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LiveConfig {
    pub pause: LivePauseConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LivePauseConfig {
    pub on_battery: bool,
    pub on_fullscreen: bool,
    pub on_idle: bool,
    pub resume_on_ac: bool,
    pub resume_on_unfullscreen: bool,
    pub resume_on_activity: bool,
}

impl Default for LivePauseConfig {
    fn default() -> Self {
        Self {
            on_battery: true,
            on_fullscreen: true,
            on_idle: true,
            resume_on_ac: true,
            resume_on_unfullscreen: true,
            resume_on_activity: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HyprpaperConfig {
    pub unload_unused: bool,
}

impl Default for HyprpaperConfig {
    fn default() -> Self {
        Self {
            unload_unused: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MpvpaperConfig {
    pub mode: String,
    pub extra_args: Vec<String>,
    pub readiness_timeout_ms: u64,
}

impl Default for MpvpaperConfig {
    fn default() -> Self {
        Self {
            mode: "per-monitor".to_string(),
            extra_args: vec![
                "--loop".to_string(),
                "--no-audio".to_string(),
                "--panscan=1.0".to_string(),
            ],
            readiness_timeout_ms: 5_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AwwwConfig {
    pub enabled: bool,
    pub transition_type: String,
    pub transition_duration: f32,
    pub transition_fps: u16,
    pub transition_pos: String,
    pub transition_bezier: String,
    pub transition_wave: String,
}

impl Default for AwwwConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            transition_type: "grow".to_string(),
            transition_duration: 2.4,
            transition_fps: 60,
            transition_pos: "center".to_string(),
            transition_bezier: ".42,0,.2,1".to_string(),
            transition_wave: "28,12".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_plan() {
        let config = Config::default();
        assert_eq!(config.general.static_backend, StaticBackendPreference::Auto);
        assert_eq!(config.general.live_backend, Backend::Mpvpaper);
        assert_eq!(config.monitors.default, "all");
        assert!(config.monitors.profiles.is_empty());
        assert!(config.live.pause.on_battery);
        assert!(config.live.pause.on_fullscreen);
        assert!(config.live.pause.on_idle);
        assert_eq!(
            config.mpvpaper.extra_args,
            ["--loop", "--no-audio", "--panscan=1.0"]
        );
        assert_eq!(config.mpvpaper.readiness_timeout_ms, 5_000);
        assert_eq!(config.awww.transition_duration, 2.4);
        assert_eq!(config.awww.transition_fps, 60);
        assert_eq!(config.awww.transition_pos, "center");
        assert_eq!(config.awww.transition_bezier, ".42,0,.2,1");
        assert_eq!(config.awww.transition_wave, "28,12");
    }

    #[test]
    fn auto_static_uses_awww_when_enabled() {
        let mut config = Config::default();
        assert_eq!(config.configured_static_backend(), Some(Backend::Hyprpaper));
        config.awww.enabled = true;
        assert_eq!(config.configured_static_backend(), Some(Backend::Awww));
    }

    #[test]
    fn serializes_default_config() {
        let raw = Config::default().to_toml_string().unwrap();

        assert!(raw.contains("[general]"));
        assert!(raw.contains("static_backend = \"auto\""));
        assert!(raw.contains("[live.pause]"));
        assert!(raw.contains("[mpvpaper]"));
    }

    #[test]
    fn parses_monitor_profile_config() {
        let raw = r#"
            [monitors]
            default = "all"

            [monitors.profiles.DP-1]
            wallpaper = "/tmp/wall.png"
            backend = "hyprpaper"
        "#;

        let config: Config = toml::from_str(raw).unwrap();
        let profile = &config.monitors.profiles["DP-1"];

        assert_eq!(
            profile.wallpaper.as_deref(),
            Some(Path::new("/tmp/wall.png"))
        );
        assert_eq!(profile.backend, BackendPreference::Hyprpaper);
    }
}
