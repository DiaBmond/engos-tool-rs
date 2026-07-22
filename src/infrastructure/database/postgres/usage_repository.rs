use sqlx::PgPool;

use crate::application::usage::ports::UsageRepository;
use crate::domain::error::AppResult;
use crate::domain::usage::{FeatureUsage, UsageEvent, UsageSummary};

pub struct PostgresUsageRepository {
    pool: PgPool,
}

impl PostgresUsageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl UsageRepository for PostgresUsageRepository {
    async fn record_batch(&self, events: &[UsageEvent]) -> AppResult<()> {
        if events.is_empty() {
            return Ok(());
        }

        // UNNEST inserts the whole batch in one round trip rather than one
        // statement per recorded call.
        let models: Vec<String> = events.iter().map(|e| e.model.clone()).collect();
        let features: Vec<String> = events
            .iter()
            .map(|e| e.feature.as_str().to_string())
            .collect();
        let prompt: Vec<i32> = events
            .iter()
            .map(|e| e.usage.prompt_tokens as i32)
            .collect();
        let output: Vec<i32> = events
            .iter()
            .map(|e| e.usage.output_tokens as i32)
            .collect();
        let total: Vec<i32> = events.iter().map(|e| e.usage.total_tokens as i32).collect();

        sqlx::query!(
            r#"
            INSERT INTO ai_usage (model, feature, prompt_tokens, output_tokens, total_tokens)
            SELECT * FROM UNNEST($1::varchar[], $2::varchar[], $3::int[], $4::int[], $5::int[])
            "#,
            &models,
            &features,
            &prompt,
            &output,
            &total
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn summarize(&self, days: u32) -> AppResult<UsageSummary> {
        let interval = format!("{days} days");

        let totals = sqlx::query!(
            r#"
            SELECT
                COUNT(*)                       AS "calls!",
                COALESCE(SUM(prompt_tokens), 0) AS "prompt!",
                COALESCE(SUM(output_tokens), 0) AS "output!",
                COALESCE(SUM(total_tokens), 0)  AS "total!"
            FROM ai_usage
            WHERE created_at >= NOW() - $1::interval
            "#,
            &interval as &str
        )
        .fetch_one(&self.pool)
        .await?;

        let rows = sqlx::query!(
            r#"
            SELECT
                feature,
                COUNT(*)                      AS "calls!",
                COALESCE(SUM(total_tokens), 0) AS "total!"
            FROM ai_usage
            WHERE created_at >= NOW() - $1::interval
            GROUP BY feature
            ORDER BY 3 DESC
            "#,
            &interval as &str
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(UsageSummary {
            calls: totals.calls.max(0) as u64,
            prompt_tokens: totals.prompt.max(0) as u64,
            output_tokens: totals.output.max(0) as u64,
            total_tokens: totals.total.max(0) as u64,
            by_feature: rows
                .into_iter()
                .map(|r| FeatureUsage {
                    feature: r.feature,
                    calls: r.calls.max(0) as u64,
                    total_tokens: r.total.max(0) as u64,
                })
                .collect(),
        })
    }
}
