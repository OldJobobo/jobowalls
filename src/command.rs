use anyhow::{Context, Result, bail};
use std::{
    ffi::OsString,
    fmt,
    process::{Command, Stdio},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
}

impl CommandSpec {
    pub fn new(program: impl Into<OsString>, args: impl IntoIterator<Item = OsString>) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().collect(),
        }
    }

    pub fn run(&self) -> Result<()> {
        let output = Command::new(&self.program)
            .args(&self.args)
            .output()
            .with_context(|| format!("failed to start command `{self}`"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            bail!(
                "command `{self}` failed: {}",
                failure_message(&output.status, &stderr, &stdout)
            );
        }

        Ok(())
    }

    pub fn output_text(&self) -> Result<String> {
        let output = Command::new(&self.program)
            .args(&self.args)
            .output()
            .with_context(|| format!("failed to start command `{self}`"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            bail!(
                "command `{self}` failed: {}",
                failure_message(&output.status, &stderr, &stdout)
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    pub fn spawn_detached(&self) -> Result<u32> {
        let child = Command::new(&self.program)
            .args(&self.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to spawn command `{self}`"))?;

        Ok(child.id())
    }
}

impl fmt::Display for CommandSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.program.to_string_lossy())?;
        for arg in &self.args {
            write!(f, " {}", shell_escape(&arg.to_string_lossy()))?;
        }
        Ok(())
    }
}

pub fn run_all(commands: &[CommandSpec]) -> Result<()> {
    for command in commands {
        command.run()?;
    }
    Ok(())
}

pub fn program_available(program: &str) -> bool {
    Command::new(program).arg("--help").output().is_ok()
}

pub fn terminate_pid(pid: u32) -> Result<bool> {
    let output = Command::new("kill")
        .arg(pid.to_string())
        .output()
        .with_context(|| format!("failed to start command `kill {pid}`"))?;

    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("No such process") {
        return Ok(false);
    }

    bail!(
        "command `kill {pid}` failed: {}",
        failure_message(&output.status, &stderr, "")
    );
}

pub fn signal_pid(pid: u32, signal: &str) -> Result<bool> {
    let output = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(pid.to_string())
        .output()
        .with_context(|| format!("failed to start command `kill -{signal} {pid}`"))?;

    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("No such process") {
        return Ok(false);
    }

    bail!(
        "command `kill -{signal} {pid}` failed: {}",
        failure_message(&output.status, &stderr, "")
    );
}

pub fn pid_is_running(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn shell_escape(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "-_./:=,".contains(ch))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn failure_message(status: &std::process::ExitStatus, stderr: &str, stdout: &str) -> String {
    let stderr = stderr.trim();
    if !stderr.is_empty() {
        return stderr.to_string();
    }

    let stdout = stdout.trim();
    if !stdout.is_empty() {
        return stdout.to_string();
    }

    format!("exited with status {status}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_commands_with_escaped_args() {
        let command = CommandSpec::new(
            "hyprctl",
            [
                OsString::from("keyword"),
                OsString::from("DP-1,/tmp/a b.png"),
            ],
        );

        assert_eq!(command.to_string(), "hyprctl keyword 'DP-1,/tmp/a b.png'");
    }
}
