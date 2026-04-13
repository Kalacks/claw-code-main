#![allow(
    clippy::assigning_clones,
    clippy::format_push_string,
    clippy::map_unwrap_or,
    clippy::single_match_else,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unused_self
)]

#[allow(dead_code)]
#[path = "../agent_layer.rs"]
mod agent_layer;
#[path = "../gui_chat.rs"]
mod claw_gui_chat;
#[path = "../gui_chrome.rs"]
mod claw_gui_chrome;
#[path = "../gui_pages.rs"]
mod claw_gui_pages;
#[path = "../gui_runtime.rs"]
mod gui_runtime;
#[allow(dead_code)]
#[path = "../llm_layer.rs"]
mod llm_layer;

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use agent_layer::{AgentWorkspaceStore, AppRecord, ThreadRecord};
use claw_gui_chrome::configure_gui_fonts;
use eframe::egui::{
    self, Align, Color32, ComboBox, Frame, Layout, RichText, ScrollArea, Stroke, TextEdit,
};
use gui_runtime::{spawn_turn, GuiTurnConfig, GuiWorkerEvent, GUI_CANCELLED_MESSAGE};
use llm_layer::{estimate_text_tokens, LlmProfile, LlmProfileStore, TurnTokenLimits};
use rfd::FileDialog;
use runtime::{
    format_currency, pricing_for_usage, resolve_pricing_for_model, ContentBlock,
    ConversationMessage, MessageRole, Session, SessionStore, TokenUsage, UsageTracker,
};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Chat,
    Models,
    Apps,
    Sessions,
}

#[allow(dead_code)]
impl Tab {
    fn all() -> [Self; 4] {
        [Self::Chat, Self::Models, Self::Apps, Self::Sessions]
    }

    fn label_v2(self, language: Language) -> &'static str {
        match self {
            Self::Chat => language.pick("聊天", "Chat"),
            Self::Models => language.pick("模型", "Models"),
            Self::Apps => language.pick("应用", "Apps"),
            Self::Sessions => language.pick("会话", "Sessions"),
        }
    }

    fn label(self, language: Language) -> &'static str {
        match self {
            Self::Chat => language.pick("聊天", "Chat"),
            Self::Models => language.pick("模型", "Models"),
            Self::Apps => language.pick("应用", "Apps"),
            Self::Sessions => language.pick("会话", "Sessions"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Language {
    Zh,
    En,
}

#[allow(dead_code)]
impl Language {
    fn pick<'a>(self, zh: &'a str, en: &'a str) -> &'a str {
        match self {
            Self::Zh => zh,
            Self::En => en,
        }
    }

    fn label_v2(self) -> &'static str {
        match self {
            Self::Zh => "中文",
            Self::En => "English",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Zh => "中文",
            Self::En => "English",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiTheme {
    Sand,
    Mist,
    Forest,
    Graphite,
}

#[allow(dead_code)]
impl UiTheme {
    fn all() -> [Self; 4] {
        [Self::Sand, Self::Mist, Self::Forest, Self::Graphite]
    }

    fn label_v2(self, language: Language) -> &'static str {
        match self {
            Self::Sand => language.pick("Sand", "Sand"),
            Self::Mist => language.pick("Mist", "Mist"),
            Self::Forest => language.pick("Forest", "Forest"),
            Self::Graphite => language.pick("Graphite", "Graphite"),
        }
    }

    fn label(self, language: Language) -> &'static str {
        match self {
            Self::Sand => language.pick("Sand", "Sand"),
            Self::Mist => language.pick("Mist", "Mist"),
            Self::Forest => language.pick("Forest", "Forest"),
            Self::Graphite => language.pick("Graphite", "Graphite"),
        }
    }

    fn label_clean(self, language: Language) -> &'static str {
        match self {
            Self::Sand => language.pick("Sand", "Sand"),
            Self::Mist => language.pick("Mist", "Mist"),
            Self::Forest => language.pick("Forest", "Forest"),
            Self::Graphite => language.pick("Graphite", "Graphite"),
        }
    }

    fn accent(self) -> Color32 {
        match self {
            Self::Sand => Color32::from_rgb(196, 110, 55),
            Self::Mist => Color32::from_rgb(45, 122, 146),
            Self::Forest => Color32::from_rgb(55, 126, 91),
            Self::Graphite => Color32::from_rgb(112, 124, 145),
        }
    }

    fn panel_fill(self) -> Color32 {
        match self {
            Self::Sand => Color32::from_rgb(250, 243, 235),
            Self::Mist => Color32::from_rgb(239, 246, 248),
            Self::Forest => Color32::from_rgb(239, 247, 242),
            Self::Graphite => Color32::from_rgb(34, 38, 44),
        }
    }

    fn subpanel_fill(self) -> Color32 {
        match self {
            Self::Sand => Color32::from_rgb(255, 250, 245),
            Self::Mist => Color32::from_rgb(247, 251, 252),
            Self::Forest => Color32::from_rgb(245, 250, 246),
            Self::Graphite => Color32::from_rgb(26, 29, 34),
        }
    }

    fn visuals(self) -> egui::Visuals {
        let mut visuals = if self == Self::Graphite {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        let accent = self.accent();
        visuals.widgets.active.bg_fill = accent;
        visuals.widgets.hovered.bg_fill = accent.gamma_multiply(0.85);
        visuals.widgets.inactive.bg_fill = self.subpanel_fill();
        visuals.selection.bg_fill = accent;
        visuals.panel_fill = self.panel_fill();
        visuals.extreme_bg_color = self.subpanel_fill();
        visuals.faint_bg_color = self.subpanel_fill();
        visuals
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

#[derive(Debug, Clone)]
struct InspectorEvent {
    title: String,
    body: String,
    is_error: bool,
}

#[derive(Debug, Clone)]
enum PendingChatSwitch {
    Workspace,
    Thread(String),
}

#[derive(Debug)]
struct ClawGuiApp {
    workspace_input: String,
    workspace: PathBuf,
    active_tab: Tab,
    language: Language,
    theme: UiTheme,
    show_help: bool,
    show_thread_form: bool,
    show_model_quick_form: bool,
    llm_key_from_env: bool,
    show_api_key: bool,
    llm_store: Option<LlmProfileStore>,
    agent_store: Option<AgentWorkspaceStore>,
    sessions: Vec<runtime::session_control::ManagedSessionSummary>,
    llm_form: LlmForm,
    limits_form: LimitsForm,
    thread_form: ThreadForm,
    app_form: AppForm,
    notice: Option<String>,
    error: Option<String>,
    active_session: Session,
    active_session_path: PathBuf,
    active_thread_name: Option<String>,
    composer: String,
    #[allow(dead_code)]
    composer_height_adjust: f32,
    attached_files: Vec<PathBuf>,
    optimistic_user_prompt: Option<String>,
    live_reply: String,
    inspector_events: Vec<InspectorEvent>,
    latest_turn_usage: TokenUsage,
    cumulative_usage: TokenUsage,
    worker_rx: Option<Receiver<GuiWorkerEvent>>,
    cancel_flag: Option<Arc<AtomicBool>>,
    pause_requested: bool,
    pending_chat_switch: Option<PendingChatSwitch>,
    confirm_folder_session_cleanup: Option<String>,
    busy: bool,
}

#[allow(dead_code)]
impl ClawGuiApp {
    fn new() -> Self {
        let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut app = Self {
            workspace_input: workspace.display().to_string(),
            workspace,
            active_tab: Tab::Chat,
            language: Language::Zh,
            theme: UiTheme::Sand,
            show_help: false,
            show_thread_form: false,
            show_model_quick_form: false,
            llm_key_from_env: false,
            show_api_key: false,
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
            active_session: Session::new(),
            active_session_path: PathBuf::new(),
            active_thread_name: None,
            composer: String::new(),
            composer_height_adjust: 0.0,
            attached_files: Vec::new(),
            optimistic_user_prompt: None,
            live_reply: String::new(),
            inspector_events: Vec::new(),
            latest_turn_usage: TokenUsage::default(),
            cumulative_usage: TokenUsage::default(),
            worker_rx: None,
            cancel_flag: None,
            pause_requested: false,
            pending_chat_switch: None,
            confirm_folder_session_cleanup: None,
            busy: false,
        };
        app.reload();
        app
    }

    fn tr<'a>(&self, zh: &'a str, en: &'a str) -> &'a str {
        fn contains_private_use_chars(value: &str) -> bool {
            value
                .chars()
                .any(|ch| ('\u{E000}'..='\u{F8FF}').contains(&ch) || ch == '\u{FFFD}')
        }
        fn zh_from_english(value: &str) -> Option<&'static str> {
            match value {
                "Chat" => Some("聊天"),
                "Models" | "Model" => Some("模型"),
                "Apps" => Some("应用"),
                "Sessions" | "Conversation" => Some("会话"),
                "Workspace" => Some("工作区"),
                "Workspace Chat" => Some("工作区聊天"),
                "Current thread" => Some("当前线程"),
                "Current workspace" => Some("当前工作区"),
                "Load" => Some("加载"),
                "Refresh" => Some("刷新"),
                "Help" => Some("帮助"),
                "Provider" => Some("提供商"),
                "Profile" => Some("配置"),
                "Base URL" => Some("基础 URL"),
                "(none)" => Some("（无）"),
                "Connection" => Some("连接"),
                "Connection hint" => Some("连接提示"),
                "Composer" => Some("输入框"),
                "Send" => Some("发送"),
                "Pause" => Some("暂停"),
                "Pause & edit last" => Some("暂停并编辑上一条"),
                "Add files" => Some("添加文件"),
                "Attached files" => Some("已附加文件"),
                "Token Usage" => Some("Token 使用"),
                "Cost Estimate" => Some("费用估算"),
                "Tool Events" => Some("工具事件"),
                "Estimated input tokens" => Some("预估输入 Token"),
                "Limits" => Some("限制"),
                "Thread" | "Threads" => Some("线程"),
                "Folders" | "Folder" => Some("文件夹"),
                "Messages" => Some("消息数"),
                "Create Thread" | "New Thread" => Some("新建线程"),
                "Thread name" => Some("线程名称"),
                "Thread folder" => Some("线程目录"),
                "Description" => Some("描述"),
                "Create" => Some("创建"),
                "Cancel" => Some("取消"),
                "Import Folder" => Some("导入文件夹"),
                "New thread here" => Some("在此新建线程"),
                "Remove" => Some("删除"),
                "New Model" => Some("新建模型"),
                "Current workspace chat" => Some("当前工作区聊天"),
                "Active profile" => Some("当前激活配置"),
                "Inline key" => Some("直接填写 Key"),
                "Env var" | "Environment variable" | "environment" => Some("环境变量"),
                "API key" => Some("API Key"),
                "Hide" => Some("隐藏"),
                "Show" => Some("显示"),
                "Write env" => Some("写入当前环境"),
                "Write + persist" => Some("写入并持久化"),
                "Force pause" => Some("强制暂停"),
                "Save" => Some("保存"),
                "Save + Activate" => Some("保存并激活"),
                "Clear form" => Some("清空表单"),
                "Imported Profiles" => Some("已导入配置"),
                "Current limits" => Some("当前限制"),
                "Key source" => Some("Key 来源"),
                "Key preview" => Some("Key 预览"),
                "Active" => Some("已激活"),
                "Save App Command" => Some("保存应用命令"),
                "Saved Apps" => Some("已保存应用"),
                "Save app" => Some("保存应用"),
                "Load into form" => Some("载入表单"),
                "Workspace Sessions" => Some("工作区会话"),
                "Total" => Some("总数"),
                "Current session" => Some("当前会话"),
                "Load into chat" => Some("载入聊天"),
                "Choose" => Some("选择"),
                "Create & Open" => Some("创建并打开"),
                "Key mode" => Some("Key 模式"),
                "Written to current process env" => Some("已写入当前进程环境变量"),
                "Written to current process and persisted env" => {
                    Some("已写入当前进程并持久化到环境变量")
                }
                "Session files cleared:" => Some("已清理会话文件数："),
                "Stopping the current reply..." => Some("正在停止当前回复..."),
                "Pausing..." => Some("正在暂停..."),
                "Shortcut: Ctrl+Enter to send" => Some("快捷键：Ctrl+Enter 发送"),
                "Composer auto-grows with content" => Some("输入区会随内容自动增高"),
                "provider default" => Some("提供商默认值"),
                _ => None,
            }
        }

        match self.language {
            Language::Zh => {
                let preferred = if zh.trim().is_empty() || contains_private_use_chars(zh) {
                    en
                } else {
                    zh
                };
                zh_from_english(preferred).unwrap_or(preferred)
            }
            Language::En => {
                if en.trim().is_empty() {
                    zh
                } else {
                    en
                }
            }
        }
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

    fn connection_error_hint(&self) -> Option<String> {
        let message = self.error.as_deref()?.to_ascii_lowercase();
        if message.contains("api key")
            || message.contains("environment variable")
            || message.contains("auth")
            || message.contains("unauthorized")
            || message.contains("invalid api key")
        {
            return Some(
                self.tr(
                    "",
                    "This looks like an API key issue. Check whether the key is empty, expired, or the profile is accidentally using env-var mode.",
                )
                .to_string(),
            );
        }
        if message.contains("base url")
            || message.contains("dns")
            || message.contains("url")
            || message.contains("connection")
            || message.contains("timed out")
            || message.contains("refused")
            || message.contains("404")
        {
            return Some(
                self.tr(
                    "",
                    "This looks like a Base URL or network issue. Check whether the base URL is correct, includes the protocol, and the service is reachable.",
                )
                .to_string(),
            );
        }
        if message.contains("model")
            || message.contains("not found")
            || message.contains("unsupported")
        {
            return Some(
                self.tr(
                    "",
                    "This looks like a model-name issue. Check whether the model id exactly matches the provider API.",
                )
                .to_string(),
            );
        }
        None
    }

    fn derive_env_var_name(&self) -> String {
        let provider = self.llm_form.provider.trim();
        if provider.is_empty() {
            return "LLM_API_KEY".to_string();
        }
        format!(
            "{}_API_KEY",
            provider.trim().to_ascii_uppercase().replace('-', "_")
        )
    }

    fn import_folder_picker(&mut self) {
        if let Some(folder) = FileDialog::new()
            .set_directory(&self.workspace)
            .pick_folder()
        {
            self.thread_form.folder = folder.display().to_string();
            self.thread_form.name = folder
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("workspace")
                .to_string();
            self.show_thread_form = true;
        }
    }

    fn add_files_picker(&mut self) {
        if let Some(files) = FileDialog::new()
            .set_directory(self.active_chat_workspace())
            .pick_files()
        {
            for file in files {
                if !self.attached_files.iter().any(|existing| existing == &file) {
                    self.attached_files.push(file);
                }
            }
        }
    }

    fn remove_attached_file(&mut self, path: &Path) {
        self.attached_files.retain(|candidate| candidate != path);
    }

    fn resolve_tool_file_path(&self, raw_path: &str) -> PathBuf {
        let trimmed = raw_path.trim();
        if trimmed.is_empty() {
            return self.active_chat_workspace();
        }
        let candidate = PathBuf::from(trimmed);
        if candidate.is_absolute() || trimmed.starts_with(r"\\?\") {
            candidate
        } else {
            self.active_chat_workspace().join(candidate)
        }
    }

    fn undo_file_change_action(
        &mut self,
        file_path: &str,
        original_file: Option<&str>,
        was_created: bool,
    ) {
        let resolved = self.resolve_tool_file_path(file_path);

        if was_created && original_file.is_none() {
            match std::fs::remove_file(&resolved) {
                Ok(()) => {
                    self.set_notice(format!("undo success: removed {}", resolved.display()));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.set_notice(format!(
                        "undo skipped: {} already removed",
                        resolved.display()
                    ));
                }
                Err(error) => {
                    self.set_error(format!("undo failed: {error}"));
                }
            }
            return;
        }

        let Some(original) = original_file else {
            self.set_error("undo failed: missing original file snapshot");
            return;
        };

        if let Some(parent) = resolved.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                self.set_error(format!("undo failed: {error}"));
                return;
            }
        }

        // 重要：撤销时直接使用工具结果中的 originalFile 快照，避免二次推断导致内容偏差。
        match std::fs::write(&resolved, original) {
            Ok(()) => self.set_notice(format!("undo success: restored {}", resolved.display())),
            Err(error) => self.set_error(format!("undo failed: {error}")),
        }
    }

    fn build_prompt_with_attachments(&self, prompt: &str) -> String {
        const MAX_ATTACHED_FILES: usize = 4;
        const MAX_FILE_CHARS: usize = 6_000;

        let mut output = prompt.to_string();
        let files = self
            .attached_files
            .iter()
            .take(MAX_ATTACHED_FILES)
            .cloned()
            .collect::<Vec<_>>();
        if files.is_empty() {
            return output;
        }

        output.push_str("\n\nAttached files:\n");
        for path in files {
            output.push_str(&format!("- {}\n", path.display()));
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    let clipped = truncate_text(&content, MAX_FILE_CHARS);
                    output.push_str(&format!("```text\n{}\n```\n", clipped));
                }
                Err(error) => {
                    output.push_str(&format!("(failed to read file: {error})\n"));
                }
            }
        }
        output
    }

    fn write_api_key_to_env_action(&mut self, persist_with_setx: bool) {
        let env_name = self
            .llm_form
            .api_key_env
            .trim()
            .to_string()
            .if_empty(&self.derive_env_var_name());
        let api_key = self.llm_form.api_key.trim().to_string();
        if api_key.is_empty() {
            self.set_error(
                self.tr("", "Enter an API key before exporting it to env.")
                    .to_string(),
            );
            return;
        }

        self.llm_form.api_key_env = env_name.clone();
        std::env::set_var(&env_name, &api_key);

        if !persist_with_setx {
            self.set_notice(format!(
                "{} {}",
                self.tr(
                    "Written to current process env",
                    "Written to current process env"
                ),
                env_name
            ));
            return;
        }

        match Command::new("setx").arg(&env_name).arg(&api_key).output() {
            Ok(output) if output.status.success() => {
                self.set_notice(format!(
                    "{} {}",
                    self.tr("", "Written to current process and persisted env"),
                    env_name
                ));
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                self.set_error(format!(
                    "{}: {}",
                    self.tr("setx failed", "setx failed"),
                    if stderr.is_empty() {
                        self.tr("unknown error", "unknown error").to_string()
                    } else {
                        stderr
                    }
                ));
            }
            Err(error) => {
                self.set_error(format!("setx failed: {error}"));
            }
        }
    }
    fn parse_workspace(&self) -> Result<PathBuf, String> {
        let raw = self.workspace_input.trim();
        if raw.is_empty() {
            return Err(self.tr("", "workspace path cannot be empty").to_string());
        }
        let path = PathBuf::from(raw);
        if !path.exists() {
            return Err(format!(
                "{}: {}",
                self.tr("workspace does not exist", "workspace does not exist"),
                path.display()
            ));
        }
        if !path.is_dir() {
            return Err(format!(
                "{}: {}",
                self.tr(
                    "workspace is not a directory",
                    "workspace is not a directory"
                ),
                path.display()
            ));
        }
        Ok(path)
    }

    fn reload(&mut self) {
        self.clear_messages();
        let workspace = match self.parse_workspace_clean() {
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

        self.sessions = SessionStore::from_cwd(&workspace)
            .and_then(|store| store.list_sessions())
            .unwrap_or_default();
        self.llm_store = Some(llm_store);
        self.agent_store = Some(agent_store);
        self.sync_active_chat();
    }

    fn load_limit_form(&mut self, limits: TurnTokenLimits) {
        self.limits_form.min_input = optional_u32_text(limits.min_input_tokens);
        self.limits_form.max_input = optional_u32_text(limits.max_input_tokens);
        self.limits_form.min_output = optional_u32_text(limits.min_output_tokens);
        self.limits_form.max_output = optional_u32_text(limits.max_output_tokens);
    }

    fn sync_active_chat(&mut self) {
        if self.busy {
            return;
        }
        let active_thread = self
            .agent_store
            .as_ref()
            .and_then(AgentWorkspaceStore::active_thread_name)
            .map(ToOwned::to_owned);
        self.active_thread_name = active_thread.clone();

        let loaded = if let Some(thread_name) = active_thread {
            self.agent_store
                .as_ref()
                .and_then(|store| store.thread(&thread_name))
                .cloned()
                .and_then(|thread| self.load_thread_session(&thread).ok())
        } else {
            self.load_workspace_session().ok()
        };

        if let Some((session, path)) = loaded {
            self.active_session = session;
            self.active_session_path = path;
            self.refresh_usage();
        }
    }

    fn load_workspace_session(&self) -> Result<(Session, PathBuf), String> {
        let store = SessionStore::from_cwd(&self.workspace).map_err(|error| error.to_string())?;
        match store.latest_session() {
            Ok(summary) => {
                let session =
                    Session::load_from_path(&summary.path).map_err(|error| error.to_string())?;
                Ok((session, summary.path))
            }
            Err(_) => {
                let session = Session::new();
                let handle = store.create_handle(&session.session_id);
                let session = session
                    .with_persistence_path(handle.path.clone())
                    .with_workspace_root(self.workspace.clone());
                session
                    .save_to_path(&handle.path)
                    .map_err(|error| error.to_string())?;
                Ok((session, handle.path))
            }
        }
    }

    fn load_thread_session(&self, thread: &ThreadRecord) -> Result<(Session, PathBuf), String> {
        let path = PathBuf::from(&thread.session_path);
        if path.exists() {
            let session = Session::load_from_path(&path).map_err(|error| error.to_string())?;
            return Ok((session, path));
        }

        let folder = PathBuf::from(&thread.folder);
        std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
        let session = Session::new()
            .with_persistence_path(path.clone())
            .with_workspace_root(folder);
        session
            .save_to_path(&path)
            .map_err(|error| error.to_string())?;
        Ok((session, path))
    }

    fn active_model(&self) -> String {
        self.llm_store
            .as_ref()
            .and_then(LlmProfileStore::active_profile)
            .map(|profile| profile.model.clone())
            .unwrap_or_else(|| "deepseek-chat".to_string())
    }

    fn active_profile_name(&self) -> Option<String> {
        self.llm_store
            .as_ref()
            .and_then(LlmProfileStore::active_profile_name)
            .map(ToOwned::to_owned)
    }

    fn active_chat_workspace(&self) -> PathBuf {
        self.agent_store
            .as_ref()
            .and_then(|store| {
                self.active_thread_name
                    .as_deref()
                    .and_then(|name| store.thread(name))
            })
            .map(|thread| PathBuf::from(&thread.folder))
            .unwrap_or_else(|| self.workspace.clone())
    }

    fn active_test_scope_key(&self) -> String {
        thread_test_scope_key(self.active_thread_name.as_deref())
    }

    fn active_test_records_dir(&self) -> PathBuf {
        test_records_dir(&self.active_chat_workspace(), &self.active_test_scope_key())
    }

    fn active_test_artifacts_dir(&self) -> PathBuf {
        test_artifacts_dir(&self.active_chat_workspace(), &self.active_test_scope_key())
    }

    fn latest_prompt_snapshot(&self) -> Option<String> {
        self.optimistic_user_prompt.clone().or_else(|| {
            self.active_session
                .prompt_history
                .last()
                .map(|entry| entry.text.clone())
        })
    }

    fn active_chat_title(&self) -> String {
        self.active_thread_name
            .clone()
            .unwrap_or_else(|| self.tr("Workspace Chat", "Workspace Chat").to_string())
    }

    fn parse_workspace_clean(&self) -> Result<PathBuf, String> {
        let raw = self.workspace_input.trim();
        if raw.is_empty() {
            return Err(self.tr("", "workspace path cannot be empty").to_string());
        }
        let path = PathBuf::from(raw);
        if !path.exists() {
            return Err(format!(
                "{}: {}",
                self.tr("workspace does not exist", "workspace does not exist"),
                path.display()
            ));
        }
        if !path.is_dir() {
            return Err(format!(
                "{}: {}",
                self.tr(
                    "workspace is not a directory",
                    "workspace is not a directory"
                ),
                path.display()
            ));
        }
        Ok(path)
    }

    fn active_profile_ref(&self) -> Option<&LlmProfile> {
        self.llm_store
            .as_ref()
            .and_then(LlmProfileStore::active_profile)
    }

    fn active_provider_clean(&self) -> String {
        self.active_profile_ref()
            .map(LlmProfile::normalized_provider)
            .unwrap_or_else(|| self.tr("environment", "environment").to_string())
    }

    fn active_base_url_clean(&self) -> String {
        self.active_profile_ref()
            .and_then(|profile| profile.base_url.clone())
            .unwrap_or_else(|| self.tr("provider default", "provider default").to_string())
    }

    fn active_chat_title_clean(&self) -> String {
        self.active_thread_name
            .clone()
            .unwrap_or_else(|| self.tr("Workspace Chat", "Workspace Chat").to_string())
    }

    fn load_profile_into_form(&mut self, profile: &LlmProfile) {
        self.llm_key_from_env = profile
            .api_key_env
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
        self.llm_form = LlmForm {
            name: profile.name.clone(),
            provider: profile.provider.clone(),
            model: profile.model.clone(),
            api_key: profile.api_key.clone().unwrap_or_default(),
            api_key_env: profile.api_key_env.clone().unwrap_or_default(),
            base_url: profile.base_url.clone().unwrap_or_default(),
        };
    }

    fn apply_visuals(&self, ctx: &egui::Context) {
        ctx.set_visuals(self.theme.visuals());
        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(12.0, 7.0);
        style.spacing.indent = 12.0;
        ctx.set_style(style);
    }

    fn refresh_usage(&mut self) {
        let tracker = UsageTracker::from_session(&self.active_session);
        self.latest_turn_usage = tracker.current_turn_usage();
        self.cumulative_usage = tracker.cumulative_usage();
    }

    fn refresh_session_list(&mut self) {
        self.sessions = SessionStore::from_cwd(&self.workspace)
            .and_then(|store| store.list_sessions())
            .unwrap_or_default();
    }

    fn select_workspace_chat(&mut self) {
        if self.busy {
            if let Some(flag) = &self.cancel_flag {
                flag.store(true, Ordering::Relaxed);
            }
            self.pause_requested = true;
            self.pending_chat_switch = Some(PendingChatSwitch::Workspace);
            self.set_notice(
                self.tr(
                    "正在停止当前回复，结束后会切换到工作区会话。",
                    "Stopping the current reply. The UI will switch to workspace chat after it ends.",
                )
                .to_string(),
            );
            return;
        }
        if let Some(store) = self.agent_store.as_mut() {
            let _ = store.clear_active_thread();
        }
        self.active_thread_name = None;
        self.sync_active_chat();
    }

    fn activate_thread(&mut self, name: &str) {
        if self.busy {
            if let Some(flag) = &self.cancel_flag {
                flag.store(true, Ordering::Relaxed);
            }
            self.pause_requested = true;
            self.pending_chat_switch = Some(PendingChatSwitch::Thread(name.to_string()));
            self.set_notice(
                self.tr(
                    "正在停止当前回复，结束后会切换线程。",
                    "Stopping the current reply. The UI will switch thread after it ends.",
                )
                .to_string(),
            );
            return;
        }
        let Some(store) = self.agent_store.as_mut() else {
            self.set_error("thread store is not loaded");
            return;
        };
        match store.set_active_thread(name) {
            Ok(()) => {
                self.active_thread_name = Some(name.to_string());
                self.sync_active_chat();
                self.active_tab = Tab::Chat;
            }
            Err(error) => self.set_error(format!("failed to activate thread: {error}")),
        }
    }

    fn remove_thread(&mut self, name: &str) {
        if self.busy {
            return;
        }
        let Some(store) = self.agent_store.as_mut() else {
            self.set_error("thread store is not loaded");
            return;
        };
        match store.remove_thread(name) {
            Ok(()) => {
                if self.active_thread_name.as_deref() == Some(name) {
                    self.active_thread_name = None;
                }
                self.sync_active_chat();
                self.set_notice(format!("thread removed: {name}"));
            }
            Err(error) => self.set_error(format!("failed to remove thread: {error}")),
        }
    }

    fn apply_pending_chat_switch(&mut self) {
        let pending = self.pending_chat_switch.take();
        match pending {
            Some(PendingChatSwitch::Workspace) => self.select_workspace_chat(),
            Some(PendingChatSwitch::Thread(name)) => self.activate_thread(&name),
            None => {}
        }
    }

    fn clear_folder_sessions_action(&mut self, folder: &str) {
        if self.busy {
            self.set_error(
                self.tr(
                    "请先等待当前回复结束，再清理会话文件。",
                    "Wait for the current reply to finish before clearing session files.",
                )
                .to_string(),
            );
            return;
        }
        let threads = self
            .agent_store
            .as_ref()
            .map(AgentWorkspaceStore::list_threads)
            .unwrap_or_default()
            .into_iter()
            .filter(|thread| thread.folder == folder)
            .collect::<Vec<_>>();
        if threads.is_empty() {
            self.set_error(
                self.tr(
                    "这个文件夹下没有可清理的线程会话。",
                    "No thread sessions were found for this folder.",
                )
                .to_string(),
            );
            return;
        }

        // 连同轮转出来的旧日志一起清理，避免文件夹内残留历史会话碎片。
        let mut removed_file_count = 0usize;
        for thread in &threads {
            removed_file_count += remove_session_file_family(&PathBuf::from(&thread.session_path));
        }

        if threads.iter().any(|thread| {
            self.active_thread_name
                .as_deref()
                .is_some_and(|active| active == thread.name)
        }) {
            self.sync_active_chat();
        } else {
            self.refresh_session_list();
        }

        self.confirm_folder_session_cleanup = None;
        self.set_notice(format!(
            "{} {}",
            self.tr("已清理会话文件数：", "Session files cleared:"),
            removed_file_count
        ));
    }

    #[cfg(any())]
    fn add_thread(&mut self) {
        let Some(store) = self.agent_store.as_mut() else {
            self.set_error("thread store is not loaded");
            return;
        };

        let name = self.thread_form.name.trim();
        let folder_input = self.thread_form.folder.trim();
        if name.is_empty() || folder_input.is_empty() {
            self.set_error(self.tr("", "thread name and folder cannot be empty"));
            return;
        }

        let folder = absolute_folder(&self.workspace, folder_input);
        if let Err(error) = std::fs::create_dir_all(&folder) {
            self.set_error(format!("failed to create folder: {error}"));
            return;
        }

        let session = Session::new();
        let session_store = match SessionStore::from_cwd(&folder) {
            Ok(store) => store,
            Err(error) => {
                self.set_error(format!("failed to create session store: {error}"));
                return;
            }
        };
        let handle = session_store.create_handle(&session.session_id);
        let session = session
            .with_persistence_path(handle.path.clone())
            .with_workspace_root(folder.clone());
        if let Err(error) = session.save_to_path(&handle.path) {
            self.set_error(format!("failed to save session: {error}"));
            return;
        }

        let record = ThreadRecord {
            name: name.to_string(),
            folder: folder.display().to_string(),
            session_id: session.session_id.clone(),
            session_path: handle.path.display().to_string(),
            description: optional_text(&self.thread_form.description),
        };
        match store.upsert_thread(record) {
            Ok(()) => {
                self.thread_form = ThreadForm::default();
                self.show_thread_form = false;
                self.activate_thread(name);
                self.set_notice(format!("thread saved: {name}"));
            }
            Err(error) => self.set_error(format!("failed to save thread: {error}")),
        }
    }

    fn persist_llm_profile(&mut self, env_mode: bool) {
        let Some(store) = self.llm_store.as_mut() else {
            self.set_error("llm store is not loaded");
            return;
        };

        let name = self.llm_form.name.trim();
        let provider = self.llm_form.provider.trim();
        let model = self.llm_form.model.trim();
        if name.is_empty() || provider.is_empty() || model.is_empty() {
            self.set_error(self.tr(
                "鍚嶇О / Provider / Model 涓嶈兘涓虹┖",
                "name / provider / model cannot be empty",
            ));
            return;
        }
        let profile = LlmProfile {
            name: name.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            base_url: optional_text(&self.llm_form.base_url),
            api_key: (!env_mode).then(|| self.llm_form.api_key.trim().to_string()),
            api_key_env: env_mode.then(|| self.llm_form.api_key_env.trim().to_string()),
            note: None,
        };

        match store.upsert_profile(profile) {
            Ok(()) => {
                self.reload();
                self.set_notice(self.tr("profile saved", "profile saved"));
            }
            Err(error) => self.set_error(format!("failed to save profile: {error}")),
        }
    }

    fn activate_profile(&mut self, name: Option<&str>) {
        let Some(store) = self.llm_store.as_mut() else {
            self.set_error("llm store is not loaded");
            return;
        };
        let result = match name {
            Some(value) => store.set_active_profile(value),
            None => store.clear_active_profile(),
        };
        match result {
            Ok(()) => {
                self.reload();
                self.set_notice(self.tr("profile switched", "profile switched"));
            }
            Err(error) => self.set_error(format!("failed to switch profile: {error}")),
        }
    }

    fn remove_profile(&mut self, name: &str) {
        let Some(store) = self.llm_store.as_mut() else {
            self.set_error("llm store is not loaded");
            return;
        };
        match store.remove_profile(name) {
            Ok(()) => {
                self.reload();
                self.set_notice(format!("profile removed: {name}"));
            }
            Err(error) => self.set_error(format!("failed to remove profile: {error}")),
        }
    }

    fn persist_limits(&mut self) {
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

        let Some(store) = self.llm_store.as_mut() else {
            self.set_error("llm store is not loaded");
            return;
        };
        match store.set_turn_token_limits(limits) {
            Ok(()) => {
                self.reload();
                self.set_notice(self.tr("token limits saved", "token limits saved"));
            }
            Err(error) => self.set_error(format!("failed to save limits: {error}")),
        }
    }

    fn add_app(&mut self) {
        let Some(store) = self.agent_store.as_mut() else {
            self.set_error("app store is not loaded");
            return;
        };
        let name = self.app_form.name.trim();
        let command = self.app_form.command.trim();
        if name.is_empty() || command.is_empty() {
            self.set_error(self.tr("", "app name and command cannot be empty"));
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
                .if_empty("quick action"),
        };
        match store.upsert_app(app) {
            Ok(()) => {
                self.app_form = AppForm::default();
                self.reload();
                self.set_notice(self.tr("app saved", "app saved"));
            }
            Err(error) => self.set_error(format!("failed to save app: {error}")),
        }
    }

    fn remove_app(&mut self, name: &str) {
        let Some(store) = self.agent_store.as_mut() else {
            self.set_error("app store is not loaded");
            return;
        };
        match store.remove_app(name) {
            Ok(()) => {
                self.reload();
                self.set_notice(format!("app removed: {name}"));
            }
            Err(error) => self.set_error(format!("failed to remove app: {error}")),
        }
    }

    fn add_thread_action(&mut self) {
        let name = self.thread_form.name.trim().to_string();
        let folder_input = self.thread_form.folder.trim().to_string();
        if name.is_empty() || folder_input.is_empty() {
            self.set_error(self.tr("", "thread name and folder cannot be empty"));
            return;
        }

        let folder = absolute_folder(&self.workspace, &folder_input);
        if let Err(error) = std::fs::create_dir_all(&folder) {
            self.set_error(format!("failed to create folder: {error}"));
            return;
        }

        let session = Session::new();
        let session_store = match SessionStore::from_cwd(&folder) {
            Ok(store) => store,
            Err(error) => {
                self.set_error(format!("failed to create session store: {error}"));
                return;
            }
        };
        let handle = session_store.create_handle(&session.session_id);
        let session = session
            .with_persistence_path(handle.path.clone())
            .with_workspace_root(folder.clone());
        if let Err(error) = session.save_to_path(&handle.path) {
            self.set_error(format!("failed to save session: {error}"));
            return;
        }

        let record = ThreadRecord {
            name: name.clone(),
            folder: folder.display().to_string(),
            session_id: session.session_id.clone(),
            session_path: handle.path.display().to_string(),
            description: optional_text(&self.thread_form.description),
        };

        let Some(store) = self.agent_store.as_mut() else {
            self.set_error("thread store is not loaded");
            return;
        };

        match store.upsert_thread(record) {
            Ok(()) => {
                self.thread_form = ThreadForm::default();
                self.show_thread_form = false;
                self.activate_thread(&name);
                self.set_notice(format!("thread saved: {name}"));
            }
            Err(error) => self.set_error(format!("failed to save thread: {error}")),
        }
    }

    fn save_profile_action(&mut self) -> bool {
        let name = self.llm_form.name.trim().to_string();
        let provider = self.llm_form.provider.trim().to_string();
        let model = self.llm_form.model.trim().to_string();
        if name.is_empty() || provider.is_empty() || model.is_empty() {
            self.set_error(self.tr(
                "鍚嶇О / Provider / Model 涓嶈兘涓虹┖",
                "name / provider / model cannot be empty",
            ));
            return false;
        }

        if self.llm_key_from_env && self.llm_form.api_key_env.trim().is_empty() {
            self.llm_form.api_key_env = self.derive_env_var_name();
        }
        if self.llm_key_from_env {
            let env_name = self.llm_form.api_key_env.trim().to_string();
            let api_key = self.llm_form.api_key.trim().to_string();
            if !env_name.is_empty() && !api_key.is_empty() {
                std::env::set_var(&env_name, &api_key);
            }
        }

        let profile = LlmProfile {
            name,
            provider,
            model,
            base_url: optional_text(&self.llm_form.base_url),
            api_key: (!self.llm_key_from_env).then(|| self.llm_form.api_key.trim().to_string()),
            api_key_env: self
                .llm_key_from_env
                .then(|| self.llm_form.api_key_env.trim().to_string()),
            note: None,
        };

        let Some(store) = self.llm_store.as_mut() else {
            self.set_error("llm store is not loaded");
            return false;
        };

        match store.upsert_profile(profile) {
            Ok(()) => {
                self.reload();
                self.set_notice(self.tr("profile saved", "profile saved"));
                true
            }
            Err(error) => {
                self.set_error(format!("failed to save profile: {error}"));
                false
            }
        }
    }

    fn switch_profile_action(&mut self, name: Option<&str>) {
        let Some(store) = self.llm_store.as_mut() else {
            self.set_error("llm store is not loaded");
            return;
        };
        let result = match name {
            Some(value) => store.set_active_profile(value),
            None => store.clear_active_profile(),
        };
        match result {
            Ok(()) => {
                self.reload();
                self.set_notice(self.tr("profile switched", "profile switched"));
            }
            Err(error) => self.set_error(format!("failed to switch profile: {error}")),
        }
    }

    fn save_limits_action(&mut self) {
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

        let Some(store) = self.llm_store.as_mut() else {
            self.set_error("llm store is not loaded");
            return;
        };

        match store.set_turn_token_limits(limits) {
            Ok(()) => {
                self.reload();
                self.set_notice(self.tr("token limits saved", "token limits saved"));
            }
            Err(error) => self.set_error(format!("failed to save limits: {error}")),
        }
    }

    fn save_app_action(&mut self) {
        let name = self.app_form.name.trim().to_string();
        let command = self.app_form.command.trim().to_string();
        if name.is_empty() || command.is_empty() {
            self.set_error(self.tr("", "app name and command cannot be empty"));
            return;
        }

        let app = AppRecord {
            name,
            command,
            description: self
                .app_form
                .description
                .trim()
                .to_string()
                .if_empty("quick action"),
        };

        let Some(store) = self.agent_store.as_mut() else {
            self.set_error("app store is not loaded");
            return;
        };

        match store.upsert_app(app) {
            Ok(()) => {
                self.app_form = AppForm::default();
                self.reload();
                self.set_notice(self.tr("app saved", "app saved"));
            }
            Err(error) => self.set_error(format!("failed to save app: {error}")),
        }
    }

    fn load_session_summary_action(
        &mut self,
        summary: &runtime::session_control::ManagedSessionSummary,
    ) {
        if self.busy {
            return;
        }
        match Session::load_from_path(&summary.path) {
            Ok(session) => {
                if let Some(store) = self.agent_store.as_mut() {
                    let _ = store.clear_active_thread();
                }
                self.active_thread_name = None;
                self.active_session = session;
                self.active_session_path = summary.path.clone();
                self.refresh_usage();
                self.active_tab = Tab::Chat;
                self.set_notice(format!("session loaded: {}", summary.id));
            }
            Err(error) => self.set_error(format!("failed to load session: {error}")),
        }
    }

    fn send_current_prompt(&mut self) {
        if self.busy {
            return;
        }
        let prompt = self.composer.trim().to_string();
        if prompt.is_empty() {
            return;
        }
        let prompt_with_attachments = self.build_prompt_with_attachments(&prompt);
        let estimated = estimate_text_tokens(&prompt_with_attachments);
        let limits = self
            .llm_store
            .as_ref()
            .map_or_else(TurnTokenLimits::default, LlmProfileStore::turn_token_limits);
        if let Err(error) = limits.check_input_estimate(estimated) {
            self.set_error(error);
            return;
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let config = GuiTurnConfig {
            workspace: self.active_chat_workspace(),
            session: self.active_session.clone(),
            session_path: self.active_session_path.clone(),
            prompt: prompt_with_attachments,
            model: self.active_model(),
            llm_profile: self
                .llm_store
                .as_ref()
                .and_then(LlmProfileStore::active_profile)
                .cloned(),
            turn_token_limits: limits,
            cancel_flag: cancel_flag.clone(),
        };
        self.optimistic_user_prompt = Some(prompt);
        self.live_reply.clear();
        self.inspector_events.clear();
        self.worker_rx = Some(rx);
        self.cancel_flag = Some(cancel_flag);
        self.pause_requested = false;
        self.busy = true;
        self.composer.clear();
        self.attached_files.clear();
        spawn_turn(config, tx);
    }

    fn pause_generation_action(&mut self) {
        if !self.busy {
            return;
        }
        if self.pause_requested {
            self.force_pause_generation_action();
            return;
        }
        if let Some(flag) = &self.cancel_flag {
            flag.store(true, Ordering::Relaxed);
        }
        self.pause_requested = true;
        self.set_notice(
            self.tr("正在停止当前回复...", "Stopping the current reply...")
                .to_string(),
        );
    }

    fn force_pause_generation_action(&mut self) {
        if !self.busy {
            return;
        }

        let retained_prompt = self.optimistic_user_prompt.clone().or_else(|| {
            self.active_session
                .prompt_history
                .last()
                .map(|entry| entry.text.clone())
        });

        let recovery_result = self.create_pause_recovery_session();
        self.worker_rx = None;
        self.cancel_flag = None;
        self.pause_requested = false;
        self.busy = false;
        self.live_reply.clear();
        self.optimistic_user_prompt = retained_prompt;

        match recovery_result {
            Ok((session, path)) => {
                self.active_session = session;
                self.active_session_path = path;
                self.refresh_session_list();
                self.inspector_events.push(InspectorEvent {
                    title: self.tr("强制暂停", "Force pause").to_string(),
                    body: self
                        .tr(
                            "已切换到新的恢复会话，避免后台卡住任务继续占用当前会话。",
                            "Switched to a recovery session so the stuck background task no longer blocks this chat.",
                        )
                        .to_string(),
                    is_error: true,
                });
                self.set_notice(
                    self.tr(
                        "已强制暂停当前等待，并切换到恢复会话。你可以编辑上一条输入，或直接发送下一条。",
                        "Force pause completed and switched to a recovery session. You can edit the last input or send the next one.",
                    )
                    .to_string(),
                );
            }
            Err(error) => {
                self.inspector_events.push(InspectorEvent {
                    title: self.tr("强制暂停", "Force pause").to_string(),
                    body: format!(
                        "{}: {}",
                        self.tr("恢复会话创建失败", "Failed to create recovery session"),
                        error
                    ),
                    is_error: true,
                });
                self.set_notice(
                    self.tr(
                        "已强制暂停界面等待，但恢复会话创建失败。建议先刷新会话列表再继续发送。",
                        "Force pause detached the UI wait, but recovery session creation failed. Refresh sessions before sending again.",
                    )
                    .to_string(),
                );
            }
        }
    }

    fn create_pause_recovery_session(&self) -> Result<(Session, PathBuf), String> {
        let workspace = self.active_chat_workspace();
        let session_store =
            SessionStore::from_cwd(&workspace).map_err(|error| error.to_string())?;
        let branch_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        let session = self
            .active_session
            .fork(Some(format!("pause-recovery-{branch_suffix}")));
        let handle = session_store.create_handle(&session.session_id);
        let session = session
            .with_persistence_path(handle.path.clone())
            .with_workspace_root(workspace);
        session
            .save_to_path(&handle.path)
            .map_err(|error| error.to_string())?;
        Ok((session, handle.path))
    }

    fn edit_last_prompt_action(&mut self) {
        if self.busy {
            return;
        }
        let prompt = self.optimistic_user_prompt.clone().or_else(|| {
            self.active_session
                .prompt_history
                .last()
                .map(|entry| entry.text.clone())
        });
        match prompt {
            Some(prompt) if !prompt.trim().is_empty() => {
                self.composer = prompt;
                self.set_notice(
                    self.tr(
                        "已载入上一条输入，可直接继续编辑。",
                        "Loaded the last input for editing.",
                    )
                    .to_string(),
                );
            }
            _ => self.set_error(
                self.tr(
                    "当前没有可编辑的上一条输入。",
                    "No previous input is available to edit.",
                )
                .to_string(),
            ),
        }
    }

    fn poll_worker_clean(&mut self) {
        let Some(rx) = self.worker_rx.take() else {
            return;
        };
        let rx = rx;
        let mut completed = false;
        while let Ok(event) = rx.try_recv() {
            match event {
                GuiWorkerEvent::AssistantDelta(text) => self.live_reply.push_str(&text),
                GuiWorkerEvent::ToolCallRequested { name, input } => {
                    self.inspector_events.push(InspectorEvent {
                        title: format!("Tool: {name}"),
                        body: clamp_inspector_event_body(&input),
                        is_error: false,
                    });
                }
                GuiWorkerEvent::ToolResult {
                    name,
                    output,
                    is_error,
                    status,
                    handoff_command,
                    handoff_reason,
                } => {
                    let (title, body) = match status.as_deref() {
                        Some("interactive_blocked") => (
                            format!("Terminal handoff: {name}"),
                            clamp_inspector_event_body(&format!(
                                "{}\n{}{}",
                                handoff_reason.unwrap_or_else(|| {
                                    "Interactive command detected in AI-managed execution."
                                        .to_string()
                                }),
                                handoff_command
                                    .as_deref()
                                    .map(|command| format!("Command: {command}\n"))
                                    .unwrap_or_default(),
                                output
                            )),
                        ),
                        Some("timeout") => (
                            format!("Timed out: {name}"),
                            clamp_inspector_event_body(&output),
                        ),
                        _ => (
                            format!("Result: {name}"),
                            clamp_inspector_event_body(&output),
                        ),
                    };
                    self.inspector_events.push(InspectorEvent {
                        title,
                        body,
                        is_error,
                    });
                }
                GuiWorkerEvent::PromptCache(event) => self.inspector_events.push(InspectorEvent {
                    title: self.tr("Prompt Cache", "Prompt Cache").to_string(),
                    body: format!("{} | drop={}", event.reason, event.token_drop),
                    is_error: event.unexpected,
                }),
                GuiWorkerEvent::Usage(usage) => self.latest_turn_usage = usage,
                GuiWorkerEvent::Completed {
                    session,
                    summary,
                    cumulative_usage,
                } => {
                    if let Some(note) = summary.auto_compaction {
                        self.inspector_events.push(InspectorEvent {
                            title: self.tr("Auto compacted", "Auto compacted").to_string(),
                            body: format!(
                                "{} {}",
                                self.tr("removed messages", "removed messages"),
                                note.removed_message_count
                            ),
                            is_error: false,
                        });
                    }

                    let limits = self
                        .llm_store
                        .as_ref()
                        .map_or_else(TurnTokenLimits::default, LlmProfileStore::turn_token_limits);
                    if let Some(warning) = limits.output_limit_warning(summary.usage.output_tokens)
                    {
                        self.inspector_events.push(InspectorEvent {
                            title: self
                                .tr("Output limit note", "Output limit note")
                                .to_string(),
                            body: warning,
                            is_error: false,
                        });
                    }

                    self.active_session = session;
                    self.latest_turn_usage = summary.usage;
                    self.cumulative_usage = cumulative_usage;
                    self.live_reply.clear();
                    self.optimistic_user_prompt = None;
                    self.cancel_flag = None;
                    self.pause_requested = false;
                    self.busy = false;
                    self.apply_pending_chat_switch();
                    completed = true;
                    self.refresh_session_list();
                }
                GuiWorkerEvent::Failed { message, session } => {
                    let retained_prompt = if message == GUI_CANCELLED_MESSAGE {
                        session
                            .prompt_history
                            .last()
                            .map(|entry| entry.text.clone())
                            .or_else(|| self.optimistic_user_prompt.clone())
                    } else {
                        None
                    };
                    self.active_session = session;
                    self.optimistic_user_prompt = retained_prompt;
                    self.cancel_flag = None;
                    self.pause_requested = false;
                    self.busy = false;
                    self.apply_pending_chat_switch();
                    completed = true;
                    if message == GUI_CANCELLED_MESSAGE {
                        self.set_notice(
                            self.tr(
                                "当前回复已暂停。你可以编辑上一条输入，或直接发送下一条。",
                                "The reply has been paused. You can edit the last input or send the next one.",
                            )
                            .to_string(),
                        );
                    } else {
                        self.set_error(message);
                    }
                    self.refresh_usage();
                    self.refresh_session_list();
                }
            }
        }
        if !completed {
            self.worker_rx = Some(rx);
        }
    }

    fn session_cost_label_clean(&self, usage: TokenUsage) -> String {
        let model = self.active_model();
        let pricing = pricing_for_usage(&model, usage)
            .or_else(|| resolve_pricing_for_model(Some(&model)).pricing);
        pricing
            .map(|resolved| {
                let estimate = usage.estimate_cost_with_pricing(resolved);
                format_currency(estimate.total_cost(), estimate.currency)
            })
            .unwrap_or_else(|| self.tr("unavailable", "unavailable").to_string())
    }

    fn session_cost_label(&self, usage: TokenUsage) -> String {
        self.session_cost_label_clean(usage)
    }
    fn render_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_help {
            return;
        }
        let title = self.tr("Help & Notes", "Help & Notes").to_string();
        let mut open = self.show_help;
        egui::Window::new(title)
            .open(&mut open)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label(self.tr(
                    "",
                    "This is a GUI workspace for multi-model coding agents, designed to keep threads, chat, tool events, and cost in one screen.",
                ));
                ui.separator();
                ui.label(
                    RichText::new(self.tr("Recommended flow", "Recommended flow"))
                        .strong()
                        .color(self.theme.accent()),
                );
                ui.label(self.tr(
                    "",
                    "1. Go to Models first, create or edit an LLM profile, then activate it.",
                ));
                ui.label(self.tr(
                    "",
                    "2. Use the left thread tree to create per-folder threads with isolated sessions.",
                ));
                ui.label(self.tr(
                    "",
                    "3. Chat in the center panel for multi-turn conversations; use Ctrl+Enter or the Send button.",
                ));
                ui.label(self.tr(
                    "",
                    "4. The inspector on the right shows tool calls, token usage, and estimated cost in RMB/USD.",
                ));
                ui.label(self.tr(
                    "",
                    "5. Use Apps to save reusable command notes, and Sessions to inspect/load workspace sessions.",
                ));
            });
        self.show_help = open;
    }

    fn render_thread_window(&mut self, ctx: &egui::Context) {
        if !self.show_thread_form {
            return;
        }
        let title = self.tr("Create Thread", "Create Thread").to_string();
        let mut create = false;
        let mut cancel = false;
        let mut open = self.show_thread_form;

        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(self.tr("Thread name", "Thread name"));
                ui.text_edit_singleline(&mut self.thread_form.name);
                ui.label(self.tr("Thread folder", "Thread folder"));
                ui.text_edit_singleline(&mut self.thread_form.folder);
                ui.label(self.tr("Description", "Description"));
                ui.add(TextEdit::multiline(&mut self.thread_form.description).desired_rows(3));
                ui.horizontal(|ui| {
                    if ui.button(self.tr("Create", "Create")).clicked() {
                        create = true;
                    }
                    if ui.button(self.tr("Cancel", "Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        self.show_thread_form = open;

        if create {
            self.add_thread_action();
        }
        if cancel {
            self.show_thread_form = false;
        }
    }

    fn render_chat_tab(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("claw_gui_threads")
            .resizable(true)
            .default_width(250.0)
            .min_width(210.0)
            .show(ctx, |ui| self.render_thread_tree(ui));

        egui::SidePanel::right("claw_gui_inspector")
            .resizable(true)
            .default_width(320.0)
            .min_width(260.0)
            .show(ctx, |ui| self.render_inspector(ui));

        egui::CentralPanel::default().show(ctx, |ui| self.render_chat_stream(ui));
    }
}

#[allow(dead_code)]
impl ClawGuiApp {
    fn render_thread_tree(&mut self, ui: &mut egui::Ui) {
        let threads = self
            .agent_store
            .as_ref()
            .map(AgentWorkspaceStore::list_threads)
            .unwrap_or_default();
        let apps = self
            .agent_store
            .as_ref()
            .map(AgentWorkspaceStore::list_apps)
            .unwrap_or_default();

        section_card(
            ui,
            self.theme,
            self.tr("Thread Tree", "Thread Tree"),
            |ui| {
                ui.horizontal(|ui| {
                    if ui.button(self.tr("New Thread", "New Thread")).clicked() {
                        self.show_thread_form = true;
                    }
                    if ui
                        .button(self.tr("Workspace Chat", "Workspace Chat"))
                        .clicked()
                    {
                        self.select_workspace_chat();
                    }
                });

                let selected = self.active_thread_name.is_none();
                if ui
                    .selectable_label(selected, self.tr("Current workspace", "Current workspace"))
                    .clicked()
                {
                    self.select_workspace_chat();
                }
                ui.small(self.workspace.display().to_string());
                ui.separator();

                if threads.is_empty() {
                    ui.label(self.tr(
                        "",
                        "No saved threads yet. Create one for each folder you want to isolate.",
                    ));
                } else {
                    let mut remove_name = None::<String>;
                    for thread in threads {
                        let selected =
                            self.active_thread_name.as_deref() == Some(thread.name.as_str());
                        Frame::group(ui.style())
                            .fill(self.theme.subpanel_fill())
                            .stroke(Stroke::new(1.0, self.theme.accent().gamma_multiply(0.30)))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if ui.selectable_label(selected, &thread.name).clicked() {
                                        self.activate_thread(&thread.name);
                                    }
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if ui.small_button(self.tr("Remove", "Remove")).clicked() {
                                            remove_name = Some(thread.name.clone());
                                        }
                                    });
                                });
                                if let Some(description) = &thread.description {
                                    if !description.trim().is_empty() {
                                        ui.small(description);
                                    }
                                }
                                ui.small(thread.folder);
                            });
                    }
                    if let Some(name) = remove_name {
                        self.remove_thread(&name);
                    }
                }
            },
        );

        ui.add_space(10.0);

        section_card(
            ui,
            self.theme,
            self.tr("App Shortcuts", "App Shortcuts"),
            |ui| {
                if apps.is_empty() {
                    ui.label(self.tr(
                        "",
                        "No saved apps yet. Use the Apps tab to store reusable commands and notes.",
                    ));
                } else {
                    for app in apps.iter().take(6) {
                        ui.label(RichText::new(&app.name).strong());
                        ui.small(&app.description);
                        ui.monospace(&app.command);
                        ui.separator();
                    }
                }
                if ui
                    .button(self.tr("Open Apps tab", "Open Apps tab"))
                    .clicked()
                {
                    self.active_tab = Tab::Apps;
                }
            },
        );
    }

    fn render_chat_stream(&mut self, ui: &mut egui::Ui) {
        let messages = self.active_session.messages.clone();
        let estimated_input = estimate_text_tokens(&self.composer);
        let composer_hint = self
            .tr(
                "",
                "Write a task, question, or edit request. Multi-turn context stays in the current thread.",
            )
            .to_string();
        let limits = self
            .llm_store
            .as_ref()
            .map_or_else(TurnTokenLimits::default, LlmProfileStore::turn_token_limits);

        section_card(ui, self.theme, &self.active_chat_title_clean(), |ui| {
            if false {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!(
                        "{}: {}",
                        self.tr("Folder", "Folder"),
                        self.active_chat_workspace().display()
                    ));
                    ui.separator();
                    ui.label(format!(
                        "{}: {}",
                        self.tr("Session", "Session"),
                        self.active_session.session_id
                    ));
                    ui.separator();
                    ui.label(format!(
                        "{}: {}",
                        self.tr("Messages", "Messages"),
                        messages.len()
                    ));
                    if self.busy {
                        ui.separator();
                        ui.label(
                            RichText::new(self.tr("Generating...", "Generating..."))
                                .strong()
                                .color(self.theme.accent()),
                        );
                    }
                });
            }
            ui.small(self.tr("输入区会随内容自动增高", "Composer auto-grows with content"));
        });

        ui.add_space(8.0);

        section_card(
            ui,
            self.theme,
            self.tr("Conversation", "Conversation"),
            |ui| {
                ScrollArea::vertical()
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if messages.is_empty()
                        && self.optimistic_user_prompt.is_none()
                        && self.live_reply.is_empty()
                    {
                        ui.add_space(24.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new(self.tr("Start a new multi-turn chat", "Start a new multi-turn chat"))
                                    .strong()
                                    .size(18.0),
                            );
                            ui.label(self.tr(
                                "",
                                "Switch threads on the left, chat in the center, inspect tools/tokens/cost on the right.",
                            ));
                        });
                        ui.add_space(24.0);
                    }

                    for message in messages
                        .iter()
                        .filter(|message| message.role != MessageRole::System)
                    {
                        render_message_card(ui, message, self.language, self.theme);
                        ui.add_space(8.0);
                    }

                    if let Some(prompt) = &self.optimistic_user_prompt {
                        let pending = ConversationMessage {
                            role: MessageRole::User,
                            blocks: vec![ContentBlock::Text { text: prompt.clone() }],
                            usage: None,
                        };
                        render_message_card(ui, &pending, self.language, self.theme);
                        ui.add_space(8.0);
                    }

                    if !self.live_reply.is_empty() {
                        let streaming = ConversationMessage {
                            role: MessageRole::Assistant,
                            blocks: vec![ContentBlock::Text {
                                text: self.live_reply.clone(),
                            }],
                            usage: None,
                        };
                        render_message_card(ui, &streaming, self.language, self.theme);
                    }
                });
            },
        );

        ui.add_space(8.0);

        section_card(ui, self.theme, self.tr("Composer", "Composer"), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "{}: {}",
                    self.tr("Estimated input tokens", "Estimated input tokens"),
                    estimated_input
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Limits", "Limits"),
                    limits.summary_line()
                ));
                ui.separator();
                ui.label(self.tr("快捷键：Ctrl+Enter 发送", "Shortcut: Ctrl+Enter to send"));
            });

            ui.add(
                TextEdit::multiline(&mut self.composer)
                    .desired_rows(6)
                    .hint_text(composer_hint),
            );

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.busy, egui::Button::new(self.tr("Send", "Send")))
                    .clicked()
                {
                    self.send_current_prompt();
                }
                if ui.button(self.tr("Clear draft", "Clear draft")).clicked() {
                    self.composer.clear();
                }
                if ui.button(self.tr("Models", "Models")).clicked() {
                    self.active_tab = Tab::Models;
                }
            });
        });
    }

    fn render_inspector(&mut self, ui: &mut egui::Ui) {
        section_card(ui, self.theme, self.tr("Connection", "Connection"), |ui| {
            ui.label(format!(
                "{}: {}",
                self.tr("Profile", "Profile"),
                self.active_profile_name()
                    .unwrap_or_else(|| self.tr("(none)", "(none)").to_string())
            ));
            ui.label(format!(
                "{}: {}",
                self.tr("Provider", "Provider"),
                self.active_provider_clean()
            ));
            ui.label(format!(
                "{}: {}",
                self.tr("Model", "Model"),
                self.active_model()
            ));
            ui.label(format!(
                "{}: {}",
                self.tr("Base URL", "Base URL"),
                self.active_base_url_clean()
            ));
        });

        ui.add_space(10.0);

        section_card(
            ui,
            self.theme,
            self.tr("Token Usage", "Token Usage"),
            |ui| {
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn input", "Turn input"),
                    self.latest_turn_usage.input_tokens
                ));
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn output", "Turn output"),
                    self.latest_turn_usage.output_tokens
                ));
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn cache write", "Turn cache write"),
                    self.latest_turn_usage.cache_creation_input_tokens
                ));
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn cache read", "Turn cache read"),
                    self.latest_turn_usage.cache_read_input_tokens
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn total", "Turn total"),
                    token_total(self.latest_turn_usage)
                ));
                ui.label(format!(
                    "{}: {}",
                    self.tr("Session total", "Session total"),
                    token_total(self.cumulative_usage)
                ));
            },
        );

        ui.add_space(10.0);

        section_card(
            ui,
            self.theme,
            self.tr("Cost Estimate", "Cost Estimate"),
            |ui| {
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn cost", "Turn cost"),
                    self.session_cost_label_clean(self.latest_turn_usage)
                ));
                ui.label(format!(
                    "{}: {}",
                    self.tr("Session cost", "Session cost"),
                    self.session_cost_label_clean(self.cumulative_usage)
                ));
            },
        );

        ui.add_space(10.0);

        section_card(
            ui,
            self.theme,
            self.tr("Tool Events", "Tool Events"),
            |ui| {
                ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.inspector_events.is_empty() {
                        ui.label(self.tr(
                            "",
                            "No tool events yet. Calls like read/edit/bash/search will appear here.",
                        ));
                    } else {
                        for event in &self.inspector_events {
                            let color = if event.is_error {
                                Color32::from_rgb(179, 60, 60)
                            } else {
                                self.theme.accent()
                            };
                            Frame::group(ui.style())
                                .fill(self.theme.subpanel_fill())
                                .stroke(Stroke::new(1.0, color.gamma_multiply(0.35)))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(&event.title).strong().color(color));
                                    ui.label(&event.body);
                                });
                            ui.add_space(6.0);
                        }
                    }
                });
            },
        );
    }
}

#[allow(dead_code)]
impl ClawGuiApp {
    fn render_models_tab(&mut self, ctx: &egui::Context) {
        let profiles = self
            .llm_store
            .as_ref()
            .map(LlmProfileStore::list_profiles)
            .unwrap_or_default();
        let env_label = self.tr("Env var", "Env var").to_string();
        let inline_label = self.tr("Inline key", "Inline key").to_string();
        let limits_summary = self
            .llm_store
            .as_ref()
            .map_or_else(TurnTokenLimits::default, LlmProfileStore::turn_token_limits)
            .summary_line();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |columns| {
                section_card(&mut columns[0], self.theme, self.tr("LLM Profile Editor", "LLM Profile Editor"), |ui| {
                    ui.label(self.tr("Name", "Name"));
                    ui.text_edit_singleline(&mut self.llm_form.name);
                    ui.label(self.tr("Provider", "Provider"));
                    ui.text_edit_singleline(&mut self.llm_form.provider);
                    ui.label(self.tr("Model", "Model"));
                    ui.text_edit_singleline(&mut self.llm_form.model);
                    ui.label(self.tr("Base URL", "Base URL"));
                    ui.text_edit_singleline(&mut self.llm_form.base_url);
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.llm_key_from_env, true, env_label.as_str());
                        ui.selectable_value(&mut self.llm_key_from_env, false, inline_label.as_str());
                    });

                    if self.llm_key_from_env {
                        ui.label(self.tr("API key environment variable", "API key environment variable"));
                        ui.text_edit_singleline(&mut self.llm_form.api_key_env);
                    } else {
                        ui.label(self.tr("API key", "API key"));
                        ui.add(TextEdit::singleline(&mut self.llm_form.api_key).password(true));
                    }

                    ui.horizontal(|ui| {
                        if ui.button(self.tr("Save", "Save")).clicked() {
                            self.save_profile_action();
                        }
                        if ui.button(self.tr("Save + Activate", "Save + Activate")).clicked() {
                            let profile_name = self.llm_form.name.trim().to_string();
                            if self.save_profile_action() {
                                self.switch_profile_action(Some(&profile_name));
                            }
                        }
                        if ui.button(self.tr("Clear form", "Clear form")).clicked() {
                            self.llm_form = LlmForm::default();
                            self.llm_key_from_env = false;
                        }
                    });
                });

                section_card(&mut columns[1], self.theme, self.tr("Saved Profiles", "Saved Profiles"), |ui| {
                    ui.label(format!(
                        "{}: {}",
                        self.tr("Current limits", "Current limits"),
                        limits_summary
                    ));
                    ui.add_space(4.0);

                    if profiles.is_empty() {
                        ui.label(self.tr(
                            "",
                            "No profiles yet. Save a deepseek / qwen / kimi / glm / openai-compatible profile first.",
                        ));
                    } else {
                        for profile in profiles {
                            let active =
                                self.active_profile_name().as_deref() == Some(profile.name.as_str());
                            Frame::group(ui.style())
                                .fill(self.theme.subpanel_fill())
                                .stroke(Stroke::new(1.0, self.theme.accent().gamma_multiply(0.30)))
                                .show(ui, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            RichText::new(&profile.name)
                                                .strong()
                                                .color(if active {
                                                    self.theme.accent()
                                                } else {
                                                    ui.visuals().text_color()
                                                }),
                                        );
                                        if active {
                                            ui.label(self.tr("Active", "Active"));
                                        }
                                    });
                                    ui.small(format!(
                                        "provider={} model={}",
                                        profile.normalized_provider(),
                                        profile.model
                                    ));
                                    if let Some(base_url) = &profile.base_url {
                                        if !base_url.trim().is_empty() {
                                            ui.small(base_url);
                                        }
                                    }
                                    ui.small(profile.key_source_label());
                                    ui.horizontal(|ui| {
                                        if ui.button(self.tr("Edit", "Edit")).clicked() {
                                            self.load_profile_into_form(&profile);
                                        }
                                        if ui.button(self.tr("Activate", "Activate")).clicked() {
                                            self.switch_profile_action(Some(&profile.name));
                                        }
                                        if ui.button(self.tr("Remove", "Remove")).clicked() {
                                            self.remove_profile(&profile.name);
                                        }
                                    });
                                });
                            ui.add_space(6.0);
                        }
                        if ui.button(self.tr("Clear active profile", "Clear active profile")).clicked() {
                            self.switch_profile_action(None);
                        }
                    }
                });
            });

            ui.add_space(12.0);

            section_card(ui, self.theme, self.tr("Per-turn Token Limits", "Per-turn Token Limits"), |ui| {
                ui.columns(4, |columns| {
                    columns[0].label(self.tr("Min input", "Min input"));
                    columns[0].text_edit_singleline(&mut self.limits_form.min_input);
                    columns[1].label(self.tr("Max input", "Max input"));
                    columns[1].text_edit_singleline(&mut self.limits_form.max_input);
                    columns[2].label(self.tr("Min output", "Min output"));
                    columns[2].text_edit_singleline(&mut self.limits_form.min_output);
                    columns[3].label(self.tr("Max output", "Max output"));
                    columns[3].text_edit_singleline(&mut self.limits_form.max_output);
                });
                ui.horizontal(|ui| {
                    if ui.button(self.tr("Save limits", "Save limits")).clicked() {
                        self.save_limits_action();
                    }
                    if ui.button(self.tr("Clear limits", "Clear limits")).clicked() {
                        self.limits_form = LimitsForm::default();
                    }
                });
            });
        });
    }

    fn render_apps_tab(&mut self, ctx: &egui::Context) {
        let apps = self
            .agent_store
            .as_ref()
            .map(AgentWorkspaceStore::list_apps)
            .unwrap_or_default();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |columns| {
                section_card(
                    &mut columns[0],
                    self.theme,
                    self.tr("Save App Command", "Save App Command"),
                    |ui| {
                        ui.label(self.tr("Name", "Name"));
                        ui.text_edit_singleline(&mut self.app_form.name);
                        ui.label(self.tr("Command", "Command"));
                        ui.text_edit_singleline(&mut self.app_form.command);
                        ui.label(self.tr("Description", "Description"));
                        ui.add(TextEdit::multiline(&mut self.app_form.description).desired_rows(4));
                        ui.horizontal(|ui| {
                            if ui.button(self.tr("Save app", "Save app")).clicked() {
                                self.save_app_action();
                            }
                            if ui.button(self.tr("Clear form", "Clear form")).clicked() {
                                self.app_form = AppForm::default();
                            }
                        });
                    },
                );

                section_card(
                    &mut columns[1],
                    self.theme,
                    self.tr("Saved Apps", "Saved Apps"),
                    |ui| {
                        if apps.is_empty() {
                            ui.label(self.tr("No saved apps yet.", "No saved apps yet."));
                        } else {
                            for app in apps {
                                Frame::group(ui.style())
                                    .fill(self.theme.subpanel_fill())
                                    .stroke(Stroke::new(
                                        1.0,
                                        self.theme.accent().gamma_multiply(0.30),
                                    ))
                                    .show(ui, |ui| {
                                        ui.label(RichText::new(&app.name).strong());
                                        ui.small(&app.description);
                                        ui.monospace(&app.command);
                                        ui.horizontal(|ui| {
                                            if ui
                                                .button(self.tr("Load into form", "Load into form"))
                                                .clicked()
                                            {
                                                self.app_form = AppForm {
                                                    name: app.name.clone(),
                                                    command: app.command.clone(),
                                                    description: app.description.clone(),
                                                };
                                            }
                                            if ui.button(self.tr("Remove", "Remove")).clicked() {
                                                self.remove_app(&app.name);
                                            }
                                        });
                                    });
                                ui.add_space(6.0);
                            }
                        }
                    },
                );
            });
        });
    }

    fn render_sessions_tab(&mut self, ctx: &egui::Context) {
        let sessions = self.sessions.clone();

        egui::CentralPanel::default().show(ctx, |ui| {
            section_card(
                ui,
                self.theme,
                self.tr("Workspace Sessions", "Workspace Sessions"),
                |ui| {
                    ui.horizontal(|ui| {
                        if ui.button(self.tr("Refresh", "Refresh")).clicked() {
                            self.refresh_session_list();
                        }
                        ui.label(format!("{}: {}", self.tr("Total", "Total"), sessions.len()));
                    });

                    if sessions.is_empty() {
                        ui.label(self.tr("", "No saved sessions in this workspace yet."));
                        return;
                    }

                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for summary in sessions {
                                let active = self.active_session_path == summary.path;
                                Frame::group(ui.style())
                                    .fill(self.theme.subpanel_fill())
                                    .stroke(Stroke::new(
                                        1.0,
                                        self.theme.accent().gamma_multiply(0.30),
                                    ))
                                    .show(ui, |ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.label(RichText::new(&summary.id).strong().color(
                                                if active {
                                                    self.theme.accent()
                                                } else {
                                                    ui.visuals().text_color()
                                                },
                                            ));
                                            if active {
                                                ui.label(
                                                    self.tr("Current session", "Current session"),
                                                );
                                            }
                                        });
                                        ui.small(format!(
                                            "{}: {}",
                                            self.tr("Messages", "Messages"),
                                            summary.message_count
                                        ));
                                        if let Some(parent) = &summary.parent_session_id {
                                            ui.small(format!(
                                                "{}: {}",
                                                self.tr("Parent", "Parent"),
                                                parent
                                            ));
                                        }
                                        if let Some(branch) = &summary.branch_name {
                                            ui.small(format!(
                                                "{}: {}",
                                                self.tr("Branch", "Branch"),
                                                branch
                                            ));
                                        }
                                        ui.small(summary.path.display().to_string());
                                        ui.horizontal(|ui| {
                                            if ui
                                                .button(self.tr("Load into chat", "Load into chat"))
                                                .clicked()
                                            {
                                                self.load_session_summary_action(&summary);
                                            }
                                        });
                                    });
                                ui.add_space(8.0);
                            }
                        });
                },
            );
        });
    }
}

#[allow(dead_code)]
impl ClawGuiApp {
    fn render_top_bar_v2(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("claw_gui_top_bar_v2").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(self.tr("Claw Client", "Claw Client"))
                        .strong()
                        .size(22.0)
                        .color(self.theme.accent()),
                );
                ui.label(
                    RichText::new(self.tr(
                        "A Codex-like visual workspace",
                        "A Codex-like visual workspace",
                    ))
                    .small(),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(self.tr("Help", "Help")).clicked() {
                        self.show_help = true;
                    }

                    ComboBox::from_id_salt("gui_theme_v2")
                        .selected_text(self.theme.label_v2(self.language))
                        .show_ui(ui, |ui| {
                            for theme in UiTheme::all() {
                                ui.selectable_value(
                                    &mut self.theme,
                                    theme,
                                    theme.label_v2(self.language),
                                );
                            }
                        });

                    ComboBox::from_id_salt("gui_lang_v2")
                        .selected_text(self.language.label_v2())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.language, Language::Zh, "中文");
                            ui.selectable_value(&mut self.language, Language::En, "English");
                        });
                });
            });
            ui.small(self.tr("输入区会随内容自动增高", "Composer auto-grows with content"));

            ui.horizontal_wrapped(|ui| {
                for tab in Tab::all() {
                    let selected = self.active_tab == tab;
                    if ui
                        .selectable_label(selected, tab.label_v2(self.language))
                        .clicked()
                    {
                        self.active_tab = tab;
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label(self.tr("Workspace", "Workspace"));
                let response = ui.add_sized(
                    [ui.available_width() - 290.0, 28.0],
                    TextEdit::singleline(&mut self.workspace_input),
                );
                if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    self.reload();
                }
                if ui.button(self.tr("Load", "Load")).clicked() {
                    self.reload();
                }
                if ui
                    .button(self.tr("Workspace Chat", "Workspace Chat"))
                    .clicked()
                {
                    self.select_workspace_chat();
                }
            });

            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "{}: {}",
                    self.tr("Model", "Model"),
                    self.active_model()
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Provider", "Provider"),
                    self.active_provider_clean()
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Base URL", "Base URL"),
                    self.active_base_url_clean()
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Thread", "Thread"),
                    self.active_chat_title_clean()
                ));
            });

            if let Some(error) = &self.error {
                ui.colored_label(Color32::from_rgb(180, 48, 48), error);
            } else if let Some(notice) = &self.notice {
                ui.colored_label(self.theme.accent(), notice);
            }

            ui.add_space(4.0);
        });
    }

    fn render_help_window_v2(&mut self, ctx: &egui::Context) {
        if !self.show_help {
            return;
        }
        let title = self.tr("Help & Notes", "Help & Notes").to_string();
        let mut open = self.show_help;
        egui::Window::new(title)
            .open(&mut open)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.label(self.tr(
                    "",
                    "This is a GUI workspace for multi-model coding agents, designed to keep threads, chat, tool events, and cost in one screen.",
                ));
                ui.separator();
                ui.label(
                    RichText::new(self.tr("Recommended flow", "Recommended flow"))
                        .strong()
                        .color(self.theme.accent()),
                );
                ui.label(self.tr(
                    "",
                    "1. Go to Models first, create or edit an LLM profile, then activate it.",
                ));
                ui.label(self.tr(
                    "",
                    "2. Use the left thread tree to create per-folder threads with isolated sessions.",
                ));
                ui.label(self.tr(
                    "",
                    "3. Chat in the center panel for multi-turn conversations; use Ctrl+Enter or the Send button.",
                ));
                ui.label(self.tr(
                    "",
                    "4. The left file tree and skills area provide workspace context, while the right inspector shows tools, token usage, and cost.",
                ));
                ui.label(self.tr(
                    "",
                    "5. Use Apps to save reusable command notes, and Sessions to inspect/load workspace sessions.",
                ));
            });
        self.show_help = open;
    }

    fn render_thread_window_v2(&mut self, ctx: &egui::Context) {
        if !self.show_thread_form {
            return;
        }
        let title = self.tr("Create Thread", "Create Thread").to_string();
        let mut create = false;
        let mut cancel = false;
        let mut open = self.show_thread_form;

        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(self.tr("Thread name", "Thread name"));
                ui.text_edit_singleline(&mut self.thread_form.name);
                ui.label(self.tr("Thread folder", "Thread folder"));
                ui.text_edit_singleline(&mut self.thread_form.folder);
                ui.label(self.tr("Description", "Description"));
                ui.add(TextEdit::multiline(&mut self.thread_form.description).desired_rows(3));
                ui.horizontal(|ui| {
                    if ui.button(self.tr("Create", "Create")).clicked() {
                        create = true;
                    }
                    if ui.button(self.tr("Cancel", "Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        self.show_thread_form = open;

        if create {
            self.add_thread_action();
        }
        if cancel {
            self.show_thread_form = false;
        }
    }

    fn render_chat_tab_v2(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("claw_gui_threads_v2")
            .resizable(true)
            .default_width(280.0)
            .min_width(230.0)
            .show(ctx, |ui| self.render_thread_tree_v2(ui));

        egui::SidePanel::right("claw_gui_inspector_v2")
            .resizable(true)
            .default_width(330.0)
            .min_width(280.0)
            .show(ctx, |ui| self.render_inspector_v2(ui));

        egui::CentralPanel::default().show(ctx, |ui| self.render_chat_stream_v2(ui));
    }
}

#[allow(dead_code)]
impl ClawGuiApp {
    fn render_thread_tree_v2(&mut self, ui: &mut egui::Ui) {
        let threads = self
            .agent_store
            .as_ref()
            .map(AgentWorkspaceStore::list_threads)
            .unwrap_or_default();
        let apps = self
            .agent_store
            .as_ref()
            .map(AgentWorkspaceStore::list_apps)
            .unwrap_or_default();
        let file_entries = collect_workspace_entries(&self.active_chat_workspace(), 1, 28);
        let skills = collect_skill_names(&self.workspace, 10);

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            section_card(ui, self.theme, self.tr("Thread Tree", "Thread Tree"), |ui| {
                ui.horizontal(|ui| {
                    if ui.button(self.tr("New Thread", "New Thread")).clicked() {
                        self.show_thread_form = true;
                    }
                    if ui.button(self.tr("Workspace Chat", "Workspace Chat")).clicked() {
                        self.select_workspace_chat();
                    }
                });

                let selected = self.active_thread_name.is_none();
                if ui
                    .selectable_label(selected, self.tr("Current workspace", "Current workspace"))
                    .clicked()
                {
                    self.select_workspace_chat();
                }
                ui.small(self.workspace.display().to_string());
                ui.separator();

                if threads.is_empty() {
                    ui.label(self.tr(
                        "",
                        "No saved threads yet. Create one for each folder you want to isolate.",
                    ));
                } else {
                    let mut remove_name = None::<String>;
                    for thread in threads {
                        let selected = self.active_thread_name.as_deref() == Some(thread.name.as_str());
                        Frame::group(ui.style())
                            .fill(self.theme.subpanel_fill())
                            .stroke(Stroke::new(1.0, self.theme.accent().gamma_multiply(0.30)))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if ui.selectable_label(selected, &thread.name).clicked() {
                                        self.activate_thread(&thread.name);
                                    }
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if ui.small_button(self.tr("Remove", "Remove")).clicked() {
                                            remove_name = Some(thread.name.clone());
                                        }
                                    });
                                });
                                if let Some(description) = &thread.description {
                                    if !description.trim().is_empty() {
                                        ui.small(description);
                                    }
                                }
                                ui.small(thread.folder);
                            });
                        ui.add_space(6.0);
                    }
                    if let Some(name) = remove_name {
                        self.remove_thread(&name);
                    }
                }
            });

            ui.add_space(10.0);

            section_card(ui, self.theme, self.tr("File Tree", "File Tree"), |ui| {
                if file_entries.is_empty() {
                    ui.label(self.tr("No files to display.", "No files to display."));
                } else {
                    for entry in file_entries {
                        ui.monospace(entry);
                    }
                }
            });

            ui.add_space(10.0);

            section_card(ui, self.theme, self.tr("Skills", "Skills"), |ui| {
                if skills.is_empty() {
                    ui.label(self.tr(
                        "",
                        "No installed skills were detected. You can still use CLI /skills list and /skills install <path>.",
                    ));
                } else {
                    for skill in skills {
                        ui.label(RichText::new(skill).strong());
                    }
                }
                ui.small(self.tr(
                    "",
                    "This area currently shows detected skill names. Install/enable actions can be added next.",
                ));
            });

            ui.add_space(10.0);

            section_card(ui, self.theme, self.tr("App Shortcuts", "App Shortcuts"), |ui| {
                if apps.is_empty() {
                    ui.label(self.tr(
                        "",
                        "No saved apps yet. Use the Apps tab to store reusable commands and notes.",
                    ));
                } else {
                    for app in apps.iter().take(6) {
                        ui.label(RichText::new(&app.name).strong());
                        ui.small(&app.description);
                        ui.monospace(&app.command);
                        ui.separator();
                    }
                }
                if ui.button(self.tr("Open Apps tab", "Open Apps tab")).clicked() {
                    self.active_tab = Tab::Apps;
                }
            });
        });
    }

    fn render_chat_stream_v2(&mut self, ui: &mut egui::Ui) {
        let messages = self.active_session.messages.clone();
        let estimated_input = estimate_text_tokens(&self.composer);
        let composer_hint = self
            .tr(
                "",
                "Write a task, question, or edit request. Multi-turn context stays in the current thread.",
            )
            .to_string();
        let limits = self
            .llm_store
            .as_ref()
            .map_or_else(TurnTokenLimits::default, LlmProfileStore::turn_token_limits);

        section_card(ui, self.theme, &self.active_chat_title_clean(), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "{}: {}",
                    self.tr("Folder", "Folder"),
                    self.active_chat_workspace().display()
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Session", "Session"),
                    self.active_session.session_id
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Messages", "Messages"),
                    messages.len()
                ));
                if self.busy {
                    ui.separator();
                    ui.label(
                        RichText::new(self.tr("Generating...", "Generating..."))
                            .strong()
                            .color(self.theme.accent()),
                    );
                }
            });
        });

        ui.add_space(8.0);

        section_card(
            ui,
            self.theme,
            self.tr("Conversation", "Conversation"),
            |ui| {
                ScrollArea::vertical()
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if messages.is_empty()
                        && self.optimistic_user_prompt.is_none()
                        && self.live_reply.is_empty()
                    {
                        ui.add_space(24.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new(self.tr("Start a new multi-turn chat", "Start a new multi-turn chat"))
                                    .strong()
                                    .size(18.0),
                            );
                            ui.label(self.tr(
                                "",
                                "Switch threads and files on the left, chat in the center, inspect tools, tokens, and cost on the right.",
                            ));
                        });
                        ui.add_space(24.0);
                    }

                    for message in messages
                        .iter()
                        .filter(|message| message.role != MessageRole::System)
                    {
                        render_message_card_v2(ui, message, self.language, self.theme);
                        ui.add_space(8.0);
                    }

                    if let Some(prompt) = &self.optimistic_user_prompt {
                        let pending = ConversationMessage {
                            role: MessageRole::User,
                            blocks: vec![ContentBlock::Text { text: prompt.clone() }],
                            usage: None,
                        };
                        render_message_card_v2(ui, &pending, self.language, self.theme);
                        ui.add_space(8.0);
                    }

                    if !self.live_reply.is_empty() {
                        let streaming = ConversationMessage {
                            role: MessageRole::Assistant,
                            blocks: vec![ContentBlock::Text {
                                text: self.live_reply.clone(),
                            }],
                            usage: None,
                        };
                        render_message_card_v2(ui, &streaming, self.language, self.theme);
                    }
                });
            },
        );

        ui.add_space(8.0);

        section_card(ui, self.theme, self.tr("Composer", "Composer"), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "{}: {}",
                    self.tr("Estimated input tokens", "Estimated input tokens"),
                    estimated_input
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Limits", "Limits"),
                    limits.summary_line()
                ));
                ui.separator();
                ui.label(self.tr(
                    "Shortcut: Ctrl+Enter to send",
                    "Shortcut: Ctrl+Enter to send",
                ));
            });

            ui.add(
                TextEdit::multiline(&mut self.composer)
                    .desired_rows(6)
                    .hint_text(composer_hint),
            );

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.busy, egui::Button::new(self.tr("Send", "Send")))
                    .clicked()
                {
                    self.send_current_prompt();
                }
                if ui.button(self.tr("Clear draft", "Clear draft")).clicked() {
                    self.composer.clear();
                }
                if ui.button(self.tr("Models", "Models")).clicked() {
                    self.active_tab = Tab::Models;
                }
            });
        });
    }

    fn render_inspector_v2(&mut self, ui: &mut egui::Ui) {
        section_card(ui, self.theme, self.tr("Connection", "Connection"), |ui| {
            ui.label(format!(
                "{}: {}",
                self.tr("Profile", "Profile"),
                self.active_profile_name()
                    .unwrap_or_else(|| self.tr("(none)", "(none)").to_string())
            ));
            ui.label(format!(
                "{}: {}",
                self.tr("Provider", "Provider"),
                self.active_provider_clean()
            ));
            ui.label(format!(
                "{}: {}",
                self.tr("Model", "Model"),
                self.active_model()
            ));
            ui.label(format!(
                "{}: {}",
                self.tr("Base URL", "Base URL"),
                self.active_base_url_clean()
            ));
        });

        ui.add_space(10.0);

        section_card(
            ui,
            self.theme,
            self.tr("Token Usage", "Token Usage"),
            |ui| {
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn input", "Turn input"),
                    self.latest_turn_usage.input_tokens
                ));
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn output", "Turn output"),
                    self.latest_turn_usage.output_tokens
                ));
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn cache write", "Turn cache write"),
                    self.latest_turn_usage.cache_creation_input_tokens
                ));
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn cache read", "Turn cache read"),
                    self.latest_turn_usage.cache_read_input_tokens
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn total", "Turn total"),
                    token_total(self.latest_turn_usage)
                ));
                ui.label(format!(
                    "{}: {}",
                    self.tr("Session total", "Session total"),
                    token_total(self.cumulative_usage)
                ));
            },
        );

        ui.add_space(10.0);

        section_card(
            ui,
            self.theme,
            self.tr("Cost Estimate", "Cost Estimate"),
            |ui| {
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn cost", "Turn cost"),
                    self.session_cost_label_clean(self.latest_turn_usage)
                ));
                ui.label(format!(
                    "{}: {}",
                    self.tr("Session cost", "Session cost"),
                    self.session_cost_label_clean(self.cumulative_usage)
                ));
            },
        );

        ui.add_space(10.0);

        section_card(
            ui,
            self.theme,
            self.tr("Tool Events", "Tool Events"),
            |ui| {
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.inspector_events.is_empty() {
                            ui.label(self.tr(
                        "",
                        "No tool events yet. Calls like read/edit/bash/search will appear here.",
                    ));
                        } else {
                            for event in &self.inspector_events {
                                let color = if event.is_error {
                                    Color32::from_rgb(179, 60, 60)
                                } else {
                                    self.theme.accent()
                                };
                                Frame::group(ui.style())
                                    .fill(self.theme.subpanel_fill())
                                    .stroke(Stroke::new(1.0, color.gamma_multiply(0.35)))
                                    .show(ui, |ui| {
                                        ui.label(RichText::new(&event.title).strong().color(color));
                                        ui.label(&event.body);
                                    });
                                ui.add_space(6.0);
                            }
                        }
                    });
            },
        );
    }
}

#[allow(dead_code)]
impl ClawGuiApp {
    fn render_models_tab_v2(&mut self, ctx: &egui::Context) {
        let profiles = self
            .llm_store
            .as_ref()
            .map(LlmProfileStore::list_profiles)
            .unwrap_or_default();
        let env_label = self.tr("Env var", "Env var").to_string();
        let inline_label = self.tr("Inline key", "Inline key").to_string();
        let limits_summary = self
            .llm_store
            .as_ref()
            .map_or_else(TurnTokenLimits::default, LlmProfileStore::turn_token_limits)
            .summary_line();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |columns| {
                section_card(&mut columns[0], self.theme, self.tr("LLM Profile Editor", "LLM Profile Editor"), |ui| {
                    ui.label(self.tr("Name", "Name"));
                    ui.text_edit_singleline(&mut self.llm_form.name);
                    ui.label(self.tr("Provider", "Provider"));
                    ui.text_edit_singleline(&mut self.llm_form.provider);
                    ui.label(self.tr("Model", "Model"));
                    ui.text_edit_singleline(&mut self.llm_form.model);
                    ui.label(self.tr("Base URL", "Base URL"));
                    ui.text_edit_singleline(&mut self.llm_form.base_url);
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.llm_key_from_env, true, env_label.as_str());
                        ui.selectable_value(&mut self.llm_key_from_env, false, inline_label.as_str());
                    });

                    if self.llm_key_from_env {
                        ui.label(self.tr("API key environment variable", "API key environment variable"));
                        ui.text_edit_singleline(&mut self.llm_form.api_key_env);
                    } else {
                        ui.label(self.tr("API key", "API key"));
                        ui.add(TextEdit::singleline(&mut self.llm_form.api_key).password(true));
                    }

                    ui.horizontal(|ui| {
                        if ui.button(self.tr("Save", "Save")).clicked() {
                            self.save_profile_action();
                        }
                        if ui.button(self.tr("Save + Activate", "Save + Activate")).clicked() {
                            let profile_name = self.llm_form.name.trim().to_string();
                            if self.save_profile_action() {
                                self.switch_profile_action(Some(&profile_name));
                            }
                        }
                        if ui.button(self.tr("Clear form", "Clear form")).clicked() {
                            self.llm_form = LlmForm::default();
                            self.llm_key_from_env = false;
                        }
                    });
                });

                section_card(&mut columns[1], self.theme, self.tr("Saved Profiles", "Saved Profiles"), |ui| {
                    ui.label(format!("{}: {}", self.tr("Current limits", "Current limits"), limits_summary));
                    ui.add_space(4.0);
                    if profiles.is_empty() {
                        ui.label(self.tr(
                            "",
                            "No profiles yet. Save a deepseek / qwen / kimi / glm / openai-compatible profile first.",
                        ));
                    } else {
                        for profile in profiles {
                            let active = self.active_profile_name().as_deref() == Some(profile.name.as_str());
                            Frame::group(ui.style())
                                .fill(self.theme.subpanel_fill())
                                .stroke(Stroke::new(1.0, self.theme.accent().gamma_multiply(0.30)))
                                .show(ui, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(RichText::new(&profile.name).strong().color(if active { self.theme.accent() } else { ui.visuals().text_color() }));
                                        if active {
                                            ui.label(self.tr("Active", "Active"));
                                        }
                                    });
                                    ui.small(format!("provider={} model={}", profile.normalized_provider(), profile.model));
                                    if let Some(base_url) = &profile.base_url {
                                        if !base_url.trim().is_empty() {
                                            ui.small(base_url);
                                        }
                                    }
                                    ui.small(profile.key_source_label());
                                    ui.horizontal(|ui| {
                                        if ui.button(self.tr("Edit", "Edit")).clicked() {
                                            self.load_profile_into_form(&profile);
                                        }
                                        if ui.button(self.tr("Activate", "Activate")).clicked() {
                                            self.switch_profile_action(Some(&profile.name));
                                        }
                                        if ui.button(self.tr("Remove", "Remove")).clicked() {
                                            self.remove_profile(&profile.name);
                                        }
                                    });
                                });
                            ui.add_space(6.0);
                        }
                    }
                });
            });

            ui.add_space(12.0);
            section_card(ui, self.theme, self.tr("Per-turn Token Limits", "Per-turn Token Limits"), |ui| {
                ui.columns(4, |columns| {
                    columns[0].label(self.tr("Min input", "Min input"));
                    columns[0].text_edit_singleline(&mut self.limits_form.min_input);
                    columns[1].label(self.tr("Max input", "Max input"));
                    columns[1].text_edit_singleline(&mut self.limits_form.max_input);
                    columns[2].label(self.tr("Min output", "Min output"));
                    columns[2].text_edit_singleline(&mut self.limits_form.min_output);
                    columns[3].label(self.tr("Max output", "Max output"));
                    columns[3].text_edit_singleline(&mut self.limits_form.max_output);
                });
                ui.horizontal(|ui| {
                    if ui.button(self.tr("Save limits", "Save limits")).clicked() {
                        self.save_limits_action();
                    }
                    if ui.button(self.tr("Clear limits", "Clear limits")).clicked() {
                        self.limits_form = LimitsForm::default();
                    }
                });
            });
        });
    }

    fn render_apps_tab_v2(&mut self, ctx: &egui::Context) {
        let apps = self
            .agent_store
            .as_ref()
            .map(AgentWorkspaceStore::list_apps)
            .unwrap_or_default();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |columns| {
                section_card(
                    &mut columns[0],
                    self.theme,
                    self.tr("Save App Command", "Save App Command"),
                    |ui| {
                        ui.label(self.tr("Name", "Name"));
                        ui.text_edit_singleline(&mut self.app_form.name);
                        ui.label(self.tr("Command", "Command"));
                        ui.text_edit_singleline(&mut self.app_form.command);
                        ui.label(self.tr("Description", "Description"));
                        ui.add(TextEdit::multiline(&mut self.app_form.description).desired_rows(4));
                        ui.horizontal(|ui| {
                            if ui.button(self.tr("Save app", "Save app")).clicked() {
                                self.save_app_action();
                            }
                            if ui.button(self.tr("Clear form", "Clear form")).clicked() {
                                self.app_form = AppForm::default();
                            }
                        });
                    },
                );

                section_card(
                    &mut columns[1],
                    self.theme,
                    self.tr("Saved Apps", "Saved Apps"),
                    |ui| {
                        if apps.is_empty() {
                            ui.label(self.tr("No saved apps yet.", "No saved apps yet."));
                        } else {
                            for app in apps {
                                Frame::group(ui.style())
                                    .fill(self.theme.subpanel_fill())
                                    .stroke(Stroke::new(
                                        1.0,
                                        self.theme.accent().gamma_multiply(0.30),
                                    ))
                                    .show(ui, |ui| {
                                        ui.label(RichText::new(&app.name).strong());
                                        ui.small(&app.description);
                                        ui.monospace(&app.command);
                                        ui.horizontal(|ui| {
                                            if ui
                                                .button(self.tr("Load into form", "Load into form"))
                                                .clicked()
                                            {
                                                self.app_form = AppForm {
                                                    name: app.name.clone(),
                                                    command: app.command.clone(),
                                                    description: app.description.clone(),
                                                };
                                            }
                                            if ui.button(self.tr("Remove", "Remove")).clicked() {
                                                self.remove_app(&app.name);
                                            }
                                        });
                                    });
                                ui.add_space(6.0);
                            }
                        }
                    },
                );
            });
        });
    }

    fn render_sessions_tab_v2(&mut self, ctx: &egui::Context) {
        let sessions = self.sessions.clone();
        egui::CentralPanel::default().show(ctx, |ui| {
            section_card(
                ui,
                self.theme,
                self.tr("Workspace Sessions", "Workspace Sessions"),
                |ui| {
                    ui.horizontal(|ui| {
                        if ui.button(self.tr("Refresh", "Refresh")).clicked() {
                            self.refresh_session_list();
                        }
                        ui.label(format!("{}: {}", self.tr("Total", "Total"), sessions.len()));
                    });

                    if sessions.is_empty() {
                        ui.label(self.tr("", "No saved sessions in this workspace yet."));
                        return;
                    }

                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for summary in sessions {
                                let active = self.active_session_path == summary.path;
                                Frame::group(ui.style())
                                    .fill(self.theme.subpanel_fill())
                                    .stroke(Stroke::new(
                                        1.0,
                                        self.theme.accent().gamma_multiply(0.30),
                                    ))
                                    .show(ui, |ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.label(RichText::new(&summary.id).strong().color(
                                                if active {
                                                    self.theme.accent()
                                                } else {
                                                    ui.visuals().text_color()
                                                },
                                            ));
                                            if active {
                                                ui.label(
                                                    self.tr("Current session", "Current session"),
                                                );
                                            }
                                        });
                                        ui.small(format!(
                                            "{}: {}",
                                            self.tr("Messages", "Messages"),
                                            summary.message_count
                                        ));
                                        if let Some(parent) = &summary.parent_session_id {
                                            ui.small(format!(
                                                "{}: {}",
                                                self.tr("Parent", "Parent"),
                                                parent
                                            ));
                                        }
                                        if let Some(branch) = &summary.branch_name {
                                            ui.small(format!(
                                                "{}: {}",
                                                self.tr("Branch", "Branch"),
                                                branch
                                            ));
                                        }
                                        ui.small(summary.path.display().to_string());
                                        ui.horizontal(|ui| {
                                            if ui
                                                .button(self.tr("Load into chat", "Load into chat"))
                                                .clicked()
                                            {
                                                self.load_session_summary_action(&summary);
                                            }
                                        });
                                    });
                                ui.add_space(8.0);
                            }
                        });
                },
            );
        });
    }
}

impl ClawGuiApp {
    fn render_help_window_v3(&mut self, ctx: &egui::Context) {
        claw_gui_chrome::render_help_window_v3(self, ctx);
    }

    #[allow(dead_code)]
    fn render_help_window_v3_legacy(&mut self, ctx: &egui::Context) {
        if !self.show_help {
            return;
        }

        let title = self.tr("Help & Notes", "Help & Notes").to_string();
        let mut open = self.show_help;
        egui::Window::new(title)
            .open(&mut open)
            .default_width(620.0)
            .show(ctx, |ui| {
                ui.label(self.tr(
                    "",
                    "This desktop GUI is built for multi-model coding agents. Use the left side for threads and context, the center for chat, and the right side for tool events, tokens, and cost.",
                ));
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                ui.label(
                    RichText::new(self.tr("Recommended flow", "Recommended flow"))
                        .strong()
                        .color(self.theme.accent()),
                );
                ui.label(self.tr(
                    "",
                    "1. Start in Models and save an LLM profile, then activate it.",
                ));
                ui.label(self.tr(
                    "",
                    "2. Create threads on the left so different folders keep separate context.",
                ));
                ui.label(self.tr(
                    "",
                    "3. Keep chatting in the center panel; press Ctrl+Enter to send.",
                ));
                ui.label(self.tr(
                    "",
                    "4. The right inspector shows connection info, token usage, RMB or USD cost, and tool events.",
                ));
                ui.label(self.tr(
                    "",
                    "5. Use Apps for reusable command notes and Sessions for loading saved workspace sessions.",
                ));
            });
        self.show_help = open;
    }
}

impl ClawGuiApp {
    #[allow(dead_code)]
    fn render_inspector_v3(&mut self, ui: &mut egui::Ui) {
        claw_gui_chat::render_inspector_v3(self, ui);
    }

    #[allow(dead_code)]
    fn render_inspector_v3_legacy(&mut self, ui: &mut egui::Ui) {
        section_card(ui, self.theme, self.tr("Status", "Status"), |ui| {
            ui.label(format!(
                "{}: {}",
                self.tr("Thread", "Thread"),
                self.active_chat_title_clean()
            ));
            ui.label(format!(
                "{}: {}",
                self.tr("Session", "Session"),
                self.active_session.session_id
            ));
            ui.label(format!(
                "{}: {}",
                self.tr("Messages", "Messages"),
                self.active_session.messages.len()
            ));
            ui.label(format!(
                "{}: {}",
                self.tr("Run state", "Run state"),
                self.tr(
                    if self.busy { "Running" } else { "Idle" },
                    if self.busy { "Running" } else { "Idle" },
                )
            ));
        });

        ui.add_space(10.0);

        section_card(ui, self.theme, self.tr("Connection", "Connection"), |ui| {
            ui.label(format!(
                "{}: {}",
                self.tr("Profile", "Profile"),
                self.active_profile_name()
                    .unwrap_or_else(|| self.tr("(none)", "(none)").to_string())
            ));
            ui.label(format!(
                "{}: {}",
                self.tr("Provider", "Provider"),
                self.active_provider_clean()
            ));
            ui.label(format!(
                "{}: {}",
                self.tr("Model", "Model"),
                self.active_model()
            ));
            ui.label(format!(
                "{}: {}",
                self.tr("Base URL", "Base URL"),
                self.active_base_url_clean()
            ));
        });

        ui.add_space(10.0);

        section_card(
            ui,
            self.theme,
            self.tr("Token Usage", "Token Usage"),
            |ui| {
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn input", "Turn input"),
                    self.latest_turn_usage.input_tokens
                ));
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn output", "Turn output"),
                    self.latest_turn_usage.output_tokens
                ));
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn cache write", "Turn cache write"),
                    self.latest_turn_usage.cache_creation_input_tokens
                ));
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn cache read", "Turn cache read"),
                    self.latest_turn_usage.cache_read_input_tokens
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn total", "Turn total"),
                    token_total(self.latest_turn_usage)
                ));
                ui.label(format!(
                    "{}: {}",
                    self.tr("Session total", "Session total"),
                    token_total(self.cumulative_usage)
                ));
            },
        );

        ui.add_space(10.0);

        section_card(
            ui,
            self.theme,
            self.tr("Cost Estimate", "Cost Estimate"),
            |ui| {
                ui.label(format!(
                    "{}: {}",
                    self.tr("Turn cost", "Turn cost"),
                    self.session_cost_label_clean(self.latest_turn_usage)
                ));
                ui.label(format!(
                    "{}: {}",
                    self.tr("Session cost", "Session cost"),
                    self.session_cost_label_clean(self.cumulative_usage)
                ));
                ui.small(self.tr(
                "",
                "Chinese-provider models prefer RMB official pricing. Other models use the configured or default price table.",
            ));
            },
        );

        ui.add_space(10.0);

        section_card(
            ui,
            self.theme,
            self.tr("Tool Events", "Tool Events"),
            |ui| {
                ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.inspector_events.is_empty() {
                        ui.label(self.tr(
                            "",
                            "No tool events yet. Calls like read / edit / bash / search will appear here.",
                        ));
                    } else {
                        for (event_index, event) in self.inspector_events.iter().enumerate() {
                            ui.push_id(("inspector_event_card", event_index), |ui| {
                                render_tool_event_card_v1(ui, event, self.language, self.theme);
                            });
                            ui.add_space(6.0);
                        }
                    }
                });
            },
        );
    }
}

impl ClawGuiApp {
    fn render_apps_tab_v3(&mut self, ctx: &egui::Context) {
        claw_gui_pages::render_apps_tab_v3(self, ctx);
    }

    fn render_sessions_tab_v3(&mut self, ctx: &egui::Context) {
        claw_gui_pages::render_sessions_tab_v3(self, ctx);
    }
}

impl ClawGuiApp {
    fn render_thread_window_v4(&mut self, ctx: &egui::Context) {
        claw_gui_pages::render_thread_window_v4(self, ctx);
    }

    fn render_model_quick_window_v1(&mut self, ctx: &egui::Context) {
        claw_gui_pages::render_model_quick_window_v1(self, ctx);
    }

    fn render_chat_tab_v4(&mut self, ctx: &egui::Context) {
        claw_gui_chat::render_chat_tab_v4(self, ctx);
    }

    #[allow(dead_code)]
    fn render_thread_sidebar_v4(&mut self, ui: &mut egui::Ui) {
        claw_gui_chat::render_thread_sidebar_v4(self, ui);
    }

    #[allow(dead_code)]
    fn render_chat_stream_v6(&mut self, ui: &mut egui::Ui) {
        claw_gui_chat::render_chat_stream_v6(self, ui);
    }

    #[allow(dead_code)]
    fn render_thread_sidebar_v4_legacy(&mut self, ui: &mut egui::Ui) {
        let threads = self
            .agent_store
            .as_ref()
            .map(AgentWorkspaceStore::list_threads)
            .unwrap_or_default();

        let mut grouped = BTreeMap::<String, Vec<ThreadRecord>>::new();
        for thread in threads {
            grouped
                .entry(thread.folder.clone())
                .or_default()
                .push(thread);
        }

        let mut activate_name = None::<String>;
        let mut remove_name = None::<String>;
        let mut import_folder = false;
        let mut create_in_folder = None::<String>;
        let mut clear_sessions_in_folder = None::<String>;

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            section_card(ui, self.theme, self.tr("Threads", "Threads"), |ui| {
                ui.horizontal(|ui| {
                    if ui.button(self.tr("Import Folder", "Import Folder")).clicked() {
                        import_folder = true;
                    }
                    if ui.button(self.tr("New Thread", "New Thread")).clicked() {
                        self.show_thread_form = true;
                    }
                });

                ui.add_space(6.0);
                if ui
                    .selectable_label(
                        self.active_thread_name.is_none(),
                        self.tr("Current workspace chat", "Current workspace chat"),
                    )
                    .clicked()
                {
                    self.select_workspace_chat();
                }
                ui.small(self.workspace.display().to_string());
            });

            ui.add_space(10.0);

            if grouped.is_empty() {
                section_card(ui, self.theme, self.tr("Folders", "Folders"), |ui| {
                    ui.label(self.tr(
                        "",
                        "No folders imported yet. Click Import Folder to create a thread group for that folder.",
                    ));
                });
            } else {
                for (folder, mut folder_threads) in grouped {
                    folder_threads.sort_by(|a, b| a.name.cmp(&b.name));
                    let folder_name = Path::new(&folder)
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or(folder.as_str())
                        .to_string();
                    section_card(ui, self.theme, &folder_name, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.small(folder.clone());
                            if ui
                                .small_button(self.tr("New thread here", "New thread here"))
                                .clicked()
                            {
                                create_in_folder = Some(folder.clone());
                            }
                            let confirm_clear =
                                self.confirm_folder_session_cleanup.as_deref() == Some(folder.as_str());
                            let clear_label = if confirm_clear {
                                self.tr("确认清理", "Confirm clear")
                            } else {
                                self.tr("清理会话", "Clear sessions")
                            };
                            if ui
                                .add_enabled(!self.busy, egui::Button::new(clear_label))
                                .clicked()
                            {
                                if confirm_clear {
                                    clear_sessions_in_folder = Some(folder.clone());
                                } else {
                                    self.confirm_folder_session_cleanup = Some(folder.clone());
                                }
                            }
                        });
                        ui.add_space(4.0);
                        for thread in folder_threads {
                            let selected =
                                self.active_thread_name.as_deref() == Some(thread.name.as_str());
                            Frame::group(ui.style())
                                .fill(self.theme.subpanel_fill())
                                .stroke(Stroke::new(
                                    1.0,
                                    self.theme.accent().gamma_multiply(if selected { 0.55 } else { 0.25 }),
                                ))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        if ui.selectable_label(selected, &thread.name).clicked() {
                                            activate_name = Some(thread.name.clone());
                                        }
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            if ui.small_button(self.tr("Remove", "Remove")).clicked() {
                                                remove_name = Some(thread.name.clone());
                                            }
                                        });
                                    });
                                    if let Some(description) = &thread.description {
                                        if !description.trim().is_empty() {
                                            ui.small(description);
                                        }
                                    }
                                });
                            ui.add_space(6.0);
                        }
                    });
                    ui.add_space(10.0);
                }
            }
        });

        if import_folder {
            self.import_folder_picker();
        }
        if let Some(name) = activate_name {
            self.activate_thread(&name);
        }
        if let Some(name) = remove_name {
            self.remove_thread(&name);
        }
        if let Some(folder) = clear_sessions_in_folder {
            self.clear_folder_sessions_action(&folder);
        }
        if let Some(folder) = create_in_folder {
            // 在侧边栏直接创建“文件夹 -> 线程”映射，减少切换上下文时的手动输入。
            let default_name = Path::new(&folder)
                .file_name()
                .and_then(|value| value.to_str())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("thread")
                .to_string();
            self.thread_form.folder = folder;
            self.thread_form.name = default_name;
            self.thread_form.description.clear();
            self.show_thread_form = true;
        }
    }

    #[allow(dead_code, unused_variables)]
    fn render_chat_stream_v6_legacy(&mut self, ui: &mut egui::Ui) {
        let messages = self.active_session.messages.clone();
        let estimated_input = estimate_text_tokens(&self.composer);
        let limits = self
            .llm_store
            .as_ref()
            .map_or_else(TurnTokenLimits::default, LlmProfileStore::turn_token_limits);
        let profiles = self
            .llm_store
            .as_ref()
            .map(LlmProfileStore::list_profiles)
            .unwrap_or_default();
        let composer_hint = self
            .tr(
                "",
                "Write a task, question, or edit request. Multi-turn context stays in the current thread.",
            )
            .to_string();

        section_card(ui, self.theme, &self.active_chat_title_clean(), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "{}: {}",
                    self.tr("Folder", "Folder"),
                    self.active_chat_workspace().display()
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Session", "Session"),
                    self.active_session.session_id
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Messages", "Messages"),
                    messages.len()
                ));
            });

            let current_profile = self
                .active_profile_name()
                .unwrap_or_else(|| self.tr("(none)", "(none)").to_string());
            let mut selected_profile = self.active_profile_name().unwrap_or_default();

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "{}: {}",
                    self.tr("Active profile", "Active profile"),
                    current_profile
                ));
                ComboBox::from_id_salt("chat_profile_switch_v6")
                    .selected_text(if selected_profile.is_empty() {
                        self.tr("(none)", "(none)")
                    } else {
                        selected_profile.as_str()
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut selected_profile,
                            String::new(),
                            self.tr("(none)", "(none)"),
                        );
                        for profile in &profiles {
                            ui.selectable_value(
                                &mut selected_profile,
                                profile.name.clone(),
                                &profile.name,
                            );
                        }
                    });
                if ui.button(self.tr("New Model", "New Model")).clicked() {
                    if self.llm_form.provider.trim().is_empty() {
                        self.llm_form.provider = "deepseek".to_string();
                    }
                    if self.llm_form.model.trim().is_empty() {
                        self.llm_form.model = "deepseek-chat".to_string();
                    }
                    self.show_model_quick_form = true;
                }
                if ui.button(self.tr("Add files", "Add files")).clicked() {
                    self.add_files_picker();
                }
                if ui
                    .add_enabled(!self.busy, egui::Button::new(self.tr("Send", "Send")))
                    .clicked()
                {
                    self.send_current_prompt();
                }
                if ui
                    .add_enabled(
                        self.busy,
                        egui::Button::new(self.tr(
                            if self.pause_requested {
                                "强制暂停"
                            } else {
                                "暂停"
                            },
                            if self.pause_requested {
                                "Force pause"
                            } else {
                                "Pause"
                            },
                        )),
                    )
                    .clicked()
                {
                    self.pause_generation_action();
                }
                if ui.button(self.tr("Models", "Models")).clicked() {
                    self.active_tab = Tab::Models;
                }
            });

            match self.active_profile_name() {
                Some(active) if active == selected_profile => {}
                Some(_) | None => {
                    if selected_profile.trim().is_empty() {
                        self.switch_profile_action(None);
                    } else {
                        self.switch_profile_action(Some(&selected_profile));
                    }
                }
            }
        });

        if let Some(hint) = self.connection_error_hint() {
            ui.add_space(8.0);
            section_card(
                ui,
                self.theme,
                self.tr("Connection hint", "Connection hint"),
                |ui| {
                    if let Some(error) = &self.error {
                        ui.colored_label(Color32::from_rgb(180, 48, 48), error);
                    }
                    ui.small(hint);
                },
            );
        }
        ui.add_space(8.0);
        let line_count = self.composer.lines().count().max(1);
        let growth_steps = u16::try_from(line_count.saturating_sub(4)).unwrap_or(u16::MAX);
        let auto_growth = (f32::from(growth_steps) * 12.0).min(120.0);
        let composer_height = (120.0 + auto_growth).clamp(120.0, 260.0);
        let composer_rows = line_count.clamp(4, 16);
        let available_height = ui.available_height();
        let conversation_height = (available_height - composer_height - 8.0).max(0.0);
        ui.with_layout(
            Layout::top_down(Align::Min),
            |ui| {
                section_card(ui, self.theme, self.tr("Conversation", "Conversation"), |ui| {
                    ui.set_min_height((conversation_height - 16.0).max(0.0));
                    ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if messages.is_empty()
                                && self.optimistic_user_prompt.is_none()
                                && self.live_reply.is_empty()
                            {
                                ui.add_space(24.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        RichText::new(self.tr("Start a new multi-turn chat", "Start a new multi-turn chat"))
                                            .strong()
                                            .size(18.0),
                                    );
                                    ui.label(self.tr(
                                        "",
                                        "Import folders on the left, create threads, then switch models, attach files, and send from the chat area.",
                                    ));
                                });
                                ui.add_space(24.0);
                            }

                            for (message_index, message) in messages
                                .iter()
                                .filter(|message| message.role != MessageRole::System)
                                .enumerate()
                            {
                                ui.push_id(("chat_message_card", message_index), |ui| {
                                    render_message_card_v3(ui, message, self.language, self.theme);
                                });
                                ui.add_space(8.0);
                            }

                            if let Some(prompt) = &self.optimistic_user_prompt {
                                let pending = ConversationMessage {
                                    role: MessageRole::User,
                                    blocks: vec![ContentBlock::Text { text: prompt.clone() }],
                                    usage: None,
                                };
                                render_message_card_v3(ui, &pending, self.language, self.theme);
                                ui.add_space(8.0);
                            }

                            if !self.live_reply.is_empty() {
                                let streaming = ConversationMessage {
                                    role: MessageRole::Assistant,
                                    blocks: vec![ContentBlock::Text {
                                        text: self.live_reply.clone(),
                                    }],
                                    usage: None,
                                };
                                render_message_card_v3(ui, &streaming, self.language, self.theme);
                                ui.add_space(8.0);
                            }

                            if !self.inspector_events.is_empty() {
                                for (event_index, event) in
                                    self.inspector_events.iter().rev().take(8).rev().enumerate()
                                {
                                    ui.push_id(("chat_tail_inspector_event", event_index), |ui| {
                                        render_tool_event_card_v1(
                                            ui,
                                            event,
                                            self.language,
                                            self.theme,
                                        );
                                    });
                                    ui.add_space(6.0);
                                }
                            }
                        });
                });
            },
        );

        ui.add_space(8.0);
        section_card(ui, self.theme, self.tr("Composer", "Composer"), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "{}: {}",
                    self.tr("Estimated input tokens", "Estimated input tokens"),
                    estimated_input
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Limits", "Limits"),
                    limits.summary_line()
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Profile", "Profile"),
                    self.active_profile_name()
                        .unwrap_or_else(|| self.tr("(none)", "(none)").to_string())
                ));
                ui.separator();
                ui.label(self.tr(
                    "Shortcut: Ctrl+Enter to send",
                    "Shortcut: Ctrl+Enter to send",
                ));
            });
            ui.small(self.tr("输入区会随内容自动增高", "Composer auto-grows with content"));

            ui.horizontal_wrapped(|ui| {
                if ui.button(self.tr("Add files", "Add files")).clicked() {
                    self.add_files_picker();
                }
                if ui
                    .add_enabled(!self.busy, egui::Button::new(self.tr("Send", "Send")))
                    .clicked()
                {
                    self.send_current_prompt();
                }
                if ui
                    .add_enabled(
                        self.busy,
                        egui::Button::new(self.tr(
                            if self.pause_requested {
                                "强制暂停"
                            } else {
                                "暂停"
                            },
                            if self.pause_requested {
                                "Force pause"
                            } else {
                                "Pause"
                            },
                        )),
                    )
                    .clicked()
                {
                    self.pause_generation_action();
                }
                let edit_last_available = self.optimistic_user_prompt.is_some()
                    || self.active_session.prompt_history.last().is_some();
                if ui
                    .add_enabled(
                        !self.busy && edit_last_available,
                        egui::Button::new(self.tr("编辑上一条", "Edit last")),
                    )
                    .clicked()
                {
                    self.edit_last_prompt_action();
                }
                if self.pause_requested {
                    ui.small(self.tr("正在暂停...", "Pausing..."));
                    ui.small(self.tr(
                        "如果仍未停止，再点一次“强制暂停”会直接切到恢复会话。",
                        "If it still does not stop, click Force pause again to switch to a recovery session.",
                    ));
                }
                if ui.button(self.tr("New Model", "New Model")).clicked() {
                    self.show_model_quick_form = true;
                }
            });

            if !self.attached_files.is_empty() {
                ui.add_space(6.0);
                ui.label(self.tr("Attached files", "Attached files"));
                let attached = self.attached_files.clone();
                ui.horizontal_wrapped(|ui| {
                    for path in attached {
                        Frame::group(ui.style())
                            .fill(self.theme.subpanel_fill())
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let label = path
                                        .file_name()
                                        .and_then(|value| value.to_str())
                                        .map(ToOwned::to_owned)
                                        .unwrap_or_else(|| path.to_string_lossy().to_string());
                                    ui.small(label);
                                    if ui.small_button("x").clicked() {
                                        self.remove_attached_file(&path);
                                    }
                                });
                            });
                    }
                });
            }

            ui.add_enabled_ui(!self.busy, |ui| {
                ui.add_sized(
                    [ui.available_width(), composer_height],
                    TextEdit::multiline(&mut self.composer)
                        .desired_rows(composer_rows)
                        .hint_text(composer_hint),
                );
            });
        });
    }

    fn render_models_tab_v5(&mut self, ctx: &egui::Context) {
        claw_gui_pages::render_models_tab_v5(self, ctx);
    }
}

impl ClawGuiApp {
    fn current_thread_label_v2(&self) -> String {
        self.active_thread_name
            .clone()
            .unwrap_or_else(|| self.tr("Workspace Chat", "Workspace Chat").to_string())
    }

    fn current_profile_label_v2(&self) -> String {
        self.active_profile_name()
            .unwrap_or_else(|| self.tr("(none)", "(none)").to_string())
    }

    fn current_provider_label_v2(&self) -> String {
        self.active_profile_ref()
            .map(LlmProfile::normalized_provider)
            .unwrap_or_else(|| self.tr("environment", "environment").to_string())
    }

    fn current_base_url_label_v2(&self) -> String {
        self.active_profile_ref()
            .and_then(|profile| profile.base_url.clone())
            .unwrap_or_else(|| self.tr("provider default", "provider default").to_string())
    }

    fn render_top_bar_v4(&mut self, ctx: &egui::Context) {
        claw_gui_chrome::render_top_bar_v4(self, ctx);
    }

    #[allow(dead_code)]
    fn render_top_bar_v4_legacy(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("claw_gui_top_bar_v4").show(ctx, |ui| {
            let sand_label = self.tr("Sand", "Sand").to_string();
            let mist_label = self.tr("Mist", "Mist").to_string();
            let forest_label = self.tr("Forest", "Forest").to_string();
            let graphite_label = self.tr("Graphite", "Graphite").to_string();
            let zh_label = self.tr("Chinese", "Chinese").to_string();
            let en_label = "English".to_string();

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Claw Client")
                        .strong()
                        .size(22.0)
                        .color(self.theme.accent()),
                );
                ui.label(
                    RichText::new(self.tr(
                        "一个受 Codex Desktop 启发的多模型工作区",
                        "A multi-model workspace inspired by Codex Desktop",
                    ))
                    .small(),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(self.tr("Help", "Help")).clicked() {
                        self.show_help = true;
                    }

                    ComboBox::from_id_salt("gui_theme_v4")
                        .selected_text(match self.theme {
                            UiTheme::Sand => sand_label.as_str(),
                            UiTheme::Mist => mist_label.as_str(),
                            UiTheme::Forest => forest_label.as_str(),
                            UiTheme::Graphite => graphite_label.as_str(),
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.theme,
                                UiTheme::Sand,
                                sand_label.as_str(),
                            );
                            ui.selectable_value(
                                &mut self.theme,
                                UiTheme::Mist,
                                mist_label.as_str(),
                            );
                            ui.selectable_value(
                                &mut self.theme,
                                UiTheme::Forest,
                                forest_label.as_str(),
                            );
                            ui.selectable_value(
                                &mut self.theme,
                                UiTheme::Graphite,
                                graphite_label.as_str(),
                            );
                        });

                    ComboBox::from_id_salt("gui_lang_v4")
                        .selected_text(match self.language {
                            Language::Zh => zh_label.as_str(),
                            Language::En => en_label.as_str(),
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.language,
                                Language::Zh,
                                zh_label.as_str(),
                            );
                            ui.selectable_value(
                                &mut self.language,
                                Language::En,
                                en_label.as_str(),
                            );
                        });
                });
            });

            ui.horizontal_wrapped(|ui| {
                for tab in Tab::all() {
                    let selected = self.active_tab == tab;
                    let label = match tab {
                        Tab::Chat => self.tr("Chat", "Chat"),
                        Tab::Models => self.tr("Models", "Models"),
                        Tab::Apps => self.tr("Apps", "Apps"),
                        Tab::Sessions => self.tr("Sessions", "Sessions"),
                    };
                    if ui.selectable_label(selected, label).clicked() {
                        self.active_tab = tab;
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label(self.tr("Workspace", "Workspace"));
                let input_width = (ui.available_width() - 300.0).max(180.0);
                let response = ui.add_sized(
                    [input_width, 28.0],
                    TextEdit::singleline(&mut self.workspace_input),
                );
                if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    self.reload();
                }
                if ui.button(self.tr("Load", "Load")).clicked() {
                    self.reload();
                }
                if ui
                    .button(self.tr("Workspace Chat", "Workspace Chat"))
                    .clicked()
                {
                    self.select_workspace_chat();
                }
                if ui.button(self.tr("Sessions", "Sessions")).clicked() {
                    self.active_tab = Tab::Sessions;
                }
            });

            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "{}: {}",
                    self.tr("Model", "Model"),
                    self.active_model()
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Profile", "Profile"),
                    self.current_profile_label_v2()
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Provider", "Provider"),
                    self.current_provider_label_v2()
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Base URL", "Base URL"),
                    self.current_base_url_label_v2()
                ));
                ui.separator();
                ui.label(format!(
                    "{}: {}",
                    self.tr("Current thread", "Current thread"),
                    self.current_thread_label_v2()
                ));
            });

            if let Some(error) = &self.error {
                ui.colored_label(Color32::from_rgb(180, 48, 48), error);
            } else if let Some(notice) = &self.notice {
                ui.colored_label(self.theme.accent(), notice);
            }

            ui.add_space(4.0);
        });
    }
}

impl eframe::App for ClawGuiApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        self.apply_visuals(ctx);
        self.poll_worker_clean();
        if self.busy {
            ctx.request_repaint_after(Duration::from_millis(120));
        }

        let send_shortcut = ctx.input(|input| {
            input.key_pressed(egui::Key::Enter) && (input.modifiers.command || input.modifiers.ctrl)
        });
        if self.active_tab == Tab::Chat && send_shortcut {
            self.send_current_prompt();
        }

        self.render_top_bar_v4(ctx);
        self.render_help_window_v3(ctx);
        self.render_thread_window_v4(ctx);
        self.render_model_quick_window_v1(ctx);

        match self.active_tab {
            Tab::Chat => self.render_chat_tab_v4(ctx),
            Tab::Models => self.render_models_tab_v5(ctx),
            Tab::Apps => self.render_apps_tab_v3(ctx),
            Tab::Sessions => self.render_sessions_tab_v3(ctx),
        }
    }
}

fn section_card(
    ui: &mut egui::Ui,
    theme: UiTheme,
    title: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    Frame::group(ui.style())
        .fill(theme.subpanel_fill())
        .stroke(Stroke::new(1.0, theme.accent().gamma_multiply(0.30)))
        .show(ui, |ui| {
            ui.label(RichText::new(title).strong().color(theme.accent()));
            ui.add_space(6.0);
            add_contents(ui);
        });
}

fn render_message_card_v3(
    ui: &mut egui::Ui,
    message: &ConversationMessage,
    language: Language,
    theme: UiTheme,
) {
    let (title, accent) = match message.role {
        MessageRole::User => (language.pick("User", "User"), theme.accent()),
        MessageRole::Assistant => (
            language.pick("Assistant", "Assistant"),
            Color32::from_rgb(79, 103, 188),
        ),
        MessageRole::Tool => (
            language.pick("Tool", "Tool"),
            Color32::from_rgb(95, 130, 88),
        ),
        MessageRole::System => (
            language.pick("System", "System"),
            Color32::from_rgb(120, 120, 120),
        ),
    };

    let fill = match message.role {
        MessageRole::User => theme.accent().gamma_multiply(0.08),
        MessageRole::Assistant | MessageRole::System => theme.subpanel_fill(),
        MessageRole::Tool => Color32::from_rgb(230, 241, 230).gamma_multiply(0.95),
    };

    Frame::group(ui.style())
        .fill(fill)
        .stroke(Stroke::new(1.0, accent.gamma_multiply(0.35)))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(title).strong().color(accent));
                if let Some(usage) = message.usage {
                    ui.separator();
                    ui.small(format!("Token {}", token_total(usage)));
                }
            });

            for (block_index, block) in message.blocks.iter().enumerate() {
                ui.push_id(("message_block_v3", block_index), |ui| match block {
                    ContentBlock::Text { text } => {
                        render_message_text_v3(ui, text, language, theme);
                    }
                    ContentBlock::ToolUse { name, input, .. } => {
                        render_tool_block_v3(ui, name, input, false, false, language, theme);
                    }
                    ContentBlock::ToolResult {
                        tool_name,
                        output,
                        is_error,
                        ..
                    } => {
                        render_tool_block_v3(
                            ui, tool_name, output, true, *is_error, language, theme,
                        );
                    }
                });
                ui.add_space(4.0);
            }
        });
}

fn render_tool_block_v3(
    ui: &mut egui::Ui,
    name: &str,
    body: &str,
    is_result: bool,
    is_error: bool,
    language: Language,
    theme: UiTheme,
) {
    let color = if is_error {
        Color32::from_rgb(183, 63, 63)
    } else {
        theme.accent()
    };
    let kind = if is_result {
        language.pick("Tool result", "Tool result")
    } else {
        language.pick("Tool call", "Tool call")
    };
    let summary = if is_result {
        summarize_tool_result(name, body, is_error, language)
    } else {
        summarize_tool_call(name, body, language)
    };

    Frame::group(ui.style())
        .fill(theme.subpanel_fill())
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.4)))
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!("{kind}: {name}"))
                    .strong()
                    .color(color),
            );
            ui.monospace(summary);
            let details = clamp_inspector_event_body(body);
            if !details.trim().is_empty() {
                let details_id = stable_hash64(&format!(
                    "tool_block:{name}:{is_result}:{is_error}:{details}"
                ));
                let details_label = if is_result {
                    language.pick("查看完整结果", "Show full result")
                } else {
                    language.pick("查看命令详情", "Show command details")
                };
                egui::CollapsingHeader::new(details_label)
                    .id_salt(("tool_block_details", details_id))
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.small_button(language.pick("复制", "Copy")).clicked() {
                                ui.ctx().copy_text(details.clone());
                            }
                        });
                        ui.monospace(&details);
                    });
            }
        });
}
fn render_tool_event_card_v1(
    ui: &mut egui::Ui,
    event: &InspectorEvent,
    language: Language,
    theme: UiTheme,
) {
    let color = if event.is_error {
        Color32::from_rgb(183, 63, 63)
    } else {
        Color32::from_rgb(95, 130, 88)
    };
    Frame::group(ui.style())
        .fill(theme.subpanel_fill())
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.35)))
        .show(ui, |ui| {
            let body = summarize_inspector_event(event, language);
            let details = clamp_inspector_event_body(&event.body);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(language.pick("Tool Event", "Tool Event"))
                        .strong()
                        .color(color),
                );
                ui.separator();
                ui.small(RichText::new(&event.title).color(color));
            });
            ui.monospace(body);
            if !details.trim().is_empty() {
                let details_id = stable_hash64(&format!(
                    "tool_event:{}:{}:{}",
                    event.title, event.is_error, details
                ));
                let details_label = if is_tool_call_event(event) {
                    language.pick("查看命令详情", "Show command details")
                } else if is_tool_result_event(event) {
                    language.pick("查看完整结果", "Show full result")
                } else {
                    language.pick("查看详情", "Show details")
                };
                egui::CollapsingHeader::new(details_label)
                    .id_salt(("tool_event_details", details_id))
                    .default_open(false)
                    .show(ui, |ui| {
                        if ui.small_button(language.pick("复制", "Copy")).clicked() {
                            ui.ctx().copy_text(details.clone());
                        }
                        ui.monospace(&details);
                    });
            }
        });
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum MessageSegment {
    Text(String),
    Code { language: String, code: String },
}

fn render_message_text_v3(ui: &mut egui::Ui, text: &str, language: Language, theme: UiTheme) {
    for segment in split_message_segments(text) {
        match segment {
            MessageSegment::Text(value) => {
                if !value.trim().is_empty() {
                    ui.label(value);
                }
            }
            MessageSegment::Code {
                language: code_language,
                code,
            } => render_code_segment_v3(ui, &code_language, &code, language, theme),
        }
    }
}

// 中文注释：将 markdown 围栏代码块拆分后独立渲染，便于复制和后续扩展 diff 卡片。
fn split_message_segments(text: &str) -> Vec<MessageSegment> {
    let normalized = text.replace("\r\n", "\n");
    let mut segments = Vec::new();
    let mut text_lines = Vec::new();
    let mut code_lines = Vec::new();
    let mut code_language = String::new();
    let mut in_code_block = false;

    for line in normalized.split('\n') {
        if let Some(rest) = line.strip_prefix("```") {
            if in_code_block {
                let code = code_lines.join("\n");
                if !code.trim().is_empty() {
                    segments.push(MessageSegment::Code {
                        language: code_language.trim().to_string(),
                        code,
                    });
                }
                code_lines.clear();
                code_language.clear();
                in_code_block = false;
            } else {
                push_text_segment(&mut segments, &mut text_lines);
                code_language = rest.trim().to_string();
                in_code_block = true;
            }
            continue;
        }

        if in_code_block {
            code_lines.push(line.to_string());
        } else {
            text_lines.push(line.to_string());
        }
    }

    if in_code_block {
        text_lines.push(format!("```{code_language}"));
        text_lines.extend(code_lines);
    }
    push_text_segment(&mut segments, &mut text_lines);

    if segments.is_empty() {
        segments.push(MessageSegment::Text(text.to_string()));
    }
    segments
}

fn push_text_segment(segments: &mut Vec<MessageSegment>, lines: &mut Vec<String>) {
    if lines.is_empty() {
        return;
    }

    let joined = lines.join("\n");
    lines.clear();
    if !joined.trim().is_empty() {
        segments.push(MessageSegment::Text(joined));
    }
}

fn render_code_segment_v3(
    ui: &mut egui::Ui,
    code_language: &str,
    code: &str,
    language: Language,
    theme: UiTheme,
) {
    let label = display_code_language(code_language, language);
    Frame::group(ui.style())
        .fill(theme.subpanel_fill())
        .stroke(Stroke::new(
            1.0,
            Color32::from_rgb(79, 103, 188).gamma_multiply(0.32),
        ))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(label)
                        .strong()
                        .color(Color32::from_rgb(79, 103, 188)),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button(language.pick("澶嶅埗", "Copy")).clicked() {
                        ui.ctx().copy_text(code.to_owned());
                    }
                });
            });
            ui.add_space(4.0);
            ui.monospace(code);
        });
}

fn display_code_language(code_language: &str, language: Language) -> String {
    let normalized = code_language
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim();
    if normalized.is_empty() {
        language.pick("text", "text").to_string()
    } else {
        normalized.to_string()
    }
}

fn summarize_inspector_event(event: &InspectorEvent, language: Language) -> String {
    if let Some(name) = event.title.strip_prefix("Tool: ") {
        return summarize_tool_call(name, &event.body, language);
    }
    if let Some(name) = event.title.strip_prefix("Result: ") {
        return summarize_tool_result(name, &event.body, event.is_error, language);
    }
    truncate_text(&event.body, 260)
}

fn is_tool_call_event(event: &InspectorEvent) -> bool {
    event.title.starts_with("Tool: ")
}

fn is_tool_result_event(event: &InspectorEvent) -> bool {
    event.title.starts_with("Result: ")
        || event.title.starts_with("Timed out: ")
        || event.title.starts_with("Terminal handoff: ")
}

// 中文注释：优先把工具调用压成“工具名 + 关键参数”的短摘要，减少会话区噪音。
// 中文注释：优先把工具调用压成“工具名 + 关键参数”的短摘要，减少会话区噪音。
fn summarize_tool_call(name: &str, body: &str, language: Language) -> String {
    if let Some(value) = parse_tool_body_json(body) {
        if let Some(summary) = summarize_tool_call_json(name, &value, language) {
            return summary;
        }
    }

    format!("{name} {}", truncate_text(body.trim(), 220))
        .trim()
        .to_string()
}

fn summarize_tool_call_json(name: &str, value: &JsonValue, language: Language) -> Option<String> {
    let lower_name = name.to_ascii_lowercase();
    if matches_tool_name(
        &lower_name,
        &["shell", "command", "bash", "powershell", "terminal"],
    ) {
        if let Some(command) = json_string_field(
            value,
            &["command", "cmd", "script", "shell_command", "bash_command"],
        ) {
            let command_len = command.chars().count();
            return Some(
                language
                    .pick("命令内容已折叠", "command hidden in summary")
                    .to_string()
                    + &format!(" ({command_len} chars)"),
            );
        }
    }

    if lower_name.contains("write") {
        if let Some(path) = json_string_field(value, &["path", "filePath", "file"]) {
            return Some(format!("{name} {}", truncate_text(&path, 120)));
        }
    }

    if lower_name.contains("read") || lower_name.contains("open") {
        if let Some(path) = json_string_field(value, &["path", "filePath", "file"]) {
            return Some(format!("{name} {}", truncate_text(&path, 120)));
        }
    }

    if lower_name.contains("search") || lower_name.contains("grep") || lower_name.contains("glob") {
        if let Some(pattern) =
            json_string_field(value, &["pattern", "query", "q", "glob", "keyword"])
        {
            return Some(format!("{name} {}", truncate_text(&pattern, 120)));
        }
    }

    if lower_name.contains("list") || lower_name.contains("ls") {
        if let Some(path) = json_string_field(value, &["path", "directory", "dir"]) {
            return Some(format!("{name} {}", truncate_text(&path, 120)));
        }
    }

    if let Some(path) = json_string_field(value, &["path", "filePath", "file"]) {
        return Some(format!("{name} {}", truncate_text(&path, 120)));
    }
    if let Some(query) = json_string_field(value, &["pattern", "query", "q", "keyword"]) {
        return Some(format!("{name} {}", truncate_text(&query, 120)));
    }

    let fallback = serde_json::to_string(value).ok()?;
    let prefix = language.pick("参数", "args");
    Some(format!("{name} {prefix} {}", truncate_text(&fallback, 160)))
}

fn summarize_tool_result(name: &str, body: &str, is_error: bool, language: Language) -> String {
    if let Some(value) = parse_tool_body_json(body) {
        if let Some(summary) = summarize_tool_result_json(name, &value, is_error, language) {
            return summary;
        }
    }

    let prefix = if is_error {
        language.pick("错误", "error")
    } else {
        language.pick("输出", "output")
    };
    format!("{prefix}: {}", truncate_text(body.trim(), 220))
}

fn summarize_tool_result_json(
    name: &str,
    value: &JsonValue,
    is_error: bool,
    language: Language,
) -> Option<String> {
    if let Some(file_type) = json_string_field(value, &["type"]) {
        if let Some(path) = json_string_field(value, &["filePath", "path", "file"]) {
            return Some(format!(
                "{} {} {}",
                name,
                file_type,
                truncate_text(&path, 120)
            ));
        }
    }

    let mut parts = Vec::new();
    if let Some(status) = json_string_field(value, &["status", "exit_status"]) {
        parts.push(format!("status={status}"));
    }
    if let Some(exit_code) = json_i64_field(value, &["exit_code", "code"]) {
        parts.push(format!("exit={exit_code}"));
    }
    if let Some(stdout) = json_string_field(value, &["stdout", "output", "message"]) {
        let label = if is_error {
            language.pick("详情", "details")
        } else {
            language.pick("输出", "output")
        };
        parts.push(format!("{label} {}", truncate_text(&stdout, 120)));
    }
    if let Some(stderr) = json_string_field(value, &["stderr", "error", "errors"]) {
        parts.push(format!(
            "{} {}",
            language.pick("错误", "error"),
            truncate_text(&stderr, 120)
        ));
    }

    if !parts.is_empty() {
        return Some(parts.join(" | "));
    }

    let fallback = serde_json::to_string(value).ok()?;
    Some(truncate_text(&fallback, 180))
}

fn parse_tool_body_json(body: &str) -> Option<JsonValue> {
    serde_json::from_str(body.trim()).ok()
}

fn json_string_field(value: &JsonValue, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(text) = value.get(*key).and_then(JsonValue::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn json_i64_field(value: &JsonValue, keys: &[&str]) -> Option<i64> {
    for key in keys {
        if let Some(number) = value.get(*key).and_then(JsonValue::as_i64) {
            return Some(number);
        }
    }
    None
}

fn matches_tool_name(name: &str, parts: &[&str]) -> bool {
    parts.iter().any(|part| name.contains(part))
}

#[allow(dead_code)]
fn render_message_card(
    ui: &mut egui::Ui,
    message: &ConversationMessage,
    language: Language,
    theme: UiTheme,
) {
    let (title, accent) = match message.role {
        MessageRole::User => (language.pick("User", "User"), theme.accent()),
        MessageRole::Assistant => (
            language.pick("Assistant", "Assistant"),
            Color32::from_rgb(79, 103, 188),
        ),
        MessageRole::Tool => (
            language.pick("Tool", "Tool"),
            Color32::from_rgb(95, 130, 88),
        ),
        MessageRole::System => (
            language.pick("System", "System"),
            Color32::from_rgb(120, 120, 120),
        ),
    };

    let fill = match message.role {
        MessageRole::User => theme.accent().gamma_multiply(0.08),
        MessageRole::Assistant | MessageRole::System => theme.subpanel_fill(),
        MessageRole::Tool => Color32::from_rgb(230, 241, 230).gamma_multiply(0.95),
    };

    Frame::group(ui.style())
        .fill(fill)
        .stroke(Stroke::new(1.0, accent.gamma_multiply(0.35)))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(title).strong().color(accent));
                if let Some(usage) = message.usage {
                    ui.separator();
                    ui.small(format!(
                        "{} {}",
                        language.pick("tokens", "tokens"),
                        token_total(usage)
                    ));
                }
            });

            for block in &message.blocks {
                match block {
                    ContentBlock::Text { text } => {
                        ui.label(text);
                    }
                    ContentBlock::ToolUse { name, input, .. } => {
                        render_tool_block(
                            ui,
                            language.pick("Tool call", "Tool call"),
                            name,
                            input,
                            false,
                            theme,
                        );
                    }
                    ContentBlock::ToolResult {
                        tool_name,
                        output,
                        is_error,
                        ..
                    } => {
                        render_tool_block(
                            ui,
                            language.pick("Tool result", "Tool result"),
                            tool_name,
                            output,
                            *is_error,
                            theme,
                        );
                    }
                }
                ui.add_space(4.0);
            }
        });
}

#[allow(dead_code)]
fn render_message_card_v2(
    ui: &mut egui::Ui,
    message: &ConversationMessage,
    language: Language,
    theme: UiTheme,
) {
    let (title, accent) = match message.role {
        MessageRole::User => (language.pick("User", "User"), theme.accent()),
        MessageRole::Assistant => (
            language.pick("Assistant", "Assistant"),
            Color32::from_rgb(79, 103, 188),
        ),
        MessageRole::Tool => (
            language.pick("Tool", "Tool"),
            Color32::from_rgb(95, 130, 88),
        ),
        MessageRole::System => (
            language.pick("System", "System"),
            Color32::from_rgb(120, 120, 120),
        ),
    };

    let fill = match message.role {
        MessageRole::User => theme.accent().gamma_multiply(0.08),
        MessageRole::Assistant | MessageRole::System => theme.subpanel_fill(),
        MessageRole::Tool => Color32::from_rgb(230, 241, 230).gamma_multiply(0.95),
    };

    Frame::group(ui.style())
        .fill(fill)
        .stroke(Stroke::new(1.0, accent.gamma_multiply(0.35)))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(title).strong().color(accent));
                if let Some(usage) = message.usage {
                    ui.separator();
                    ui.small(format!(
                        "{} {}",
                        language.pick("tokens", "tokens"),
                        token_total(usage)
                    ));
                }
            });

            for block in &message.blocks {
                match block {
                    ContentBlock::Text { text } => {
                        ui.label(text);
                    }
                    ContentBlock::ToolUse { name, input, .. } => {
                        render_tool_block_v2(
                            ui,
                            language.pick("Tool call", "Tool call"),
                            name,
                            input,
                            false,
                            theme,
                        );
                    }
                    ContentBlock::ToolResult {
                        tool_name,
                        output,
                        is_error,
                        ..
                    } => {
                        render_tool_block_v2(
                            ui,
                            language.pick("Tool result", "Tool result"),
                            tool_name,
                            output,
                            *is_error,
                            theme,
                        );
                    }
                }
                ui.add_space(4.0);
            }
        });
}

#[allow(dead_code)]
fn render_tool_block(
    ui: &mut egui::Ui,
    kind: &str,
    name: &str,
    body: &str,
    is_error: bool,
    theme: UiTheme,
) {
    let color = if is_error {
        Color32::from_rgb(183, 63, 63)
    } else {
        theme.accent()
    };

    Frame::group(ui.style())
        .fill(theme.subpanel_fill())
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.4)))
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!("{kind}: {name}"))
                    .strong()
                    .color(color),
            );
            ui.monospace(truncate_text(body, 500));
        });
}

#[allow(dead_code)]
fn render_tool_block_v2(
    ui: &mut egui::Ui,
    kind: &str,
    name: &str,
    body: &str,
    is_error: bool,
    theme: UiTheme,
) {
    let color = if is_error {
        Color32::from_rgb(183, 63, 63)
    } else {
        theme.accent()
    };

    Frame::group(ui.style())
        .fill(theme.subpanel_fill())
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.4)))
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!("{kind}: {name}"))
                    .strong()
                    .color(color),
            );
            ui.monospace(truncate_text(body, 500));
        });
}

fn optional_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn optional_u32_text(value: Option<u32>) -> String {
    value.map_or_else(String::new, |number| number.to_string())
}

fn parse_optional_u32(value: &str) -> Result<Option<u32>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<u32>()
        .map(Some)
        .map_err(|error| format!("invalid integer '{trimmed}': {error}"))
}

fn absolute_folder(base: &Path, input: &str) -> PathBuf {
    let path = PathBuf::from(input);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

// 中文注释：GUI 测试产物统一落在工作区 `.claw/gui-tests/` 下，便于线程级清理。
fn thread_test_scope_key(active_thread_name: Option<&str>) -> String {
    let raw = active_thread_name.unwrap_or("workspace").trim();
    let mut key = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            key.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '-' | '_' | ' ') {
            key.push('-');
        }
    }

    let normalized = key.trim_matches('-').to_string();
    if normalized.is_empty() {
        "workspace".to_string()
    } else {
        normalized
    }
}

fn test_records_dir(workspace: &Path, scope_key: &str) -> PathBuf {
    workspace
        .join(".claw")
        .join("gui-tests")
        .join("records")
        .join(scope_key)
}

fn test_artifacts_dir(workspace: &Path, scope_key: &str) -> PathBuf {
    workspace
        .join(".claw")
        .join("gui-tests")
        .join("artifacts")
        .join(scope_key)
}

fn remove_session_file_family(session_path: &Path) -> usize {
    let mut removed = 0usize;
    if session_path.exists() && std::fs::remove_file(session_path).is_ok() {
        removed += 1;
    }

    let Some(parent) = session_path.parent() else {
        return removed;
    };
    let stem = session_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("session");
    let prefix = format!("{stem}.rot-");

    let rotated_paths = std::fs::read_dir(parent)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| {
                    name.starts_with(&prefix)
                        && Path::new(name)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
                })
        })
        .collect::<Vec<_>>();
    for path in rotated_paths {
        if std::fs::remove_file(path).is_ok() {
            removed += 1;
        }
    }

    removed
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    let total = value.chars().count();
    if total <= max_chars {
        return value.to_string();
    }
    let shortened = value.chars().take(max_chars).collect::<String>();
    format!("{shortened}...")
}

fn stable_hash64(value: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

// 中文注释：详情仅用于 GUI 展示，这里做长度上限保护，避免超长输出拖慢界面渲染。
// 中文注释：详情仅用于 GUI 展示，这里做长度上限保护，避免超长输出拖慢界面渲染。
fn clamp_inspector_event_body(value: &str) -> String {
    let max_chars = 16_000;
    let total = value.chars().count();
    if total <= max_chars {
        return value.to_string();
    }
    let shortened = value.chars().take(max_chars).collect::<String>();
    format!(
        "{shortened}\n\n[GUI detail clipped: {}/{} chars]",
        max_chars, total
    )
}

fn token_total(usage: TokenUsage) -> u32 {
    usage.input_tokens
        + usage.output_tokens
        + usage.cache_creation_input_tokens
        + usage.cache_read_input_tokens
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

fn collect_workspace_entries(root: &Path, max_depth: usize, max_entries: usize) -> Vec<String> {
    fn walk(
        current: &Path,
        base: &Path,
        depth: usize,
        max_depth: usize,
        max_entries: usize,
        out: &mut Vec<String>,
    ) {
        if depth > max_depth || out.len() >= max_entries {
            return;
        }

        let Ok(read_dir) = std::fs::read_dir(current) else {
            return;
        };

        let mut entries = read_dir
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        entries.sort();

        for path in entries {
            if out.len() >= max_entries {
                break;
            }
            let name = path
                .strip_prefix(base)
                .ok()
                .unwrap_or(path.as_path())
                .display()
                .to_string();
            let is_dir = path.is_dir();
            let indent = "  ".repeat(depth);
            let marker = if is_dir { "[D]" } else { "[F]" };
            out.push(format!("{indent}{marker} {name}"));

            if is_dir {
                walk(&path, base, depth + 1, max_depth, max_entries, out);
            }
        }
    }

    let mut out = Vec::new();
    walk(root, root, 0, max_depth, max_entries, &mut out);
    out
}

fn collect_skill_names(workspace: &Path, max_entries: usize) -> Vec<String> {
    fn push_skill_names(root: &Path, names: &mut BTreeSet<String>, max_entries: usize) {
        if !root.exists() || names.len() >= max_entries {
            return;
        }
        let Ok(read_dir) = std::fs::read_dir(root) else {
            return;
        };

        for entry in read_dir.filter_map(Result::ok) {
            if names.len() >= max_entries {
                break;
            }
            let path = entry.path();
            if path.is_dir() {
                let direct_skill = path.join("SKILL.md");
                if direct_skill.exists() {
                    if let Some(name) = path.file_name().and_then(|value| value.to_str()) {
                        names.insert(name.to_string());
                    }
                    continue;
                }

                if let Ok(children) = std::fs::read_dir(&path) {
                    for child in children.filter_map(Result::ok) {
                        let child_path = child.path();
                        if child_path.is_dir() && child_path.join("SKILL.md").exists() {
                            if let Some(name) =
                                child_path.file_name().and_then(|value| value.to_str())
                            {
                                names.insert(name.to_string());
                            }
                        }
                        if names.len() >= max_entries {
                            break;
                        }
                    }
                }
            }
        }
    }

    let mut roots = Vec::new();
    roots.push(workspace.join(".claw").join("skills"));
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        roots.push(PathBuf::from(codex_home).join("skills"));
    }
    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        roots.push(PathBuf::from(user_profile).join(".codex").join("skills"));
    }

    let mut names = BTreeSet::new();
    for root in roots {
        push_skill_names(&root, &mut names, max_entries);
        if names.len() >= max_entries {
            break;
        }
    }

    names.into_iter().collect()
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Claw GUI")
            .with_inner_size([1480.0, 920.0])
            .with_min_inner_size([1180.0, 760.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Claw GUI",
        options,
        Box::new(|cc| {
            configure_gui_fonts(&cc.egui_ctx);
            Ok(Box::new(ClawGuiApp::new()))
        }),
    )
}
