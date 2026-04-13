use std::env;
use std::io;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(windows)]
use encoding_rs::GB18030;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::sandbox::{
    build_linux_sandbox_command, resolve_sandbox_status_for_request, FilesystemIsolationMode,
    SandboxConfig, SandboxStatus,
};
use crate::ConfigLoader;

pub const DEFAULT_SHELL_TIMEOUT_MS: u64 = 300_000;

const INTERACTIVE_BLOCK_STATUS: &str = "interactive_blocked";
const TIMEOUT_STATUS: &str = "timeout";
const UNIX_INTERACTIVE_MARKERS: &[&str] = &[" read ", "\nread ", ";read ", " select ", "\nselect "];
const WINDOWS_INTERACTIVE_MARKERS: &[&str] = &[
    " pause ",
    "\npause ",
    "& pause",
    "&& pause",
    " set /p ",
    "\nset /p ",
    " choice ",
    "\nchoice ",
];
const POWERSHELL_INTERACTIVE_MARKERS: &[&str] = &["read-host", "-noexit"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractiveShellBlock {
    pub reason: String,
}

/// Input schema for the built-in bash execution tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BashCommandInput {
    pub command: String,
    pub timeout: Option<u64>,
    pub description: Option<String>,
    #[serde(rename = "run_in_background")]
    pub run_in_background: Option<bool>,
    #[serde(rename = "dangerouslyDisableSandbox")]
    pub dangerously_disable_sandbox: Option<bool>,
    #[serde(rename = "namespaceRestrictions")]
    pub namespace_restrictions: Option<bool>,
    #[serde(rename = "isolateNetwork")]
    pub isolate_network: Option<bool>,
    #[serde(rename = "filesystemMode")]
    pub filesystem_mode: Option<FilesystemIsolationMode>,
    #[serde(rename = "allowedMounts")]
    pub allowed_mounts: Option<Vec<String>>,
}

/// Output returned from a bash tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BashCommandOutput {
    pub stdout: String,
    pub stderr: String,
    #[serde(rename = "rawOutputPath")]
    pub raw_output_path: Option<String>,
    pub interrupted: bool,
    #[serde(rename = "isImage")]
    pub is_image: Option<bool>,
    #[serde(rename = "backgroundTaskId")]
    pub background_task_id: Option<String>,
    #[serde(rename = "backgroundedByUser")]
    pub backgrounded_by_user: Option<bool>,
    #[serde(rename = "assistantAutoBackgrounded")]
    pub assistant_auto_backgrounded: Option<bool>,
    #[serde(rename = "dangerouslyDisableSandbox")]
    pub dangerously_disable_sandbox: Option<bool>,
    #[serde(rename = "returnCodeInterpretation")]
    pub return_code_interpretation: Option<String>,
    #[serde(rename = "noOutputExpected")]
    pub no_output_expected: Option<bool>,
    #[serde(rename = "structuredContent")]
    pub structured_content: Option<Vec<serde_json::Value>>,
    #[serde(rename = "persistedOutputPath")]
    pub persisted_output_path: Option<String>,
    #[serde(rename = "persistedOutputSize")]
    pub persisted_output_size: Option<u64>,
    #[serde(rename = "sandboxStatus")]
    pub sandbox_status: Option<SandboxStatus>,
}

/// Executes a shell command with the requested sandbox settings.
pub fn execute_bash(input: BashCommandInput) -> io::Result<BashCommandOutput> {
    let cwd = env::current_dir()?;
    let sandbox_status = sandbox_status_for_input(&input, &cwd);

    // 中文注释：AI 托管执行只允许非交互脚本，命中 `pause/read/Read-Host`
    // 一类命令时直接返回终端移交信号，避免当前会话线程被外部脚本卡住。
    if let Some(block) = detect_interactive_shell_command(&input.command, &cwd) {
        return Ok(interactive_blocked_output(
            &input.command,
            &block.reason,
            Some(sandbox_status),
            input.dangerously_disable_sandbox,
        ));
    }

    if input.run_in_background.unwrap_or(false) {
        let mut child = prepare_command(&input.command, &cwd, &sandbox_status, false);
        let child = child
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        return Ok(BashCommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            raw_output_path: None,
            interrupted: false,
            is_image: None,
            background_task_id: Some(child.id().to_string()),
            backgrounded_by_user: Some(false),
            assistant_auto_backgrounded: Some(false),
            dangerously_disable_sandbox: input.dangerously_disable_sandbox,
            return_code_interpretation: None,
            no_output_expected: Some(true),
            structured_content: None,
            persisted_output_path: None,
            persisted_output_size: None,
            sandbox_status: Some(sandbox_status),
        });
    }

    execute_bash_sync(input, sandbox_status, cwd)
}

pub fn resolve_shell_timeout_ms(timeout_ms: Option<u64>) -> u64 {
    timeout_ms.unwrap_or(DEFAULT_SHELL_TIMEOUT_MS)
}

pub fn detect_interactive_shell_command(
    command: &str,
    cwd: &std::path::Path,
) -> Option<InteractiveShellBlock> {
    let normalized = normalize_command_text(command);

    if matches_any_marker(&normalized, WINDOWS_INTERACTIVE_MARKERS) {
        return Some(InteractiveShellBlock {
            reason: "detected Windows interactive command such as `pause` or `set /p`".to_string(),
        });
    }

    if matches_any_marker(&normalized, POWERSHELL_INTERACTIVE_MARKERS) {
        return Some(InteractiveShellBlock {
            reason: "detected PowerShell interactive command such as `Read-Host` or `-NoExit`"
                .to_string(),
        });
    }

    if matches_any_marker(&normalized, UNIX_INTERACTIVE_MARKERS) {
        return Some(InteractiveShellBlock {
            reason: "detected shell interactive command such as `read` or `select`".to_string(),
        });
    }

    for script_path in referenced_script_paths(command, cwd) {
        if let Some(reason) = detect_interactive_script_contents(&script_path) {
            return Some(InteractiveShellBlock {
                reason: format!(
                    "detected interactive statement in script `{}`: {reason}",
                    script_path.display()
                ),
            });
        }
    }

    None
}

pub fn interactive_blocked_output(
    command: &str,
    reason: &str,
    sandbox_status: Option<SandboxStatus>,
    dangerously_disable_sandbox: Option<bool>,
) -> BashCommandOutput {
    BashCommandOutput {
        stdout: String::new(),
        stderr: format!(
            "Interactive command blocked: {reason}. Re-run this command in a user terminal: {command}"
        ),
        raw_output_path: None,
        interrupted: false,
        is_image: None,
        background_task_id: None,
        backgrounded_by_user: None,
        assistant_auto_backgrounded: None,
        dangerously_disable_sandbox,
        return_code_interpretation: Some(INTERACTIVE_BLOCK_STATUS.to_string()),
        no_output_expected: Some(false),
        structured_content: Some(vec![json!({
            "kind": "requires_terminal_handoff",
            "command": command,
            "reason": reason,
            "suggestedAction": "run_in_user_terminal",
        })]),
        persisted_output_path: None,
        persisted_output_size: None,
        sandbox_status,
    }
}

pub fn timeout_output(
    timeout_ms: u64,
    stdout: String,
    stderr: Option<String>,
    sandbox_status: Option<SandboxStatus>,
    dangerously_disable_sandbox: Option<bool>,
) -> BashCommandOutput {
    let stderr = stderr.unwrap_or_else(|| format!("Command exceeded timeout of {timeout_ms} ms"));
    BashCommandOutput {
        stdout,
        stderr,
        raw_output_path: None,
        interrupted: true,
        is_image: None,
        background_task_id: None,
        backgrounded_by_user: None,
        assistant_auto_backgrounded: None,
        dangerously_disable_sandbox,
        return_code_interpretation: Some(TIMEOUT_STATUS.to_string()),
        no_output_expected: Some(false),
        structured_content: Some(vec![json!({
            "kind": "timed_out",
            "timeoutMs": timeout_ms,
        })]),
        persisted_output_path: None,
        persisted_output_size: None,
        sandbox_status,
    }
}

fn execute_bash_sync(
    input: BashCommandInput,
    sandbox_status: SandboxStatus,
    cwd: std::path::PathBuf,
) -> io::Result<BashCommandOutput> {
    let timeout_ms = resolve_shell_timeout_ms(input.timeout);
    let mut command = prepare_command(&input.command, &cwd, &sandbox_status, true);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let started = Instant::now();

    // 中文注释：这里统一给 shell 命令加默认超时，并在超时后主动 kill，
    // 避免 `pause` 或脚本内部等待输入时无限挂起。
    loop {
        if child.try_wait()?.is_some() {
            let output = child.wait_with_output()?;
            return Ok(finished_output(
                output,
                false,
                Some(sandbox_status),
                input.dangerously_disable_sandbox,
            ));
        }

        if started.elapsed() >= Duration::from_millis(timeout_ms) {
            let _ = child.kill();
            let output = child.wait_with_output()?;
            let stdout = truncate_output(&decode_command_output(&output.stdout));
            let stderr = truncate_output(&decode_command_output(&output.stderr));
            let stderr = if stderr.trim().is_empty() {
                None
            } else {
                Some(format!(
                    "{}\nCommand exceeded timeout of {timeout_ms} ms",
                    stderr.trim_end()
                ))
            };
            return Ok(timeout_output(
                timeout_ms,
                stdout,
                stderr,
                Some(sandbox_status),
                input.dangerously_disable_sandbox,
            ));
        }

        std::thread::sleep(Duration::from_millis(10));
    }
}

fn finished_output(
    output: std::process::Output,
    interrupted: bool,
    sandbox_status: Option<SandboxStatus>,
    dangerously_disable_sandbox: Option<bool>,
) -> BashCommandOutput {
    let stdout = truncate_output(&decode_command_output(&output.stdout));
    let stderr = truncate_output(&decode_command_output(&output.stderr));
    let no_output_expected = Some(stdout.trim().is_empty() && stderr.trim().is_empty());
    let return_code_interpretation = output.status.code().and_then(|code| {
        if code == 0 {
            None
        } else {
            Some(format!("exit_code:{code}"))
        }
    });

    BashCommandOutput {
        stdout,
        stderr,
        raw_output_path: None,
        interrupted,
        is_image: None,
        background_task_id: None,
        backgrounded_by_user: None,
        assistant_auto_backgrounded: None,
        dangerously_disable_sandbox,
        return_code_interpretation,
        no_output_expected,
        structured_content: None,
        persisted_output_path: None,
        persisted_output_size: None,
        sandbox_status,
    }
}

fn decode_command_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    // 先走 UTF-8 快路径；失败时再按平台策略兜底，减少对现有行为的影响。
    if let Ok(utf8) = std::str::from_utf8(bytes) {
        return utf8.to_string();
    }

    #[cfg(windows)]
    {
        if let Some(decoded) = decode_windows_console_bytes(bytes) {
            return decoded;
        }
    }

    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(windows)]
fn decode_windows_console_bytes(bytes: &[u8]) -> Option<String> {
    // Windows 中文环境里，cmd 输出常见为 GBK/GB18030；这里做无替换解码兜底，避免工具事件出现乱码。
    GB18030
        .decode_without_bom_handling_and_without_replacement(bytes)
        .map(std::borrow::Cow::into_owned)
}

fn sandbox_status_for_input(input: &BashCommandInput, cwd: &std::path::Path) -> SandboxStatus {
    let config = ConfigLoader::default_for(cwd).load().map_or_else(
        |_| SandboxConfig::default(),
        |runtime_config| runtime_config.sandbox().clone(),
    );
    let request = config.resolve_request(
        input.dangerously_disable_sandbox.map(|disabled| !disabled),
        input.namespace_restrictions,
        input.isolate_network,
        input.filesystem_mode,
        input.allowed_mounts.clone(),
    );
    resolve_sandbox_status_for_request(&request, cwd)
}

fn prepare_command(
    command: &str,
    cwd: &std::path::Path,
    sandbox_status: &SandboxStatus,
    create_dirs: bool,
) -> Command {
    if create_dirs {
        prepare_sandbox_dirs(cwd);
    }

    if let Some(launcher) = build_linux_sandbox_command(command, cwd, sandbox_status) {
        let mut prepared = Command::new(launcher.program);
        prepared.args(launcher.args);
        prepared.current_dir(cwd);
        prepared.envs(launcher.env);
        return prepared;
    }

    #[cfg(windows)]
    let mut prepared = {
        let mut prepared = Command::new("cmd");
        prepared.arg("/C").arg(command);
        prepared
    };

    #[cfg(not(windows))]
    let mut prepared = {
        let mut prepared = Command::new("sh");
        prepared.arg("-lc").arg(command);
        prepared
    };

    prepared.current_dir(cwd);
    if sandbox_status.filesystem_active {
        prepared.env("HOME", cwd.join(".sandbox-home"));
        prepared.env("TMPDIR", cwd.join(".sandbox-tmp"));
        prepared.env("TMP", cwd.join(".sandbox-tmp"));
        prepared.env("TEMP", cwd.join(".sandbox-tmp"));
    }
    prepared
}

fn prepare_sandbox_dirs(cwd: &std::path::Path) {
    let _ = std::fs::create_dir_all(cwd.join(".sandbox-home"));
    let _ = std::fs::create_dir_all(cwd.join(".sandbox-tmp"));
}

fn normalize_command_text(command: &str) -> String {
    let mut normalized = command.to_ascii_lowercase();
    normalized = normalized.replace('\r', "\n");
    normalized = normalized.replace('\n', " ");
    normalized = normalized.replace('\t', " ");
    normalized = normalized.replace('|', " | ");
    normalized = normalized.replace('&', " & ");
    normalized = normalized.replace(';', " ; ");
    format!(" {} ", normalized)
}

fn matches_any_marker(command: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| command.contains(marker))
}

fn referenced_script_paths(command: &str, cwd: &std::path::Path) -> Vec<std::path::PathBuf> {
    command
        .split_whitespace()
        .filter_map(clean_command_token)
        .filter(|token| looks_like_script_path(token))
        .map(|token| resolve_script_path(&token, cwd))
        .filter(|path| path.is_file())
        .collect()
}

fn clean_command_token(token: &str) -> Option<String> {
    let trimmed = token.trim_matches(|c: char| {
        matches!(
            c,
            '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';'
        )
    });
    (!trimmed.is_empty()).then_some(trimmed.to_string())
}

fn looks_like_script_path(token: &str) -> bool {
    let path = std::path::Path::new(token);
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "bat" | "cmd" | "ps1" | "sh" | "bash" | "zsh"
    )
}

fn resolve_script_path(token: &str, cwd: &std::path::Path) -> std::path::PathBuf {
    let path = std::path::Path::new(token);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn detect_interactive_script_contents(path: &std::path::Path) -> Option<&'static str> {
    let contents = std::fs::read_to_string(path).ok()?;
    let normalized = normalize_command_text(&contents);
    if matches_any_marker(&normalized, WINDOWS_INTERACTIVE_MARKERS) {
        return Some("Windows interactive statement");
    }
    if matches_any_marker(&normalized, POWERSHELL_INTERACTIVE_MARKERS) {
        return Some("PowerShell interactive statement");
    }
    if matches_any_marker(&normalized, UNIX_INTERACTIVE_MARKERS) {
        return Some("shell interactive statement");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        detect_interactive_shell_command, execute_bash, resolve_shell_timeout_ms, BashCommandInput,
        DEFAULT_SHELL_TIMEOUT_MS,
    };
    use crate::sandbox::FilesystemIsolationMode;

    fn hello_command() -> String {
        if cfg!(windows) {
            "echo hello".to_string()
        } else {
            "printf 'hello'".to_string()
        }
    }

    #[test]
    fn executes_simple_command() {
        let output = execute_bash(BashCommandInput {
            command: hello_command(),
            timeout: Some(1_000),
            description: None,
            run_in_background: Some(false),
            dangerously_disable_sandbox: Some(false),
            namespace_restrictions: Some(false),
            isolate_network: Some(false),
            filesystem_mode: Some(FilesystemIsolationMode::WorkspaceOnly),
            allowed_mounts: None,
        })
        .expect("bash command should execute");

        assert_eq!(output.stdout.trim(), "hello");
        assert!(!output.interrupted);
        assert!(output.sandbox_status.is_some());
    }

    #[test]
    fn disables_sandbox_when_requested() {
        let output = execute_bash(BashCommandInput {
            command: hello_command(),
            timeout: Some(1_000),
            description: None,
            run_in_background: Some(false),
            dangerously_disable_sandbox: Some(true),
            namespace_restrictions: None,
            isolate_network: None,
            filesystem_mode: None,
            allowed_mounts: None,
        })
        .expect("bash command should execute");

        assert!(!output.sandbox_status.expect("sandbox status").enabled);
    }

    #[test]
    fn falls_back_to_default_shell_timeout() {
        assert_eq!(resolve_shell_timeout_ms(None), DEFAULT_SHELL_TIMEOUT_MS);
        assert_eq!(resolve_shell_timeout_ms(Some(123)), 123);
    }

    #[test]
    fn blocks_inline_interactive_commands() {
        let cwd = std::env::current_dir().expect("cwd");
        let command = if cfg!(windows) {
            "echo hi && pause"
        } else {
            "echo hi; read answer"
        };
        let blocked =
            detect_interactive_shell_command(command, &cwd).expect("interactive command blocked");
        assert!(!blocked.reason.is_empty());
    }

    #[test]
    fn blocks_referenced_interactive_scripts() {
        let root = std::env::temp_dir().join(format!(
            "clawd-interactive-script-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("create temp root");
        let script_path = if cfg!(windows) {
            root.join("interactive.cmd")
        } else {
            root.join("interactive.sh")
        };
        let script_body = if cfg!(windows) {
            "@echo off\r\npause\r\n"
        } else {
            "#!/bin/sh\nread answer\n"
        };
        std::fs::write(&script_path, script_body).expect("write script");

        let command = script_path.display().to_string();
        let blocked =
            detect_interactive_shell_command(&command, &root).expect("interactive script blocked");
        assert!(blocked.reason.contains("interactive"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn execute_bash_returns_terminal_handoff_for_interactive_commands() {
        let output = execute_bash(BashCommandInput {
            command: if cfg!(windows) {
                "echo hi && pause".to_string()
            } else {
                "echo hi; read answer".to_string()
            },
            timeout: None,
            description: None,
            run_in_background: Some(false),
            dangerously_disable_sandbox: Some(false),
            namespace_restrictions: Some(false),
            isolate_network: Some(false),
            filesystem_mode: Some(FilesystemIsolationMode::WorkspaceOnly),
            allowed_mounts: None,
        })
        .expect("interactive command should return structured output");

        assert_eq!(
            output.return_code_interpretation.as_deref(),
            Some("interactive_blocked")
        );
        assert!(output.stderr.contains("user terminal"));
    }
}

/// Maximum output bytes before truncation (16 KiB, matching upstream).
const MAX_OUTPUT_BYTES: usize = 16_384;

/// Truncate output to `MAX_OUTPUT_BYTES`, appending a marker when trimmed.
fn truncate_output(s: &str) -> String {
    if s.len() <= MAX_OUTPUT_BYTES {
        return s.to_string();
    }
    // Find the last valid UTF-8 boundary at or before MAX_OUTPUT_BYTES
    let mut end = MAX_OUTPUT_BYTES;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = s[..end].to_string();
    truncated.push_str("\n\n[output truncated — exceeded 16384 bytes]");
    truncated
}

#[cfg(test)]
mod truncation_tests {
    use super::*;

    #[test]
    fn short_output_unchanged() {
        let s = "hello world";
        assert_eq!(truncate_output(s), s);
    }

    #[test]
    fn long_output_truncated() {
        let s = "x".repeat(20_000);
        let result = truncate_output(&s);
        assert!(result.len() < 20_000);
        assert!(result.ends_with("[output truncated — exceeded 16384 bytes]"));
    }

    #[test]
    fn exact_boundary_unchanged() {
        let s = "a".repeat(MAX_OUTPUT_BYTES);
        assert_eq!(truncate_output(&s), s);
    }

    #[test]
    fn one_over_boundary_truncated() {
        let s = "a".repeat(MAX_OUTPUT_BYTES + 1);
        let result = truncate_output(&s);
        assert!(result.contains("[output truncated"));
    }
}
