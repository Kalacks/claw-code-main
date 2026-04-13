use crate::session::Session;

const DEFAULT_INPUT_COST_PER_MILLION: f64 = 15.0;
const DEFAULT_OUTPUT_COST_PER_MILLION: f64 = 75.0;
const DEFAULT_CACHE_CREATION_COST_PER_MILLION: f64 = 18.75;
const DEFAULT_CACHE_READ_COST_PER_MILLION: f64 = 1.5;
const DEEPSEEK_INPUT_COST_PER_MILLION: f64 = 2.0;
const DEEPSEEK_OUTPUT_COST_PER_MILLION: f64 = 3.0;
const DEEPSEEK_CACHE_READ_COST_PER_MILLION: f64 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Currency {
    Usd,
    Cny,
}

impl Currency {
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Usd => "$",
            Self::Cny => "¥",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PricingSource {
    Official,
    EstimatedDefault,
    Unavailable,
}

impl PricingSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Official => "official",
            Self::EstimatedDefault => "estimated-default",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedPricing {
    pub pricing: Option<ModelPricing>,
    pub source: PricingSource,
    pub currency: Currency,
}

/// Per-million-token pricing used for cost estimation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPricing {
    pub currency: Currency,
    pub input_cost_per_million: f64,
    pub output_cost_per_million: f64,
    pub cache_creation_cost_per_million: f64,
    pub cache_read_cost_per_million: f64,
}

impl ModelPricing {
    #[must_use]
    pub const fn default_sonnet_tier() -> Self {
        Self {
            currency: Currency::Usd,
            input_cost_per_million: DEFAULT_INPUT_COST_PER_MILLION,
            output_cost_per_million: DEFAULT_OUTPUT_COST_PER_MILLION,
            cache_creation_cost_per_million: DEFAULT_CACHE_CREATION_COST_PER_MILLION,
            cache_read_cost_per_million: DEFAULT_CACHE_READ_COST_PER_MILLION,
        }
    }

    #[must_use]
    pub const fn deepseek_tier() -> Self {
        Self {
            currency: Currency::Cny,
            input_cost_per_million: DEEPSEEK_INPUT_COST_PER_MILLION,
            output_cost_per_million: DEEPSEEK_OUTPUT_COST_PER_MILLION,
            cache_creation_cost_per_million: 0.0,
            cache_read_cost_per_million: DEEPSEEK_CACHE_READ_COST_PER_MILLION,
        }
    }
}

/// Token counters accumulated for a conversation turn or session.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_creation_input_tokens: u32,
    pub cache_read_input_tokens: u32,
}

/// Estimated cost derived from a [`TokenUsage`] sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UsageCostEstimate {
    pub currency: Currency,
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_creation_cost: f64,
    pub cache_read_cost: f64,
}

impl UsageCostEstimate {
    #[must_use]
    pub fn total_cost(self) -> f64 {
        self.input_cost + self.output_cost + self.cache_creation_cost + self.cache_read_cost
    }

    #[must_use]
    pub fn total_cost_usd(self) -> f64 {
        self.total_cost()
    }
}

#[derive(Debug, Clone, Copy)]
struct PricingTier {
    max_input_tokens: u32,
    input_cost_per_million: f64,
    output_cost_per_million: f64,
}

/// Returns pricing metadata for a known model alias or family.
#[must_use]
pub fn pricing_for_model(model: &str) -> Option<ModelPricing> {
    pricing_for_usage(model, TokenUsage::default())
}

#[must_use]
pub fn pricing_for_usage(model: &str, usage: TokenUsage) -> Option<ModelPricing> {
    let normalized = model.to_ascii_lowercase();
    pricing_for_western_model(&normalized)
        .or_else(|| pricing_for_kimi_model(&normalized))
        .or_else(|| pricing_for_qwen_model(&normalized, usage))
        .or_else(|| pricing_for_glm_model(&normalized))
}

fn pricing_for_western_model(normalized: &str) -> Option<ModelPricing> {
    if normalized.starts_with("deepseek") {
        return Some(ModelPricing::deepseek_tier());
    }
    if normalized.contains("haiku") {
        return Some(ModelPricing {
            currency: Currency::Usd,
            input_cost_per_million: 1.0,
            output_cost_per_million: 5.0,
            cache_creation_cost_per_million: 1.25,
            cache_read_cost_per_million: 0.1,
        });
    }
    if normalized.contains("opus") {
        return Some(ModelPricing {
            currency: Currency::Usd,
            input_cost_per_million: 15.0,
            output_cost_per_million: 75.0,
            cache_creation_cost_per_million: 18.75,
            cache_read_cost_per_million: 1.5,
        });
    }
    if normalized.contains("sonnet") {
        return Some(ModelPricing::default_sonnet_tier());
    }
    None
}

fn pricing_for_kimi_model(normalized: &str) -> Option<ModelPricing> {
    if normalized.starts_with("kimi-k2.5") {
        return Some(fixed_cny_pricing(4.0, 21.0, 0.0, 0.7));
    }
    if normalized.starts_with("kimi-k2-thinking")
        || normalized.starts_with("kimi-k2")
        || normalized == "kimi-k2-0905-preview"
        || normalized == "kimi-k2-0711-preview"
    {
        return Some(fixed_cny_pricing(4.0, 16.0, 0.0, 1.0));
    }
    None
}

fn pricing_for_qwen_model(normalized: &str, usage: TokenUsage) -> Option<ModelPricing> {
    pricing_for_qwen_core(normalized, usage)
        .or_else(|| pricing_for_qwen_coder(normalized, usage))
        .or_else(|| pricing_for_qwen_flash_family(normalized, usage))
}

#[allow(clippy::too_many_lines)]
fn pricing_for_qwen_core(normalized: &str, usage: TokenUsage) -> Option<ModelPricing> {
    if normalized.starts_with("qwq-plus") {
        return Some(fixed_cny_pricing(1.6, 4.0, 0.0, 0.0));
    }
    if normalized.starts_with("qwen3-max") {
        return Some(tiered_cny_pricing(
            billed_input_tokens(usage),
            &[
                PricingTier {
                    max_input_tokens: 32 * 1024,
                    input_cost_per_million: 2.5,
                    output_cost_per_million: 10.0,
                },
                PricingTier {
                    max_input_tokens: 128 * 1024,
                    input_cost_per_million: 4.0,
                    output_cost_per_million: 16.0,
                },
                PricingTier {
                    max_input_tokens: 252 * 1024,
                    input_cost_per_million: 7.0,
                    output_cost_per_million: 28.0,
                },
            ],
            None,
        ));
    }
    if normalized.starts_with("qwen-max") {
        return Some(fixed_cny_pricing(2.4, 9.6, 0.0, 0.0));
    }
    if normalized.starts_with("qwen3.6-plus") {
        return Some(tiered_cny_pricing(
            billed_input_tokens(usage),
            &[
                PricingTier {
                    max_input_tokens: 256 * 1024,
                    input_cost_per_million: 2.0,
                    output_cost_per_million: 12.0,
                },
                PricingTier {
                    max_input_tokens: 1_000_000,
                    input_cost_per_million: 8.0,
                    output_cost_per_million: 48.0,
                },
            ],
            None,
        ));
    }
    if normalized.starts_with("qwen3.5-plus") {
        return Some(tiered_cny_pricing(
            billed_input_tokens(usage),
            &[
                PricingTier {
                    max_input_tokens: 128 * 1024,
                    input_cost_per_million: 0.8,
                    output_cost_per_million: 4.8,
                },
                PricingTier {
                    max_input_tokens: 256 * 1024,
                    input_cost_per_million: 2.0,
                    output_cost_per_million: 12.0,
                },
                PricingTier {
                    max_input_tokens: 1_000_000,
                    input_cost_per_million: 4.0,
                    output_cost_per_million: 24.0,
                },
            ],
            None,
        ));
    }
    if normalized.starts_with("qwen-plus") {
        return Some(tiered_cny_pricing(
            billed_input_tokens(usage),
            &[
                PricingTier {
                    max_input_tokens: 128 * 1024,
                    input_cost_per_million: 0.8,
                    output_cost_per_million: qwen_plus_output_cost(normalized, 2.0, 8.0),
                },
                PricingTier {
                    max_input_tokens: 256 * 1024,
                    input_cost_per_million: 2.4,
                    output_cost_per_million: qwen_plus_output_cost(normalized, 20.0, 24.0),
                },
                PricingTier {
                    max_input_tokens: 1_000_000,
                    input_cost_per_million: 4.8,
                    output_cost_per_million: qwen_plus_output_cost(normalized, 48.0, 64.0),
                },
            ],
            None,
        ));
    }
    if normalized.starts_with("qwen-turbo") {
        let output_cost = if normalized.contains("thinking") {
            3.0
        } else {
            0.6
        };
        return Some(fixed_cny_pricing(0.3, output_cost, 0.0, 0.0));
    }
    None
}

fn pricing_for_qwen_coder(normalized: &str, usage: TokenUsage) -> Option<ModelPricing> {
    if normalized.starts_with("qwen3-coder-plus") {
        return Some(tiered_cny_pricing(
            billed_input_tokens(usage),
            &[
                PricingTier {
                    max_input_tokens: 32 * 1024,
                    input_cost_per_million: 4.0,
                    output_cost_per_million: 16.0,
                },
                PricingTier {
                    max_input_tokens: 128 * 1024,
                    input_cost_per_million: 6.0,
                    output_cost_per_million: 24.0,
                },
                PricingTier {
                    max_input_tokens: 256 * 1024,
                    input_cost_per_million: 10.0,
                    output_cost_per_million: 40.0,
                },
                PricingTier {
                    max_input_tokens: 1_000_000,
                    input_cost_per_million: 20.0,
                    output_cost_per_million: 200.0,
                },
            ],
            Some(0.2),
        ));
    }
    if normalized.starts_with("qwen3-coder-flash") {
        return Some(tiered_cny_pricing(
            billed_input_tokens(usage),
            &[
                PricingTier {
                    max_input_tokens: 32 * 1024,
                    input_cost_per_million: 1.0,
                    output_cost_per_million: 4.0,
                },
                PricingTier {
                    max_input_tokens: 128 * 1024,
                    input_cost_per_million: 1.5,
                    output_cost_per_million: 6.0,
                },
                PricingTier {
                    max_input_tokens: 256 * 1024,
                    input_cost_per_million: 2.5,
                    output_cost_per_million: 10.0,
                },
                PricingTier {
                    max_input_tokens: 1_000_000,
                    input_cost_per_million: 5.0,
                    output_cost_per_million: 25.0,
                },
            ],
            Some(0.2),
        ));
    }
    if normalized.starts_with("qwen-coder-plus") {
        return Some(fixed_cny_pricing(3.5, 7.0, 0.0, 0.0));
    }
    if normalized.starts_with("qwen-coder-turbo") {
        return Some(fixed_cny_pricing(2.0, 6.0, 0.0, 0.0));
    }
    None
}

fn pricing_for_qwen_flash_family(normalized: &str, usage: TokenUsage) -> Option<ModelPricing> {
    if normalized.starts_with("qwen3.5-flash") {
        return Some(tiered_cny_pricing(
            billed_input_tokens(usage),
            &[
                PricingTier {
                    max_input_tokens: 128 * 1024,
                    input_cost_per_million: 0.2,
                    output_cost_per_million: 2.0,
                },
                PricingTier {
                    max_input_tokens: 256 * 1024,
                    input_cost_per_million: 0.8,
                    output_cost_per_million: 8.0,
                },
                PricingTier {
                    max_input_tokens: 1_000_000,
                    input_cost_per_million: 1.2,
                    output_cost_per_million: 12.0,
                },
            ],
            None,
        ));
    }
    if normalized.starts_with("qwen-flash") {
        return Some(tiered_cny_pricing(
            billed_input_tokens(usage),
            &[
                PricingTier {
                    max_input_tokens: 128 * 1024,
                    input_cost_per_million: 0.15,
                    output_cost_per_million: 1.5,
                },
                PricingTier {
                    max_input_tokens: 256 * 1024,
                    input_cost_per_million: 0.6,
                    output_cost_per_million: 6.0,
                },
                PricingTier {
                    max_input_tokens: 1_000_000,
                    input_cost_per_million: 1.2,
                    output_cost_per_million: 12.0,
                },
            ],
            None,
        ));
    }
    None
}

fn pricing_for_glm_model(normalized: &str) -> Option<ModelPricing> {
    if normalized.starts_with("glm-4-plus") {
        return Some(uniform_cny_token_pricing(5.0));
    }
    if normalized.starts_with("glm-4-airx") {
        return Some(uniform_cny_token_pricing(10.0));
    }
    if normalized.starts_with("glm-4-air") {
        return Some(uniform_cny_token_pricing(0.5));
    }
    if normalized.starts_with("glm-4-flashx") {
        return Some(uniform_cny_token_pricing(0.1));
    }
    if normalized.starts_with("glm-4-flash") {
        return Some(uniform_cny_token_pricing(0.0));
    }
    None
}

fn fixed_cny_pricing(
    input_cost_per_million: f64,
    output_cost_per_million: f64,
    cache_creation_cost_per_million: f64,
    cache_read_cost_per_million: f64,
) -> ModelPricing {
    ModelPricing {
        currency: Currency::Cny,
        input_cost_per_million,
        output_cost_per_million,
        cache_creation_cost_per_million,
        cache_read_cost_per_million,
    }
}

fn uniform_cny_token_pricing(total_cost_per_million: f64) -> ModelPricing {
    fixed_cny_pricing(total_cost_per_million, total_cost_per_million, 0.0, 0.0)
}

fn tiered_cny_pricing(
    input_tokens: u32,
    tiers: &[PricingTier],
    cache_read_input_ratio: Option<f64>,
) -> ModelPricing {
    let selected = tiers
        .iter()
        .find(|tier| input_tokens <= tier.max_input_tokens)
        .unwrap_or_else(|| {
            tiers
                .last()
                .expect("tiered pricing requires at least one tier")
        });
    fixed_cny_pricing(
        selected.input_cost_per_million,
        selected.output_cost_per_million,
        0.0,
        cache_read_input_ratio.map_or(0.0, |ratio| selected.input_cost_per_million * ratio),
    )
}

fn billed_input_tokens(usage: TokenUsage) -> u32 {
    usage
        .input_tokens
        .saturating_add(usage.cache_creation_input_tokens)
        .saturating_add(usage.cache_read_input_tokens)
}

fn qwen_plus_output_cost(normalized_model: &str, non_thinking: f64, thinking: f64) -> f64 {
    if normalized_model.contains("thinking") || normalized_model.starts_with("qwq") {
        thinking
    } else {
        non_thinking
    }
}

#[must_use]
pub fn preferred_currency_for_model(model: &str) -> Currency {
    if is_chinese_parent_model(model) {
        Currency::Cny
    } else {
        Currency::Usd
    }
}

#[must_use]
pub fn resolve_pricing_for_model(model: Option<&str>) -> ResolvedPricing {
    let Some(model_name) = model else {
        let pricing = ModelPricing::default_sonnet_tier();
        return ResolvedPricing {
            pricing: Some(pricing),
            source: PricingSource::EstimatedDefault,
            currency: pricing.currency,
        };
    };

    if let Some(pricing) = pricing_for_model(model_name) {
        return ResolvedPricing {
            pricing: Some(pricing),
            source: PricingSource::Official,
            currency: pricing.currency,
        };
    }

    let currency = preferred_currency_for_model(model_name);
    if currency == Currency::Cny {
        ResolvedPricing {
            pricing: None,
            source: PricingSource::Unavailable,
            currency,
        }
    } else {
        let pricing = ModelPricing::default_sonnet_tier();
        ResolvedPricing {
            pricing: Some(pricing),
            source: PricingSource::EstimatedDefault,
            currency: pricing.currency,
        }
    }
}

fn is_chinese_parent_model(model: &str) -> bool {
    let normalized = model.to_ascii_lowercase();
    normalized.starts_with("deepseek")
        || normalized.starts_with("qwen")
        || normalized.starts_with("qwq")
        || normalized.starts_with("kimi")
        || normalized.contains("moonshot")
        || normalized.starts_with("glm")
        || normalized.starts_with("chatglm")
        || normalized.starts_with("doubao")
        || normalized.contains("ernie")
        || normalized.contains("wenxin")
        || normalized.starts_with("hunyuan")
        || normalized.starts_with("baichuan")
        || normalized.contains("minimax")
        || normalized.starts_with("abab")
        || normalized == "yi"
        || normalized.starts_with("yi-")
        || normalized.contains("lingyi")
}

impl TokenUsage {
    #[must_use]
    pub fn total_tokens(self) -> u32 {
        self.input_tokens
            + self.output_tokens
            + self.cache_creation_input_tokens
            + self.cache_read_input_tokens
    }

    #[must_use]
    pub fn estimate_cost_usd(self) -> UsageCostEstimate {
        self.estimate_cost_with_pricing(ModelPricing::default_sonnet_tier())
    }

    #[must_use]
    pub fn estimate_cost_usd_with_pricing(self, pricing: ModelPricing) -> UsageCostEstimate {
        self.estimate_cost_with_pricing(pricing)
    }

    #[must_use]
    pub fn estimate_cost_for_model(self, model: &str) -> UsageCostEstimate {
        pricing_for_usage(model, self).map_or_else(
            || self.estimate_cost_usd(),
            |pricing| self.estimate_cost_with_pricing(pricing),
        )
    }

    #[must_use]
    pub fn estimate_cost_with_pricing(self, pricing: ModelPricing) -> UsageCostEstimate {
        UsageCostEstimate {
            currency: pricing.currency,
            input_cost: cost_for_tokens(self.input_tokens, pricing.input_cost_per_million),
            output_cost: cost_for_tokens(self.output_tokens, pricing.output_cost_per_million),
            cache_creation_cost: cost_for_tokens(
                self.cache_creation_input_tokens,
                pricing.cache_creation_cost_per_million,
            ),
            cache_read_cost: cost_for_tokens(
                self.cache_read_input_tokens,
                pricing.cache_read_cost_per_million,
            ),
        }
    }

    #[must_use]
    pub fn summary_lines(self, label: &str) -> Vec<String> {
        self.summary_lines_for_model(label, None)
    }

    #[must_use]
    pub fn summary_lines_for_model(self, label: &str, model: Option<&str>) -> Vec<String> {
        let resolved = resolve_pricing_for_model(model);
        let model_suffix =
            model.map_or_else(String::new, |model_name| format!(" model={model_name}"));
        let pricing = model
            .and_then(|model_name| pricing_for_usage(model_name, self))
            .or(resolved.pricing);
        match pricing {
            Some(pricing) => {
                let cost = self.estimate_cost_with_pricing(pricing);
                vec![
                    format!(
                        "{label}: total_tokens={} input={} output={} cache_write={} cache_read={} estimated_cost={}{} pricing={} currency={}",
                        self.total_tokens(),
                        self.input_tokens,
                        self.output_tokens,
                        self.cache_creation_input_tokens,
                        self.cache_read_input_tokens,
                        format_currency(cost.total_cost(), cost.currency),
                        model_suffix,
                        resolved.source.as_str(),
                        cost.currency.symbol(),
                    ),
                    format!(
                        "  cost breakdown: input={} output={} cache_write={} cache_read={}",
                        format_currency(cost.input_cost, cost.currency),
                        format_currency(cost.output_cost, cost.currency),
                        format_currency(cost.cache_creation_cost, cost.currency),
                        format_currency(cost.cache_read_cost, cost.currency),
                    ),
                ]
            }
            None => vec![
                format!(
                    "{label}: total_tokens={} input={} output={} cache_write={} cache_read={} estimated_cost=unavailable{} pricing={} currency={}",
                    self.total_tokens(),
                    self.input_tokens,
                    self.output_tokens,
                    self.cache_creation_input_tokens,
                    self.cache_read_input_tokens,
                    model_suffix,
                    resolved.source.as_str(),
                    resolved.currency.symbol(),
                ),
                "  cost breakdown: unavailable".to_string(),
            ],
        }
    }
}

fn cost_for_tokens(tokens: u32, usd_per_million_tokens: f64) -> f64 {
    f64::from(tokens) / 1_000_000.0 * usd_per_million_tokens
}

#[must_use]
/// Formats a dollar-denominated value for CLI display.
pub fn format_usd(amount: f64) -> String {
    format_currency(amount, Currency::Usd)
}

#[must_use]
pub fn format_currency(amount: f64, currency: Currency) -> String {
    format!("{}{amount:.4}", currency.symbol())
}

/// Aggregates token usage across a running session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageTracker {
    latest_turn: TokenUsage,
    cumulative: TokenUsage,
    turns: u32,
}

impl UsageTracker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn from_session(session: &Session) -> Self {
        let mut tracker = Self::new();
        for message in &session.messages {
            if let Some(usage) = message.usage {
                tracker.record(usage);
            }
        }
        tracker
    }

    pub fn record(&mut self, usage: TokenUsage) {
        self.latest_turn = usage;
        self.cumulative.input_tokens += usage.input_tokens;
        self.cumulative.output_tokens += usage.output_tokens;
        self.cumulative.cache_creation_input_tokens += usage.cache_creation_input_tokens;
        self.cumulative.cache_read_input_tokens += usage.cache_read_input_tokens;
        self.turns += 1;
    }

    #[must_use]
    pub fn current_turn_usage(&self) -> TokenUsage {
        self.latest_turn
    }

    #[must_use]
    pub fn cumulative_usage(&self) -> TokenUsage {
        self.cumulative
    }

    #[must_use]
    pub fn turns(&self) -> u32 {
        self.turns
    }
}

#[cfg(test)]
mod tests {
    use super::{
        format_currency, format_usd, preferred_currency_for_model, pricing_for_model,
        pricing_for_usage, resolve_pricing_for_model, Currency, PricingSource, TokenUsage,
        UsageTracker,
    };
    use crate::session::{ContentBlock, ConversationMessage, MessageRole, Session};

    #[test]
    fn tracks_true_cumulative_usage() {
        let mut tracker = UsageTracker::new();
        tracker.record(TokenUsage {
            input_tokens: 10,
            output_tokens: 4,
            cache_creation_input_tokens: 2,
            cache_read_input_tokens: 1,
        });
        tracker.record(TokenUsage {
            input_tokens: 20,
            output_tokens: 6,
            cache_creation_input_tokens: 3,
            cache_read_input_tokens: 2,
        });

        assert_eq!(tracker.turns(), 2);
        assert_eq!(tracker.current_turn_usage().input_tokens, 20);
        assert_eq!(tracker.current_turn_usage().output_tokens, 6);
        assert_eq!(tracker.cumulative_usage().output_tokens, 10);
        assert_eq!(tracker.cumulative_usage().input_tokens, 30);
        assert_eq!(tracker.cumulative_usage().total_tokens(), 48);
    }

    #[test]
    fn computes_cost_summary_lines() {
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            cache_creation_input_tokens: 100_000,
            cache_read_input_tokens: 200_000,
        };

        let cost = usage.estimate_cost_usd();
        assert_eq!(format_usd(cost.input_cost), "$15.0000");
        assert_eq!(format_usd(cost.output_cost), "$37.5000");
        let lines = usage.summary_lines_for_model("usage", Some("claude-sonnet-4-20250514"));
        assert!(lines[0].contains("estimated_cost=$54.6750"));
        assert!(lines[0].contains("model=claude-sonnet-4-20250514"));
        assert!(lines[1].contains("cache_read=$0.3000"));
    }

    #[test]
    fn supports_model_specific_pricing() {
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };

        let haiku = pricing_for_model("claude-haiku-4-5-20251001").expect("haiku pricing");
        let opus = pricing_for_model("claude-opus-4-6").expect("opus pricing");
        let haiku_cost = usage.estimate_cost_with_pricing(haiku);
        let opus_cost = usage.estimate_cost_with_pricing(opus);
        assert_eq!(format_usd(haiku_cost.total_cost_usd()), "$3.5000");
        assert_eq!(format_usd(opus_cost.total_cost_usd()), "$52.5000");
    }

    #[test]
    fn marks_unknown_model_pricing_as_fallback() {
        let usage = TokenUsage {
            input_tokens: 20_000,
            output_tokens: 10_000,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };
        let lines = usage.summary_lines_for_model("usage", Some("custom-model"));
        assert!(lines[0].contains("pricing=estimated-default"));
    }

    #[test]
    fn deepseek_uses_official_rmb_pricing() {
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 200_000,
        };

        let pricing = pricing_for_model("deepseek-chat").expect("deepseek pricing");
        let cost = usage.estimate_cost_with_pricing(pricing);
        assert_eq!(cost.currency, Currency::Cny);
        assert_eq!(format_currency(cost.total_cost(), cost.currency), "¥3.5400");
        let lines = usage.summary_lines_for_model("usage", Some("deepseek-chat"));
        assert!(lines[0].contains("estimated_cost=¥3.5400"));
        assert!(lines[0].contains("pricing=official"));
    }

    #[test]
    fn qwen_models_use_official_rmb_pricing() {
        let usage = TokenUsage {
            input_tokens: 20_000,
            output_tokens: 10_000,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };
        let pricing = pricing_for_usage("qwen3-coder-plus", usage).expect("qwen pricing");
        let cost = usage.estimate_cost_with_pricing(pricing);
        assert_eq!(cost.currency, Currency::Cny);
        assert_eq!(format_currency(cost.total_cost(), cost.currency), "¥0.2400");
        let lines = usage.summary_lines_for_model("usage", Some("qwen3-coder-plus"));
        assert!(lines[0].contains("estimated_cost=¥0.2400"));
        assert!(lines[0].contains("pricing=official"));
        assert!(lines[0].contains("currency=¥"));
        assert_eq!(
            preferred_currency_for_model("qwen3-coder-plus"),
            Currency::Cny
        );
    }

    #[test]
    fn resolves_pricing_source_for_known_and_unknown_models() {
        let deepseek = resolve_pricing_for_model(Some("deepseek-chat"));
        assert_eq!(deepseek.source, PricingSource::Official);
        assert_eq!(deepseek.currency, Currency::Cny);
        assert!(deepseek.pricing.is_some());

        let custom = resolve_pricing_for_model(Some("custom-model"));
        assert_eq!(custom.source, PricingSource::EstimatedDefault);
        assert_eq!(custom.currency, Currency::Usd);
        assert!(custom.pricing.is_some());

        let kimi = resolve_pricing_for_model(Some("kimi-k2.5"));
        assert_eq!(kimi.source, PricingSource::Official);
        assert_eq!(kimi.currency, Currency::Cny);
        assert!(kimi.pricing.is_some());

        let glm = resolve_pricing_for_model(Some("glm-4-plus"));
        assert_eq!(glm.source, PricingSource::Official);
        assert_eq!(glm.currency, Currency::Cny);
        assert!(glm.pricing.is_some());

        let unknown_chinese = resolve_pricing_for_model(Some("doubao-pro"));
        assert_eq!(unknown_chinese.source, PricingSource::Unavailable);
        assert_eq!(unknown_chinese.currency, Currency::Cny);
        assert!(unknown_chinese.pricing.is_none());
    }

    #[test]
    fn kimi_and_glm_use_official_rmb_pricing() {
        let kimi_usage = TokenUsage {
            input_tokens: 1_000,
            output_tokens: 500,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 100,
        };
        let kimi_pricing = pricing_for_usage("kimi-k2.5", kimi_usage).expect("kimi pricing");
        let kimi_cost = kimi_usage.estimate_cost_with_pricing(kimi_pricing);
        assert_eq!(
            format_currency(kimi_cost.total_cost(), kimi_cost.currency),
            "¥0.0146"
        );

        let glm_usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };
        let glm_pricing = pricing_for_usage("glm-4-plus", glm_usage).expect("glm pricing");
        let glm_cost = glm_usage.estimate_cost_with_pricing(glm_pricing);
        assert_eq!(
            format_currency(glm_cost.total_cost(), glm_cost.currency),
            "¥7.5000"
        );
    }

    #[test]
    fn unknown_chinese_models_still_avoid_fake_usd_fallback() {
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 100,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };
        let lines = usage.summary_lines_for_model("usage", Some("doubao-pro"));
        assert!(lines[0].contains("estimated_cost=unavailable"));
        assert!(lines[0].contains("pricing=unavailable"));
        assert!(lines[0].contains("currency=¥"));
    }

    #[test]
    fn reconstructs_usage_from_session_messages() {
        let mut session = Session::new();
        session.messages = vec![ConversationMessage {
            role: MessageRole::Assistant,
            blocks: vec![ContentBlock::Text {
                text: "done".to_string(),
            }],
            usage: Some(TokenUsage {
                input_tokens: 5,
                output_tokens: 2,
                cache_creation_input_tokens: 1,
                cache_read_input_tokens: 0,
            }),
        }];

        let tracker = UsageTracker::from_session(&session);
        assert_eq!(tracker.turns(), 1);
        assert_eq!(tracker.cumulative_usage().total_tokens(), 8);
    }
}
