use crate::{backends::mpvpaper, command::CommandSpec};
use anyhow::{Result, bail};
use std::time::Duration;

pub fn list() -> Result<String> {
    Ok(format!("{}\n", names()?.join("\n")))
}

pub fn names() -> Result<Vec<String>> {
    let hyprctl_json = CommandSpec::new("hyprctl", ["-j".into(), "monitors".into()]);
    match output_text_with_retries(&hyprctl_json, 3, Duration::from_millis(75)) {
        Ok(output) => {
            let names = parse_hyprctl_monitor_names_json(&output);
            if !names.is_empty() {
                return Ok(names);
            }

            let names = parse_hyprctl_monitor_text(&output);
            if !names.is_empty() {
                return Ok(names);
            }

            let hyprctl_text = CommandSpec::new("hyprctl", ["monitors".into()]);
            if let Ok(output) =
                output_text_with_retries(&hyprctl_text, 3, Duration::from_millis(75))
            {
                let names = parse_hyprctl_monitor_text(&output);
                if !names.is_empty() {
                    return Ok(names);
                }
            }
        }
        Err(hyprctl_json_error) => {
            let hyprctl_text = CommandSpec::new("hyprctl", ["monitors".into()]);
            if let Ok(output) =
                output_text_with_retries(&hyprctl_text, 3, Duration::from_millis(75))
            {
                let names = parse_hyprctl_monitor_text(&output);
                if !names.is_empty() {
                    return Ok(names);
                }
            }

            match mpvpaper::list_outputs_command().output_text() {
                Ok(output) => {
                    let names = output
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<_>>();

                    if !names.is_empty() {
                        return Ok(names);
                    }
                }
                Err(mpvpaper_error) => bail!(
                    "unable to list monitors; hyprctl failed with `{hyprctl_json_error}`, mpvpaper failed with `{mpvpaper_error}`"
                ),
            }
        }
    }

    bail!("hyprctl returned no monitor names")
}

fn output_text_with_retries(
    command: &CommandSpec,
    attempts: usize,
    delay: Duration,
) -> Result<String> {
    let attempts = attempts.max(1);
    let mut last_error = None;

    for attempt in 0..attempts {
        match command.output_text() {
            Ok(output) => return Ok(output),
            Err(error) => {
                last_error = Some(error);
                if attempt + 1 < attempts {
                    std::thread::sleep(delay);
                }
            }
        }
    }

    match last_error {
        Some(error) => Err(error),
        None => bail!("command `{command}` was not attempted"),
    }
}

fn parse_hyprctl_monitor_names_json(output: &str) -> Vec<String> {
    match serde_json::from_str::<serde_json::Value>(output) {
        Ok(monitors) => monitors
            .as_array()
            .map(|monitors| {
                monitors
                    .iter()
                    .filter_map(|monitor| monitor.get("name").and_then(|name| name.as_str()))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn parse_hyprctl_monitor_text(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.strip_prefix("Monitor ")?;
            let (name, _) = line.split_once(' ')?;
            Some(name.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hyprctl_json_monitor_names() {
        let output = r#"[{"name":"DP-1"},{"name":"HDMI-A-1"}]"#;

        assert_eq!(
            parse_hyprctl_monitor_names_json(output),
            ["DP-1".to_string(), "HDMI-A-1".to_string()]
        );
    }

    #[test]
    fn parses_hyprctl_text_monitor_names() {
        let output = "Monitor DP-1 (ID 0):\nMonitor HDMI-A-1 (ID 1):\n";

        assert_eq!(
            parse_hyprctl_monitor_text(output),
            ["DP-1".to_string(), "HDMI-A-1".to_string()]
        );
    }
}
