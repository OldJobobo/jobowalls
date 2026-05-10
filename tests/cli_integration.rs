use serde_json::Value;
use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
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

    fn fake_hyprctl_hyprpaper(&self) {
        self.fake_program(
            "hyprctl",
            r#"#!/usr/bin/env bash
printf 'hyprctl %s\n' "$*" >>"${JOBOWALLS_TEST_LOG:?}"
if [[ "$*" == "hyprpaper listloaded" ]]; then
  exit 0
fi
if [[ "$*" == "-j monitors" ]]; then
  printf '[{"name":"DP-1"}]\n'
  exit 0
fi
exit 0
"#,
        );
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
fn forced_hyprpaper_set_runs_expected_command_sequence_and_writes_state() {
    let harness = CliHarness::new();
    harness.fake_hyprctl_hyprpaper();
    let wallpaper = harness.root.join("wall.png");
    write_png(&wallpaper);

    let output = harness.run(&[
        "set",
        wallpaper.to_str().unwrap(),
        "--backend",
        "hyprpaper",
        "--monitor",
        "DP-1",
    ]);
    assert_success(&output);

    let canonical = fs::canonicalize(&wallpaper).unwrap();
    let expected = vec![
        "hyprctl hyprpaper listloaded".to_string(),
        format!("hyprctl hyprpaper preload {}", canonical.display()),
        format!("hyprctl hyprpaper wallpaper DP-1,{}", canonical.display()),
        "hyprctl hyprpaper unload unused".to_string(),
    ];
    let log = fs::read_to_string(&harness.log).unwrap();
    let actual = log.lines().map(str::to_string).collect::<Vec<_>>();
    assert_eq!(actual, expected);

    let state: Value = serde_json::from_str(&fs::read_to_string(&harness.state).unwrap()).unwrap();
    assert_eq!(state["active_backend"], "hyprpaper");
    assert_eq!(state["mode"], "static");
    assert_eq!(state["wallpaper"], canonical.display().to_string());
    assert_eq!(state["monitors"]["DP-1"]["backend"], "hyprpaper");
    assert_eq!(
        state["last_command"],
        format!(
            "set {} --monitor DP-1 --backend hyprpaper",
            canonical.display()
        )
    );
}
