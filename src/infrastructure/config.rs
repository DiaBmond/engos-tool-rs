use std::env;
use std::time::Duration;

use crate::domain::error::{AppError, AppResult, Secret};
use crate::domain::usage::TokenPricing;

const DEFAULT_GEMINI_MODEL: &str = "gemini-2.5-flash";

/// Token allowance the usage report measures against. Not enforced — this is a
/// personal tool, so the point is visibility, not rationing.
const DEFAULT_TOKEN_BUDGET: u64 = 1_000_000;

/// Rough Gemini 2.5 Flash list price in USD per million tokens, used only to
/// turn token counts into a readable estimate. Override both when the model or
/// the provider's pricing changes.
const DEFAULT_PRICE_INPUT_PER_MTOK: f64 = 0.30;
const DEFAULT_PRICE_OUTPUT_PER_MTOK: f64 = 2.50;

/// Everything the process reads from the environment, validated once at start-up
/// so a misconfigured deployment fails immediately instead of on first request.
///
/// `RUST_LOG` and `LOG_FORMAT` are deliberately absent: logging is initialised
/// before this runs, so that configuration errors can themselves be logged.
#[derive(Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database_url: Secret,
    pub redis_url: Secret,
    pub line_channel_secret: Secret,
    pub line_access_token: Secret,
    pub gemini_api_key: Secret,
    pub gemini_model: String,
    pub db_max_connections: u32,
    pub db_acquire_timeout: Duration,
    pub ai_token_budget: u64,
    pub ai_pricing: TokenPricing,
}

impl AppConfig {
    pub fn from_env() -> AppResult<Self> {
        Ok(Self {
            host: optional("HOST", "0.0.0.0"),
            port: parsed("PORT", 8080)?,
            database_url: Secret::new(required("DATABASE_URL")?),
            redis_url: Secret::new(required("REDIS_URL")?),
            // Without this the webhook cannot be authenticated at all, so it is
            // mandatory rather than defaulted.
            line_channel_secret: Secret::new(required("LINE_CHANNEL_SECRET")?),
            line_access_token: Secret::new(required("LINE_CHANNEL_ACCESS_TOKEN")?),
            gemini_api_key: Secret::new(required("GEMINI_API_KEY")?),
            gemini_model: optional("GEMINI_MODEL", DEFAULT_GEMINI_MODEL),
            db_max_connections: parsed("DB_MAX_CONNECTIONS", 20)?,
            db_acquire_timeout: Duration::from_secs(parsed("DB_ACQUIRE_TIMEOUT_SECS", 5)?),
            ai_token_budget: parsed("AI_TOKEN_BUDGET", DEFAULT_TOKEN_BUDGET)?,
            ai_pricing: TokenPricing {
                input_per_mtok: parsed("AI_PRICE_INPUT_PER_MTOK", DEFAULT_PRICE_INPUT_PER_MTOK)?,
                output_per_mtok: parsed("AI_PRICE_OUTPUT_PER_MTOK", DEFAULT_PRICE_OUTPUT_PER_MTOK)?,
            },
        })
    }

    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

fn required(key: &str) -> AppResult<String> {
    let value = env::var(key)
        .map_err(|_| AppError::Config(format!("required environment variable {key} is not set")))?;

    if value.trim().is_empty() {
        return Err(AppError::Config(format!(
            "required environment variable {key} is empty"
        )));
    }

    Ok(value)
}

fn optional(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn parsed<T: std::str::FromStr>(key: &str, default: T) -> AppResult<T> {
    match env::var(key) {
        Err(_) => Ok(default),
        Ok(raw) if raw.trim().is_empty() => Ok(default),
        Ok(raw) => raw
            .trim()
            .parse()
            .map_err(|_| AppError::Config(format!("{key} is not a valid value: {raw:?}"))),
    }
}
