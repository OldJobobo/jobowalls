use crate::{backends::model::Backend, media::MediaKind, orchestrator::SetPlan};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub version: u8,
    pub active_backend: Backend,
    pub mode: MediaKind,
    pub wallpaper: String,
    pub monitors: BTreeMap<String, MonitorState>,
    #[serde(default)]
    pub collections: BTreeMap<String, CollectionState>,
    #[serde(default)]
    pub last_command: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl State {
    pub fn from_set_plan(plan: &SetPlan, pid: Option<u32>) -> Self {
        Self::from_monitor_entries(plan, [(plan.monitor.clone(), pid)])
    }

    pub fn from_monitor_entries(
        plan: &SetPlan,
        entries: impl IntoIterator<Item = (String, Option<u32>)>,
    ) -> Self {
        let mut monitors = BTreeMap::new();

        for (monitor, pid) in entries {
            monitors.insert(
                monitor,
                MonitorState {
                    backend: plan.backend,
                    wallpaper: plan.wallpaper.display().to_string(),
                    pid,
                },
            );
        }

        if monitors.is_empty() {
            monitors.insert(
                plan.monitor.clone(),
                MonitorState {
                    backend: plan.backend,
                    wallpaper: plan.wallpaper.display().to_string(),
                    pid: None,
                },
            );
        }

        Self {
            version: 1,
            active_backend: plan.backend,
            mode: plan.media_kind,
            wallpaper: plan.wallpaper.display().to_string(),
            monitors,
            collections: BTreeMap::new(),
            last_command: None,
            updated_at: OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc()),
        }
    }

    pub fn from_restored_entries(
        active_backend: Backend,
        mode: MediaKind,
        wallpaper: String,
        entries: impl IntoIterator<Item = (String, Option<u32>)>,
    ) -> Self {
        let monitors = entries
            .into_iter()
            .map(|(monitor, pid)| {
                (
                    monitor,
                    MonitorState {
                        backend: active_backend,
                        wallpaper: wallpaper.clone(),
                        pid,
                    },
                )
            })
            .collect();

        Self {
            version: 1,
            active_backend,
            mode,
            wallpaper,
            monitors,
            collections: BTreeMap::new(),
            last_command: None,
            updated_at: OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc()),
        }
    }

    pub fn merged_with_monitor_entries(
        existing: Option<&State>,
        plan: &SetPlan,
        entries: impl IntoIterator<Item = (String, Option<u32>)>,
    ) -> Self {
        let mut state = existing
            .cloned()
            .unwrap_or_else(|| Self::from_set_plan(plan, None));
        state.active_backend = plan.backend;
        state.mode = plan.media_kind;
        state.wallpaper = plan.wallpaper.display().to_string();

        if plan.monitor == "all" {
            state.monitors.clear();
        }

        let mut changed = false;
        for (monitor, pid) in entries {
            changed = true;
            state.monitors.insert(
                monitor,
                MonitorState {
                    backend: plan.backend,
                    wallpaper: plan.wallpaper.display().to_string(),
                    pid,
                },
            );
        }

        if !changed {
            state.monitors.insert(
                plan.monitor.clone(),
                MonitorState {
                    backend: plan.backend,
                    wallpaper: plan.wallpaper.display().to_string(),
                    pid: None,
                },
            );
        }

        state.updated_at =
            OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        state
    }

    pub fn single_monitor_plan(&self) -> Result<SetPlan> {
        let monitor = self
            .monitors
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "all".to_string());

        Ok(SetPlan {
            wallpaper: self.wallpaper.clone().into(),
            media_kind: self.mode,
            backend: self.active_backend,
            monitor,
        })
    }

    pub fn monitor_plans(&self) -> Vec<SetPlan> {
        if self.monitors.is_empty() {
            return vec![SetPlan {
                wallpaper: self.wallpaper.clone().into(),
                media_kind: self.mode,
                backend: self.active_backend,
                monitor: "all".to_string(),
            }];
        }

        self.monitors
            .keys()
            .map(|monitor| SetPlan {
                wallpaper: self.wallpaper.clone().into(),
                media_kind: self.mode,
                backend: self.active_backend,
                monitor: monitor.clone(),
            })
            .collect()
    }

    pub fn clear_live_pids(&mut self) {
        for monitor in self.monitors.values_mut() {
            if monitor.backend == Backend::Mpvpaper {
                monitor.pid = None;
            }
        }
        self.updated_at = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    }

    pub fn clear_live_pids_for_monitors(&mut self, monitors: &[String]) {
        if monitors.iter().any(|monitor| monitor == "all") {
            self.clear_live_pids();
            return;
        }

        for monitor in monitors {
            if let Some(state) = self.monitors.get_mut(monitor)
                && state.backend == Backend::Mpvpaper
            {
                state.pid = None;
            }
        }
        self.updated_at = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    }

    pub fn clear_backend_pids_for_monitors(&mut self, backend: Backend, monitors: &[String]) {
        if monitors.iter().any(|monitor| monitor == "all") {
            for monitor in self.monitors.values_mut() {
                if monitor.backend == backend {
                    monitor.pid = None;
                }
            }
            self.updated_at =
                OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
            return;
        }

        for monitor in monitors {
            if let Some(state) = self.monitors.get_mut(monitor)
                && state.backend == backend
            {
                state.pid = None;
            }
        }
        self.updated_at = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    }

    pub fn record_last_command(&mut self, command: impl Into<String>) {
        self.last_command = Some(command.into());
        self.updated_at = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    }

    pub fn with_last_command(mut self, command: impl Into<String>) -> Self {
        self.record_last_command(command);
        self
    }

    pub fn record_collection(
        &mut self,
        collection: &Path,
        wallpaper: &Path,
        index: usize,
        shuffled: bool,
    ) {
        let collection = collection.display().to_string();
        let wallpaper = wallpaper.display().to_string();
        let entry = self.collections.entry(collection).or_default();

        entry.last_index = Some(index);
        entry.last_wallpaper = Some(wallpaper.clone());

        if shuffled {
            entry.shuffle_history.retain(|seen| seen != &wallpaper);
            entry.shuffle_history.push(wallpaper);
        }

        self.updated_at = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    }

    pub fn load(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }

        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read state {}", path.display()))?;
        let state = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse state {}", path.display()))?;
        Ok(Some(state))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create state dir {}", parent.display()))?;
        }

        let raw = serde_json::to_string_pretty(self).context("failed to serialize state")?;
        fs::write(path, raw).with_context(|| format!("failed to write state {}", path.display()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorState {
    pub backend: Backend,
    pub wallpaper: String,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionState {
    pub last_index: Option<usize>,
    pub last_wallpaper: Option<String>,
    #[serde(default)]
    pub shuffle_history: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn saves_and_loads_state() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let state = State {
            version: 1,
            active_backend: Backend::Mpvpaper,
            mode: MediaKind::Live,
            wallpaper: "/tmp/rain.mp4".to_string(),
            monitors: BTreeMap::new(),
            collections: BTreeMap::new(),
            last_command: None,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        };

        state.save(&path).unwrap();
        assert_eq!(State::load(&path).unwrap(), Some(state));
    }

    #[test]
    fn creates_state_from_set_plan() {
        let plan = SetPlan {
            wallpaper: "/tmp/wall.png".into(),
            media_kind: MediaKind::Static,
            backend: Backend::Hyprpaper,
            monitor: "all".to_string(),
        };

        let state = State::from_set_plan(&plan, None);

        assert_eq!(state.version, 1);
        assert_eq!(state.active_backend, Backend::Hyprpaper);
        assert_eq!(state.mode, MediaKind::Static);
        assert_eq!(state.monitors["all"].pid, None);
        assert_eq!(state.last_command, None);
    }

    #[test]
    fn creates_state_from_multiple_monitor_entries() {
        let plan = SetPlan {
            wallpaper: "/tmp/rain.mp4".into(),
            media_kind: MediaKind::Live,
            backend: Backend::Mpvpaper,
            monitor: "all".to_string(),
        };

        let state = State::from_monitor_entries(
            &plan,
            [
                ("DP-1".to_string(), Some(100)),
                ("DP-3".to_string(), Some(101)),
            ],
        );

        assert_eq!(state.monitors.len(), 2);
        assert_eq!(state.monitors["DP-1"].pid, Some(100));
        assert_eq!(state.monitors["DP-3"].pid, Some(101));
    }

    #[test]
    fn merges_named_monitor_without_dropping_other_outputs() {
        let live_plan = SetPlan {
            wallpaper: "/tmp/rain.mp4".into(),
            media_kind: MediaKind::Live,
            backend: Backend::Mpvpaper,
            monitor: "all".to_string(),
        };
        let existing = State::from_monitor_entries(
            &live_plan,
            [
                ("DP-1".to_string(), Some(100)),
                ("HDMI-A-1".to_string(), Some(101)),
            ],
        );
        let static_plan = SetPlan {
            wallpaper: "/tmp/wall.png".into(),
            media_kind: MediaKind::Static,
            backend: Backend::Hyprpaper,
            monitor: "DP-1".to_string(),
        };

        let state = State::merged_with_monitor_entries(
            Some(&existing),
            &static_plan,
            [("DP-1".to_string(), None)],
        );

        assert_eq!(state.monitors.len(), 2);
        assert_eq!(state.monitors["DP-1"].backend, Backend::Hyprpaper);
        assert_eq!(state.monitors["HDMI-A-1"].backend, Backend::Mpvpaper);
        assert_eq!(state.monitors["HDMI-A-1"].pid, Some(101));
    }

    #[test]
    fn clears_live_pids_only() {
        let plan = SetPlan {
            wallpaper: "/tmp/rain.mp4".into(),
            media_kind: MediaKind::Live,
            backend: Backend::Mpvpaper,
            monitor: "DP-1".to_string(),
        };
        let mut state = State::from_set_plan(&plan, Some(100));

        state.clear_live_pids();

        assert_eq!(state.monitors["DP-1"].pid, None);
    }

    #[test]
    fn records_collection_progress() {
        let plan = SetPlan {
            wallpaper: "/tmp/walls/a.jpg".into(),
            media_kind: MediaKind::Static,
            backend: Backend::Awww,
            monitor: "all".to_string(),
        };
        let mut state = State::from_set_plan(&plan, None);

        state.record_collection(
            Path::new("/tmp/walls"),
            Path::new("/tmp/walls/a.jpg"),
            2,
            true,
        );

        let collection = &state.collections["/tmp/walls"];
        assert_eq!(collection.last_index, Some(2));
        assert_eq!(
            collection.last_wallpaper.as_deref(),
            Some("/tmp/walls/a.jpg")
        );
        assert_eq!(collection.shuffle_history, ["/tmp/walls/a.jpg"]);
    }

    #[test]
    fn records_last_successful_command() {
        let plan = SetPlan {
            wallpaper: "/tmp/wall.png".into(),
            media_kind: MediaKind::Static,
            backend: Backend::Hyprpaper,
            monitor: "DP-1".to_string(),
        };
        let mut state = State::from_set_plan(&plan, None);

        state.record_last_command("set /tmp/wall.png");

        assert_eq!(state.last_command.as_deref(), Some("set /tmp/wall.png"));
    }
}
