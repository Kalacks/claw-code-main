use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const AGENT_LAYER_FILE: &str = ".claw/agent-layer.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadRecord {
    pub name: String,
    pub folder: String,
    pub session_id: String,
    pub session_path: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRecord {
    pub name: String,
    pub command: String,
    pub description: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistedAgentLayer {
    #[serde(default)]
    threads: Vec<ThreadRecord>,
    #[serde(default)]
    active_thread: Option<String>,
    #[serde(default)]
    apps: Vec<AppRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLayerSummary {
    pub active_thread_name: Option<String>,
    pub thread_count: usize,
    pub app_count: usize,
}

#[derive(Debug, Clone)]
pub struct AgentWorkspaceStore {
    path: PathBuf,
    state: PersistedAgentLayer,
}

impl AgentWorkspaceStore {
    pub fn load_for(cwd: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let path = cwd.join(AGENT_LAYER_FILE);
        if !path.exists() {
            return Ok(Self {
                path,
                state: PersistedAgentLayer::default(),
            });
        }
        let raw = fs::read_to_string(&path)?;
        let state = serde_json::from_str::<PersistedAgentLayer>(&raw).unwrap_or_default();
        Ok(Self { path, state })
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, serde_json::to_string_pretty(&self.state)?)?;
        Ok(())
    }

    #[must_use]
    pub fn storage_path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn list_threads(&self) -> Vec<ThreadRecord> {
        let mut threads = self.state.threads.clone();
        threads.sort_by(|left, right| left.name.cmp(&right.name));
        threads
    }

    #[must_use]
    pub fn thread(&self, name: &str) -> Option<&ThreadRecord> {
        self.state.threads.iter().find(|thread| thread.name == name)
    }

    pub fn upsert_thread(
        &mut self,
        thread: ThreadRecord,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if thread.name.trim().is_empty() {
            return Err("thread name cannot be empty".into());
        }
        if thread.folder.trim().is_empty() {
            return Err("thread folder cannot be empty".into());
        }
        if thread.session_id.trim().is_empty() {
            return Err("thread session_id cannot be empty".into());
        }
        if thread.session_path.trim().is_empty() {
            return Err("thread session_path cannot be empty".into());
        }
        if let Some(existing) = self
            .state
            .threads
            .iter_mut()
            .find(|existing| existing.name == thread.name)
        {
            *existing = thread;
        } else {
            self.state.threads.push(thread);
        }
        self.save()
    }

    pub fn remove_thread(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let before = self.state.threads.len();
        self.state.threads.retain(|thread| thread.name != name);
        if before == self.state.threads.len() {
            return Err(format!("thread '{name}' does not exist").into());
        }
        if self.state.active_thread.as_deref() == Some(name) {
            self.state.active_thread = None;
        }
        self.save()
    }

    pub fn set_active_thread(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        if self.thread(name).is_none() {
            return Err(format!("thread '{name}' does not exist").into());
        }
        self.state.active_thread = Some(name.to_string());
        self.save()
    }

    pub fn clear_active_thread(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.state.active_thread = None;
        self.save()
    }

    #[must_use]
    pub fn active_thread_name(&self) -> Option<&str> {
        self.state.active_thread.as_deref()
    }

    #[must_use]
    pub fn summary(&self) -> AgentLayerSummary {
        AgentLayerSummary {
            active_thread_name: self.active_thread_name().map(ToOwned::to_owned),
            thread_count: self.state.threads.len(),
            app_count: self.state.apps.len(),
        }
    }

    #[must_use]
    pub fn list_apps(&self) -> Vec<AppRecord> {
        let mut apps = self.state.apps.clone();
        apps.sort_by(|left, right| left.name.cmp(&right.name));
        apps
    }

    #[must_use]
    pub fn app(&self, name: &str) -> Option<&AppRecord> {
        self.state.apps.iter().find(|app| app.name == name)
    }

    pub fn upsert_app(&mut self, app: AppRecord) -> Result<(), Box<dyn std::error::Error>> {
        if app.name.trim().is_empty() {
            return Err("app name cannot be empty".into());
        }
        if app.command.trim().is_empty() {
            return Err("app command cannot be empty".into());
        }
        if app.description.trim().is_empty() {
            return Err("app description cannot be empty".into());
        }
        if let Some(existing) = self
            .state
            .apps
            .iter_mut()
            .find(|existing| existing.name == app.name)
        {
            *existing = app;
        } else {
            self.state.apps.push(app);
        }
        self.save()
    }

    pub fn remove_app(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let before = self.state.apps.len();
        self.state.apps.retain(|app| app.name != name);
        if before == self.state.apps.len() {
            return Err(format!("app '{name}' does not exist").into());
        }
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::{AgentWorkspaceStore, AppRecord, ThreadRecord};
    use std::fs;

    fn temp_workspace(label: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let root = std::env::temp_dir().join(format!("claw-agent-layer-{label}-{nonce}"));
        fs::create_dir_all(&root).expect("temp workspace");
        root
    }

    #[test]
    fn thread_and_app_registry_round_trip() {
        let workspace = temp_workspace("roundtrip");
        let mut store = AgentWorkspaceStore::load_for(&workspace).expect("load store");

        store
            .upsert_thread(ThreadRecord {
                name: "backend".to_string(),
                folder: workspace.display().to_string(),
                session_id: "session-backend".to_string(),
                session_path: workspace
                    .join(".claw")
                    .join("sessions")
                    .join("session-backend.jsonl")
                    .display()
                    .to_string(),
                description: Some("API service".to_string()),
            })
            .expect("upsert thread");
        store.set_active_thread("backend").expect("activate thread");
        store
            .upsert_app(AppRecord {
                name: "review".to_string(),
                command: "/review".to_string(),
                description: "Run code review workflow".to_string(),
            })
            .expect("upsert app");

        let loaded = AgentWorkspaceStore::load_for(&workspace).expect("reload");
        assert_eq!(loaded.list_threads().len(), 1);
        assert_eq!(loaded.active_thread_name(), Some("backend"));
        assert_eq!(loaded.list_apps().len(), 1);

        let _ = fs::remove_dir_all(workspace);
    }
}
