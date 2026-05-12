use anyhow::{Context, Result, bail};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const APPLY_TIMEOUT: Duration = Duration::from_secs(15);

pub fn apply_wallpaper(path: &Path, monitor: &str) -> Result<()> {
    let mut child = Command::new("jobowalls")
        .arg("set")
        .arg(path)
        .arg("--monitor")
        .arg(monitor)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| "failed to run jobowalls set")?;

    let deadline = Instant::now() + APPLY_TIMEOUT;
    loop {
        if child
            .try_wait()
            .context("failed to inspect jobowalls set process")?
            .is_some()
        {
            break;
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!(
                "jobowalls set timed out after {} seconds",
                APPLY_TIMEOUT.as_secs()
            );
        }

        std::thread::sleep(Duration::from_millis(50));
    }

    let output = child
        .wait_with_output()
        .context("failed to collect jobowalls set output")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = if !stderr.trim().is_empty() {
            stderr.trim()
        } else {
            stdout.trim()
        };
        bail!("jobowalls set failed: {message}");
    }

    Ok(())
}
