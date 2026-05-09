use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Parser)]
#[command(
    name = "jobowalls-shell",
    about = "Compact layer-shell wallpaper picker for JoboWalls"
)]
pub struct ShellArgs {
    /// Folder to browse. Defaults to recent shell folder, Omarchy theme backgrounds, then ~/Pictures/Wallpapers.
    pub folder: Option<PathBuf>,

    /// Target monitor passed to `jobowalls set`.
    #[arg(long, default_value = "all")]
    pub monitor: String,

    /// Overlay position.
    #[arg(long, value_enum, default_value_t = ShellPosition::Bottom)]
    pub position: ShellPosition,

    /// Overlay width in pixels.
    #[arg(long, default_value_t = 640)]
    pub width: i32,

    /// Overlay height in pixels.
    #[arg(long, default_value_t = 170)]
    pub height: i32,

    /// Disable animated live previews.
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
