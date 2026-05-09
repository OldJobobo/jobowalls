use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellState {
    pub version: u8,
    #[serde(default)]
    pub last_folder: Option<PathBuf>,
    #[serde(default = "default_monitor")]
    pub last_monitor: String,
    #[serde(default)]
    pub last_index_by_folder: BTreeMap<String, usize>,
}

impl ShellState {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default_with_version());
        }

        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read shell state {}", path.display()))?;
        let mut state: Self = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse shell state {}", path.display()))?;
        if state.version == 0 {
            state.version = 1;
        }
        if state.last_monitor.is_empty() {
            state.last_monitor = default_monitor();
        }
        Ok(state)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create shell state dir {}", parent.display())
            })?;
        }

        let raw = serde_json::to_string_pretty(self).context("failed to serialize shell state")?;
        fs::write(path, raw)
            .with_context(|| format!("failed to write shell state {}", path.display()))
    }

    pub fn default_with_version() -> Self {
        Self {
            version: 1,
            last_folder: None,
            last_monitor: default_monitor(),
            last_index_by_folder: BTreeMap::new(),
        }
    }

    pub fn remembered_index(&self, folder: &Path, len: usize) -> usize {
        self.last_index_by_folder
            .get(&folder.display().to_string())
            .copied()
            .unwrap_or_default()
            .min(len.saturating_sub(1))
    }

    pub fn remember(&mut self, folder: &Path, monitor: &str, index: usize) {
        self.version = 1;
        self.last_folder = Some(folder.to_path_buf());
        self.last_monitor = monitor.to_string();
        self.last_index_by_folder
            .insert(folder.display().to_string(), index);
    }
}

pub fn shell_state_path() -> PathBuf {
    dirs::state_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".local/state")
        })
        .join("jobowalls")
        .join("shell.json")
}

pub fn runtime_state_path() -> PathBuf {
    dirs::state_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".local/state")
        })
        .join("jobowalls")
        .join("state.json")
}

fn default_monitor() -> String {
    "all".to_string()
}

impl Default for ShellState {
    fn default() -> Self {
        Self::default_with_version()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_loads_shell_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shell.json");
        let mut state = ShellState::default();
        state.remember(Path::new("/tmp/walls"), "DP-1", 4);

        state.save(&path).unwrap();
        assert_eq!(ShellState::load(&path).unwrap(), state);
    }
}
