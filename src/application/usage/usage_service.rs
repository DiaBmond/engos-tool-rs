use super::ports::{UsageReport, UsageRepository, UsageUseCase};
use crate::domain::error::AppResult;
use crate::domain::usage::TokenPricing;

pub struct UsageService<R: UsageRepository> {
    repo: R,
    budget_tokens: u64,
    pricing: TokenPricing,
    model: String,
}

impl<R: UsageRepository> UsageService<R> {
    pub fn new(repo: R, budget_tokens: u64, pricing: TokenPricing, model: String) -> Self {
        Self {
            repo,
            budget_tokens,
            pricing,
            model,
        }
    }
}

impl<R: UsageRepository> UsageUseCase for UsageService<R> {
    async fn report(&self, days: u32) -> AppResult<UsageReport> {
        Ok(UsageReport {
            period_days: days,
            summary: self.repo.summarize(days).await?,
            budget_tokens: self.budget_tokens,
            pricing: self.pricing,
            model: self.model.clone(),
        })
    }
}
