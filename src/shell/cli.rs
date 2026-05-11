use crate::config::{Config, ShellPositionConfig};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Parser)]
#[command(
    name = "jobowalls-shell",
    version,
    about = "Compact layer-shell wallpaper picker for JoboWalls"
)]
pub struct ShellArgs {
    /// Folder to browse. Defaults to recent shell folder, Omarchy theme backgrounds, then ~/Pictures/Wallpapers.
    pub folder: Option<PathBuf>,

    /// Target monitor passed to `jobowalls set`.
    #[arg(long)]
    pub monitor: Option<String>,

    /// Overlay position.
    #[arg(long, value_enum)]
    pub position: Option<ShellPosition>,

    /// Debug window width in pixels.
    #[arg(long, default_value_t = 0)]
    pub width: i32,

    /// Overlay height in pixels.
    #[arg(long)]
    pub height: Option<i32>,

    /// Disable applying the selected wallpaper to the desktop while browsing.
    #[arg(long)]
    pub no_live_preview: bool,

    /// Open as a normal decorated GTK window for layout debugging.
    #[arg(long)]
    pub debug_window: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ShellPosition {
    Bottom,
    Center,
}

impl ShellArgs {
    pub fn apply_config_defaults(mut self, config: &Config) -> Self {
        if self.monitor.is_none() {
            self.monitor = Some(config.shell.monitor.clone());
        }
        if self.position.is_none() {
            self.position = Some(config.shell.position.into());
        }
        if self.height.is_none() {
            self.height = Some(config.shell.height);
        }
        if !config.shell.live_preview {
            self.no_live_preview = true;
        }
        self
    }

    pub fn monitor(&self) -> &str {
        self.monitor.as_deref().unwrap_or("all")
    }

    pub fn position(&self) -> ShellPosition {
        self.position.unwrap_or(ShellPosition::Bottom)
    }

    pub fn height(&self) -> i32 {
        self.height.unwrap_or(340)
    }
}

impl From<ShellPositionConfig> for ShellPosition {
    fn from(value: ShellPositionConfig) -> Self {
        match value {
            ShellPositionConfig::Bottom => Self::Bottom,
            ShellPositionConfig::Center => Self::Center,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ShellPositionConfig};

    #[test]
    fn config_supplies_missing_shell_args() {
        let mut config = Config::default();
        config.shell.monitor = "DP-1".to_string();
        config.shell.position = ShellPositionConfig::Center;
        config.shell.height = 410;
        config.shell.live_preview = false;

        let args = ShellArgs {
            folder: None,
            monitor: None,
            position: None,
            width: 0,
            height: None,
            no_live_preview: false,
            debug_window: false,
        }
        .apply_config_defaults(&config);

        assert_eq!(args.monitor(), "DP-1");
        assert_eq!(args.position(), ShellPosition::Center);
        assert_eq!(args.height(), 410);
        assert!(args.no_live_preview);
    }

    #[test]
    fn cli_shell_args_override_config() {
        let mut config = Config::default();
        config.shell.monitor = "DP-1".to_string();
        config.shell.position = ShellPositionConfig::Center;
        config.shell.height = 410;

        let args = ShellArgs {
            folder: None,
            monitor: Some("HDMI-A-1".to_string()),
            position: Some(ShellPosition::Bottom),
            width: 0,
            height: Some(300),
            no_live_preview: false,
            debug_window: false,
        }
        .apply_config_defaults(&config);

        assert_eq!(args.monitor(), "HDMI-A-1");
        assert_eq!(args.position(), ShellPosition::Bottom);
        assert_eq!(args.height(), 300);
    }
}
