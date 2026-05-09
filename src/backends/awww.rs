use crate::{
    backends::model::{Backend, WallpaperBackend},
    command::{CommandSpec, program_available},
    config::AwwwConfig,
    media::MediaKind,
    orchestrator::SetPlan,
};
use std::ffi::OsString;

pub struct AwwwBackend;

impl WallpaperBackend for AwwwBackend {
    fn backend(&self) -> Backend {
        Backend::Awww
    }

    fn name(&self) -> &'static str {
        "awww"
    }

    fn is_available(&self) -> bool {
        program_available("awww")
    }

    fn supports(&self, media: MediaKind) -> bool {
        media == MediaKind::Static
    }
}

pub fn apply_command(plan: &SetPlan, config: &AwwwConfig) -> CommandSpec {
    let mut args = vec![
        OsString::from("img"),
        plan.wallpaper.as_os_str().to_os_string(),
        OsString::from("--transition-type"),
        OsString::from(config.transition_type.as_str()),
        OsString::from("--transition-duration"),
        OsString::from(config.transition_duration.to_string()),
        OsString::from("--transition-fps"),
        OsString::from(config.transition_fps.to_string()),
        OsString::from("--transition-pos"),
        OsString::from(config.transition_pos.as_str()),
        OsString::from("--transition-bezier"),
        OsString::from(config.transition_bezier.as_str()),
        OsString::from("--transition-wave"),
        OsString::from(config.transition_wave.as_str()),
    ];

    if plan.monitor != "all" {
        args.push(OsString::from("--outputs"));
        args.push(OsString::from(plan.monitor.as_str()));
    }

    CommandSpec::new("awww", args)
}

pub fn apply_instant_command(plan: &SetPlan) -> CommandSpec {
    let mut args = vec![
        OsString::from("img"),
        plan.wallpaper.as_os_str().to_os_string(),
        OsString::from("--transition-type"),
        OsString::from("none"),
        OsString::from("--transition-step"),
        OsString::from("255"),
    ];

    if plan.monitor != "all" {
        args.push(OsString::from("--outputs"));
        args.push(OsString::from(plan.monitor.as_str()));
    }

    CommandSpec::new("awww", args)
}

pub fn query_command() -> CommandSpec {
    CommandSpec::new("awww", [OsString::from("query")])
}

pub fn daemon_command() -> CommandSpec {
    CommandSpec::new("awww-daemon", [OsString::from("--quiet")])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backends::model::Backend, config::AwwwConfig, media::MediaKind, orchestrator::SetPlan,
    };
    use std::path::PathBuf;

    #[test]
    fn builds_awww_command_for_specific_monitor() {
        let plan = SetPlan {
            wallpaper: PathBuf::from("/tmp/wall.jpg"),
            media_kind: MediaKind::Static,
            backend: Backend::Awww,
            monitor: "DP-1".to_string(),
        };

        assert_eq!(
            apply_command(&plan, &AwwwConfig::default()).to_string(),
            "awww img /tmp/wall.jpg --transition-type grow --transition-duration 2.4 --transition-fps 60 --transition-pos center --transition-bezier .42,0,.2,1 --transition-wave 28,12 --outputs DP-1"
        );
    }

    #[test]
    fn builds_awww_lifecycle_commands() {
        assert_eq!(query_command().to_string(), "awww query");
        assert_eq!(daemon_command().to_string(), "awww-daemon --quiet");
    }

    #[test]
    fn builds_instant_awww_command_for_live_to_static() {
        let plan = SetPlan {
            wallpaper: PathBuf::from("/tmp/wall.jpg"),
            media_kind: MediaKind::Static,
            backend: Backend::Awww,
            monitor: "DP-1".to_string(),
        };

        assert_eq!(
            apply_instant_command(&plan).to_string(),
            "awww img /tmp/wall.jpg --transition-type none --transition-step 255 --outputs DP-1"
        );
    }
}
