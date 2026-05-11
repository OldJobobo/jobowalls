use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::{TempDir, tempdir};

struct ScriptHarness {
    _dir: TempDir,
    root: PathBuf,
    prefix: PathBuf,
    bindir: PathBuf,
    appdir: PathBuf,
    fakebin: PathBuf,
}

impl ScriptHarness {
    fn new() -> Self {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let prefix = root.join("prefix");
        let bindir = prefix.join("bin");
        let appdir = prefix.join("share/applications");
        let fakebin = root.join("fakebin");
        fs::create_dir_all(&fakebin).unwrap();

        Self {
            _dir: dir,
            root,
            prefix,
            bindir,
            appdir,
            fakebin,
        }
    }

    fn path(&self) -> String {
        let existing = env::var_os("PATH").unwrap_or_default();
        format!("{}:{}", self.fakebin.display(), existing.to_string_lossy())
    }

    fn run_script(&self, script: &str, args: &[&str]) -> Output {
        let mut command = Command::new("bash");
        command
            .arg(script)
            .args(args)
            .env("PREFIX", &self.prefix)
            .env("BINDIR", &self.bindir)
            .env("APPDIR", &self.appdir)
            .env("PATH", self.path())
            .env("HOME", self.root.join("home"));
        command.output().unwrap()
    }

    fn run_install_with_default_user_prefix(&self) -> Output {
        let mut command = Command::new("bash");
        command
            .arg("install.sh")
            .env("PATH", self.path())
            .env("HOME", self.root.join("home"));
        command.output().unwrap()
    }

    fn fake_program(&self, name: &str, body: &str) {
        let path = self.fakebin.join(name);
        fs::write(&path, body).unwrap();
        make_executable(&path);
    }

    fn fake_mpvpaper_available(&self) {
        self.fake_program(
            "mpvpaper",
            r#"#!/usr/bin/env bash
exit 0
"#,
        );
    }

    fn fake_curl_failure(&self) {
        self.fake_program(
            "curl",
            r#"#!/usr/bin/env bash
echo "download failed intentionally" >&2
exit 22
"#,
        );
    }

    fn fake_source_build_tools(&self) {
        self.fake_program(
            "cargo",
            r#"#!/usr/bin/env bash
root="$(pwd)"
mkdir -p "$root/target/release" "$root/gui/src-tauri/target/release"
for bin in \
  "$root/target/release/jobowalls" \
  "$root/target/release/jobowalls-shell" \
  "$root/gui/src-tauri/target/release/jobowalls-gui"
do
  printf '#!/usr/bin/env bash\necho built\n' >"$bin"
  chmod 0755 "$bin"
done
"#,
        );
        self.fake_program(
            "npm",
            r#"#!/usr/bin/env bash
exit 0
"#,
        );
    }

    fn fake_curl_release_download(&self, archive: &Path) {
        let body = format!(
            r#"#!/usr/bin/env bash
out=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -o)
      out="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
if [[ -z "$out" ]]; then
  echo "missing -o" >&2
  exit 2
fi
cp "{}" "$out"
"#,
            archive.display()
        );
        self.fake_program("curl", &body);
    }
}

fn make_executable(path: &Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success\nstdout:\n{}\nstderr:\n{}",
        stdout(output),
        stderr(output)
    );
}

fn fixture_release_archive(root: &Path, old_gui_shape: bool) -> PathBuf {
    let fixture = root.join("release");
    fs::create_dir_all(fixture.join("bin")).unwrap();
    fs::create_dir_all(fixture.join("share/applications")).unwrap();

    for name in ["jobowalls", "jobowalls-shell"] {
        let path = fixture.join("bin").join(name);
        fs::write(&path, format!("#!/usr/bin/env bash\necho {name}\n")).unwrap();
        make_executable(&path);
    }

    if old_gui_shape {
        let path = fixture.join("bin/jobowalls-gui");
        fs::write(&path, "#!/usr/bin/env bash\necho gui-bin\n").unwrap();
        make_executable(&path);
    } else {
        for name in ["jobowalls-gui-bin", "jobowalls-gui"] {
            let path = fixture.join("bin").join(name);
            fs::write(&path, format!("#!/usr/bin/env bash\necho {name}\n")).unwrap();
            make_executable(&path);
        }
    }

    fs::write(
        fixture.join("share/applications/dev.jobowalls.picker.desktop"),
        "[Desktop Entry]\nType=Application\nExec=jobowalls-gui\n",
    )
    .unwrap();

    let archive = root.join("jobowalls-test.tar.gz");
    let status = Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&fixture)
        .arg(".")
        .status()
        .unwrap();
    assert!(status.success());
    archive
}

#[test]
fn install_help_exits_successfully() {
    let harness = ScriptHarness::new();

    let output = harness.run_script("install.sh", &["--help"]);

    assert_success(&output);
    assert!(stdout(&output).contains("Usage: ./install.sh"));
}

#[test]
fn uninstall_help_exits_successfully() {
    let harness = ScriptHarness::new();

    let output = harness.run_script("uninstall.sh", &["--help"]);

    assert_success(&output);
    assert!(stdout(&output).contains("Usage: ./uninstall.sh"));
}

#[test]
fn install_from_release_copies_binaries_and_rewrites_desktop_exec() {
    let harness = ScriptHarness::new();
    let archive = fixture_release_archive(&harness.root, false);
    harness.fake_curl_release_download(&archive);
    harness.fake_mpvpaper_available();

    let output = harness.run_script("install.sh", &[]);

    assert_success(&output);
    assert!(harness.bindir.join("jobowalls").exists());
    assert!(harness.bindir.join("jobowalls-shell").exists());
    assert!(harness.bindir.join("jobowalls-gui").exists());
    assert!(harness.bindir.join("jobowalls-gui-bin").exists());
    let desktop = fs::read_to_string(harness.appdir.join("dev.jobowalls.picker.desktop")).unwrap();
    assert!(desktop.contains(&format!("Exec={}/jobowalls-gui", harness.bindir.display())));
}

#[test]
fn install_from_old_release_shape_creates_gui_wrapper() {
    let harness = ScriptHarness::new();
    let archive = fixture_release_archive(&harness.root, true);
    harness.fake_curl_release_download(&archive);
    harness.fake_mpvpaper_available();

    let output = harness.run_script("install.sh", &[]);

    assert_success(&output);
    assert!(harness.bindir.join("jobowalls-gui-bin").exists());
    let wrapper = fs::read_to_string(harness.bindir.join("jobowalls-gui")).unwrap();
    assert!(wrapper.contains("WEBKIT_DISABLE_DMABUF_RENDERER"));
}

#[test]
fn install_falls_back_to_source_build_when_release_download_fails() {
    let harness = ScriptHarness::new();
    harness.fake_curl_failure();
    harness.fake_source_build_tools();
    harness.fake_mpvpaper_available();

    let output = harness.run_script("install.sh", &[]);

    assert_success(&output);
    assert!(stdout(&output).contains("falling back to source build"));
    assert!(harness.bindir.join("jobowalls").exists());
    assert!(harness.bindir.join("jobowalls-shell").exists());
    assert!(harness.bindir.join("jobowalls-gui-bin").exists());
    assert!(harness.bindir.join("jobowalls-gui").exists());
}

#[test]
fn install_rejects_unsupported_source_build_profile() {
    let harness = ScriptHarness::new();
    harness.fake_mpvpaper_available();

    let mut command = Command::new("bash");
    let output = command
        .arg("install.sh")
        .env("BUILD_FROM_SOURCE", "1")
        .env("PROFILE", "weird")
        .env("PREFIX", &harness.prefix)
        .env("BINDIR", &harness.bindir)
        .env("APPDIR", &harness.appdir)
        .env("PATH", harness.path())
        .env("HOME", harness.root.join("home"))
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(stderr(&output).contains("unsupported PROFILE: weird"));
}

#[test]
fn install_default_user_path_update_is_idempotent() {
    let harness = ScriptHarness::new();
    let archive = fixture_release_archive(&harness.root, false);
    harness.fake_curl_release_download(&archive);
    harness.fake_mpvpaper_available();
    fs::create_dir_all(harness.root.join("home")).unwrap();

    let first = harness.run_install_with_default_user_prefix();
    assert_success(&first);
    let second = harness.run_install_with_default_user_prefix();
    assert_success(&second);

    let profile = fs::read_to_string(harness.root.join("home/.profile")).unwrap();
    assert_eq!(
        profile
            .matches("# jobowalls: add user-local binaries to PATH")
            .count(),
        1
    );
    assert!(profile.contains("export PATH=\"$HOME/.local/bin:$PATH\""));
}

#[test]
fn uninstall_removes_installed_files_and_is_idempotent() {
    let harness = ScriptHarness::new();
    fs::create_dir_all(&harness.bindir).unwrap();
    fs::create_dir_all(&harness.appdir).unwrap();
    for path in [
        harness.bindir.join("jobowalls"),
        harness.bindir.join("jobowalls-shell"),
        harness.bindir.join("jobowalls-gui"),
        harness.bindir.join("jobowalls-gui-bin"),
        harness.appdir.join("dev.jobowalls.picker.desktop"),
    ] {
        fs::write(path, "").unwrap();
    }

    let output = harness.run_script("uninstall.sh", &[]);
    assert_success(&output);
    assert!(!harness.bindir.join("jobowalls").exists());
    assert!(!harness.bindir.join("jobowalls-shell").exists());
    assert!(!harness.bindir.join("jobowalls-gui").exists());
    assert!(!harness.bindir.join("jobowalls-gui-bin").exists());
    assert!(!harness.appdir.join("dev.jobowalls.picker.desktop").exists());

    let output = harness.run_script("uninstall.sh", &[]);
    assert_success(&output);
    assert!(stdout(&output).contains("nothing to remove"));
}
