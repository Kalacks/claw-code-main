use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const LLM_LAYER_FILE: &str = ".claw/llm-layer.json";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_field_names)]
pub struct TurnTokenLimits {
    pub min_input_tokens: Option<u32>,
    pub max_input_tokens: Option<u32>,
    pub min_output_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
}

impl TurnTokenLimits {
    pub fn validate(self) -> Result<Self, String> {
        if let (Some(min), Some(max)) = (self.min_input_tokens, self.max_input_tokens) {
            if min > max {
                return Err(format!(
                    "invalid input token range: min_input_tokens ({min}) cannot exceed max_input_tokens ({max})"
                ));
            }
        }
        if let (Some(min), Some(max)) = (self.min_output_tokens, self.max_output_tokens) {
            if min > max {
                return Err(format!(
                    "invalid output token range: min_output_tokens ({min}) cannot exceed max_output_tokens ({max})"
                ));
            }
        }
        Ok(self)
    }

    pub fn effective_output_max(self, model_default_max: u32) -> u32 {
        self.max_output_tokens
            .map_or(model_default_max, |limit| limit.clamp(1, model_default_max))
    }

    pub fn check_input_estimate(self, estimated_input_tokens: u32) -> Result<(), String> {
        if let Some(min) = self.min_input_tokens {
            if estimated_input_tokens < min {
                return Err(format!(
                    "turn input token estimate {estimated_input_tokens} is below configured min_input_tokens {min}"
                ));
            }
        }
        if let Some(max) = self.max_input_tokens {
            if estimated_input_tokens > max {
                return Err(format!(
                    "turn input token estimate {estimated_input_tokens} exceeds configured max_input_tokens {max}"
                ));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn output_limit_warning(self, output_tokens: u32) -> Option<String> {
        if let Some(min) = self.min_output_tokens {
            if output_tokens < min {
                return Some(format!(
                    "turn output tokens {output_tokens} are below configured min_output_tokens {min}"
                ));
            }
        }
        if let Some(max) = self.max_output_tokens {
            if output_tokens > max {
                return Some(format!(
                    "turn output tokens {output_tokens} exceed configured max_output_tokens {max}"
                ));
            }
        }
        None
    }

    #[must_use]
    pub fn summary_line(self) -> String {
        fn label(value: Option<u32>) -> String {
            value.map_or_else(|| "-".to_string(), |number| number.to_string())
        }

        format!(
            "input {}..{} / output {}..{}",
            label(self.min_input_tokens),
            label(self.max_input_tokens),
            label(self.min_output_tokens),
            label(self.max_output_tokens),
        )
    }
}

#[must_use]
pub fn estimate_text_tokens(text: &str) -> u32 {
    let bytes = text.len().max(1);
    let estimate = bytes / 4 + 1;
    u32::try_from(estimate).unwrap_or(u32::MAX)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmProfile {
    pub name: String,
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

impl LlmProfile {
    #[must_use]
    pub fn normalized_provider(&self) -> String {
        let provider = self.provider.trim().to_ascii_lowercase();
        match provider.as_str() {
            "anthropic" | "claude" => "anthropic".to_string(),
            "xai" | "grok" => "xai".to_string(),
            "deepseek" => "deepseek".to_string(),
            "qwen" | "dashscope" | "tongyi" | "aliyun" => "qwen".to_string(),
            "openai" | "gpt" => "openai".to_string(),
            "compat" | "openai-compatible" => "compat".to_string(),
            _ => provider,
        }
    }

    #[must_use]
    pub fn key_source_label(&self) -> String {
        if let Some(env_key) = self
            .api_key_env
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return format!("env:{env_key}");
        }
        "inline".to_string()
    }

    pub fn resolved_api_key(&self) -> Result<String, String> {
        if let Some(env_key) = self
            .api_key_env
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let value = std::env::var(env_key).map_err(|_| {
                format!(
                    "profile '{}' expects API key in environment variable '{}'",
                    self.name, env_key
                )
            })?;
            if !value.trim().is_empty() {
                return Ok(value);
            }
            return Err(format!(
                "profile '{}' environment variable '{}' is empty",
                self.name, env_key
            ));
        }

        if let Some(api_key) = self
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Ok(api_key.to_string());
        }

        Err(format!(
            "profile '{}' has no API key configured; set api_key or api_key_env",
            self.name
        ))
    }

    #[must_use]
    pub fn masked_key_preview(&self) -> String {
        let inline = self
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(key) = inline {
            return mask_key(key);
        }
        self.api_key_env
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map_or_else(
                || "(missing)".to_string(),
                |env_key| format!("env:{env_key}"),
            )
    }
}

fn mask_key(key: &str) -> String {
    let trimmed = key.trim();
    let chars = trimmed.chars().collect::<Vec<_>>();
    if chars.len() <= 8 {
        return "*".repeat(chars.len());
    }
    let prefix = chars.iter().take(4).collect::<String>();
    let suffix = chars
        .iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{prefix}****{suffix}")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistedLlmLayer {
    #[serde(default)]
    profiles: Vec<LlmProfile>,
    #[serde(default)]
    active_profile: Option<String>,
    #[serde(default)]
    turn_token_limits: TurnTokenLimits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmLayerSummary {
    pub active_profile_name: Option<String>,
    pub active_provider: Option<String>,
    pub active_model: Option<String>,
    pub active_base_url: Option<String>,
    pub profile_count: usize,
    pub turn_token_limits: TurnTokenLimits,
}

#[derive(Debug, Clone)]
pub struct LlmProfileStore {
    path: PathBuf,
    state: PersistedLlmLayer,
}

impl LlmProfileStore {
    pub fn load_for(cwd: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let path = cwd.join(LLM_LAYER_FILE);
        if !path.exists() {
            return Ok(Self {
                path,
                state: PersistedLlmLayer::default(),
            });
        }
        let raw = fs::read_to_string(&path)?;
        let state = serde_json::from_str::<PersistedLlmLayer>(&raw).unwrap_or_default();
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
    pub fn list_profiles(&self) -> Vec<LlmProfile> {
        let mut profiles = self.state.profiles.clone();
        profiles.sort_by(|left, right| left.name.cmp(&right.name));
        profiles
    }

    #[must_use]
    pub fn profile(&self, name: &str) -> Option<&LlmProfile> {
        self.state
            .profiles
            .iter()
            .find(|profile| profile.name == name)
    }

    pub fn upsert_profile(
        &mut self,
        profile: LlmProfile,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if profile.name.trim().is_empty() {
            return Err("profile name cannot be empty".into());
        }
        if profile.model.trim().is_empty() {
            return Err("profile model cannot be empty".into());
        }
        if profile.provider.trim().is_empty() {
            return Err("profile provider cannot be empty".into());
        }
        if profile
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
            && profile
                .api_key_env
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
        {
            return Err("profile must set api_key or api_key_env".into());
        }

        if let Some(existing) = self
            .state
            .profiles
            .iter_mut()
            .find(|candidate| candidate.name == profile.name)
        {
            *existing = profile;
        } else {
            self.state.profiles.push(profile);
        }
        self.save()
    }

    pub fn remove_profile(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let before = self.state.profiles.len();
        self.state.profiles.retain(|profile| profile.name != name);
        if before == self.state.profiles.len() {
            return Err(format!("profile '{name}' does not exist").into());
        }
        if self.state.active_profile.as_deref() == Some(name) {
            self.state.active_profile = None;
        }
        self.save()
    }

    pub fn set_active_profile(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        if self.profile(name).is_none() {
            return Err(format!("profile '{name}' does not exist").into());
        }
        self.state.active_profile = Some(name.to_string());
        self.save()
    }

    pub fn clear_active_profile(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.state.active_profile = None;
        self.save()
    }

    #[must_use]
    pub fn active_profile_name(&self) -> Option<&str> {
        self.state.active_profile.as_deref()
    }

    #[must_use]
    pub fn active_profile(&self) -> Option<&LlmProfile> {
        self.active_profile_name()
            .and_then(|name| self.profile(name))
    }

    #[must_use]
    pub fn summary(&self) -> LlmLayerSummary {
        let active_profile = self.active_profile();
        LlmLayerSummary {
            active_profile_name: self.active_profile_name().map(ToOwned::to_owned),
            active_provider: active_profile.map(LlmProfile::normalized_provider),
            active_model: active_profile.map(|profile| profile.model.clone()),
            active_base_url: active_profile
                .and_then(|profile| profile.base_url.as_deref().map(str::trim))
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            profile_count: self.state.profiles.len(),
            turn_token_limits: self.state.turn_token_limits,
        }
    }

    #[must_use]
    pub fn turn_token_limits(&self) -> TurnTokenLimits {
        self.state.turn_token_limits
    }

    pub fn set_turn_token_limits(
        &mut self,
        limits: TurnTokenLimits,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.state.turn_token_limits = limits.validate()?;
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::{estimate_text_tokens, LlmProfile, LlmProfileStore, TurnTokenLimits};
    use std::fs;

    fn temp_workspace(label: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let root = std::env::temp_dir().join(format!("claw-llm-layer-{label}-{nonce}"));
        fs::create_dir_all(&root).expect("temp workspace");
        root
    }

    #[test]
    fn limits_validate_and_format_summary() {
        let limits = TurnTokenLimits {
            min_input_tokens: Some(10),
            max_input_tokens: Some(100),
            min_output_tokens: Some(20),
            max_output_tokens: Some(200),
        }
        .validate()
        .expect("valid limits");
        assert_eq!(limits.summary_line(), "input 10..100 / output 20..200");
        assert_eq!(limits.effective_output_max(80), 80);
    }

    #[test]
    fn profile_store_round_trips_profiles_and_active_selection() {
        let workspace = temp_workspace("store-roundtrip");
        let mut store = LlmProfileStore::load_for(&workspace).expect("load empty store");
        store
            .upsert_profile(LlmProfile {
                name: "deepseek-main".to_string(),
                provider: "deepseek".to_string(),
                model: "deepseek-chat".to_string(),
                base_url: Some("https://api.deepseek.com".to_string()),
                api_key: Some("sk-example".to_string()),
                api_key_env: None,
                note: Some("primary profile".to_string()),
            })
            .expect("upsert profile");
        store.set_active_profile("deepseek-main").expect("activate");
        store
            .set_turn_token_limits(TurnTokenLimits {
                min_input_tokens: Some(1),
                max_input_tokens: Some(4096),
                min_output_tokens: Some(1),
                max_output_tokens: Some(2048),
            })
            .expect("save limits");

        let loaded = LlmProfileStore::load_for(&workspace).expect("reload store");
        assert_eq!(loaded.list_profiles().len(), 1);
        assert_eq!(loaded.active_profile_name(), Some("deepseek-main"));
        assert_eq!(loaded.turn_token_limits().max_output_tokens, Some(2048));

        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn token_estimation_uses_byte_heuristic() {
        assert!(estimate_text_tokens("hello world") >= 3);
    }

    #[test]
    fn normalizes_qwen_provider_aliases() {
        let profile = LlmProfile {
            name: "qwen".to_string(),
            provider: "dashscope".to_string(),
            model: "qwen-max".to_string(),
            base_url: None,
            api_key: Some("sk".to_string()),
            api_key_env: None,
            note: None,
        };
        assert_eq!(profile.normalized_provider(), "qwen");
    }
}
