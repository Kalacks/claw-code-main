#[allow(dead_code)]
#[path = "../agent_layer.rs"]
mod agent_layer;
#[allow(dead_code)]
#[path = "../llm_layer.rs"]
mod llm_layer;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use agent_layer::{AgentWorkspaceStore, AppRecord, ThreadRecord};
use eframe::egui;
use llm_layer::{LlmProfile, LlmProfileStore, TurnTokenLimits};
use runtime::session_control::ManagedSessionSummary;
use runtime::{Session, SessionStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Overview,
    Models,
    Threads,
    Apps,
    Sessions,
}

impl Tab {
    fn entries() -> [(Self, &'static str); 5] {
        [
            (Self::Overview, "Overview"),
            (Self::Models, "Models"),
            (Self::Threads, "Threads"),
            (Self::Apps, "Apps"),
            (Self::Sessions, "Sessions"),
        ]
    }
}

#[derive(Debug, Clone, Default)]
struct LlmForm {
    name: String,
    provider: String,
    model: String,
    api_key: String,
    api_key_env: String,
    base_url: String,
}

#[derive(Debug, Clone, Default)]
struct LimitsForm {
    min_input: String,
    max_input: String,
    min_output: String,
    max_output: String,
}

#[derive(Debug, Clone, Default)]
struct ThreadForm {
    name: String,
    folder: String,
    description: String,
}

#[derive(Debug, Clone, Default)]
struct AppForm {
    name: String,
    command: String,
    description: String,
}

#[derive(Debug)]
struct ClawGuiApp {
    workspace_input: String,
    workspace: PathBuf,
    active_tab: Tab,
    llm_store: Option<LlmProfileStore>,
    agent_store: Option<AgentWorkspaceStore>,
    sessions: Vec<ManagedSessionSummary>,
    llm_form: LlmForm,
    limits_form: LimitsForm,
    thread_form: ThreadForm,
    app_form: AppForm,
    notice: Option<String>,
    error: Option<String>,
}

impl ClawGuiApp {
    fn new() -> Self {
        let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut app = Self {
            workspace_input: workspace.display().to_string(),
            workspace,
            active_tab: Tab::Overview,
            llm_store: None,
            agent_store: None,
            sessions: Vec::new(),
            llm_form: LlmForm {
                provider: "deepseek".to_string(),
                model: "deepseek-chat".to_string(),
                base_url: "https://api.deepseek.com".to_string(),
                ..LlmForm::default()
            },
            limits_form: LimitsForm::default(),
            thread_form: ThreadForm::default(),
            app_form: AppForm::default(),
            notice: None,
            error: None,
        };
        app.reload();
        app
    }

    fn clear_messages(&mut self) {
        self.notice = None;
        self.error = None;
    }

    fn set_notice(&mut self, message: impl Into<String>) {
        self.notice = Some(message.into());
        self.error = None;
    }

    fn set_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
        self.notice = None;
    }

    fn parse_workspace(&self) -> Result<PathBuf, String> {
        let raw = self.workspace_input.trim();
        if raw.is_empty() {
            return Err("workspace path cannot be empty".to_string());
        }
        let path = PathBuf::from(raw);
        if !path.exists() {
            return Err(format!("workspace does not exist: {}", path.display()));
        }
        if !path.is_dir() {
            return Err(format!("workspace is not a directory: {}", path.display()));
        }
        Ok(path)
    }

    fn reload(&mut self) {
        self.clear_messages();
        let workspace = match self.parse_workspace() {
            Ok(path) => path,
            Err(error) => {
                self.set_error(error);
                return;
            }
        };
        self.workspace = workspace.clone();

        let llm_store = match LlmProfileStore::load_for(&workspace) {
            Ok(store) => store,
            Err(error) => {
                self.set_error(format!("failed to load llm layer: {error}"));
                return;
            }
        };
        self.load_limit_form(llm_store.turn_token_limits());

        let agent_store = match AgentWorkspaceStore::load_for(&workspace) {
            Ok(store) => store,
            Err(error) => {
                self.set_error(format!("failed to load agent layer: {error}"));
                return;
            }
        };

        let sessions = SessionStore::from_cwd(&workspace)
            .and_then(|store| store.list_sessions())
            .unwrap_or_default();

        self.llm_store = Some(llm_store);
        self.agent_store = Some(agent_store);
        self.sessions = sessions;
    }

    fn load_limit_form(&mut self, limits: TurnTokenLimits) {
        self.limits_form.min_input = optional_u32_text(limits.min_input_tokens);
        self.limits_form.max_input = optional_u32_text(limits.max_input_tokens);
        self.limits_form.min_output = optional_u32_text(limits.min_output_tokens);
        self.limits_form.max_output = optional_u32_text(limits.max_output_tokens);
    }

    fn persist_llm_profile(&mut self, env_mode: bool) {
        self.clear_messages();
        let Some(store) = self.llm_store.as_mut() else {
            self.set_error("llm store is not loaded");
            return;
        };

        let name = self.llm_form.name.trim();
        let provider = self.llm_form.provider.trim();
        let model = self.llm_form.model.trim();
        if name.is_empty() || provider.is_empty() || model.is_empty() {
            self.set_error("name/provider/model cannot be empty");
            return;
        }
        let key_env = self.llm_form.api_key_env.trim();
        let key_inline = self.llm_form.api_key.trim();
        if env_mode && key_env.is_empty() {
            self.set_error("api_key_env is required when using env mode");
            return;
        }
        if !env_mode && key_inline.is_empty() {
            self.set_error("api_key is required when using inline mode");
            return;
        }

        let profile = LlmProfile {
            name: name.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            base_url: optional_text(&self.llm_form.base_url),
            api_key: if env_mode {
                None
            } else {
                Some(key_inline.to_string())
            },
            api_key_env: if env_mode {
                Some(key_env.to_string())
            } else {
                None
            },
            note: None,
        };

        if let Err(error) = store.upsert_profile(profile) {
            self.set_error(format!("failed to save profile: {error}"));
            return;
        }
        self.set_notice("profile saved");
        self.reload();
    }

    fn activate_profile(&mut self, name: &str) {
        self.clear_messages();
        let Some(store) = self.llm_store.as_mut() else {
            self.set_error("llm store is not loaded");
            return;
        };
        match store.set_active_profile(name) {
            Ok(()) => {
                self.set_notice(format!("active profile switched to {name}"));
                self.reload();
            }
            Err(error) => self.set_error(format!("failed to activate profile: {error}")),
        }
    }

    fn remove_profile(&mut self, name: &str) {
        self.clear_messages();
        let Some(store) = self.llm_store.as_mut() else {
            self.set_error("llm store is not loaded");
            return;
        };
        match store.remove_profile(name) {
            Ok(()) => {
                self.set_notice(format!("profile removed: {name}"));
                self.reload();
            }
            Err(error) => self.set_error(format!("failed to remove profile: {error}")),
        }
    }

    fn persist_limits(&mut self) {
        self.clear_messages();
        let limits = match (
            parse_optional_u32(&self.limits_form.min_input),
            parse_optional_u32(&self.limits_form.max_input),
            parse_optional_u32(&self.limits_form.min_output),
            parse_optional_u32(&self.limits_form.max_output),
        ) {
            (Ok(min_input), Ok(max_input), Ok(min_output), Ok(max_output)) => TurnTokenLimits {
                min_input_tokens: min_input,
                max_input_tokens: max_input,
                min_output_tokens: min_output,
                max_output_tokens: max_output,
            },
            (Err(error), _, _, _)
            | (_, Err(error), _, _)
            | (_, _, Err(error), _)
            | (_, _, _, Err(error)) => {
                self.set_error(error);
                return;
            }
        };
        let limits = match limits.validate() {
            Ok(validated) => validated,
            Err(error) => {
                self.set_error(error);
                return;
            }
        };
        let Some(store) = self.llm_store.as_mut() else {
            self.set_error("llm store is not loaded");
            return;
        };
        match store.set_turn_token_limits(limits) {
            Ok(()) => {
                self.set_notice("token limits saved");
                self.reload();
            }
            Err(error) => self.set_error(format!("failed to save limits: {error}")),
        }
    }

    fn clear_limits(&mut self) {
        self.limits_form = LimitsForm::default();
        self.persist_limits();
    }

    fn add_thread(&mut self) {
        self.clear_messages();
        let Some(store) = self.agent_store.as_mut() else {
            self.set_error("thread store is not loaded");
            return;
        };

        let name = self.thread_form.name.trim();
        let folder_input = self.thread_form.folder.trim();
        if name.is_empty() || folder_input.is_empty() {
            self.set_error("thread name/folder cannot be empty");
            return;
        }
        let folder = absolute_folder(&self.workspace, folder_input);
        if let Err(error) = std::fs::create_dir_all(&folder) {
            self.set_error(format!("failed to create folder: {error}"));
            return;
        }
        let session_id = format!("thread-{}-{}", sanitize_id(name), epoch_millis());
        let handle =
            match SessionStore::from_cwd(&folder).map(|store| store.create_handle(&session_id)) {
                Ok(handle) => handle,
                Err(error) => {
                    self.set_error(format!("failed to prepare session store: {error}"));
                    return;
                }
            };

        if !handle.path.exists() {
            let session = Session::new().with_persistence_path(handle.path.clone());
            if let Err(error) = session.save_to_path(&handle.path) {
                self.set_error(format!("failed to initialize session file: {error}"));
                return;
            }
        }

        let description = optional_text(&self.thread_form.description);
        let thread = ThreadRecord {
            name: name.to_string(),
            folder: folder.display().to_string(),
            session_id: handle.id.clone(),
            session_path: handle.path.display().to_string(),
            description,
        };
        match store.upsert_thread(thread) {
            Ok(()) => {
                self.set_notice(format!("thread saved: {name}"));
                self.thread_form = ThreadForm::default();
                self.reload();
            }
            Err(error) => self.set_error(format!("failed to save thread: {error}")),
        }
    }

    fn activate_thread(&mut self, name: &str) {
        self.clear_messages();
        let Some(store) = self.agent_store.as_mut() else {
            self.set_error("thread store is not loaded");
            return;
        };
        match store.set_active_thread(name) {
            Ok(()) => {
                self.set_notice(format!("active thread switched to {name}"));
                self.reload();
            }
            Err(error) => self.set_error(format!("failed to activate thread: {error}")),
        }
    }

    fn remove_thread(&mut self, name: &str) {
        self.clear_messages();
        let Some(store) = self.agent_store.as_mut() else {
            self.set_error("thread store is not loaded");
            return;
        };
        match store.remove_thread(name) {
            Ok(()) => {
                self.set_notice(format!("thread removed: {name}"));
                self.reload();
            }
            Err(error) => self.set_error(format!("failed to remove thread: {error}")),
        }
    }

    fn add_app(&mut self) {
        self.clear_messages();
        let Some(store) = self.agent_store.as_mut() else {
            self.set_error("app store is not loaded");
            return;
        };

        let name = self.app_form.name.trim();
        let command = self.app_form.command.trim();
        if name.is_empty() || command.is_empty() {
            self.set_error("app name/command cannot be empty");
            return;
        }
        let app = AppRecord {
            name: name.to_string(),
            command: command.to_string(),
            description: self
                .app_form
                .description
                .trim()
                .to_string()
                .if_empty("custom app command"),
        };
        match store.upsert_app(app) {
            Ok(()) => {
                self.set_notice(format!("app saved: {name}"));
                self.app_form = AppForm::default();
                self.reload();
            }
            Err(error) => self.set_error(format!("failed to save app: {error}")),
        }
    }

    fn remove_app(&mut self, name: &str) {
        self.clear_messages();
        let Some(store) = self.agent_store.as_mut() else {
            self.set_error("app store is not loaded");
            return;
        };
        match store.remove_app(name) {
            Ok(()) => {
                self.set_notice(format!("app removed: {name}"));
                self.reload();
            }
            Err(error) => self.set_error(format!("failed to remove app: {error}")),
        }
    }

    fn active_model(&self) -> String {
        self.llm_store
            .as_ref()
            .and_then(LlmProfileStore::active_profile)
            .map(|profile| profile.model.clone())
            .unwrap_or_else(|| "deepseek-chat".to_string())
    }

    fn launch_command(&self) -> String {
        format!(
            "cargo run -p rusty-claude-cli -- --model {}",
            self.active_model()
        )
    }

    fn launch_repl_terminal(&mut self) {
        let model = self.active_model();
        let escaped_workspace = powershell_escape(&self.workspace);
        let escaped_model = model.replace('\'', "''");
        let script = format!(
            "Set-Location -LiteralPath '{escaped_workspace}'; cargo run -p rusty-claude-cli -- --model {escaped_model}"
        );
        let spawn_result = Command::new("cmd")
            .args([
                "/C",
                "start",
                "",
                "powershell",
                "-NoExit",
                "-Command",
                &script,
            ])
            .spawn();
        match spawn_result {
            Ok(_) => self.set_notice(format!("terminal launched with model {model}")),
            Err(error) => self.set_error(format!("failed to launch terminal: {error}")),
        }
    }

    fn render_top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Workspace");
            ui.text_edit_singleline(&mut self.workspace_input);
            if ui.button("Load").clicked() {
                self.reload();
            }
            if ui.button("Launch REPL").clicked() {
                self.launch_repl_terminal();
            }
            if ui.button("Copy launch command").clicked() {
                ui.ctx().copy_text(self.launch_command());
                self.set_notice("launch command copied");
            }
        });

        if let Some(notice) = &self.notice {
            ui.colored_label(egui::Color32::from_rgb(40, 160, 90), notice);
        }
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::from_rgb(200, 60, 60), error);
        }
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.heading("Claw GUI");
        ui.label("Codex-style client preview");
        ui.separator();
        for (tab, label) in Tab::entries() {
            ui.selectable_value(&mut self.active_tab, tab, label);
        }
        ui.separator();

        let profile_count = self
            .llm_store
            .as_ref()
            .map_or(0_usize, |store| store.list_profiles().len());
        let thread_count = self
            .agent_store
            .as_ref()
            .map_or(0_usize, |store| store.list_threads().len());
        let app_count = self
            .agent_store
            .as_ref()
            .map_or(0_usize, |store| store.list_apps().len());
        ui.label(format!("Profiles: {profile_count}"));
        ui.label(format!("Threads: {thread_count}"));
        ui.label(format!("Apps: {app_count}"));
        ui.label(format!("Sessions: {}", self.sessions.len()));
    }

    fn render_overview(&self, ui: &mut egui::Ui) {
        ui.heading("Workspace Overview");
        ui.label(format!("Path: {}", self.workspace.display()));

        if let Some(store) = &self.llm_store {
            let summary = store.summary();
            ui.separator();
            ui.label(format!(
                "Active profile: {}",
                summary
                    .active_profile_name
                    .unwrap_or_else(|| "(none)".to_string())
            ));
            ui.label(format!(
                "Active provider: {}",
                summary
                    .active_provider
                    .unwrap_or_else(|| "(none)".to_string())
            ));
            ui.label(format!(
                "Active model: {}",
                summary.active_model.unwrap_or_else(|| "(none)".to_string())
            ));
            ui.label(format!(
                "Token limits: {}",
                summary.turn_token_limits.summary_line()
            ));
        }

        if let Some(store) = &self.agent_store {
            let summary = store.summary();
            ui.separator();
            ui.label(format!(
                "Active thread: {}",
                summary
                    .active_thread_name
                    .unwrap_or_else(|| "(none)".to_string())
            ));
            ui.label(format!("Saved threads: {}", summary.thread_count));
            ui.label(format!("Saved apps: {}", summary.app_count));
        }

        ui.separator();
        ui.label("Billing");
        ui.label("China-headquartered model vendors are tracked in RMB (CNY) in CLI cost output.");
        ui.label("Per-turn token + cost is displayed in the terminal client after each answer.");
    }

    fn render_models(&mut self, ui: &mut egui::Ui) {
        ui.heading("Models / LLM Profiles");
        ui.label(
            "Profiles are isolated by name, so different API keys and base URLs do not conflict.",
        );
        ui.separator();

        ui.label("Add / Update Profile");
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut self.llm_form.name);
            ui.label("Provider");
            ui.text_edit_singleline(&mut self.llm_form.provider);
            ui.label("Model");
            ui.text_edit_singleline(&mut self.llm_form.model);
        });
        ui.horizontal(|ui| {
            ui.label("API Key (inline)");
            ui.add(egui::TextEdit::singleline(&mut self.llm_form.api_key).password(true));
            ui.label("API Key Env");
            ui.text_edit_singleline(&mut self.llm_form.api_key_env);
        });
        ui.horizontal(|ui| {
            ui.label("Base URL");
            ui.text_edit_singleline(&mut self.llm_form.base_url);
            if ui.button("Save (env key)").clicked() {
                self.persist_llm_profile(true);
            }
            if ui.button("Save (inline key)").clicked() {
                self.persist_llm_profile(false);
            }
        });

        ui.separator();
        ui.label("Turn Token Limits");
        ui.horizontal(|ui| {
            ui.label("Min input");
            ui.text_edit_singleline(&mut self.limits_form.min_input);
            ui.label("Max input");
            ui.text_edit_singleline(&mut self.limits_form.max_input);
            ui.label("Min output");
            ui.text_edit_singleline(&mut self.limits_form.min_output);
            ui.label("Max output");
            ui.text_edit_singleline(&mut self.limits_form.max_output);
        });
        ui.horizontal(|ui| {
            if ui.button("Save limits").clicked() {
                self.persist_limits();
            }
            if ui.button("Clear limits").clicked() {
                self.clear_limits();
            }
        });

        ui.separator();
        ui.label("Saved Profiles");
        let (profiles, active_name) = if let Some(store) = &self.llm_store {
            (
                store.list_profiles(),
                store.active_profile_name().map(ToOwned::to_owned),
            )
        } else {
            (Vec::new(), None)
        };
        if profiles.is_empty() {
            ui.label("No profiles yet.");
            return;
        }

        egui::Grid::new("profile_grid")
            .striped(true)
            .show(ui, |ui| {
                ui.label("Name");
                ui.label("Provider");
                ui.label("Model");
                ui.label("Key");
                ui.label("Base URL");
                ui.label("Actions");
                ui.end_row();

                for profile in profiles {
                    let is_active = active_name.as_deref() == Some(profile.name.as_str());
                    ui.label(if is_active {
                        format!("{} (active)", profile.name)
                    } else {
                        profile.name.clone()
                    });
                    ui.label(profile.normalized_provider());
                    ui.label(profile.model.clone());
                    ui.label(profile.key_source_label());
                    ui.label(
                        profile
                            .base_url
                            .clone()
                            .unwrap_or_else(|| "(provider default)".to_string()),
                    );
                    let name = profile.name.clone();
                    ui.horizontal(|ui| {
                        if ui.button("Use").clicked() {
                            self.activate_profile(&name);
                        }
                        if ui.button("Remove").clicked() {
                            self.remove_profile(&name);
                        }
                    });
                    ui.end_row();
                }
            });
    }

    fn render_threads(&mut self, ui: &mut egui::Ui) {
        ui.heading("Threads");
        ui.label("Create per-folder work threads similar to Codex workspace threads.");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut self.thread_form.name);
            ui.label("Folder");
            ui.text_edit_singleline(&mut self.thread_form.folder);
        });
        ui.horizontal(|ui| {
            ui.label("Description");
            ui.text_edit_singleline(&mut self.thread_form.description);
            if ui.button("Save thread").clicked() {
                self.add_thread();
            }
        });

        ui.separator();
        let (threads, active_name) = if let Some(store) = &self.agent_store {
            (
                store.list_threads(),
                store.active_thread_name().map(ToOwned::to_owned),
            )
        } else {
            (Vec::new(), None)
        };
        if threads.is_empty() {
            ui.label("No threads yet.");
            return;
        }

        egui::Grid::new("thread_grid").striped(true).show(ui, |ui| {
            ui.label("Name");
            ui.label("Folder");
            ui.label("Session");
            ui.label("Description");
            ui.label("Actions");
            ui.end_row();

            for thread in threads {
                let is_active = active_name.as_deref() == Some(thread.name.as_str());
                ui.label(if is_active {
                    format!("{} (active)", thread.name)
                } else {
                    thread.name.clone()
                });
                ui.label(thread.folder.clone());
                ui.label(thread.session_id.clone());
                ui.label(
                    thread
                        .description
                        .clone()
                        .unwrap_or_else(|| "-".to_string()),
                );
                let name = thread.name.clone();
                ui.horizontal(|ui| {
                    if ui.button("Switch").clicked() {
                        self.activate_thread(&name);
                    }
                    if ui.button("Remove").clicked() {
                        self.remove_thread(&name);
                    }
                });
                ui.end_row();
            }
        });
    }

    fn render_apps(&mut self, ui: &mut egui::Ui) {
        ui.heading("Apps / Skills Entry");
        ui.label("Save reusable prompts or slash command workflows.");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut self.app_form.name);
            ui.label("Command");
            ui.text_edit_singleline(&mut self.app_form.command);
        });
        ui.horizontal(|ui| {
            ui.label("Description");
            ui.text_edit_singleline(&mut self.app_form.description);
            if ui.button("Save app").clicked() {
                self.add_app();
            }
        });

        ui.separator();
        let apps = self
            .agent_store
            .as_ref()
            .map_or_else(Vec::new, AgentWorkspaceStore::list_apps);
        if apps.is_empty() {
            ui.label("No apps yet.");
            return;
        }

        egui::Grid::new("app_grid").striped(true).show(ui, |ui| {
            ui.label("Name");
            ui.label("Command");
            ui.label("Description");
            ui.label("Actions");
            ui.end_row();

            for app in apps {
                ui.label(app.name.clone());
                ui.label(app.command.clone());
                ui.label(app.description.clone());
                let name = app.name.clone();
                ui.horizontal(|ui| {
                    if ui.button("Copy").clicked() {
                        ui.ctx().copy_text(app.command.clone());
                        self.set_notice(format!("copied command for {}", app.name));
                    }
                    if ui.button("Remove").clicked() {
                        self.remove_app(&name);
                    }
                });
                ui.end_row();
            }
        });
    }

    fn render_sessions(&mut self, ui: &mut egui::Ui) {
        ui.heading("Sessions");
        ui.horizontal(|ui| {
            ui.label("Session files under current workspace");
            if ui.button("Refresh").clicked() {
                self.reload();
            }
        });
        ui.separator();
        if self.sessions.is_empty() {
            ui.label("No managed sessions found.");
            return;
        }

        egui::Grid::new("session_grid")
            .striped(true)
            .show(ui, |ui| {
                ui.label("Session ID");
                ui.label("Messages");
                ui.label("Modified (epoch ms)");
                ui.label("Path");
                ui.end_row();
                for session in &self.sessions {
                    ui.label(session.id.clone());
                    ui.label(session.message_count.to_string());
                    ui.label(session.modified_epoch_millis.to_string());
                    ui.label(session.path.display().to_string());
                    ui.end_row();
                }
            });
    }
}

impl eframe::App for ClawGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_bar")
            .resizable(false)
            .show(ctx, |ui| self.render_top_bar(ui));

        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(220.0)
            .show(ctx, |ui| self.render_sidebar(ui));

        egui::CentralPanel::default().show(ctx, |ui| match self.active_tab {
            Tab::Overview => self.render_overview(ui),
            Tab::Models => self.render_models(ui),
            Tab::Threads => self.render_threads(ui),
            Tab::Apps => self.render_apps(ui),
            Tab::Sessions => self.render_sessions(ui),
        });
    }
}

fn optional_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn optional_u32_text(value: Option<u32>) -> String {
    value.map_or_else(String::new, |v| v.to_string())
}

fn parse_optional_u32(value: &str) -> Result<Option<u32>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") || trimmed == "-" {
        return Ok(None);
    }
    let parsed = trimmed
        .parse::<u32>()
        .map_err(|_| format!("invalid token limit: '{trimmed}'"))?;
    if parsed == 0 {
        return Err("token limits must be greater than 0".to_string());
    }
    Ok(Some(parsed))
}

fn absolute_folder(workspace: &Path, folder: &str) -> PathBuf {
    let path = PathBuf::from(folder.trim());
    if path.is_absolute() {
        path
    } else {
        workspace.join(path)
    }
}

fn sanitize_id(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        "thread".to_string()
    } else {
        trimmed.to_string()
    }
}

fn epoch_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map_or(0, |duration| duration.as_millis())
}

fn powershell_escape(path: &Path) -> String {
    path.display().to_string().replace('\'', "''")
}

trait StringExt {
    fn if_empty(self, fallback: &str) -> String;
}

impl StringExt for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.trim().is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

fn main() -> Result<(), eframe::Error> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Claw Client GUI (Preview)",
        native_options,
        Box::new(|_cc| Ok(Box::new(ClawGuiApp::new()))),
    )
}
