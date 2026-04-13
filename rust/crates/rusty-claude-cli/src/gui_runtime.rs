use std::collections::BTreeSet;
use std::env;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use api::{
    detect_provider_kind, AnthropicClient, ContentBlockDelta, InputContentBlock, InputMessage,
    MessageRequest, MessageResponse, OpenAiCompatClient, OpenAiCompatConfig, OutputContentBlock,
    PromptCache, ProviderClient, ProviderKind, StreamEvent as ApiStreamEvent, ToolChoice,
    ToolResultContentBlock,
};
use runtime::{
    load_system_prompt, ApiClient, ApiRequest, AssistantEvent, ContentBlock, ConversationMessage,
    ConversationRuntime, MessageRole, PermissionMode, PermissionPolicy, PromptCacheEvent,
    RuntimeError, Session, TokenUsage, ToolError, ToolExecutor, TurnSummary,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tools::GlobalToolRegistry;

use crate::llm_layer::{estimate_text_tokens, LlmProfile, TurnTokenLimits};

pub const GUI_CANCELLED_MESSAGE: &str = "__GUI_CANCELLED__";

#[derive(Debug, Clone)]
pub struct GuiTurnConfig {
    pub workspace: PathBuf,
    pub session: Session,
    pub session_path: PathBuf,
    pub prompt: String,
    pub model: String,
    pub llm_profile: Option<LlmProfile>,
    pub turn_token_limits: TurnTokenLimits,
    pub cancel_flag: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub enum GuiWorkerEvent {
    AssistantDelta(String),
    ToolCallRequested {
        name: String,
        input: String,
    },
    ToolResult {
        name: String,
        output: String,
        is_error: bool,
        status: Option<String>,
        handoff_command: Option<String>,
        handoff_reason: Option<String>,
    },
    PromptCache(PromptCacheEvent),
    Usage(TokenUsage),
    Completed {
        session: Session,
        summary: TurnSummary,
        cumulative_usage: TokenUsage,
    },
    Failed {
        message: String,
        session: Session,
    },
}

const DEFAULT_DATE: &str = "2026-03-31";
const POST_TOOL_STALL_TIMEOUT: Duration = Duration::from_secs(10);
const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(150);
const CANCELLED_TOOL_MESSAGE: &str = "tool execution cancelled by pause request";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ToolResultMetadata {
    status: Option<String>,
    handoff_command: Option<String>,
    handoff_reason: Option<String>,
}
const MODEL_SUMMARY_SNIPPET_CHARS: usize = 160;

#[derive(Debug, Deserialize)]
struct ToolSearchRequest {
    query: String,
    max_results: Option<usize>,
}

pub fn spawn_turn(config: GuiTurnConfig, tx: Sender<GuiWorkerEvent>) {
    thread::spawn(move || {
        let _ = tx.send(GuiWorkerEvent::Usage(TokenUsage::default()));
        match run_turn(config, &tx) {
            Ok((session, summary, cumulative_usage)) => {
                let _ = tx.send(GuiWorkerEvent::Completed {
                    session,
                    summary,
                    cumulative_usage,
                });
            }
            Err((message, session)) => {
                let _ = tx.send(GuiWorkerEvent::Failed { message, session });
            }
        }
    });
}

#[allow(clippy::result_large_err)]
fn run_turn(
    config: GuiTurnConfig,
    tx: &Sender<GuiWorkerEvent>,
) -> Result<(Session, TurnSummary, TokenUsage), (String, Session)> {
    if let Err(error) = env::set_current_dir(&config.workspace) {
        return Err((
            format!("failed to switch workspace: {error}"),
            config.session,
        ));
    }

    let estimated_input_tokens = estimate_text_tokens(&config.prompt);
    if let Err(error) = config
        .turn_token_limits
        .check_input_estimate(estimated_input_tokens)
    {
        return Err((error, config.session));
    }

    let session = config
        .session
        .with_persistence_path(config.session_path.clone())
        .with_workspace_root(config.workspace.clone());

    let tool_registry = GlobalToolRegistry::builtin();
    let policy = permission_policy(PermissionMode::DangerFullAccess, &tool_registry)
        .map_err(|error| (error, session.clone()))?;
    let system_prompt = build_system_prompt(
        &config.workspace,
        &config.model,
        config.llm_profile.as_ref(),
    )
    .map_err(|error| (error, session.clone()))?;

    let client = GuiProviderRuntimeClient::new(
        tx.clone(),
        session.session_id.clone(),
        config.model.clone(),
        tool_registry.clone(),
        config.llm_profile.clone(),
        config.turn_token_limits,
        config.cancel_flag.clone(),
    )
    .map_err(|error| (error, session.clone()))?;
    let executor = GuiToolExecutor::new(
        tx.clone(),
        tool_registry.clone(),
        config.cancel_flag.clone(),
    );
    let mut runtime = ConversationRuntime::new(session, client, executor, policy, system_prompt);
    let _ = runtime
        .session_mut()
        .push_prompt_entry(config.prompt.clone());

    match runtime.run_turn(&config.prompt, None) {
        Ok(summary) => {
            let session = runtime.session().clone();
            let cumulative_usage = runtime.usage().cumulative_usage();
            let _ = session.save_to_path(&config.session_path);
            Ok((session, summary, cumulative_usage))
        }
        Err(error) => {
            let session = runtime.session().clone();
            let _ = session.save_to_path(&config.session_path);
            if config.cancel_flag.load(Ordering::Relaxed)
                || error.to_string().contains(GUI_CANCELLED_MESSAGE)
            {
                Err((GUI_CANCELLED_MESSAGE.to_string(), session))
            } else {
                Err((error.to_string(), session))
            }
        }
    }
}

fn build_system_prompt(
    workspace: &Path,
    model: &str,
    profile: Option<&LlmProfile>,
) -> Result<Vec<String>, String> {
    let mut prompt = load_system_prompt(
        workspace.to_path_buf(),
        DEFAULT_DATE,
        env::consts::OS,
        "unknown",
    )
    .map_err(|error| error.to_string())?;
    append_connection_prompt_details(&mut prompt, model, profile);
    Ok(prompt)
}

fn append_connection_prompt_details(
    prompt: &mut Vec<String>,
    model: &str,
    profile: Option<&LlmProfile>,
) {
    let provider = profile.map_or_else(
        || provider_label(detect_provider_kind(model)).to_string(),
        LlmProfile::normalized_provider,
    );
    let base_url = profile
        .and_then(|value| value.base_url.clone())
        .unwrap_or_else(|| "(provider default)".to_string());
    prompt.push(format!(
        "# Active model connection\n - Runtime shell: Claw GUI.\n - Connected provider: {provider}.\n - Connected model: {model}.\n - Connected base URL: {base_url}.\n - If the user asks which API or model is in use, answer with the connected provider/model above and do not claim to be Claude unless the connected provider is Anthropic."
    ));
}

fn provider_label(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Anthropic => "anthropic",
        ProviderKind::Xai => "xai",
        ProviderKind::OpenAi => "openai",
        ProviderKind::DeepSeek => "deepseek",
    }
}

fn permission_policy(
    mode: PermissionMode,
    tool_registry: &GlobalToolRegistry,
) -> Result<PermissionPolicy, String> {
    Ok(tool_registry.permission_specs(None)?.into_iter().fold(
        PermissionPolicy::new(mode),
        |policy, (name, required_permission)| {
            policy.with_tool_requirement(name, required_permission)
        },
    ))
}

fn provider_client_from_llm_profile(profile: &LlmProfile) -> Result<ProviderClient, String> {
    let provider = profile.normalized_provider();
    let api_key = profile.resolved_api_key()?;
    let base_url = profile.base_url.clone();

    let client = match provider.as_str() {
        "anthropic" => {
            let client = base_url.map_or_else(
                || AnthropicClient::new(api_key.clone()),
                |url| AnthropicClient::new(api_key.clone()).with_base_url(url),
            );
            ProviderClient::Anthropic(client)
        }
        "xai" => {
            let client = base_url.map_or_else(
                || OpenAiCompatClient::new(api_key.clone(), OpenAiCompatConfig::xai()),
                |url| {
                    OpenAiCompatClient::new(api_key.clone(), OpenAiCompatConfig::xai())
                        .with_base_url(url)
                },
            );
            ProviderClient::Xai(client)
        }
        "deepseek" => {
            let client = base_url.map_or_else(
                || OpenAiCompatClient::new(api_key.clone(), OpenAiCompatConfig::deepseek()),
                |url| {
                    OpenAiCompatClient::new(api_key.clone(), OpenAiCompatConfig::deepseek())
                        .with_base_url(url)
                },
            );
            ProviderClient::DeepSeek(client)
        }
        "qwen" => {
            let client = base_url.map_or_else(
                || OpenAiCompatClient::new(api_key.clone(), OpenAiCompatConfig::qwen()),
                |url| {
                    OpenAiCompatClient::new(api_key.clone(), OpenAiCompatConfig::qwen())
                        .with_base_url(url)
                },
            );
            ProviderClient::OpenAi(client)
        }
        "openai" | "compat" => {
            let client = base_url.map_or_else(
                || OpenAiCompatClient::new(api_key.clone(), OpenAiCompatConfig::openai()),
                |url| {
                    OpenAiCompatClient::new(api_key.clone(), OpenAiCompatConfig::openai())
                        .with_base_url(url)
                },
            );
            ProviderClient::OpenAi(client)
        }
        other => {
            return Err(format!(
                "unsupported llm provider '{}' for profile '{}'",
                other, profile.name
            ));
        }
    };

    Ok(client)
}

fn convert_messages(messages: &[ConversationMessage]) -> Vec<InputMessage> {
    let sanitized_messages = sanitize_messages_for_api(messages);
    let mut known_tool_use_ids = BTreeSet::new();
    let mut converted = Vec::new();

    for message in &sanitized_messages {
        let role = match message.role {
            MessageRole::System | MessageRole::User | MessageRole::Tool => "user",
            MessageRole::Assistant => "assistant",
        };
        let mut content = Vec::new();
        for block in &message.blocks {
            match block {
                ContentBlock::Text { text } => {
                    content.push(InputContentBlock::Text { text: text.clone() });
                }
                ContentBlock::ToolUse { id, name, input } => {
                    known_tool_use_ids.insert(id.clone());
                    content.push(InputContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        // 中文注释：工具协议层必须保留原始 schema，避免摘要 JSON 污染 tool_use 参数。
                        input: model_tool_call_input_for_api(name, input),
                    });
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    tool_name,
                    output,
                    is_error,
                    ..
                } => {
                    if known_tool_use_ids.contains(tool_use_id) {
                        content.push(InputContentBlock::ToolResult {
                            tool_use_id: tool_use_id.clone(),
                            content: vec![ToolResultContentBlock::Text {
                                text: summarize_tool_result_output_for_model(
                                    tool_name, output, *is_error,
                                ),
                            }],
                            is_error: *is_error,
                        });
                    } else {
                        // 修复历史脏数据：若缺少前序 tool_use，降级为普通文本，避免向兼容 API 发送非法 role=tool。
                        content.push(InputContentBlock::Text {
                            text: format!(
                                "[unpaired tool result {tool_use_id}] {}",
                                summarize_tool_result_output_for_model(
                                    tool_name, output, *is_error
                                )
                            ),
                        });
                    }
                }
            }
        }
        if !content.is_empty() {
            converted.push(InputMessage {
                role: role.to_string(),
                content,
            });
        }
    }

    converted
}

// 中文注释：模型上下文只保留工具调用摘要，原始命令参数只在 GUI 中展示，降低 token 与敏感信息回灌风险。
fn model_tool_call_input_for_api(_tool_name: &str, raw_input: &str) -> Value {
    serde_json::from_str::<Value>(raw_input).unwrap_or_else(|_| {
        json!({
            "raw_input": truncate_for_model(raw_input, MODEL_SUMMARY_SNIPPET_CHARS)
        })
    })
}

// 中文注释：仅用于文本摘要（中断/孤儿工具消息），不参与真实 tool_use 协议参数。
fn summarize_tool_call_input_for_model(tool_name: &str, raw_input: &str) -> String {
    let parsed = serde_json::from_str::<Value>(raw_input).ok();
    let lower_name = tool_name.to_ascii_lowercase();
    let summary = if is_shell_like_tool(&lower_name) {
        "shell command executed (command text hidden; see GUI tool events)".to_string()
    } else if let Some(path) = parsed
        .as_ref()
        .and_then(|value| json_string_field(value, &["path", "filePath", "file", "directory"]))
    {
        format!("target path: {}", truncate_for_model(&path, 120))
    } else if let Some(query) = parsed
        .as_ref()
        .and_then(|value| json_string_field(value, &["query", "q", "pattern", "keyword"]))
    {
        format!("query: {}", truncate_for_model(&query, 120))
    } else {
        format!("tool `{tool_name}` called; full input hidden in GUI")
    };

    summary
}

// 中文注释：工具结果发送给模型时进行结构化压缩，保留状态与关键片段，避免完整 stdout/stderr 占满上下文。
fn summarize_tool_result_output_for_model(tool_name: &str, output: &str, is_error: bool) -> String {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return format!("tool `{tool_name}` returned empty output");
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if let Some(summary) = summarize_tool_result_json_for_model(tool_name, &value, is_error) {
            return summary;
        }
    }

    let status = if is_error { "error" } else { "ok" };
    format!(
        "tool `{tool_name}` {status}: {}",
        truncate_for_model(trimmed, MODEL_SUMMARY_SNIPPET_CHARS)
    )
}

fn summarize_tool_result_json_for_model(
    tool_name: &str,
    value: &Value,
    is_error: bool,
) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(status) = json_string_field(value, &["status"]) {
        parts.push(format!("status={status}"));
    }
    if let Some(exit_code) = json_i64_field(value, &["exit_code", "exitCode", "code"]) {
        parts.push(format!("exit_code={exit_code}"));
    }
    if let Some(stdout) = json_string_field(value, &["stdout", "output", "message"]) {
        parts.push(format!(
            "stdout={}",
            truncate_for_model(&stdout, MODEL_SUMMARY_SNIPPET_CHARS)
        ));
    }
    if let Some(stderr) = json_string_field(value, &["stderr", "error", "errors"]) {
        parts.push(format!(
            "stderr={}",
            truncate_for_model(&stderr, MODEL_SUMMARY_SNIPPET_CHARS)
        ));
    }
    if let Some(path) = json_string_field(value, &["path", "filePath", "file"]) {
        parts.push(format!("path={}", truncate_for_model(&path, 120)));
    }
    if parts.is_empty() {
        return None;
    }
    let status = if is_error { "error" } else { "ok" };
    Some(format!(
        "tool `{tool_name}` {status}: {}",
        parts.join(" | ")
    ))
}

fn json_string_field(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(text) = value.get(*key).and_then(Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn json_i64_field(value: &Value, keys: &[&str]) -> Option<i64> {
    for key in keys {
        if let Some(number) = value.get(*key).and_then(Value::as_i64) {
            return Some(number);
        }
    }
    None
}

fn truncate_for_model(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let mut collected = String::new();
    for _ in 0..limit {
        if let Some(ch) = chars.next() {
            collected.push(ch);
        } else {
            return collected;
        }
    }
    if chars.next().is_some() {
        collected.push_str("...");
    }
    collected
}

fn is_shell_like_tool(tool_name: &str) -> bool {
    ["shell", "command", "bash", "powershell", "terminal"]
        .iter()
        .any(|part| tool_name.contains(part))
}

fn sanitize_messages_for_api(messages: &[ConversationMessage]) -> Vec<ConversationMessage> {
    let mut sanitized = Vec::new();
    let mut pending_assistant: Option<ConversationMessage> = None;
    let mut pending_tool_messages = Vec::<ConversationMessage>::new();
    let mut pending_tool_use_ids = BTreeSet::<String>::new();

    for message in messages {
        if pending_assistant.is_some() && message.role != MessageRole::Tool {
            flush_incomplete_tool_turn(
                &mut sanitized,
                &mut pending_assistant,
                &mut pending_tool_messages,
                &mut pending_tool_use_ids,
            );
        }

        match message.role {
            MessageRole::Assistant => {
                let tool_use_ids = message
                    .blocks
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::ToolUse { id, .. } => Some(id.clone()),
                        ContentBlock::Text { .. } | ContentBlock::ToolResult { .. } => None,
                    })
                    .collect::<BTreeSet<_>>();
                if tool_use_ids.is_empty() {
                    sanitized.push(message.clone());
                } else {
                    pending_assistant = Some(message.clone());
                    pending_tool_messages.clear();
                    pending_tool_use_ids = tool_use_ids;
                }
            }
            MessageRole::Tool => {
                if pending_assistant.is_none() {
                    sanitized.push(downgrade_tool_message(message.clone()));
                    continue;
                }

                let matched_blocks = message
                    .blocks
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::ToolResult { tool_use_id, .. }
                            if pending_tool_use_ids.contains(tool_use_id) =>
                        {
                            pending_tool_use_ids.remove(tool_use_id);
                            Some(block.clone())
                        }
                        ContentBlock::Text { text } if !text.trim().is_empty() => {
                            Some(block.clone())
                        }
                        ContentBlock::ToolUse { .. }
                        | ContentBlock::ToolResult { .. }
                        | ContentBlock::Text { .. } => None,
                    })
                    .collect::<Vec<_>>();
                if !matched_blocks.is_empty() {
                    pending_tool_messages.push(ConversationMessage {
                        role: MessageRole::Tool,
                        blocks: matched_blocks,
                        usage: message.usage,
                    });
                }
                if pending_tool_use_ids.is_empty() {
                    flush_completed_tool_turn(
                        &mut sanitized,
                        &mut pending_assistant,
                        &mut pending_tool_messages,
                    );
                }
            }
            MessageRole::System | MessageRole::User => sanitized.push(message.clone()),
        }
    }

    if pending_assistant.is_some() {
        flush_incomplete_tool_turn(
            &mut sanitized,
            &mut pending_assistant,
            &mut pending_tool_messages,
            &mut pending_tool_use_ids,
        );
    }

    sanitized
}

fn flush_completed_tool_turn(
    sanitized: &mut Vec<ConversationMessage>,
    pending_assistant: &mut Option<ConversationMessage>,
    pending_tool_messages: &mut Vec<ConversationMessage>,
) {
    if let Some(message) = pending_assistant.take() {
        sanitized.push(message);
    }
    sanitized.append(pending_tool_messages);
}

fn flush_incomplete_tool_turn(
    sanitized: &mut Vec<ConversationMessage>,
    pending_assistant: &mut Option<ConversationMessage>,
    pending_tool_messages: &mut Vec<ConversationMessage>,
    pending_tool_use_ids: &mut BTreeSet<String>,
) {
    if let Some(message) = pending_assistant.take() {
        sanitized.push(downgrade_assistant_tool_message(message));
    }
    sanitized.extend(pending_tool_messages.drain(..).map(downgrade_tool_message));
    pending_tool_use_ids.clear();
}

fn downgrade_assistant_tool_message(message: ConversationMessage) -> ConversationMessage {
    let mut downgraded_blocks = Vec::new();
    for block in message.blocks {
        match block {
            ContentBlock::Text { text } if !text.trim().is_empty() => {
                downgraded_blocks.push(ContentBlock::Text { text });
            }
            ContentBlock::ToolUse { name, input, .. } => {
                downgraded_blocks.push(ContentBlock::Text {
                    text: format!(
                        "[interrupted tool call] {name} {}",
                        summarize_tool_call_input_for_model(&name, &input)
                    ),
                });
            }
            ContentBlock::ToolResult {
                tool_name, output, ..
            } => {
                downgraded_blocks.push(ContentBlock::Text {
                    text: format!(
                        "[interrupted tool result] {}",
                        summarize_tool_result_output_for_model(&tool_name, &output, false)
                    ),
                });
            }
            ContentBlock::Text { .. } => {}
        }
    }
    if downgraded_blocks.is_empty() {
        downgraded_blocks.push(ContentBlock::Text {
            text: "[interrupted assistant tool turn]".to_string(),
        });
    }
    ConversationMessage {
        role: MessageRole::Assistant,
        blocks: downgraded_blocks,
        usage: message.usage,
    }
}

fn downgrade_tool_message(message: ConversationMessage) -> ConversationMessage {
    let mut downgraded_blocks = Vec::new();
    for block in message.blocks {
        match block {
            ContentBlock::ToolResult {
                tool_name,
                output,
                is_error,
                ..
            } => {
                let prefix = if is_error {
                    "[orphan tool error]"
                } else {
                    "[orphan tool result]"
                };
                downgraded_blocks.push(ContentBlock::Text {
                    text: format!(
                        "{prefix} {}",
                        summarize_tool_result_output_for_model(&tool_name, &output, is_error)
                    ),
                });
            }
            ContentBlock::Text { text } if !text.trim().is_empty() => {
                downgraded_blocks.push(ContentBlock::Text { text });
            }
            ContentBlock::ToolUse { name, input, .. } => {
                downgraded_blocks.push(ContentBlock::Text {
                    text: format!(
                        "[orphan tool call] {name} {}",
                        summarize_tool_call_input_for_model(&name, &input)
                    ),
                });
            }
            ContentBlock::Text { .. } => {}
        }
    }
    if downgraded_blocks.is_empty() {
        downgraded_blocks.push(ContentBlock::Text {
            text: "[orphan tool message]".to_string(),
        });
    }
    ConversationMessage {
        role: MessageRole::Tool,
        blocks: downgraded_blocks,
        usage: message.usage,
    }
}

struct GuiToolExecutor {
    tx: Sender<GuiWorkerEvent>,
    tool_registry: GlobalToolRegistry,
    cancel_flag: Arc<AtomicBool>,
}

impl GuiToolExecutor {
    fn new(
        tx: Sender<GuiWorkerEvent>,
        tool_registry: GlobalToolRegistry,
        cancel_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            tx,
            tool_registry,
            cancel_flag,
        }
    }
}

fn execute_registry_tool(
    tool_registry: &GlobalToolRegistry,
    tool_name: &str,
    value: serde_json::Value,
) -> Result<String, ToolError> {
    if tool_name == "ToolSearch" {
        let request: ToolSearchRequest = serde_json::from_value(value)
            .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
        serde_json::to_string_pretty(&tool_registry.search(
            &request.query,
            request.max_results.unwrap_or(5),
            None,
            None,
        ))
        .map_err(|error| ToolError::new(error.to_string()))
    } else {
        tool_registry
            .execute(tool_name, &value)
            .map_err(ToolError::new)
    }
}

fn send_tool_result_event(
    tx: &Sender<GuiWorkerEvent>,
    tool_name: &str,
    output: &str,
    is_error: bool,
) {
    let metadata = tool_result_metadata(output);
    let _ = tx.send(GuiWorkerEvent::ToolResult {
        name: tool_name.to_string(),
        output: output.to_string(),
        is_error,
        status: metadata.status,
        handoff_command: metadata.handoff_command,
        handoff_reason: metadata.handoff_reason,
    });
}

fn tool_result_metadata(output: &str) -> ToolResultMetadata {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(output) else {
        return ToolResultMetadata::default();
    };

    let mut metadata = ToolResultMetadata {
        status: parsed
            .get("returnCodeInterpretation")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        ..ToolResultMetadata::default()
    };

    if let Some(items) = parsed
        .get("structuredContent")
        .and_then(serde_json::Value::as_array)
    {
        for item in items {
            if item.get("kind").and_then(serde_json::Value::as_str)
                == Some("requires_terminal_handoff")
            {
                metadata.handoff_command = item
                    .get("command")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                metadata.handoff_reason = item
                    .get("reason")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                break;
            }
        }
    }

    metadata
}

async fn await_with_cancel<T, E, F>(
    cancel_flag: &Arc<AtomicBool>,
    future: F,
) -> Result<T, RuntimeError>
where
    F: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    tokio::pin!(future);
    loop {
        if cancel_flag.load(Ordering::Relaxed) {
            return Err(RuntimeError::new(GUI_CANCELLED_MESSAGE));
        }
        if let Ok(result) = tokio::time::timeout(CANCEL_POLL_INTERVAL, &mut future).await {
            return result.map_err(|error| RuntimeError::new(error.to_string()));
        }
    }
}

impl ToolExecutor for GuiToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        if self.cancel_flag.load(Ordering::Relaxed) {
            return Err(ToolError::cancelled(CANCELLED_TOOL_MESSAGE));
        }
        let value = serde_json::from_str(input)
            .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
        let tool_registry = self.tool_registry.clone();
        let tool_name_owned = tool_name.to_string();
        let (result_tx, result_rx) = mpsc::channel();
        thread::spawn(move || {
            let result = execute_registry_tool(&tool_registry, &tool_name_owned, value);
            let _ = result_tx.send(result);
        });

        loop {
            if self.cancel_flag.load(Ordering::Relaxed) {
                send_tool_result_event(&self.tx, tool_name, CANCELLED_TOOL_MESSAGE, true);
                return Err(ToolError::cancelled(CANCELLED_TOOL_MESSAGE));
            }
            match result_rx.recv_timeout(CANCEL_POLL_INTERVAL) {
                Ok(result) => match result {
                    Ok(output) => {
                        if self.cancel_flag.load(Ordering::Relaxed) {
                            send_tool_result_event(
                                &self.tx,
                                tool_name,
                                CANCELLED_TOOL_MESSAGE,
                                true,
                            );
                            return Err(ToolError::cancelled(CANCELLED_TOOL_MESSAGE));
                        }
                        send_tool_result_event(&self.tx, tool_name, &output, false);
                        return Ok(output);
                    }
                    Err(error) => {
                        send_tool_result_event(&self.tx, tool_name, &error.to_string(), true);
                        return Err(error);
                    }
                },
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(ToolError::new("tool execution worker disconnected"));
                }
            }
        }
    }
}

struct GuiProviderRuntimeClient {
    runtime: tokio::runtime::Runtime,
    client: ProviderClient,
    session_id: String,
    model: String,
    tool_registry: GlobalToolRegistry,
    tx: Sender<GuiWorkerEvent>,
    turn_token_limits: TurnTokenLimits,
    cancel_flag: Arc<AtomicBool>,
}

impl GuiProviderRuntimeClient {
    fn new(
        tx: Sender<GuiWorkerEvent>,
        session_id: String,
        model: String,
        tool_registry: GlobalToolRegistry,
        llm_profile: Option<LlmProfile>,
        turn_token_limits: TurnTokenLimits,
        cancel_flag: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let client = if let Some(profile) = llm_profile {
            provider_client_from_llm_profile(&profile)?
        } else {
            ProviderClient::from_model(&model).map_err(|error| error.to_string())?
        }
        .with_prompt_cache(PromptCache::new(&session_id));

        Ok(Self {
            runtime: tokio::runtime::Runtime::new().map_err(|error| error.to_string())?,
            client,
            session_id,
            model,
            tool_registry,
            tx,
            turn_token_limits,
            cancel_flag,
        })
    }

    #[allow(clippy::too_many_lines)]
    async fn consume_stream(
        &self,
        message_request: &MessageRequest,
        apply_stall_timeout: bool,
    ) -> Result<Vec<AssistantEvent>, RuntimeError> {
        let stream_future = self.client.stream_message(message_request);
        tokio::pin!(stream_future);
        let mut stream = loop {
            if self.cancel_flag.load(Ordering::Relaxed) {
                return Err(RuntimeError::new(GUI_CANCELLED_MESSAGE));
            }
            if let Ok(result) = tokio::time::timeout(CANCEL_POLL_INTERVAL, &mut stream_future).await
            {
                break result.map_err(|error| RuntimeError::new(error.to_string()))?;
            }
        };
        let mut events = Vec::new();
        let mut pending_tool: Option<(String, String, String)> = None;
        let mut saw_stop = false;
        let mut received_any_event = false;
        let stall_deadline =
            apply_stall_timeout.then(|| tokio::time::Instant::now() + POST_TOOL_STALL_TIMEOUT);

        loop {
            if self.cancel_flag.load(Ordering::Relaxed) {
                return Err(RuntimeError::new(GUI_CANCELLED_MESSAGE));
            }
            let next = if let Some(deadline) = stall_deadline.filter(|_| !received_any_event) {
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                if remaining.is_zero() {
                    return Err(RuntimeError::new(
                        "post-tool stall: model did not respond within timeout",
                    ));
                }
                match tokio::time::timeout(remaining.min(CANCEL_POLL_INTERVAL), stream.next_event())
                    .await
                {
                    Ok(inner) => inner.map_err(|error| RuntimeError::new(error.to_string()))?,
                    Err(_) => continue,
                }
            } else {
                match tokio::time::timeout(CANCEL_POLL_INTERVAL, stream.next_event()).await {
                    Ok(inner) => inner.map_err(|error| RuntimeError::new(error.to_string()))?,
                    Err(_) => continue,
                }
            };

            let Some(event) = next else {
                break;
            };
            received_any_event = true;

            match event {
                ApiStreamEvent::MessageStart(start) => {
                    for block in start.message.content {
                        push_output_block(&self.tx, block, &mut events, &mut pending_tool, true);
                    }
                }
                ApiStreamEvent::ContentBlockStart(start) => {
                    push_output_block(
                        &self.tx,
                        start.content_block,
                        &mut events,
                        &mut pending_tool,
                        true,
                    );
                }
                ApiStreamEvent::ContentBlockDelta(delta) => match delta.delta {
                    ContentBlockDelta::TextDelta { text } => {
                        if !text.is_empty() {
                            let _ = self.tx.send(GuiWorkerEvent::AssistantDelta(text.clone()));
                            events.push(AssistantEvent::TextDelta(text));
                        }
                    }
                    ContentBlockDelta::InputJsonDelta { partial_json } => {
                        if let Some((_, _, input)) = &mut pending_tool {
                            input.push_str(&partial_json);
                        }
                    }
                    ContentBlockDelta::ThinkingDelta { .. }
                    | ContentBlockDelta::SignatureDelta { .. } => {}
                },
                ApiStreamEvent::ContentBlockStop(_) => {
                    if let Some((id, name, input)) = pending_tool.take() {
                        let _ = self.tx.send(GuiWorkerEvent::ToolCallRequested {
                            name: name.clone(),
                            input: input.clone(),
                        });
                        events.push(AssistantEvent::ToolUse { id, name, input });
                    }
                }
                ApiStreamEvent::MessageDelta(delta) => {
                    let usage = delta.usage.token_usage();
                    let _ = self.tx.send(GuiWorkerEvent::Usage(usage));
                    events.push(AssistantEvent::Usage(usage));
                }
                ApiStreamEvent::MessageStop(_) => {
                    saw_stop = true;
                    events.push(AssistantEvent::MessageStop);
                }
            }
        }

        push_prompt_cache_record(&self.client, &self.tx, &mut events);

        if !saw_stop
            && events.iter().any(|event| {
                matches!(event, AssistantEvent::TextDelta(text) if !text.is_empty())
                    || matches!(event, AssistantEvent::ToolUse { .. })
            })
        {
            events.push(AssistantEvent::MessageStop);
        }

        if events
            .iter()
            .any(|event| matches!(event, AssistantEvent::MessageStop))
        {
            return Ok(events);
        }

        let response = await_with_cancel(
            &self.cancel_flag,
            self.client.send_message(&MessageRequest {
                stream: false,
                ..message_request.clone()
            }),
        )
        .await?;
        if self.cancel_flag.load(Ordering::Relaxed) {
            return Err(RuntimeError::new(GUI_CANCELLED_MESSAGE));
        }
        let mut fallback_events = response_to_events(response);
        for event in &fallback_events {
            relay_runtime_event(&self.tx, event);
        }
        push_prompt_cache_record(&self.client, &self.tx, &mut fallback_events);
        Ok(fallback_events)
    }
}

impl ApiClient for GuiProviderRuntimeClient {
    fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        let converted_messages = convert_messages(&request.messages);
        let is_post_tool = request_ends_with_tool_result_messages(&converted_messages);
        let model_default_max_tokens = api::max_tokens_for_model(&self.model);
        let message_request = MessageRequest {
            model: self.model.clone(),
            max_tokens: self
                .turn_token_limits
                .effective_output_max(model_default_max_tokens),
            messages: converted_messages,
            system: (!request.system_prompt.is_empty()).then(|| request.system_prompt.join("\n\n")),
            tools: Some(self.tool_registry.definitions(None)),
            tool_choice: Some(ToolChoice::Auto),
            stream: true,
            ..Default::default()
        };

        self.runtime.block_on(async {
            let max_attempts: usize = if is_post_tool { 2 } else { 1 };
            for attempt in 1..=max_attempts {
                match self
                    .consume_stream(&message_request, is_post_tool && attempt == 1)
                    .await
                {
                    Ok(events) => return Ok(events),
                    Err(error)
                        if error.to_string().contains("post-tool stall")
                            && attempt < max_attempts => {}
                    Err(error) => {
                        return Err(RuntimeError::new(format!(
                            "session {}: {}",
                            self.session_id, error
                        )));
                    }
                }
            }
            Err(RuntimeError::new("post-tool continuation nudge exhausted"))
        })
    }
}

fn push_output_block(
    tx: &Sender<GuiWorkerEvent>,
    block: OutputContentBlock,
    events: &mut Vec<AssistantEvent>,
    pending_tool: &mut Option<(String, String, String)>,
    streaming_tool_input: bool,
) {
    match block {
        OutputContentBlock::Text { text } => {
            if !text.is_empty() {
                let _ = tx.send(GuiWorkerEvent::AssistantDelta(text.clone()));
                events.push(AssistantEvent::TextDelta(text));
            }
        }
        OutputContentBlock::ToolUse { id, name, input } => {
            let initial_input = if streaming_tool_input
                && input.is_object()
                && input.as_object().is_some_and(serde_json::Map::is_empty)
            {
                String::new()
            } else {
                input.to_string()
            };
            *pending_tool = Some((id, name, initial_input));
        }
        OutputContentBlock::Thinking { .. } | OutputContentBlock::RedactedThinking { .. } => {}
    }
}

fn response_to_events(response: MessageResponse) -> Vec<AssistantEvent> {
    let mut events = Vec::new();
    let mut pending_tool = None;
    for block in response.content {
        push_output_block(
            &dummy_sender(),
            block,
            &mut events,
            &mut pending_tool,
            false,
        );
        if let Some((id, name, input)) = pending_tool.take() {
            events.push(AssistantEvent::ToolUse { id, name, input });
        }
    }
    events.push(AssistantEvent::Usage(response.usage.token_usage()));
    events.push(AssistantEvent::MessageStop);
    events
}

fn dummy_sender() -> Sender<GuiWorkerEvent> {
    let (tx, _rx) = std::sync::mpsc::channel();
    tx
}

fn relay_runtime_event(tx: &Sender<GuiWorkerEvent>, event: &AssistantEvent) {
    match event {
        AssistantEvent::TextDelta(text) => {
            let _ = tx.send(GuiWorkerEvent::AssistantDelta(text.clone()));
        }
        AssistantEvent::ToolUse { name, input, .. } => {
            let _ = tx.send(GuiWorkerEvent::ToolCallRequested {
                name: name.clone(),
                input: input.clone(),
            });
        }
        AssistantEvent::Usage(usage) => {
            let _ = tx.send(GuiWorkerEvent::Usage(*usage));
        }
        AssistantEvent::PromptCache(event) => {
            let _ = tx.send(GuiWorkerEvent::PromptCache(event.clone()));
        }
        AssistantEvent::MessageStop => {}
    }
}

fn push_prompt_cache_record(
    client: &ProviderClient,
    tx: &Sender<GuiWorkerEvent>,
    events: &mut Vec<AssistantEvent>,
) {
    if let Some(record) = client.take_last_prompt_cache_record() {
        if let Some(event) = record.cache_break.map(|cache_break| PromptCacheEvent {
            unexpected: cache_break.unexpected,
            reason: cache_break.reason,
            previous_cache_read_input_tokens: cache_break.previous_cache_read_input_tokens,
            current_cache_read_input_tokens: cache_break.current_cache_read_input_tokens,
            token_drop: cache_break.token_drop,
        }) {
            let _ = tx.send(GuiWorkerEvent::PromptCache(event.clone()));
            events.push(AssistantEvent::PromptCache(event));
        }
    }
}

fn request_ends_with_tool_result_messages(messages: &[InputMessage]) -> bool {
    messages.last().is_some_and(|message| {
        message
            .content
            .iter()
            .any(|block| matches!(block, InputContentBlock::ToolResult { .. }))
    })
}

#[cfg(test)]
mod tests {
    use super::{convert_messages, sanitize_messages_for_api, tool_result_metadata};
    use runtime::{ContentBlock, ConversationMessage, MessageRole};
    use serde_json::json;

    #[test]
    fn drops_incomplete_tool_calls_before_api_conversion() {
        let messages = vec![
            ConversationMessage::user_text("run a tool"),
            ConversationMessage {
                role: MessageRole::Assistant,
                blocks: vec![ContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "bash".to_string(),
                    input: "{\"command\":\"echo hi\"}".to_string(),
                }],
                usage: None,
            },
            ConversationMessage::user_text("continue"),
        ];

        let sanitized = sanitize_messages_for_api(&messages);
        assert_eq!(sanitized.len(), 3);
        assert!(matches!(
            &sanitized[1].blocks[0],
            ContentBlock::Text { text } if text.contains("[interrupted tool call]")
        ));

        let converted = convert_messages(&messages);
        assert!(converted.iter().all(|message| message
            .content
            .iter()
            .all(|block| !matches!(block, api::InputContentBlock::ToolUse { .. }))));
    }

    #[test]
    fn keeps_complete_tool_turns_intact() {
        let messages = vec![
            ConversationMessage::user_text("run a tool"),
            ConversationMessage {
                role: MessageRole::Assistant,
                blocks: vec![ContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "bash".to_string(),
                    input: "{\"command\":\"echo hi\"}".to_string(),
                }],
                usage: None,
            },
            ConversationMessage::tool_result("tool-1", "bash", "hi", false),
        ];

        let converted = convert_messages(&messages);
        assert!(converted.iter().any(|message| message
            .content
            .iter()
            .any(|block| matches!(block, api::InputContentBlock::ToolUse { .. }))));
        assert!(converted.iter().any(|message| message
            .content
            .iter()
            .any(|block| matches!(block, api::InputContentBlock::ToolResult { .. }))));
    }

    #[test]
    fn preserves_tool_use_schema_in_model_payload() {
        let messages = vec![
            ConversationMessage::user_text("run bash"),
            ConversationMessage {
                role: MessageRole::Assistant,
                blocks: vec![ContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "bash".to_string(),
                    input: "{\"command\":\"echo VERY_SECRET_COMMAND\"}".to_string(),
                }],
                usage: None,
            },
            ConversationMessage::tool_result("tool-1", "bash", "ok", false),
        ];

        let converted = convert_messages(&messages);
        let serialized_input = converted
            .iter()
            .flat_map(|message| message.content.iter())
            .find_map(|block| match block {
                api::InputContentBlock::ToolUse { input, .. } => Some(input.to_string()),
                _ => None,
            })
            .expect("tool_use payload should exist");

        assert!(serialized_input.contains("\"command\""));
        assert!(serialized_input.contains("VERY_SECRET_COMMAND"));
        assert!(!serialized_input.contains("details_redacted"));
    }

    #[test]
    fn compresses_tool_result_payload_for_model_context() {
        let stdout_blob = format!("{}TAIL_MARKER", "x".repeat(600));
        let raw_output = serde_json::json!({
            "exit_code": 1,
            "stdout": stdout_blob,
            "stderr": "permission denied"
        })
        .to_string();
        let messages = vec![
            ConversationMessage::user_text("run bash"),
            ConversationMessage {
                role: MessageRole::Assistant,
                blocks: vec![ContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "bash".to_string(),
                    input: "{\"command\":\"echo test\"}".to_string(),
                }],
                usage: None,
            },
            ConversationMessage::tool_result("tool-1", "bash", raw_output.clone(), true),
        ];

        let converted = convert_messages(&messages);
        let summarized_output = converted
            .iter()
            .flat_map(|message| message.content.iter())
            .find_map(|block| match block {
                api::InputContentBlock::ToolResult { content, .. } => {
                    content.first().and_then(|item| match item {
                        api::ToolResultContentBlock::Text { text } => Some(text.clone()),
                        api::ToolResultContentBlock::Json { .. } => None,
                    })
                }
                _ => None,
            })
            .expect("tool_result payload should exist");

        assert!(summarized_output.contains("exit_code=1"));
        assert!(summarized_output.contains("stderr="));
        assert!(summarized_output.len() < raw_output.len());
        assert!(!summarized_output.contains("TAIL_MARKER"));
    }

    #[test]
    fn drops_partial_multi_tool_turns_before_api_conversion() {
        let messages = vec![
            ConversationMessage::user_text("run two tools"),
            ConversationMessage {
                role: MessageRole::Assistant,
                blocks: vec![
                    ContentBlock::ToolUse {
                        id: "tool-1".to_string(),
                        name: "bash".to_string(),
                        input: "{\"command\":\"echo one\"}".to_string(),
                    },
                    ContentBlock::ToolUse {
                        id: "tool-2".to_string(),
                        name: "bash".to_string(),
                        input: "{\"command\":\"echo two\"}".to_string(),
                    },
                ],
                usage: None,
            },
            ConversationMessage::tool_result("tool-1", "bash", "one", false),
            ConversationMessage::user_text("continue"),
        ];

        let sanitized = sanitize_messages_for_api(&messages);
        assert_eq!(sanitized.len(), 4);
        assert!(matches!(
            &sanitized[1].blocks[0],
            ContentBlock::Text { text } if text.contains("[interrupted tool call]")
        ));
        assert!(matches!(
            &sanitized[2].blocks[0],
            ContentBlock::Text { text } if text.contains("[orphan tool result]")
        ));

        let converted = convert_messages(&messages);
        assert!(converted
            .iter()
            .all(|message| message.content.iter().all(|block| !matches!(
                block,
                api::InputContentBlock::ToolUse { .. } | api::InputContentBlock::ToolResult { .. }
            ))));
    }

    #[test]
    fn extracts_terminal_handoff_metadata_from_tool_output() {
        let metadata = tool_result_metadata(
            &json!({
                "returnCodeInterpretation": "interactive_blocked",
                "structuredContent": [{
                    "kind": "requires_terminal_handoff",
                    "command": "demo.cmd",
                    "reason": "contains pause"
                }]
            })
            .to_string(),
        );

        assert_eq!(metadata.status.as_deref(), Some("interactive_blocked"));
        assert_eq!(metadata.handoff_command.as_deref(), Some("demo.cmd"));
        assert_eq!(metadata.handoff_reason.as_deref(), Some("contains pause"));
    }
}
