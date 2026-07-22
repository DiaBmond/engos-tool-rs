use std::fmt;

/// Tokens consumed by one AI call.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
}

impl TokenUsage {
    pub fn new(prompt_tokens: u32, output_tokens: u32, total_tokens: u32) -> Self {
        Self {
            prompt_tokens,
            output_tokens,
            // Providers do not always echo a total; derive it when missing so
            // the figure is never silently zero.
            total_tokens: if total_tokens == 0 {
                prompt_tokens.saturating_add(output_tokens)
            } else {
                total_tokens
            },
        }
    }

    pub fn is_empty(&self) -> bool {
        self.total_tokens == 0
    }
}

/// Which part of the app spent the tokens. Low cardinality, used for the
/// per-feature breakdown in the usage report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiFeature {
    VocabGenerate,
    VocabEvaluate,
    SentenceAnalyze,
    RoleplayScenario,
    RoleplayTurn,
    RoleplayEvaluate,
}

impl AiFeature {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::VocabGenerate => "vocab_generate",
            Self::VocabEvaluate => "vocab_evaluate",
            Self::SentenceAnalyze => "sentence_analyze",
            Self::RoleplayScenario => "roleplay_scenario",
            Self::RoleplayTurn => "roleplay_turn",
            Self::RoleplayEvaluate => "roleplay_evaluate",
        }
    }

    pub fn label_th(&self) -> &'static str {
        match self {
            Self::VocabGenerate => "สร้างคำศัพท์",
            Self::VocabEvaluate => "ตรวจคำตอบศัพท์",
            Self::SentenceAnalyze => "ตรวจประโยค",
            Self::RoleplayScenario => "สร้างสถานการณ์",
            Self::RoleplayTurn => "สนทนาโรลเพลย์",
            Self::RoleplayEvaluate => "ประเมินโรลเพลย์",
        }
    }
}

impl fmt::Display for AiFeature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One recorded AI call, queued for persistence.
#[derive(Debug, Clone)]
pub struct UsageEvent {
    pub model: String,
    pub feature: AiFeature,
    pub usage: TokenUsage,
}

/// Aggregated spend over a period, plus the same figures broken down by feature.
#[derive(Debug, Clone, Default)]
pub struct UsageSummary {
    pub calls: u64,
    pub prompt_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub by_feature: Vec<FeatureUsage>,
}

#[derive(Debug, Clone)]
pub struct FeatureUsage {
    pub feature: String,
    pub calls: u64,
    pub total_tokens: u64,
}

/// Price per million tokens, split by direction. Configurable because provider
/// pricing changes independently of this code.
#[derive(Debug, Clone, Copy)]
pub struct TokenPricing {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

impl UsageSummary {
    /// Estimated spend in the currency the prices are expressed in.
    pub fn estimated_cost(&self, pricing: TokenPricing) -> f64 {
        let input = self.prompt_tokens as f64 / 1_000_000.0 * pricing.input_per_mtok;
        let output = self.output_tokens as f64 / 1_000_000.0 * pricing.output_per_mtok;
        input + output
    }

    /// Tokens left before the configured budget is reached.
    pub fn remaining_tokens(&self, budget: u64) -> u64 {
        budget.saturating_sub(self.total_tokens)
    }

    /// Share of the budget consumed, clamped to 100%.
    pub fn budget_used_percent(&self, budget: u64) -> f64 {
        if budget == 0 {
            return 0.0;
        }
        ((self.total_tokens as f64 / budget as f64) * 100.0).min(100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRICING: TokenPricing = TokenPricing {
        input_per_mtok: 0.30,
        output_per_mtok: 2.50,
    };

    #[test]
    fn total_is_derived_when_the_provider_omits_it() {
        let u = TokenUsage::new(100, 50, 0);
        assert_eq!(u.total_tokens, 150);
    }

    #[test]
    fn provider_total_is_preferred_when_present() {
        let u = TokenUsage::new(100, 50, 175);
        assert_eq!(
            u.total_tokens, 175,
            "cached tokens can make the total larger"
        );
    }

    #[test]
    fn cost_uses_separate_input_and_output_rates() {
        let summary = UsageSummary {
            prompt_tokens: 1_000_000,
            output_tokens: 1_000_000,
            ..Default::default()
        };
        assert!((summary.estimated_cost(PRICING) - 2.80).abs() < 1e-9);
    }

    #[test]
    fn remaining_saturates_instead_of_underflowing() {
        let summary = UsageSummary {
            total_tokens: 5_000,
            ..Default::default()
        };
        assert_eq!(summary.remaining_tokens(1_000), 0);
        assert_eq!(summary.remaining_tokens(8_000), 3_000);
    }

    #[test]
    fn budget_percent_is_clamped_and_safe_at_zero_budget() {
        let summary = UsageSummary {
            total_tokens: 5_000,
            ..Default::default()
        };
        assert_eq!(
            summary.budget_used_percent(0),
            0.0,
            "must not divide by zero"
        );
        assert_eq!(summary.budget_used_percent(1_000), 100.0, "clamped");
        assert!((summary.budget_used_percent(10_000) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn feature_names_are_stable_for_storage() {
        assert_eq!(AiFeature::RoleplayTurn.as_str(), "roleplay_turn");
    }
}
