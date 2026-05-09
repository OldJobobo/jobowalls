use crate::{
    backends::model::{Backend, WallpaperBackend},
    command::{CommandSpec, program_available},
    config::MpvpaperConfig,
    media::MediaKind,
    orchestrator::SetPlan,
};
use std::{ffi::OsString, path::Path};

pub struct MpvpaperBackend;

impl WallpaperBackend for MpvpaperBackend {
    fn backend(&self) -> Backend {
        Backend::Mpvpaper
    }

    fn name(&self) -> &'static str {
        "mpvpaper"
    }

    fn is_available(&self) -> bool {
        program_available("mpvpaper")
    }

    fn supports(&self, media: MediaKind) -> bool {
        media == MediaKind::Live
    }
}

pub fn start_command(plan: &SetPlan, config: &MpvpaperConfig) -> CommandSpec {
    CommandSpec::new(
        "mpvpaper",
        [
            OsString::from("--mpv-options"),
            OsString::from(mpv_options(&config.extra_args)),
            OsString::from(plan.monitor.as_str()),
            plan.wallpaper.as_os_str().to_os_string(),
        ],
    )
}

pub fn start_command_with_ipc(
    plan: &SetPlan,
    config: &MpvpaperConfig,
    ipc_socket: &Path,
) -> CommandSpec {
    let mut extra_args = config.extra_args.clone();
    extra_args.push(format!("--input-ipc-server={}", ipc_socket.display()));

    CommandSpec::new(
        "mpvpaper",
        [
            OsString::from("--mpv-options"),
            OsString::from(mpv_options(&extra_args)),
            OsString::from(plan.monitor.as_str()),
            plan.wallpaper.as_os_str().to_os_string(),
        ],
    )
}

pub fn list_outputs_command() -> CommandSpec {
    CommandSpec::new("mpvpaper", [OsString::from("--help-output")])
}

fn mpv_options(extra_args: &[String]) -> String {
    extra_args
        .iter()
        .map(|arg| arg.trim_start_matches("--"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{backends::model::Backend, media::MediaKind, orchestrator::SetPlan};
    use std::path::PathBuf;

    #[test]
    fn builds_mpvpaper_command_with_normalized_mpv_options() {
        let plan = SetPlan {
            wallpaper: PathBuf::from("/tmp/rain.mp4"),
            media_kind: MediaKind::Live,
            backend: Backend::Mpvpaper,
            monitor: "DP-1".to_string(),
        };
        let config = MpvpaperConfig {
            mode: "per-monitor".to_string(),
            extra_args: vec![
                "--loop".to_string(),
                "--no-audio".to_string(),
                "--panscan=1.0".to_string(),
            ],
            readiness_timeout_ms: 5_000,
        };

        assert_eq!(
            start_command(&plan, &config).to_string(),
            "mpvpaper --mpv-options 'loop no-audio panscan=1.0' DP-1 /tmp/rain.mp4"
        );
    }

    #[test]
    fn builds_mpvpaper_command_with_ipc_socket() {
        let plan = SetPlan {
            wallpaper: PathBuf::from("/tmp/rain.mp4"),
            media_kind: MediaKind::Live,
            backend: Backend::Mpvpaper,
            monitor: "DP-1".to_string(),
        };
        let config = MpvpaperConfig::default();

        assert_eq!(
            start_command_with_ipc(&plan, &config, Path::new("/tmp/jobowalls.sock")).to_string(),
            "mpvpaper --mpv-options 'loop no-audio panscan=1.0 input-ipc-server=/tmp/jobowalls.sock' DP-1 /tmp/rain.mp4"
        );
    }
}
