use crate::{
    backends::model::{Backend, WallpaperBackend},
    command::{CommandSpec, program_available},
    config::HyprpaperConfig,
    media::MediaKind,
    orchestrator::SetPlan,
};
use std::{ffi::OsString, path::Path};

pub struct HyprpaperBackend;

impl WallpaperBackend for HyprpaperBackend {
    fn backend(&self) -> Backend {
        Backend::Hyprpaper
    }

    fn name(&self) -> &'static str {
        "hyprpaper"
    }

    fn is_available(&self) -> bool {
        program_available("hyprpaper")
    }

    fn supports(&self, media: MediaKind) -> bool {
        media == MediaKind::Static
    }
}

pub fn apply_commands(
    plan: &SetPlan,
    monitors: &[String],
    config: &HyprpaperConfig,
) -> Vec<CommandSpec> {
    let targets = if plan.monitor == "all" {
        monitors.to_vec()
    } else {
        vec![plan.monitor.clone()]
    };

    let mut commands = vec![CommandSpec::new(
        "hyprctl",
        [
            OsString::from("hyprpaper"),
            OsString::from("preload"),
            plan.wallpaper.as_os_str().to_os_string(),
        ],
    )];

    commands.extend(targets.into_iter().map(|monitor| {
        CommandSpec::new(
            "hyprctl",
            [
                OsString::from("hyprpaper"),
                OsString::from("wallpaper"),
                OsString::from(wallpaper_target(&monitor, &plan.wallpaper)),
            ],
        )
    }));

    if config.unload_unused {
        commands.push(CommandSpec::new(
            "hyprctl",
            [
                OsString::from("hyprpaper"),
                OsString::from("unload"),
                OsString::from("unused"),
            ],
        ));
    }

    commands
}

pub fn wallpaper_commands(plan: &SetPlan, monitors: &[String]) -> Vec<CommandSpec> {
    let targets = if plan.monitor == "all" {
        monitors.to_vec()
    } else {
        vec![plan.monitor.clone()]
    };

    targets
        .into_iter()
        .map(|monitor| {
            CommandSpec::new(
                "hyprctl",
                [
                    OsString::from("hyprpaper"),
                    OsString::from("wallpaper"),
                    OsString::from(wallpaper_target(&monitor, &plan.wallpaper)),
                ],
            )
        })
        .collect()
}

pub fn query_command() -> CommandSpec {
    CommandSpec::new(
        "hyprctl",
        [OsString::from("hyprpaper"), OsString::from("listloaded")],
    )
}

pub fn daemon_command() -> CommandSpec {
    CommandSpec::new("hyprpaper", [])
}

fn wallpaper_target(monitor: &str, wallpaper: &Path) -> String {
    format!("{monitor},{}", wallpaper.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backends::model::Backend, config::HyprpaperConfig, media::MediaKind, orchestrator::SetPlan,
    };
    use std::path::PathBuf;

    #[test]
    fn builds_hyprpaper_commands_for_specific_monitor() {
        let plan = SetPlan {
            wallpaper: PathBuf::from("/tmp/wall.png"),
            media_kind: MediaKind::Static,
            backend: Backend::Hyprpaper,
            monitor: "DP-1".to_string(),
        };

        let commands = wallpaper_commands(&plan, &[]);

        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].to_string(),
            "hyprctl hyprpaper wallpaper DP-1,/tmp/wall.png"
        );
    }

    #[test]
    fn expands_all_outputs() {
        let plan = SetPlan {
            wallpaper: PathBuf::from("/tmp/wall.png"),
            media_kind: MediaKind::Static,
            backend: Backend::Hyprpaper,
            monitor: "all".to_string(),
        };

        let commands = wallpaper_commands(&plan, &["DP-1".to_string(), "DP-3".to_string()]);

        assert_eq!(commands.len(), 2);
        assert_eq!(
            commands[0].to_string(),
            "hyprctl hyprpaper wallpaper DP-1,/tmp/wall.png"
        );
        assert_eq!(
            commands[1].to_string(),
            "hyprctl hyprpaper wallpaper DP-3,/tmp/wall.png"
        );
    }

    #[test]
    fn wraps_wallpaper_commands_with_preload_and_unload() {
        let plan = SetPlan {
            wallpaper: PathBuf::from("/tmp/wall.png"),
            media_kind: MediaKind::Static,
            backend: Backend::Hyprpaper,
            monitor: "DP-1".to_string(),
        };

        let commands = apply_commands(&plan, &[], &HyprpaperConfig::default());

        assert_eq!(commands.len(), 3);
        assert_eq!(
            commands[0].to_string(),
            "hyprctl hyprpaper preload /tmp/wall.png"
        );
        assert_eq!(
            commands[1].to_string(),
            "hyprctl hyprpaper wallpaper DP-1,/tmp/wall.png"
        );
        assert_eq!(commands[2].to_string(), "hyprctl hyprpaper unload unused");
    }

    #[test]
    fn builds_hyprpaper_lifecycle_commands() {
        assert_eq!(query_command().to_string(), "hyprctl hyprpaper listloaded");
        assert_eq!(daemon_command().to_string(), "hyprpaper");
    }
}
