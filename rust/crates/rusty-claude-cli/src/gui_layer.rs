use crate::agent_layer::{AppRecord, ThreadRecord};
use crate::llm_layer::{LlmLayerSummary, LlmProfile};
use crate::ui_layer::DashboardUsage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiConnectionState {
    pub connected_line: String,
    pub model: String,
    pub provider: String,
    pub base_url: String,
    pub workspace: String,
    pub directory: String,
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiProfileCard {
    pub name: String,
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub key_source: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiThreadCard {
    pub name: String,
    pub folder: String,
    pub session_id: String,
    pub description: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiAppCard {
    pub name: String,
    pub command: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiWorkspaceSnapshot {
    pub connection: GuiConnectionState,
    pub llm_summary: LlmLayerSummary,
    pub profiles: Vec<GuiProfileCard>,
    pub threads: Vec<GuiThreadCard>,
    pub apps: Vec<GuiAppCard>,
    pub usage: Option<DashboardUsage>,
}

impl GuiWorkspaceSnapshot {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn from_state(
        connection: GuiConnectionState,
        llm_summary: LlmLayerSummary,
        profiles: &[LlmProfile],
        active_profile_name: Option<&str>,
        threads: &[ThreadRecord],
        active_thread_name: Option<&str>,
        apps: &[AppRecord],
        usage: Option<DashboardUsage>,
    ) -> Self {
        Self {
            connection,
            llm_summary,
            profiles: profiles
                .iter()
                .map(|profile| GuiProfileCard {
                    name: profile.name.clone(),
                    provider: profile.normalized_provider(),
                    model: profile.model.clone(),
                    base_url: profile
                        .base_url
                        .as_deref()
                        .unwrap_or("(provider default)")
                        .to_string(),
                    key_source: profile.key_source_label(),
                    is_active: active_profile_name == Some(profile.name.as_str()),
                })
                .collect(),
            threads: threads
                .iter()
                .map(|thread| GuiThreadCard {
                    name: thread.name.clone(),
                    folder: thread.folder.clone(),
                    session_id: thread.session_id.clone(),
                    description: thread
                        .description
                        .clone()
                        .unwrap_or_else(|| "-".to_string()),
                    is_active: active_thread_name == Some(thread.name.as_str()),
                })
                .collect(),
            apps: apps
                .iter()
                .map(|app| GuiAppCard {
                    name: app.name.clone(),
                    command: app.command.clone(),
                    description: app.description.clone(),
                })
                .collect(),
            usage,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GuiConnectionState, GuiWorkspaceSnapshot};
    use crate::agent_layer::{AppRecord, ThreadRecord};
    use crate::llm_layer::{LlmLayerSummary, LlmProfile, TurnTokenLimits};

    #[test]
    fn gui_snapshot_marks_active_items() {
        let snapshot = GuiWorkspaceSnapshot::from_state(
            GuiConnectionState {
                connected_line: "Connected: deepseek-chat via deepseek".to_string(),
                model: "deepseek-chat".to_string(),
                provider: "deepseek".to_string(),
                base_url: "https://api.deepseek.com".to_string(),
                workspace: "clean".to_string(),
                directory: "/tmp/project".to_string(),
                session_id: "session-1".to_string(),
            },
            LlmLayerSummary {
                active_profile_name: Some("deepseek-main".to_string()),
                active_provider: Some("deepseek".to_string()),
                active_model: Some("deepseek-chat".to_string()),
                active_base_url: Some("https://api.deepseek.com".to_string()),
                profile_count: 1,
                turn_token_limits: TurnTokenLimits::default(),
            },
            &[LlmProfile {
                name: "deepseek-main".to_string(),
                provider: "deepseek".to_string(),
                model: "deepseek-chat".to_string(),
                base_url: Some("https://api.deepseek.com".to_string()),
                api_key: None,
                api_key_env: Some("DEEPSEEK_API_KEY".to_string()),
                note: None,
            }],
            Some("deepseek-main"),
            &[ThreadRecord {
                name: "backend".to_string(),
                folder: "/tmp/project".to_string(),
                session_id: "thread-1".to_string(),
                session_path: "/tmp/project/.claw/thread.jsonl".to_string(),
                description: Some("API work".to_string()),
            }],
            Some("backend"),
            &[AppRecord {
                name: "review".to_string(),
                command: "/review".to_string(),
                description: "Run review".to_string(),
            }],
            None,
        );

        assert!(snapshot.profiles[0].is_active);
        assert!(snapshot.threads[0].is_active);
        assert_eq!(snapshot.apps[0].name, "review");
    }
}
