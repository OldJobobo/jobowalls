use serde_json::Value;
use std::{
    env, fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::{TempDir, tempdir};

struct CliHarness {
    _dir: TempDir,
    root: PathBuf,
    bin_dir: PathBuf,
    config: PathBuf,
    state: PathBuf,
    log: PathBuf,
}

impl CliHarness {
    fn new() -> Self {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let bin_dir = root.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();

        Self {
            _dir: dir,
            root: root.clone(),
            bin_dir,
            config: root.join("config.toml"),
            state: root.join("state.json"),
            log: root.join("commands.log"),
        }
    }

    fn path(&self) -> String {
        let existing = env::var_os("PATH").unwrap_or_default();
        format!("{}:{}", self.bin_dir.display(), existing.to_string_lossy())
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_jobowalls"));
        command
            .arg("--config")
            .arg(&self.config)
            .arg("--state")
            .arg(&self.state)
            .env("PATH", self.path())
            .env("HOME", &self.root)
            .env("JOBOWALLS_TEST_LOG", &self.log);
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }

    fn fake_program(&self, name: &str, body: &str) {
        let path = self.bin_dir.join(name);
        fs::write(&path, body).unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    fn fake_swaybg(&self) {
        self.fake_program(
            "swaybg",
            r#"#!/usr/bin/env bash
printf 'swaybg %s\n' "$*" >>"${JOBOWALLS_TEST_LOG:?}"
if [[ "$*" == "--help" ]]; then
  exit 0
fi
sleep 2
exit 0
"#,
        );
    }

    fn fake_swaybg_exits_immediately(&self) {
        self.fake_program(
            "swaybg",
            r#"#!/usr/bin/env bash
printf 'swaybg %s\n' "$*" >>"${JOBOWALLS_TEST_LOG:?}"
exit 0
"#,
        );
    }

    fn fake_hyprctl_monitors(&self) {
        self.fake_program(
            "hyprctl",
            r#"#!/usr/bin/env bash
printf 'hyprctl %s\n' "$*" >>"${JOBOWALLS_TEST_LOG:?}"
if [[ "$*" == "-j monitors" ]]; then
  printf '[{"name":"DP-1"},{"name":"HDMI-A-1"}]\n'
  exit 0
fi
if [[ "$*" == "monitors" ]]; then
  printf 'Monitor DP-1 (ID 0):\nMonitor HDMI-A-1 (ID 1):\n'
  exit 0
fi
exit 0
"#,
        );
    }

    fn fake_hyprctl_json_empty_then_text_monitors(&self) {
        self.fake_program(
            "hyprctl",
            r#"#!/usr/bin/env bash
printf 'hyprctl %s\n' "$*" >>"${JOBOWALLS_TEST_LOG:?}"
if [[ "$*" == "-j monitors" ]]; then
  printf '[]\n'
  exit 0
fi
if [[ "$*" == "monitors" ]]; then
  printf 'Monitor DP-2 (ID 0):\nMonitor HDMI-A-2 (ID 1):\n'
  exit 0
fi
exit 0
"#,
        );
    }

    fn fake_hyprctl_failing(&self) {
        self.fake_program(
            "hyprctl",
            r#"#!/usr/bin/env bash
printf 'hyprctl %s\n' "$*" >>"${JOBOWALLS_TEST_LOG:?}"
printf 'hyprctl failed intentionally\n' >&2
exit 1
"#,
        );
    }

    fn fake_mpvpaper_outputs(&self) {
        self.fake_program(
            "mpvpaper",
            r#"#!/usr/bin/env bash
printf 'mpvpaper %s\n' "$*" >>"${JOBOWALLS_TEST_LOG:?}"
if [[ "$*" == "--help-output" ]]; then
  printf 'eDP-1\nDP-4\n'
  exit 0
fi
exit 0
"#,
        );
    }

    fn fake_doctor_commands(&self) {
        self.fake_program(
            "hyprctl",
            r#"#!/usr/bin/env bash
printf 'hyprctl %s\n' "$*" >>"${JOBOWALLS_TEST_LOG:?}"
if [[ "$*" == "-j monitors" ]]; then
  printf '[{"name":"DP-1"}]\n'
  exit 0
fi
if [[ "$*" == "activewindow" || "$*" == "-j activewindow" ]]; then
  printf '{"fullscreen":false}\n'
  exit 0
fi
exit 0
"#,
        );
        for name in ["swaybg", "mpvpaper", "awww", "awww-daemon"] {
            self.fake_program(
                name,
                r#"#!/usr/bin/env bash
printf '%s %s\n' "$(basename "$0")" "$*" >>"${JOBOWALLS_TEST_LOG:?}"
exit 0
"#,
            );
        }
    }

    fn fake_kill_all_success(&self) {
        self.fake_program(
            "kill",
            r#"#!/usr/bin/env bash
printf 'kill %s\n' "$*" >>"${JOBOWALLS_TEST_LOG:?}"
exit 0
"#,
        );
    }

    fn fake_ps_without_omarchy_swaybg(&self) {
        self.fake_program(
            "ps",
            r#"#!/usr/bin/env bash
printf 'ps %s\n' "$*" >>"${JOBOWALLS_TEST_LOG:?}"
exit 0
"#,
        );
    }

    fn fake_ps_with_omarchy_swaybg(&self) {
        let body = format!(
            r#"#!/usr/bin/env bash
printf 'ps %s\n' "$*" >>"${{JOBOWALLS_TEST_LOG:?}}"
printf '444 /usr/bin/swaybg -i {}/.config/omarchy/current/background -m fill\n'
exit 0
"#,
            self.root.display()
        );
        self.fake_program("ps", &body);
    }

    fn omarchy_current_background(&self) -> PathBuf {
        self.root
            .join(".config")
            .join("omarchy")
            .join("current")
            .join("background")
    }

    fn stdout(output: &Output) -> String {
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    fn stderr(output: &Output) -> String {
        String::from_utf8_lossy(&output.stderr).to_string()
    }
}

fn write_png(path: &Path) {
    fs::write(path, b"\x89PNG\r\n\x1a\nrest").unwrap();
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success\nstdout:\n{}\nstderr:\n{}",
        CliHarness::stdout(output),
        CliHarness::stderr(output)
    );
}

fn assert_failure(output: &Output) {
    assert!(
        !output.status.success(),
        "expected failure\nstdout:\n{}\nstderr:\n{}",
        CliHarness::stdout(output),
        CliHarness::stderr(output)
    );
}

fn wait_for_log_contains(path: &Path, needle: &str) -> bool {
    for _ in 0..20 {
        if fs::read_to_string(path)
            .map(|log| log.contains(needle))
            .unwrap_or(false)
        {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    false
}

fn read_link(path: &Path) -> PathBuf {
    fs::read_link(path).unwrap()
}

#[test]
fn dry_run_static_json_reports_canonical_plan() {
    let harness = CliHarness::new();
    harness.fake_swaybg();
    let wallpaper = harness.root.join("wall with spaces.png");
    write_png(&wallpaper);

    let output = harness.run(&[
        "set",
        wallpaper.to_str().unwrap(),
        "--dry-run",
        "--json",
        "--monitor",
        "DP-1",
    ]);
    assert_success(&output);

    let value: Value = serde_json::from_str(&CliHarness::stdout(&output)).unwrap();
    assert_eq!(
        value["wallpaper"],
        fs::canonicalize(&wallpaper).unwrap().display().to_string()
    );
    assert_eq!(value["media_kind"], "static");
    assert_eq!(value["backend"], "swaybg");
    assert_eq!(value["monitor"], "DP-1");
}

#[test]
fn dry_run_live_json_reports_mpvpaper_plan() {
    let harness = CliHarness::new();
    let wallpaper = harness.root.join("rain.mp4");
    fs::write(&wallpaper, b"not a real video").unwrap();

    let output = harness.run(&[
        "set",
        wallpaper.to_str().unwrap(),
        "--dry-run",
        "--json",
        "--monitor",
        "DP-1",
    ]);
    assert_success(&output);

    let value: Value = serde_json::from_str(&CliHarness::stdout(&output)).unwrap();
    assert_eq!(
        value["wallpaper"],
        fs::canonicalize(&wallpaper).unwrap().display().to_string()
    );
    assert_eq!(value["media_kind"], "live");
    assert_eq!(value["backend"], "mpvpaper");
    assert_eq!(value["monitor"], "DP-1");
}

#[test]
fn set_missing_file_fails_before_backend_calls() {
    let harness = CliHarness::new();
    harness.fake_swaybg();
    let missing = harness.root.join("missing.png");

    let output = harness.run(&["set", missing.to_str().unwrap(), "--dry-run", "--json"]);
    assert_failure(&output);

    assert!(CliHarness::stderr(&output).contains("wallpaper path does not exist"));
    assert!(!harness.log.exists());
}

#[test]
fn status_json_reports_missing_state() {
    let harness = CliHarness::new();

    let output = harness.run(&["status", "--json"]);
    assert_success(&output);

    let value: Value = serde_json::from_str(&CliHarness::stdout(&output)).unwrap();
    assert_eq!(value["state_exists"], false);
    assert!(value.as_object().unwrap().get("wallpaper").is_none());
}

#[test]
fn config_print_default_emits_parseable_toml() {
    let harness = CliHarness::new();

    let output = harness.run(&["config", "print-default"]);
    assert_success(&output);

    let value: toml::Value = toml::from_str(&CliHarness::stdout(&output)).unwrap();
    assert_eq!(value["general"]["static_backend"].as_str(), Some("auto"));
    assert_eq!(value["general"]["live_backend"].as_str(), Some("mpvpaper"));
}

#[test]
fn config_init_writes_config_and_refuses_overwrite_without_force() {
    let harness = CliHarness::new();

    let output = harness.run(&["config", "init"]);
    assert_success(&output);
    assert!(harness.config.exists());

    let output = harness.run(&["config", "init"]);
    assert_failure(&output);
    assert!(CliHarness::stderr(&output).contains("config already exists"));

    let output = harness.run(&["config", "init", "--force"]);
    assert_success(&output);
}

#[test]
fn list_monitors_uses_hyprctl_json_output() {
    let harness = CliHarness::new();
    harness.fake_hyprctl_monitors();

    let output = harness.run(&["list-monitors"]);
    assert_success(&output);

    assert_eq!(CliHarness::stdout(&output), "DP-1\nHDMI-A-1\n");
    let log = fs::read_to_string(&harness.log).unwrap();
    assert!(log.contains("hyprctl -j monitors"));
}

#[test]
fn list_monitors_falls_back_to_hyprctl_text_when_json_is_empty() {
    let harness = CliHarness::new();
    harness.fake_hyprctl_json_empty_then_text_monitors();

    let output = harness.run(&["list-monitors"]);
    assert_success(&output);

    assert_eq!(CliHarness::stdout(&output), "DP-2\nHDMI-A-2\n");
    let log = fs::read_to_string(&harness.log).unwrap();
    assert!(log.contains("hyprctl -j monitors"));
    assert!(log.contains("hyprctl monitors"));
}

#[test]
fn list_monitors_falls_back_to_mpvpaper_outputs_when_hyprctl_fails() {
    let harness = CliHarness::new();
    harness.fake_hyprctl_failing();
    harness.fake_mpvpaper_outputs();

    let output = harness.run(&["list-monitors"]);
    assert_success(&output);

    assert_eq!(CliHarness::stdout(&output), "eDP-1\nDP-4\n");
    let log = fs::read_to_string(&harness.log).unwrap();
    assert!(log.contains("hyprctl -j monitors"));
    assert!(log.contains("hyprctl monitors"));
    assert!(log.contains("mpvpaper --help-output"));
}

#[test]
fn collection_next_executes_selection_and_records_progress() {
    let harness = CliHarness::new();
    harness.fake_swaybg();
    let collection = harness.root.join("walls");
    fs::create_dir_all(&collection).unwrap();
    let first = collection.join("a.png");
    let second = collection.join("b.png");
    write_png(&first);
    write_png(&second);

    let output = harness.run(&["next", collection.to_str().unwrap(), "--monitor", "DP-1"]);
    assert_success(&output);

    let canonical_collection = fs::canonicalize(&collection).unwrap();
    let canonical_first = fs::canonicalize(&first).unwrap();
    let stdout = CliHarness::stdout(&output);
    assert!(stdout.contains(&format!("collection: {}", canonical_collection.display())));
    assert!(stdout.contains(&format!("selected: {}", canonical_first.display())));

    let state: Value = serde_json::from_str(&fs::read_to_string(&harness.state).unwrap()).unwrap();
    assert_eq!(state["active_backend"], "swaybg");
    assert_eq!(state["wallpaper"], canonical_first.display().to_string());
    assert_eq!(state["monitors"]["DP-1"]["backend"], "swaybg");
    assert_eq!(
        state["collections"][canonical_collection.display().to_string()]["last_index"],
        0
    );
    assert_eq!(
        state["collections"][canonical_collection.display().to_string()]["last_wallpaper"],
        canonical_first.display().to_string()
    );
    assert_eq!(
        state["last_command"],
        format!(
            "next {} --monitor DP-1 --backend swaybg",
            canonical_collection.display()
        )
    );
}

#[test]
fn collection_previous_uses_saved_collection_progress() {
    let harness = CliHarness::new();
    harness.fake_swaybg();
    let collection = harness.root.join("walls");
    fs::create_dir_all(&collection).unwrap();
    let first = collection.join("a.png");
    let second = collection.join("b.png");
    write_png(&first);
    write_png(&second);

    assert_success(&harness.run(&["next", collection.to_str().unwrap(), "--monitor", "DP-1"]));
    let output = harness.run(&[
        "previous",
        collection.to_str().unwrap(),
        "--monitor",
        "DP-1",
    ]);
    assert_success(&output);

    let canonical_collection = fs::canonicalize(&collection).unwrap();
    let canonical_second = fs::canonicalize(&second).unwrap();
    let state: Value = serde_json::from_str(&fs::read_to_string(&harness.state).unwrap()).unwrap();

    assert_eq!(state["wallpaper"], canonical_second.display().to_string());
    assert_eq!(
        state["collections"][canonical_collection.display().to_string()]["last_index"],
        1
    );
    assert_eq!(
        state["last_command"],
        format!(
            "previous {} --monitor DP-1 --backend swaybg",
            canonical_collection.display()
        )
    );
}

#[test]
fn collection_shuffle_records_shuffle_history() {
    let harness = CliHarness::new();
    harness.fake_swaybg();
    let collection = harness.root.join("walls");
    fs::create_dir_all(&collection).unwrap();
    let first = fs::canonicalize({
        let path = collection.join("a.png");
        write_png(&path);
        path
    })
    .unwrap();
    let second = fs::canonicalize({
        let path = collection.join("b.png");
        write_png(&path);
        path
    })
    .unwrap();

    let output = harness.run(&["shuffle", collection.to_str().unwrap(), "--monitor", "DP-1"]);
    assert_success(&output);

    let canonical_collection = fs::canonicalize(&collection).unwrap();
    let state: Value = serde_json::from_str(&fs::read_to_string(&harness.state).unwrap()).unwrap();
    let selected = state["wallpaper"].as_str().unwrap();
    assert!(
        selected == first.display().to_string() || selected == second.display().to_string(),
        "unexpected selected wallpaper {selected}"
    );
    assert_eq!(
        state["collections"][canonical_collection.display().to_string()]["shuffle_history"],
        serde_json::json!([selected])
    );
    assert_eq!(
        state["last_command"],
        format!(
            "shuffle {} --monitor DP-1 --backend swaybg",
            canonical_collection.display()
        )
    );
}

#[test]
fn restore_static_swaybg_state_reapplies_and_records_restore() {
    let harness = CliHarness::new();
    harness.fake_hyprctl_monitors();
    harness.fake_swaybg();
    harness.fake_ps_without_omarchy_swaybg();
    let wallpaper = harness.root.join("restore.png");
    write_png(&wallpaper);
    let canonical = fs::canonicalize(&wallpaper).unwrap();
    let state = serde_json::json!({
        "version": 1,
        "active_backend": "swaybg",
        "mode": "static",
        "wallpaper": canonical.display().to_string(),
        "monitors": {
            "DP-1": {
                "backend": "swaybg",
                "wallpaper": canonical.display().to_string(),
                "pid": null
            }
        },
        "collections": {
            "/tmp/walls": {
                "last_index": 2,
                "last_wallpaper": "/tmp/walls/c.png",
                "shuffle_history": []
            }
        },
        "last_command": "set old",
        "updated_at": "2026-05-08T00:00:00Z"
    });
    fs::write(
        &harness.state,
        serde_json::to_string_pretty(&state).unwrap(),
    )
    .unwrap();

    let output = harness.run(&["restore"]);
    assert_success(&output);

    assert!(wait_for_log_contains(
        &harness.log,
        &format!("swaybg -i {} -m fill -o DP-1", canonical.display())
    ));

    let restored: Value =
        serde_json::from_str(&fs::read_to_string(&harness.state).unwrap()).unwrap();
    assert_eq!(restored["active_backend"], "swaybg");
    assert_eq!(restored["last_command"], "restore");
    assert_eq!(restored["collections"]["/tmp/walls"]["last_index"], 2);
}

#[test]
fn restore_live_dry_run_prints_mpvpaper_commands_without_starting_processes() {
    let harness = CliHarness::new();
    let wallpaper = harness.root.join("rain.mp4");
    fs::write(&wallpaper, b"video").unwrap();
    let canonical = fs::canonicalize(&wallpaper).unwrap();
    let state = serde_json::json!({
        "version": 1,
        "active_backend": "mpvpaper",
        "mode": "live",
        "wallpaper": canonical.display().to_string(),
        "monitors": {
            "DP-1": {
                "backend": "mpvpaper",
                "wallpaper": canonical.display().to_string(),
                "pid": 12345
            },
            "HDMI-A-1": {
                "backend": "mpvpaper",
                "wallpaper": canonical.display().to_string(),
                "pid": 12346
            }
        },
        "collections": {},
        "last_command": "set old",
        "updated_at": "2026-05-08T00:00:00Z"
    });
    fs::write(
        &harness.state,
        serde_json::to_string_pretty(&state).unwrap(),
    )
    .unwrap();

    let output = harness.run(&["restore", "--dry-run"]);
    assert_success(&output);

    let stdout = CliHarness::stdout(&output);
    assert!(stdout.contains("restore backend: mpvpaper"));
    assert!(stdout.contains(&format!(
        "mpvpaper --mpv-options 'loop no-audio panscan=1.0' DP-1 {}",
        canonical.display()
    )));
    assert!(stdout.contains(&format!(
        "mpvpaper --mpv-options 'loop no-audio panscan=1.0' HDMI-A-1 {}",
        canonical.display()
    )));
    assert!(!harness.log.exists());
}

#[test]
fn restore_mixed_dry_run_uses_per_monitor_backend_and_wallpaper() {
    let harness = CliHarness::new();
    let static_wallpaper = harness.root.join("wall.png");
    let live_wallpaper = harness.root.join("rain.mp4");
    write_png(&static_wallpaper);
    fs::write(&live_wallpaper, b"video").unwrap();
    let static_canonical = fs::canonicalize(&static_wallpaper).unwrap();
    let live_canonical = fs::canonicalize(&live_wallpaper).unwrap();
    let state = serde_json::json!({
        "version": 1,
        "active_backend": "swaybg",
        "mode": "static",
        "wallpaper": "/tmp/top-level-should-not-be-used.png",
        "monitors": {
            "DP-1": {
                "backend": "swaybg",
                "wallpaper": static_canonical.display().to_string(),
                "pid": 200
            },
            "HDMI-A-1": {
                "backend": "mpvpaper",
                "wallpaper": live_canonical.display().to_string(),
                "pid": 12345
            }
        },
        "collections": {},
        "last_command": "set old",
        "updated_at": "2026-05-08T00:00:00Z"
    });
    fs::write(
        &harness.state,
        serde_json::to_string_pretty(&state).unwrap(),
    )
    .unwrap();

    let output = harness.run(&["restore", "--dry-run"]);
    assert_success(&output);

    let stdout = CliHarness::stdout(&output);
    assert!(stdout.contains("planned backend: swaybg"));
    assert!(stdout.contains("monitor: DP-1"));
    assert!(stdout.contains(&format!("wallpaper: {}", static_canonical.display())));
    assert!(stdout.contains("planned backend: mpvpaper"));
    assert!(stdout.contains("monitor: HDMI-A-1"));
    assert!(stdout.contains(&format!(
        "mpvpaper --mpv-options 'loop no-audio panscan=1.0' HDMI-A-1 {}",
        live_canonical.display()
    )));
    assert!(!harness.log.exists());
}

#[test]
fn doctor_reports_paths_backends_monitors_and_stale_state() {
    let harness = CliHarness::new();
    harness.fake_doctor_commands();
    let state = serde_json::json!({
        "version": 1,
        "active_backend": "mpvpaper",
        "mode": "live",
        "wallpaper": "/tmp/rain.mp4",
        "monitors": {
            "DP-1": {
                "backend": "mpvpaper",
                "wallpaper": "/tmp/rain.mp4",
                "pid": 999999
            }
        },
        "collections": {},
        "last_command": "set old",
        "updated_at": "2026-05-08T00:00:00Z"
    });
    fs::write(
        &harness.state,
        serde_json::to_string_pretty(&state).unwrap(),
    )
    .unwrap();

    let output = harness.run(&["doctor"]);
    assert_success(&output);

    let stdout = CliHarness::stdout(&output);
    assert!(stdout.contains(&format!("config: {}", harness.config.display())));
    assert!(stdout.contains("swaybg available: true"));
    assert!(stdout.contains("active monitors: 1"));
    assert!(stdout.contains("  DP-1"));
    assert!(stdout.contains("saved backend: mpvpaper"));
    assert!(stdout.contains("saved monitor DP-1: backend=mpvpaper, pid_status=stale"));
    assert!(stdout.contains("stale owned live pids: 1"));
}

#[test]
fn stop_live_terminates_only_recorded_mpvpaper_pids_and_clears_them() {
    let harness = CliHarness::new();
    harness.fake_kill_all_success();
    let state = serde_json::json!({
        "version": 1,
        "active_backend": "mpvpaper",
        "mode": "live",
        "wallpaper": "/tmp/rain.mp4",
        "monitors": {
            "DP-1": {
                "backend": "mpvpaper",
                "wallpaper": "/tmp/rain.mp4",
                "pid": 111
            },
            "HDMI-A-1": {
                "backend": "mpvpaper",
                "wallpaper": "/tmp/rain.mp4",
                "pid": 222
            },
            "DP-3": {
                "backend": "swaybg",
                "wallpaper": "/tmp/wall.png",
                "pid": 333
            }
        },
        "collections": {},
        "last_command": "set old",
        "updated_at": "2026-05-08T00:00:00Z"
    });
    fs::write(
        &harness.state,
        serde_json::to_string_pretty(&state).unwrap(),
    )
    .unwrap();

    let output = harness.run(&["stop-live"]);
    assert_success(&output);

    assert!(CliHarness::stdout(&output).contains("stopped 2 owned live wallpaper process(es)"));
    let log = fs::read_to_string(&harness.log).unwrap();
    assert!(log.contains("kill 111"));
    assert!(log.contains("kill 222"));
    assert!(!log.contains("kill 333"));

    let state: Value = serde_json::from_str(&fs::read_to_string(&harness.state).unwrap()).unwrap();
    assert_eq!(state["monitors"]["DP-1"]["pid"], Value::Null);
    assert_eq!(state["monitors"]["HDMI-A-1"]["pid"], Value::Null);
    assert_eq!(state["monitors"]["DP-3"]["pid"], 333);
    assert_eq!(state["last_command"], "stop");
}

#[test]
fn static_swaybg_set_for_named_monitor_stops_only_that_live_pid() {
    let harness = CliHarness::new();
    harness.fake_swaybg();
    harness.fake_hyprctl_monitors();
    harness.fake_kill_all_success();
    harness.fake_ps_with_omarchy_swaybg();
    let wallpaper = harness.root.join("wall.png");
    write_png(&wallpaper);
    let canonical = fs::canonicalize(&wallpaper).unwrap();
    let state = serde_json::json!({
        "version": 1,
        "active_backend": "mpvpaper",
        "mode": "live",
        "wallpaper": "/tmp/rain.mp4",
        "monitors": {
            "DP-1": {
                "backend": "mpvpaper",
                "wallpaper": "/tmp/rain.mp4",
                "pid": 111
            },
            "HDMI-A-1": {
                "backend": "mpvpaper",
                "wallpaper": "/tmp/rain.mp4",
                "pid": 222
            }
        },
        "collections": {},
        "last_command": "set old",
        "updated_at": "2026-05-08T00:00:00Z"
    });
    fs::write(
        &harness.state,
        serde_json::to_string_pretty(&state).unwrap(),
    )
    .unwrap();

    let output = harness.run(&[
        "set",
        wallpaper.to_str().unwrap(),
        "--backend",
        "swaybg",
        "--monitor",
        "DP-1",
    ]);
    assert_success(&output);

    let log = fs::read_to_string(&harness.log).unwrap();
    assert!(log.contains("kill 111"));
    assert!(!log.contains("kill 222"));
    assert!(!log.contains("kill 444"));

    let state: Value = serde_json::from_str(&fs::read_to_string(&harness.state).unwrap()).unwrap();
    assert_eq!(state["active_backend"], "swaybg");
    assert_eq!(state["monitors"]["DP-1"]["backend"], "swaybg");
    assert_eq!(
        state["monitors"]["DP-1"]["wallpaper"],
        canonical.display().to_string()
    );
    assert!(state["monitors"]["DP-1"]["pid"].as_u64().is_some());
    assert_eq!(state["monitors"]["HDMI-A-1"]["backend"], "mpvpaper");
    assert_eq!(state["monitors"]["HDMI-A-1"]["pid"], 222);
    assert_eq!(read_link(&harness.omarchy_current_background()), canonical);
}

#[test]
fn static_swaybg_set_for_secondary_monitor_preserves_existing_omarchy_background_link() {
    let harness = CliHarness::new();
    harness.fake_swaybg();
    harness.fake_hyprctl_monitors();
    harness.fake_ps_without_omarchy_swaybg();
    let old_wallpaper = harness.root.join("old.png");
    let wallpaper = harness.root.join("wall.png");
    write_png(&old_wallpaper);
    write_png(&wallpaper);
    let link = harness.omarchy_current_background();
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    symlink(&old_wallpaper, &link).unwrap();

    let output = harness.run(&[
        "set",
        wallpaper.to_str().unwrap(),
        "--backend",
        "swaybg",
        "--monitor",
        "HDMI-A-1",
    ]);
    assert_success(&output);

    assert_eq!(read_link(&link), old_wallpaper);
}

#[test]
fn static_swaybg_set_for_all_replaces_existing_omarchy_background_link() {
    let harness = CliHarness::new();
    harness.fake_swaybg();
    harness.fake_hyprctl_monitors();
    harness.fake_ps_without_omarchy_swaybg();
    let old_wallpaper = harness.root.join("old.png");
    let wallpaper = harness.root.join("wall.png");
    write_png(&old_wallpaper);
    write_png(&wallpaper);
    let link = harness.omarchy_current_background();
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    symlink(&old_wallpaper, &link).unwrap();

    let output = harness.run(&[
        "set",
        wallpaper.to_str().unwrap(),
        "--backend",
        "swaybg",
        "--monitor",
        "all",
    ]);
    assert_success(&output);

    assert_eq!(read_link(&link), fs::canonicalize(&wallpaper).unwrap());
}

#[test]
fn adopt_omarchy_claims_current_background_as_per_monitor_swaybg_state() {
    let harness = CliHarness::new();
    harness.fake_swaybg();
    harness.fake_hyprctl_monitors();
    harness.fake_ps_with_omarchy_swaybg();
    harness.fake_kill_all_success();
    let wallpaper = harness.root.join("wall.png");
    write_png(&wallpaper);
    let link = harness.omarchy_current_background();
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    symlink(&wallpaper, &link).unwrap();

    let output = harness.run(&["adopt-omarchy", "--json"]);
    assert_success(&output);

    let value: Value = serde_json::from_str(&CliHarness::stdout(&output)).unwrap();
    assert_eq!(value["adopted"], true);
    assert_eq!(
        value["wallpaper"],
        fs::canonicalize(&wallpaper).unwrap().display().to_string()
    );

    let log = fs::read_to_string(&harness.log).unwrap();
    assert!(log.contains("kill 444"));
    assert!(log.contains("swaybg -i"));
    assert!(log.contains("-o DP-1"));
    assert!(log.contains("-o HDMI-A-1"));

    let state: Value = serde_json::from_str(&fs::read_to_string(&harness.state).unwrap()).unwrap();
    assert_eq!(state["active_backend"], "swaybg");
    assert_eq!(state["monitors"]["DP-1"]["wallpaper"], value["wallpaper"]);
    assert_eq!(
        state["monitors"]["HDMI-A-1"]["wallpaper"],
        value["wallpaper"]
    );
    assert!(state["monitors"]["DP-1"]["pid"].as_u64().is_some());
    assert!(state["monitors"]["HDMI-A-1"]["pid"].as_u64().is_some());
}

#[test]
fn live_set_does_not_update_omarchy_background_link() {
    let harness = CliHarness::new();
    harness.fake_hyprctl_monitors();
    harness.fake_ps_without_omarchy_swaybg();
    harness.fake_kill_all_success();
    harness.fake_program(
        "mpvpaper",
        r#"#!/usr/bin/env bash
printf 'mpvpaper %s\n' "$*" >>"${JOBOWALLS_TEST_LOG:?}"
if [[ "$*" == "--help" ]]; then
  exit 0
fi
if [[ "$*" == "--help-output" ]]; then
  printf 'DP-1\n'
  exit 0
fi
socket="${2#*input-ipc-server=}"
socket="${socket%% *}"
python3 - "$socket" <<'PY'
import os
import socket
import sys
import time

path = sys.argv[1]
try:
    os.unlink(path)
except FileNotFoundError:
    pass

server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(path)
server.listen(1)
deadline = time.time() + 2
while time.time() < deadline:
    server.settimeout(max(0.1, deadline - time.time()))
    try:
        conn, _ = server.accept()
    except TimeoutError:
        continue
    with conn:
        conn.recv(4096)
        conn.sendall(b'{"request_id":1,"error":"success","data":{"w":3840,"h":2160}}\n')
server.close()
PY
"#,
    );
    let old_wallpaper = harness.root.join("old.png");
    let wallpaper = harness.root.join("live.mp4");
    write_png(&old_wallpaper);
    fs::write(&wallpaper, b"not really mp4").unwrap();
    let link = harness.omarchy_current_background();
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    symlink(&old_wallpaper, &link).unwrap();

    let output = harness.run(&[
        "set",
        wallpaper.to_str().unwrap(),
        "--backend",
        "mpvpaper",
        "--monitor",
        "DP-1",
    ]);
    assert_success(&output);

    assert_eq!(read_link(&link), old_wallpaper);
}

#[test]
fn static_swaybg_set_fails_when_spawned_process_exits_immediately() {
    let harness = CliHarness::new();
    harness.fake_swaybg_exits_immediately();
    harness.fake_ps_without_omarchy_swaybg();
    let wallpaper = harness.root.join("wall.png");
    write_png(&wallpaper);

    let output = harness.run(&[
        "set",
        wallpaper.to_str().unwrap(),
        "--backend",
        "swaybg",
        "--monitor",
        "DP-1",
    ]);
    assert_failure(&output);

    assert!(CliHarness::stderr(&output).contains("swaybg exited immediately after start"));
    assert!(!harness.state.exists());
}
