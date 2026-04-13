use crate::agent_layer::AgentLayerSummary;
use crate::llm_layer::{LlmLayerSummary, TurnTokenLimits};

#[derive(Debug, Clone)]
pub struct ClientSurface {
    pub model: String,
    pub connected_line: String,
    pub provider: String,
    pub base_url: String,
    pub permission_mode: String,
    pub git_branch: String,
    pub workspace_state: String,
    pub directory: String,
    pub session_id: String,
    pub session_path: String,
    pub llm: LlmLayerSummary,
    pub agent: AgentLayerSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardUsage {
    pub message_count: usize,
    pub turns: u32,
    pub latest_total_tokens: u32,
    pub session_total_tokens: u32,
    pub currency: String,
    pub pricing: String,
    pub latest_cost: String,
    pub session_cost: String,
}

pub fn render_client_layer_help() -> String {
    "Client Layers
  LLM layer        import isolated provider profiles with /llm import or /llm import-env
  Agent layer      bind folders to threads with /thread add and install reusable skills/apps
  UI layer         inspect /status, /cost, and automatic per-turn token + fee reports"
        .to_string()
}

pub fn render_startup_dashboard(surface: &ClientSurface) -> String {
    [
        "Claw Client".to_string(),
        format!("  Connected        {}", surface.connected_line),
        format!("  Model            {}", surface.model),
        format!("  Provider         {}", surface.provider),
        format!("  Base URL         {}", surface.base_url),
        format!("  Permission mode  {}", surface.permission_mode),
        format!("  Git branch       {}", surface.git_branch),
        format!("  Workspace        {}", surface.workspace_state),
        format!("  Directory        {}", surface.directory),
        format!("  Session          {}", surface.session_id),
        format!("  Auto-save        {}", surface.session_path),
        String::new(),
        render_client_workspace_overview(&surface.llm, &surface.agent),
        String::new(),
        "Entry Points".to_string(),
        "  Workspace        /workspace | /status | /cost".to_string(),
        "  Models           /llm list | /llm import | /llm limits".to_string(),
        "  Threads          /thread list | /thread add | /thread switch <name>".to_string(),
        "  Skills           /skills list | /skills install <path>".to_string(),
        "  Apps             /app list | /app add <name> <command> [description]".to_string(),
        "  Sessions         /session list | /resume latest".to_string(),
        String::new(),
        "Input".to_string(),
        "  Type /help for commands | Tab for workflow completions | Shift+Enter for newline"
            .to_string(),
    ]
    .join("\n")
}

pub fn render_workspace_dashboard(
    surface: &ClientSurface,
    usage: Option<&DashboardUsage>,
) -> String {
    let mut sections = vec![
        "Workspace".to_string(),
        format!("  Connected        {}", surface.connected_line),
        format!("  Model            {}", surface.model),
        format!("  Provider         {}", surface.provider),
        format!("  Base URL         {}", surface.base_url),
        format!("  Permission mode  {}", surface.permission_mode),
        format!("  Git branch       {}", surface.git_branch),
        format!("  Workspace        {}", surface.workspace_state),
        format!("  Directory        {}", surface.directory),
        format!("  Session          {}", surface.session_id),
        format!("  Auto-save        {}", surface.session_path),
        String::new(),
        render_client_workspace_overview(&surface.llm, &surface.agent),
    ];

    if let Some(usage) = usage {
        sections.push(String::new());
        sections.push("Usage".to_string());
        sections.push(format!("  Messages         {}", usage.message_count));
        sections.push(format!("  Turns            {}", usage.turns));
        sections.push(format!("  Turn total       {}", usage.latest_total_tokens));
        sections.push(format!("  Session total    {}", usage.session_total_tokens));
        sections.push(format!("  Currency         {}", usage.currency));
        sections.push(format!("  Pricing          {}", usage.pricing));
        sections.push(format!("  Turn cost        {}", usage.latest_cost));
        sections.push(format!("  Session cost     {}", usage.session_cost));
    }

    sections.push(String::new());
    sections.push("Actions".to_string());
    sections.push("  Change model     /llm use <name> | /model <name>".to_string());
    sections.push("  Change folder    /workspace <path> | /thread switch <name>".to_string());
    sections.push("  Add thread       /thread add <name> <folder> [description]".to_string());
    sections.push("  Add skill        /skills install <path>".to_string());
    sections.push("  Add app          /app add <name> <command> [description]".to_string());

    sections.join("\n")
}

pub fn render_client_workspace_overview(
    llm: &LlmLayerSummary,
    agent: &AgentLayerSummary,
) -> String {
    format!(
        "Workspace Layers
  LLM profile      {}
  LLM provider     {}
  LLM base URL     {}
  Saved profiles   {}
  Turn limits      {}
  Active thread    {}
  Saved threads    {}
  Saved apps       {}
  Skills           /skills list | /skills install <path>",
        llm.active_profile_name.as_deref().unwrap_or("(none)"),
        llm.active_provider.as_deref().unwrap_or("(auto)"),
        llm.active_base_url
            .as_deref()
            .unwrap_or("(provider default)"),
        llm.profile_count,
        llm.turn_token_limits.summary_line(),
        agent.active_thread_name.as_deref().unwrap_or("(none)"),
        agent.thread_count,
        agent.app_count,
    )
}

pub fn render_llm_help(active_profile: Option<&str>, limits: TurnTokenLimits) -> String {
    format!(
        "LLM
  Active profile   {}
  Turn limits      {}
  Purpose          keep provider/model/base-url/api-key isolated per profile
  Usage            /llm [list|import|import-env|use|remove|clear-active|limits|help]
  Import inline    /llm import <name> <provider> <model> <api-key> [base-url]
  Import env key   /llm import-env <name> <provider> <model> <api-key-env> [base-url]
  Legacy aliases   /llm add | /llm add-env
  Use profile      /llm use <name>
  Remove profile   /llm remove <name>
  Show limits      /llm limits
  Set limits       /llm limits set --min-input <n> --max-input <n> --min-output <n> --max-output <n>
  Clear limits     /llm limits clear
  Pricing          /cost plus automatic turn token/cost report after each answer
  Providers        anthropic | deepseek | openai | xai | compat",
        active_profile.unwrap_or("(none)"),
        limits.summary_line(),
    )
}

pub fn render_thread_help(active_thread: Option<&str>) -> String {
    format!(
        "Threads
  Active thread    {}
  Usage            /thread [list|add|switch|remove|help]
  Add thread       /thread add <name> <folder> [description]
  Switch thread    /thread switch <name>
  Remove thread    /thread remove <name>
  Purpose          bind a folder + session so you can jump contexts quickly
  Skills           /skills list | /skills install <path>
  Apps             /app list | /app add <name> <command> [description]",
        active_thread.unwrap_or("(none)")
    )
}

pub fn render_app_help() -> String {
    "Apps
  Usage            /app [list|add|remove|run|help]
  Add app          /app add <name> <command> [description]
  Run app          /app run <name>
  Remove app       /app remove <name>
  Notes            command can be a slash command (/review) or a prompt template
  Skills           use /skills list and /skills install <path> to add skills
  Threading        pair with /thread add to create folder-scoped workflows"
        .to_string()
}
