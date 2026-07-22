use std::future::Future;

use crate::domain::error::AppResult;
use crate::domain::usage::{TokenPricing, UsageEvent, UsageSummary};

/// Driven port: persistence for AI token accounting.
pub trait UsageRepository: Send + Sync {
    /// Writes a batch of recorded calls. Batched because usage is flushed from a
    /// background queue rather than on the request path.
    fn record_batch(&self, events: &[UsageEvent]) -> impl Future<Output = AppResult<()>> + Send;

    /// Totals over the trailing `days`, with a per-feature breakdown.
    fn summarize(&self, days: u32) -> impl Future<Output = AppResult<UsageSummary>> + Send;
}

/// A rendered usage report.
#[derive(Debug, Clone)]
pub struct UsageReport {
    pub period_days: u32,
    pub summary: UsageSummary,
    pub budget_tokens: u64,
    pub pricing: TokenPricing,
    pub model: String,
}

impl UsageReport {
    pub fn estimated_cost(&self) -> f64 {
        self.summary.estimated_cost(self.pricing)
    }

    pub fn remaining_tokens(&self) -> u64 {
        self.summary.remaining_tokens(self.budget_tokens)
    }

    pub fn budget_used_percent(&self) -> f64 {
        self.summary.budget_used_percent(self.budget_tokens)
    }
}

/// Driving port: what the transport layer may ask about AI spend.
pub trait UsageUseCase: Send + Sync {
    fn report(&self, days: u32) -> impl Future<Output = AppResult<UsageReport>> + Send;
}
