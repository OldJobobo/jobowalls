use anyhow::{Context, Result, bail};
use std::{path::Path, process::Command};

pub fn apply_wallpaper(path: &Path, monitor: &str) -> Result<()> {
    let output = Command::new("jobowalls")
        .arg("set")
        .arg(path)
        .arg("--monitor")
        .arg(monitor)
        .output()
        .with_context(|| "failed to run jobowalls set")?;

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
