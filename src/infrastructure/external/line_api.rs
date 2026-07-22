use std::time::Duration;

use reqwest::Client;
use serde_json::{Value, json};

use crate::application::messaging::ports::MessagingPort;
use crate::domain::error::{AppError, AppResult, Secret, redact_secrets};

const LINE_API_BASE: &str = "https://api.line.me/v2/bot/message";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// LINE rejects text messages longer than this.
const MAX_MESSAGE_CHARS: usize = 5_000;

const MAX_LOGGED_BODY: usize = 512;

#[derive(Clone)]
pub struct LineClient {
    http_client: Client,
    access_token: Secret,
}

impl LineClient {
    pub fn new(access_token: String) -> AppResult<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| AppError::Config(format!("failed to build LINE HTTP client: {e}")))?;

        Ok(Self {
            http_client,
            access_token: Secret::new(access_token),
        })
    }

    /// Truncates to LINE's limit on a character boundary.
    fn clamp(text: &str) -> String {
        if text.chars().count() <= MAX_MESSAGE_CHARS {
            return text.to_string();
        }
        text.chars().take(MAX_MESSAGE_CHARS - 1).collect::<String>() + "…"
    }

    async fn send_request(&self, endpoint: &str, payload: &Value) -> AppResult<()> {
        let url = format!("{LINE_API_BASE}/{endpoint}");

        let response = self
            .http_client
            .post(&url)
            .bearer_auth(self.access_token.expose())
            .json(payload)
            .send()
            .await
            .map_err(|e| {
                AppError::Messaging(format!(
                    "request to LINE failed: {}",
                    redact_secrets(&e.to_string())
                ))
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let mut snippet = redact_secrets(&body);
            snippet.truncate(MAX_LOGGED_BODY);
            return Err(AppError::Messaging(format!(
                "LINE returned {status}: {snippet}"
            )));
        }

        Ok(())
    }
}

impl MessagingPort for LineClient {
    async fn reply_text(&self, reply_token: &str, text: &str) -> AppResult<()> {
        let payload = json!({
            "replyToken": reply_token,
            "messages": [{ "type": "text", "text": Self::clamp(text) }]
        });

        self.send_request("reply", &payload).await
    }

    async fn push_text(&self, user_id: &str, text: &str) -> AppResult<()> {
        let payload = json!({
            "to": user_id,
            "messages": [{ "type": "text", "text": Self::clamp(text) }]
        });

        self.send_request("push", &payload).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_messages_pass_through_untouched() {
        assert_eq!(LineClient::clamp("hello"), "hello");
    }

    #[test]
    fn long_messages_are_truncated_to_the_line_limit() {
        let clamped = LineClient::clamp(&"a".repeat(MAX_MESSAGE_CHARS + 100));
        assert_eq!(clamped.chars().count(), MAX_MESSAGE_CHARS);
        assert!(clamped.ends_with('…'));
    }

    #[test]
    fn truncation_respects_multibyte_boundaries() {
        // Thai text would panic on a naive byte-index truncation.
        let clamped = LineClient::clamp(&"ก".repeat(MAX_MESSAGE_CHARS + 10));
        assert_eq!(clamped.chars().count(), MAX_MESSAGE_CHARS);
    }
}
