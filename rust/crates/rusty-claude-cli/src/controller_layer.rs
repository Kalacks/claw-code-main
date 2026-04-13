use crate::agent_layer::{AgentWorkspaceStore, AppRecord, ThreadRecord};
use crate::llm_layer::{LlmProfile, LlmProfileStore, TurnTokenLimits};
use crate::ui_layer::{render_app_help, render_llm_help, render_thread_help};
use commands::SlashCommand;
use runtime::SessionStore;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControllerDispatch<T> {
    Message(String),
    Action(T),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmCommandAction {
    List,
    ImportInline { profile: LlmProfile },
    ImportEnv { profile: LlmProfile },
    Use { name: String },
    ClearActive,
    Remove { name: String },
    Limits(LlmLimitsAction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmLimitsAction {
    Show,
    Clear,
    Set(TurnTokenLimits),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadCommandAction {
    List,
    Add {
        name: String,
        folder: String,
        description: Option<String>,
    },
    Switch {
        name: String,
    },
    Remove {
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppCommandAction {
    List,
    Add {
        name: String,
        command: String,
        description: String,
    },
    Remove {
        name: String,
    },
    Run {
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmCommandEffect {
    Activate {
        profile: LlmProfile,
    },
    ClearActive,
    UpdateLimits {
        label: &'static str,
        limits: TurnTokenLimits,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadCommandEffect {
    Add { draft: PreparedThreadAdd },
    Switch { target: PreparedThreadSwitch },
}

#[derive(Debug, Clone)]
pub enum AppCommandEffect {
    Run {
        app: AppRecord,
        target: AppRunTarget,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedThreadAdd {
    pub record: ThreadRecord,
    pub folder_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedThreadSwitch {
    pub thread: ThreadRecord,
    pub folder_path: PathBuf,
    pub session_path: PathBuf,
}

#[derive(Debug, Clone)]
pub enum AppRunTarget {
    Slash(SlashCommand),
    Prompt(String),
}

pub fn parse_llm_command(
    args: Option<&str>,
    active_profile_name: Option<&str>,
    turn_limits: TurnTokenLimits,
) -> ControllerDispatch<LlmCommandAction> {
    let Some(raw_args) = args.map(str::trim).filter(|value| !value.is_empty()) else {
        return ControllerDispatch::Message(render_llm_help(active_profile_name, turn_limits));
    };

    let parts = raw_args.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() {
        return ControllerDispatch::Message(render_llm_help(active_profile_name, turn_limits));
    }

    match parts[0] {
        "help" | "-h" | "--help" => {
            ControllerDispatch::Message(render_llm_help(active_profile_name, turn_limits))
        }
        "list" => ControllerDispatch::Action(LlmCommandAction::List),
        "add" | "import" => {
            if parts.len() < 5 {
                return ControllerDispatch::Message(
                    "Usage: /llm import <name> <provider> <model> <api-key> [base-url]".to_string(),
                );
            }
            let (provider, base_url) = match validate_llm_profile_input(
                parts[1],
                parts[2],
                parts[3],
                None,
                parts.get(5).copied(),
            ) {
                Ok(validated) => validated,
                Err(error) => {
                    return ControllerDispatch::Message(format!("invalid llm profile: {error}"));
                }
            };
            ControllerDispatch::Action(LlmCommandAction::ImportInline {
                profile: LlmProfile {
                    name: parts[1].to_string(),
                    provider,
                    model: parts[3].to_string(),
                    api_key: Some(parts[4].to_string()),
                    api_key_env: None,
                    base_url,
                    note: None,
                },
            })
        }
        "add-env" | "import-env" => {
            if parts.len() < 5 {
                return ControllerDispatch::Message(
                    "Usage: /llm import-env <name> <provider> <model> <api-key-env> [base-url]"
                        .to_string(),
                );
            }
            let (provider, base_url) = match validate_llm_profile_input(
                parts[1],
                parts[2],
                parts[3],
                Some(parts[4]),
                parts.get(5).copied(),
            ) {
                Ok(validated) => validated,
                Err(error) => {
                    return ControllerDispatch::Message(format!("invalid llm profile: {error}"));
                }
            };
            ControllerDispatch::Action(LlmCommandAction::ImportEnv {
                profile: LlmProfile {
                    name: parts[1].to_string(),
                    provider,
                    model: parts[3].to_string(),
                    api_key: None,
                    api_key_env: Some(parts[4].to_string()),
                    base_url,
                    note: None,
                },
            })
        }
        "use" => match parts.get(1) {
            Some(name) => ControllerDispatch::Action(LlmCommandAction::Use {
                name: (*name).to_string(),
            }),
            None => ControllerDispatch::Message("Usage: /llm use <name>".to_string()),
        },
        "clear-active" => ControllerDispatch::Action(LlmCommandAction::ClearActive),
        "remove" => match parts.get(1) {
            Some(name) => ControllerDispatch::Action(LlmCommandAction::Remove {
                name: (*name).to_string(),
            }),
            None => ControllerDispatch::Message("Usage: /llm remove <name>".to_string()),
        },
        "limits" => match parse_llm_limits_action(&parts[1..]) {
            ControllerDispatch::Message(message) => ControllerDispatch::Message(message),
            ControllerDispatch::Action(action) => {
                ControllerDispatch::Action(LlmCommandAction::Limits(action))
            }
        },
        other => ControllerDispatch::Message(format!(
            "Unknown /llm action '{}'\n{}",
            other,
            render_llm_help(active_profile_name, turn_limits)
        )),
    }
}

pub fn parse_thread_command(
    args: Option<&str>,
    active_thread_name: Option<&str>,
) -> ControllerDispatch<ThreadCommandAction> {
    let Some(raw_args) = args.map(str::trim).filter(|value| !value.is_empty()) else {
        return ControllerDispatch::Message(render_thread_help(active_thread_name));
    };

    let parts = raw_args.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() {
        return ControllerDispatch::Message(render_thread_help(active_thread_name));
    }

    match parts[0] {
        "help" | "-h" | "--help" => {
            ControllerDispatch::Message(render_thread_help(active_thread_name))
        }
        "list" => ControllerDispatch::Action(ThreadCommandAction::List),
        "add" => {
            if parts.len() < 3 {
                return ControllerDispatch::Message(
                    "Usage: /thread add <name> <folder> [description]".to_string(),
                );
            }
            let description = if parts.len() > 3 {
                Some(parts[3..].join(" "))
            } else {
                None
            };
            ControllerDispatch::Action(ThreadCommandAction::Add {
                name: parts[1].to_string(),
                folder: parts[2].to_string(),
                description,
            })
        }
        "switch" => match parts.get(1) {
            Some(name) => ControllerDispatch::Action(ThreadCommandAction::Switch {
                name: (*name).to_string(),
            }),
            None => ControllerDispatch::Message("Usage: /thread switch <name>".to_string()),
        },
        "remove" => match parts.get(1) {
            Some(name) => ControllerDispatch::Action(ThreadCommandAction::Remove {
                name: (*name).to_string(),
            }),
            None => ControllerDispatch::Message("Usage: /thread remove <name>".to_string()),
        },
        other => ControllerDispatch::Message(format!(
            "Unknown /thread action '{}'\n{}",
            other,
            render_thread_help(active_thread_name)
        )),
    }
}

pub fn parse_app_command(args: Option<&str>) -> ControllerDispatch<AppCommandAction> {
    let Some(raw_args) = args.map(str::trim).filter(|value| !value.is_empty()) else {
        return ControllerDispatch::Message(render_app_help());
    };

    let parts = raw_args.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() {
        return ControllerDispatch::Message(render_app_help());
    }

    match parts[0] {
        "help" | "-h" | "--help" => ControllerDispatch::Message(render_app_help()),
        "list" => ControllerDispatch::Action(AppCommandAction::List),
        "add" => {
            if parts.len() < 3 {
                return ControllerDispatch::Message(
                    "Usage: /app add <name> <command> [description]".to_string(),
                );
            }
            let description = if parts.len() > 3 {
                parts[3..].join(" ")
            } else {
                "custom app command".to_string()
            };
            ControllerDispatch::Action(AppCommandAction::Add {
                name: parts[1].to_string(),
                command: parts[2].to_string(),
                description,
            })
        }
        "remove" => match parts.get(1) {
            Some(name) => ControllerDispatch::Action(AppCommandAction::Remove {
                name: (*name).to_string(),
            }),
            None => ControllerDispatch::Message("Usage: /app remove <name>".to_string()),
        },
        "run" => match parts.get(1) {
            Some(name) => ControllerDispatch::Action(AppCommandAction::Run {
                name: (*name).to_string(),
            }),
            None => ControllerDispatch::Message("Usage: /app run <name>".to_string()),
        },
        other => ControllerDispatch::Message(format!(
            "Unknown /app action '{}'\n{}",
            other,
            render_app_help()
        )),
    }
}

pub fn execute_llm_controller_action(
    store: &mut LlmProfileStore,
    action: LlmCommandAction,
    current_limits: TurnTokenLimits,
) -> Result<ControllerDispatch<LlmCommandEffect>, Box<dyn std::error::Error>> {
    match action {
        LlmCommandAction::List => Ok(ControllerDispatch::Message(format_llm_profiles_report(
            store.storage_path(),
            store.active_profile_name(),
            current_limits,
            &store.list_profiles(),
        ))),
        LlmCommandAction::ImportInline { profile } => {
            store.upsert_profile(profile.clone())?;
            Ok(ControllerDispatch::Message(
                format_llm_profile_saved_report(&profile, "inline"),
            ))
        }
        LlmCommandAction::ImportEnv { profile } => {
            store.upsert_profile(profile.clone())?;
            Ok(ControllerDispatch::Message(
                format_llm_profile_saved_report(&profile, &profile.key_source_label()),
            ))
        }
        LlmCommandAction::Use { name } => {
            let Some(profile) = store.profile(&name).cloned() else {
                return Ok(ControllerDispatch::Message(format!(
                    "llm profile '{}' not found",
                    name
                )));
            };
            store.set_active_profile(&name)?;
            Ok(ControllerDispatch::Action(LlmCommandEffect::Activate {
                profile,
            }))
        }
        LlmCommandAction::ClearActive => {
            store.clear_active_profile()?;
            Ok(ControllerDispatch::Action(LlmCommandEffect::ClearActive))
        }
        LlmCommandAction::Remove { name } => {
            store.remove_profile(&name)?;
            Ok(ControllerDispatch::Message(
                format_llm_profile_removed_report(&name),
            ))
        }
        LlmCommandAction::Limits(LlmLimitsAction::Show) => Ok(ControllerDispatch::Message(
            format_llm_limits_report(current_limits, store.storage_path()),
        )),
        LlmCommandAction::Limits(LlmLimitsAction::Clear) => {
            let limits = TurnTokenLimits::default();
            store.set_turn_token_limits(limits)?;
            Ok(ControllerDispatch::Action(LlmCommandEffect::UpdateLimits {
                label: "cleared",
                limits,
            }))
        }
        LlmCommandAction::Limits(LlmLimitsAction::Set(limits)) => {
            let limits = limits.validate()?;
            store.set_turn_token_limits(limits)?;
            Ok(ControllerDispatch::Action(LlmCommandEffect::UpdateLimits {
                label: "updated",
                limits,
            }))
        }
    }
}

pub fn execute_thread_controller_action(
    store: &mut AgentWorkspaceStore,
    action: ThreadCommandAction,
) -> Result<ControllerDispatch<ThreadCommandEffect>, Box<dyn std::error::Error>> {
    match action {
        ThreadCommandAction::List => Ok(ControllerDispatch::Message(format_threads_report(
            store.storage_path(),
            store.active_thread_name(),
            &store.list_threads(),
        ))),
        ThreadCommandAction::Add {
            name,
            folder,
            description,
        } => Ok(ControllerDispatch::Action(ThreadCommandEffect::Add {
            draft: prepare_thread_add(
                &std::env::current_dir()?,
                &name,
                &folder,
                description,
                current_epoch_millis(),
            )?,
        })),
        ThreadCommandAction::Switch { name } => {
            let Some(thread) = store.thread(&name).cloned() else {
                return Ok(ControllerDispatch::Message(format!(
                    "thread '{}' not found",
                    name
                )));
            };
            Ok(ControllerDispatch::Action(ThreadCommandEffect::Switch {
                target: prepare_thread_switch(thread)?,
            }))
        }
        ThreadCommandAction::Remove { name } => {
            store.remove_thread(&name)?;
            Ok(ControllerDispatch::Message(format_thread_removed_report(
                &name,
            )))
        }
    }
}

pub fn execute_app_controller_action(
    store: &mut AgentWorkspaceStore,
    action: AppCommandAction,
) -> Result<ControllerDispatch<AppCommandEffect>, Box<dyn std::error::Error>> {
    match action {
        AppCommandAction::List => Ok(ControllerDispatch::Message(format_apps_report(
            store.storage_path(),
            &store.list_apps(),
        ))),
        AppCommandAction::Add {
            name,
            command,
            description,
        } => {
            store.upsert_app(AppRecord {
                name: name.clone(),
                command: command.clone(),
                description: description.clone(),
            })?;
            Ok(ControllerDispatch::Message(format_app_saved_report(
                &name,
                &command,
                &description,
            )))
        }
        AppCommandAction::Remove { name } => {
            store.remove_app(&name)?;
            Ok(ControllerDispatch::Message(format_app_removed_report(
                &name,
            )))
        }
        AppCommandAction::Run { name } => {
            let Some(app) = store.app(&name).cloned() else {
                return Ok(ControllerDispatch::Message(format!(
                    "app '{}' not found",
                    name
                )));
            };
            let target = prepare_app_run_target(&app.command)?;
            Ok(ControllerDispatch::Action(AppCommandEffect::Run {
                app,
                target,
            }))
        }
    }
}

pub fn prepare_thread_add(
    cwd: &Path,
    name: &str,
    folder: &str,
    description: Option<String>,
    now_epoch_millis: u128,
) -> Result<PreparedThreadAdd, Box<dyn std::error::Error>> {
    let folder_path = resolve_workspace_path(cwd, folder);
    // 线程准备阶段只负责解析目录与会话文件位置；
    // 真正的 Session 装载和 runtime 切换仍由主流程负责。
    let store = SessionStore::from_cwd(&folder_path)
        .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
    let handle = store.create_handle(&build_thread_session_id(name, now_epoch_millis));
    Ok(PreparedThreadAdd {
        record: ThreadRecord {
            name: name.to_string(),
            folder: folder_path.display().to_string(),
            session_id: handle.id.clone(),
            session_path: handle.path.display().to_string(),
            description,
        },
        folder_path,
    })
}

pub fn prepare_thread_switch(
    thread: ThreadRecord,
) -> Result<PreparedThreadSwitch, Box<dyn std::error::Error>> {
    let folder_path = PathBuf::from(&thread.folder);
    let session_path = PathBuf::from(&thread.session_path);
    Ok(PreparedThreadSwitch {
        thread,
        folder_path,
        session_path,
    })
}

pub fn prepare_app_run_target(command: &str) -> Result<AppRunTarget, Box<dyn std::error::Error>> {
    if command.trim_start().starts_with('/') {
        if let Some(parsed) = SlashCommand::parse(command)? {
            return Ok(AppRunTarget::Slash(parsed));
        }
    }
    Ok(AppRunTarget::Prompt(command.to_string()))
}

pub fn format_llm_profiles_report(
    storage_path: &Path,
    active_profile_name: Option<&str>,
    turn_limits: TurnTokenLimits,
    profiles: &[LlmProfile],
) -> String {
    let mut lines = vec![format!(
        "LLM Profiles
  File             {}
  Active profile   {}
  Profiles         {}
  Turn limits      {}",
        storage_path.display(),
        active_profile_name.unwrap_or("(none)"),
        profiles.len(),
        turn_limits.summary_line(),
    )];

    if profiles.is_empty() {
        lines.push(
            "  Empty state      No saved LLM profiles yet. Use /llm import or /llm import-env."
                .to_string(),
        );
        return lines.join("\n");
    }

    lines.extend(profiles.iter().map(|profile| {
        let marker = if active_profile_name == Some(profile.name.as_str()) {
            "active"
        } else {
            "saved"
        };
        format!(
            "  {name:<16} {marker:<8} provider={provider:<10} model={model} key={key} base_url={base_url}",
            name = profile.name,
            provider = profile.normalized_provider(),
            model = profile.model,
            key = profile.masked_key_preview(),
            base_url = profile.base_url.as_deref().unwrap_or("(provider default)"),
        )
    }));

    lines.join("\n")
}

pub fn format_threads_report(
    storage_path: &Path,
    active_thread_name: Option<&str>,
    threads: &[ThreadRecord],
) -> String {
    let mut lines = vec![format!(
        "Threads
  File             {}
  Active thread    {}
  Threads          {}",
        storage_path.display(),
        active_thread_name.unwrap_or("(none)"),
        threads.len(),
    )];

    if threads.is_empty() {
        lines.push(
            "  Empty state      No saved threads yet. Use /thread add <name> <folder> [description]."
                .to_string(),
        );
        return lines.join("\n");
    }

    lines.extend(threads.iter().map(|thread| {
        let marker = if active_thread_name == Some(thread.name.as_str()) {
            "active"
        } else {
            "saved"
        };
        format!(
            "  {name:<16} {marker:<8} folder={folder} session={session} desc={description}",
            name = thread.name,
            folder = thread.folder,
            session = thread.session_id,
            description = thread.description.as_deref().unwrap_or("-"),
        )
    }));

    lines.join("\n")
}

pub fn format_apps_report(storage_path: &Path, apps: &[AppRecord]) -> String {
    let mut lines = vec![format!(
        "Apps
  File             {}
  Apps             {}
  Thread workflows /thread add <name> <folder> [description]",
        storage_path.display(),
        apps.len(),
    )];

    if apps.is_empty() {
        lines.push(
            "  Empty state      No saved apps yet. Use /app add <name> <command> [description]."
                .to_string(),
        );
        lines.push("  Built-in tips    /skills list · /mcp list · /plugin list".to_string());
        return lines.join("\n");
    }

    lines.extend(apps.iter().map(|app| {
        format!(
            "  {name:<16} {command:<24} {description}",
            name = app.name,
            command = app.command,
            description = app.description
        )
    }));

    lines.join("\n")
}

pub fn format_llm_profile_saved_report(profile: &LlmProfile, key_source: &str) -> String {
    format!(
        "LLM profile saved\n  Name             {}\n  Provider         {}\n  Model            {}\n  Base URL         {}\n  Key source       {}",
        profile.name,
        profile.normalized_provider(),
        profile.model,
        profile.base_url.as_deref().unwrap_or("(provider default)"),
        key_source,
    )
}

pub fn format_llm_profile_activated_report(
    profile: &LlmProfile,
    resolved_model: &str,
    connected_line: &str,
) -> String {
    format!(
        "LLM profile activated\n  Profile          {}\n  Provider         {}\n  Model            {}\n  Connected        {}",
        profile.name,
        profile.normalized_provider(),
        resolved_model,
        connected_line,
    )
}

pub fn format_llm_profile_cleared_report() -> String {
    "LLM profile cleared\n  Active profile   (none)".to_string()
}

pub fn format_llm_profile_removed_report(name: &str) -> String {
    format!("LLM profile removed\n  Name             {}", name)
}

pub fn format_llm_limits_report(current: TurnTokenLimits, storage_path: &Path) -> String {
    format!(
        "LLM token limits\n  Current          {}\n  Persistence      {}",
        current.summary_line(),
        storage_path.display(),
    )
}

pub fn format_llm_limits_updated_report(label: &str, current: TurnTokenLimits) -> String {
    format!(
        "LLM token limits {}\n  Current          {}",
        label,
        current.summary_line(),
    )
}

pub fn format_thread_saved_report(
    name: &str,
    folder: &Path,
    session_id: &str,
    session_file: &Path,
) -> String {
    format!(
        "Thread saved\n  Name             {}\n  Folder           {}\n  Session          {}\n  Session file     {}",
        name,
        folder.display(),
        session_id,
        session_file.display(),
    )
}

pub fn format_thread_switched_report(
    name: &str,
    folder: &Path,
    session_id: &str,
    session_file: &Path,
) -> String {
    format!(
        "Thread switched\n  Name             {}\n  Folder           {}\n  Session          {}\n  File             {}",
        name,
        folder.display(),
        session_id,
        session_file.display(),
    )
}

pub fn format_thread_removed_report(name: &str) -> String {
    format!("Thread removed\n  Name             {}", name)
}

pub fn format_app_saved_report(name: &str, command: &str, description: &str) -> String {
    format!(
        "App saved\n  Name             {}\n  Command          {}\n  Description      {}",
        name, command, description
    )
}

pub fn format_app_removed_report(name: &str) -> String {
    format!("App removed\n  Name             {}", name)
}

pub fn validate_llm_profile_input(
    name: &str,
    provider: &str,
    model: &str,
    api_key_env: Option<&str>,
    base_url: Option<&str>,
) -> Result<(String, Option<String>), String> {
    if !is_valid_profile_name(name) {
        return Err("profile name must use letters, numbers, '.', '_' or '-' only".to_string());
    }
    let normalized_provider = LlmProfile {
        name: name.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        base_url: None,
        api_key: Some("placeholder".to_string()),
        api_key_env: None,
        note: None,
    }
    .normalized_provider();
    if !matches!(
        normalized_provider.as_str(),
        "anthropic" | "deepseek" | "openai" | "xai" | "compat"
    ) {
        return Err(
            "provider must be one of anthropic | deepseek | openai | xai | compat".to_string(),
        );
    }
    if model.trim().is_empty() {
        return Err("model cannot be empty".to_string());
    }
    if let Some(env_key) = api_key_env {
        if !is_valid_env_var_name(env_key) {
            return Err("api-key-env must be a valid environment variable name".to_string());
        }
    }
    let normalized_base_url = match base_url.map(str::trim).filter(|value| !value.is_empty()) {
        Some(url) if url.starts_with("https://") || url.starts_with("http://") => {
            Some(url.to_string())
        }
        Some(_) => return Err("base-url must start with http:// or https://".to_string()),
        None => None,
    };
    Ok((normalized_provider, normalized_base_url))
}

pub fn parse_optional_token_limit(raw: &str) -> Result<Option<u32>, String> {
    if matches!(raw, "-" | "none" | "None" | "NONE") {
        return Ok(None);
    }
    let parsed = raw
        .parse::<u32>()
        .map_err(|_| format!("invalid token limit '{raw}': expected positive integer or none"))?;
    if parsed == 0 {
        return Err("token limits must be greater than zero".to_string());
    }
    Ok(Some(parsed))
}

#[must_use]
pub fn resolve_workspace_path(cwd: &Path, raw: &str) -> PathBuf {
    let candidate = PathBuf::from(raw);
    if candidate.is_absolute() {
        candidate
    } else {
        cwd.join(candidate)
    }
}

#[must_use]
pub fn sanitize_for_session_id(name: &str) -> String {
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

#[must_use]
pub fn current_epoch_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map_or(0, |duration| duration.as_millis())
}

#[must_use]
pub fn build_thread_session_id(name: &str, now_epoch_millis: u128) -> String {
    format!(
        "thread-{}-{now_epoch_millis}",
        sanitize_for_session_id(name)
    )
}

fn parse_llm_limits_action(args: &[&str]) -> ControllerDispatch<LlmLimitsAction> {
    if args.is_empty() || matches!(args[0], "show" | "list") {
        return ControllerDispatch::Action(LlmLimitsAction::Show);
    }

    match args[0] {
        "clear" => ControllerDispatch::Action(LlmLimitsAction::Clear),
        "set" => {
            if args.len() < 3 {
                return ControllerDispatch::Message(
                    "Usage: /llm limits set --min-input <n|none> --max-input <n|none> --min-output <n|none> --max-output <n|none>"
                        .to_string(),
                );
            }
            let mut limits = TurnTokenLimits::default();
            let mut index = 1;
            while index < args.len() {
                let flag = args[index];
                let Some(raw_value) = args.get(index + 1) else {
                    return ControllerDispatch::Message(format!("Missing value for {flag}"));
                };
                let value = match parse_optional_token_limit(raw_value) {
                    Ok(value) => value,
                    Err(error) => return ControllerDispatch::Message(error),
                };
                match flag {
                    "--min-input" => limits.min_input_tokens = value,
                    "--max-input" => limits.max_input_tokens = value,
                    "--min-output" => limits.min_output_tokens = value,
                    "--max-output" => limits.max_output_tokens = value,
                    _ => {
                        return ControllerDispatch::Message(format!(
                            "Unknown /llm limits flag '{flag}'. Use --min-input/--max-input/--min-output/--max-output."
                        ));
                    }
                }
                index += 2;
            }
            match limits.validate() {
                Ok(limits) => ControllerDispatch::Action(LlmLimitsAction::Set(limits)),
                Err(error) => ControllerDispatch::Message(error),
            }
        }
        other => ControllerDispatch::Message(format!("Unknown /llm limits action '{other}'")),
    }
}

fn is_valid_profile_name(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn is_valid_env_var_name(value: &str) -> bool {
    let trimmed = value.trim();
    let mut chars = trimmed.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::{
        build_thread_session_id, execute_app_controller_action, execute_llm_controller_action,
        execute_thread_controller_action, parse_app_command, parse_llm_command,
        parse_thread_command, prepare_app_run_target, prepare_thread_add, resolve_workspace_path,
        sanitize_for_session_id, AppCommandAction, AppCommandEffect, AppRunTarget,
        ControllerDispatch, LlmCommandAction, LlmCommandEffect, ThreadCommandAction,
        ThreadCommandEffect,
    };
    use crate::agent_layer::{AgentWorkspaceStore, AppRecord, ThreadRecord};
    use crate::llm_layer::{LlmProfile, LlmProfileStore, TurnTokenLimits};
    use std::fs;
    use std::path::PathBuf;

    fn temp_workspace(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("clawd-controller-{unique}-{label}"))
    }

    #[test]
    fn llm_parser_returns_help_when_no_args() {
        let parsed = parse_llm_command(None, Some("deepseek-main"), TurnTokenLimits::default());
        match parsed {
            ControllerDispatch::Message(message) => assert!(message.contains("LLM")),
            ControllerDispatch::Action(_) => panic!("expected help message"),
        }
    }

    #[test]
    fn thread_parser_extracts_add_payload() {
        let parsed = parse_thread_command(Some("add api ../api backend thread"), None);
        assert_eq!(
            parsed,
            ControllerDispatch::Action(ThreadCommandAction::Add {
                name: "api".to_string(),
                folder: "../api".to_string(),
                description: Some("backend thread".to_string()),
            })
        );
    }

    #[test]
    fn app_parser_extracts_run_payload() {
        let parsed = parse_app_command(Some("run review"));
        assert_eq!(
            parsed,
            ControllerDispatch::Action(super::AppCommandAction::Run {
                name: "review".to_string(),
            })
        );
    }

    #[test]
    fn llm_parser_extracts_use_action() {
        let parsed = parse_llm_command(Some("use deepseek-main"), None, TurnTokenLimits::default());
        assert_eq!(
            parsed,
            ControllerDispatch::Action(LlmCommandAction::Use {
                name: "deepseek-main".to_string(),
            })
        );
    }

    #[test]
    fn session_id_helpers_generate_stable_shape() {
        assert_eq!(sanitize_for_session_id("api worker"), "api-worker");
        assert_eq!(
            build_thread_session_id("api worker", 123),
            "thread-api-worker-123"
        );
    }

    #[test]
    fn resolve_workspace_path_resolves_relative_and_absolute() {
        let cwd = PathBuf::from("E:\\repo\\workspace");
        let relative = resolve_workspace_path(&cwd, "rust");
        assert_eq!(relative, PathBuf::from("E:\\repo\\workspace\\rust"));

        let absolute = resolve_workspace_path(&cwd, "E:\\tmp\\x");
        assert_eq!(absolute, PathBuf::from("E:\\tmp\\x"));
    }

    #[test]
    fn llm_executor_returns_activation_effect_for_existing_profile() {
        let root = temp_workspace("llm-executor");
        fs::create_dir_all(&root).expect("workspace should exist");
        let mut store = LlmProfileStore::load_for(&root).expect("store should load");
        store
            .upsert_profile(LlmProfile {
                name: "deepseek-main".to_string(),
                provider: "deepseek".to_string(),
                model: "deepseek-chat".to_string(),
                api_key: None,
                api_key_env: Some("DEEPSEEK_API_KEY".to_string()),
                base_url: Some("https://api.deepseek.com".to_string()),
                note: None,
            })
            .expect("profile should persist");

        let dispatch = execute_llm_controller_action(
            &mut store,
            LlmCommandAction::Use {
                name: "deepseek-main".to_string(),
            },
            TurnTokenLimits::default(),
        )
        .expect("execution should succeed");

        match dispatch {
            ControllerDispatch::Action(LlmCommandEffect::Activate { profile }) => {
                assert_eq!(profile.name, "deepseek-main");
                assert_eq!(store.active_profile_name(), Some("deepseek-main"));
            }
            other => panic!("unexpected llm dispatch: {other:?}"),
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn thread_executor_returns_switch_effect_for_existing_thread() {
        let root = temp_workspace("thread-executor");
        fs::create_dir_all(&root).expect("workspace should exist");
        let mut store = AgentWorkspaceStore::load_for(&root).expect("store should load");
        store
            .upsert_thread(ThreadRecord {
                name: "workspace-chat".to_string(),
                folder: root.display().to_string(),
                session_id: "thread-workspace-chat-1".to_string(),
                session_path: root.join("thread.jsonl").display().to_string(),
                description: Some("workspace default".to_string()),
            })
            .expect("thread should persist");

        let dispatch = execute_thread_controller_action(
            &mut store,
            ThreadCommandAction::Switch {
                name: "workspace-chat".to_string(),
            },
        )
        .expect("execution should succeed");

        match dispatch {
            ControllerDispatch::Action(ThreadCommandEffect::Switch { target }) => {
                assert_eq!(target.thread.name, "workspace-chat");
                assert_eq!(target.folder_path, root);
            }
            other => panic!("unexpected thread dispatch: {other:?}"),
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn app_executor_persists_records_and_returns_run_effect() {
        let root = temp_workspace("app-executor");
        fs::create_dir_all(&root).expect("workspace should exist");
        let mut store = AgentWorkspaceStore::load_for(&root).expect("store should load");

        let saved = execute_app_controller_action(
            &mut store,
            AppCommandAction::Add {
                name: "status".to_string(),
                command: "/status".to_string(),
                description: "show workspace status".to_string(),
            },
        )
        .expect("add should succeed");
        assert!(matches!(saved, ControllerDispatch::Message(_)));

        let dispatch = execute_app_controller_action(
            &mut store,
            AppCommandAction::Run {
                name: "status".to_string(),
            },
        )
        .expect("run should succeed");

        match dispatch {
            ControllerDispatch::Action(AppCommandEffect::Run { app, target }) => {
                assert_eq!(
                    app,
                    AppRecord {
                        name: "status".to_string(),
                        command: "/status".to_string(),
                        description: "show workspace status".to_string(),
                    }
                );
                assert!(matches!(target, AppRunTarget::Slash(_)));
            }
            other => panic!("unexpected app dispatch: {other:?}"),
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prepare_thread_add_builds_record_and_session_path() {
        let root = temp_workspace("thread-add");
        fs::create_dir_all(&root).expect("workspace should exist");

        let draft = prepare_thread_add(&root, "api chat", "child", Some("desc".to_string()), 42)
            .expect("draft should build");

        assert_eq!(draft.record.name, "api chat");
        assert!(draft.folder_path.ends_with("child"));
        assert_eq!(draft.record.session_id, "thread-api-chat-42");
        assert!(std::path::Path::new(&draft.record.session_path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prepare_app_run_target_distinguishes_slash_and_prompt() {
        let slash = prepare_app_run_target("/status").expect("slash target");
        assert!(matches!(slash, AppRunTarget::Slash(_)));

        let prompt = prepare_app_run_target("show status").expect("prompt target");
        match prompt {
            AppRunTarget::Prompt(text) => assert_eq!(text, "show status"),
            other @ AppRunTarget::Slash(_) => panic!("unexpected target: {other:?}"),
        }
    }
}
