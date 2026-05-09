use crate::{
    backends::model::{Backend, WallpaperBackend},
    command::{CommandSpec, program_available},
    media::MediaKind,
    orchestrator::SetPlan,
};
use std::ffi::OsString;

pub struct SwaybgBackend;

impl WallpaperBackend for SwaybgBackend {
    fn backend(&self) -> Backend {
        Backend::Swaybg
    }

    fn name(&self) -> &'static str {
        "swaybg"
    }

    fn is_available(&self) -> bool {
        program_available("swaybg")
    }

    fn supports(&self, media: MediaKind) -> bool {
        media == MediaKind::Static
    }
}

pub fn start_command(plan: &SetPlan) -> CommandSpec {
    let mut args = vec![
        OsString::from("-i"),
        plan.wallpaper.as_os_str().to_os_string(),
        OsString::from("-m"),
        OsString::from("fill"),
    ];

    if plan.monitor != "all" {
        args.push(OsString::from("-o"));
        args.push(OsString::from(plan.monitor.as_str()));
    }

    CommandSpec::new("swaybg", args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{backends::model::Backend, media::MediaKind, orchestrator::SetPlan};
    use std::path::PathBuf;

    #[test]
    fn builds_swaybg_command_for_all_monitors() {
        let plan = SetPlan {
            wallpaper: PathBuf::from("/tmp/wall.jpg"),
            media_kind: MediaKind::Static,
            backend: Backend::Swaybg,
            monitor: "all".to_string(),
        };

        assert_eq!(
            start_command(&plan).to_string(),
            "swaybg -i /tmp/wall.jpg -m fill"
        );
    }

    #[test]
    fn builds_swaybg_command_for_specific_monitor() {
        let plan = SetPlan {
            wallpaper: PathBuf::from("/tmp/wall.jpg"),
            media_kind: MediaKind::Static,
            backend: Backend::Swaybg,
            monitor: "DP-1".to_string(),
        };

        assert_eq!(
            start_command(&plan).to_string(),
            "swaybg -i /tmp/wall.jpg -m fill -o DP-1"
        );
    }
}
