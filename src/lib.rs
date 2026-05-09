pub mod cli;
pub mod collection;
pub mod command;
pub mod config;
pub mod media;
pub mod monitors;
pub mod orchestrator;
pub mod shell;
pub mod state;

pub mod backends {
    pub mod awww;
    pub mod hyprpaper;
    pub mod model;
    pub mod mpvpaper;
    pub mod swaybg;
}
