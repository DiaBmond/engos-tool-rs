use std::time::Duration;

use reqwest::Client;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::mpsc::UnboundedSender;

use crate::domain::error::{AppError, AppResult, Secret, redact_secrets};
use crate::domain::usage::{AiFeature, TokenUsage, UsageEvent};

const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Longest upstream error body we echo into logs. Provider errors can be large
/// and occasionally quote the request back.
const MAX_LOGGED_BODY: usize = 512;

#[derive(Clone)]
pub struct GeminiClient {
    http_client: Client,
    api_key: Secret,
    model: String,
    /// Where token accounting is sent. Unbounded and non-blocking so a slow or
    /// failing usage writer can never delay or fail a learner's turn.
    usage_tx: Option<UnboundedSender<UsageEvent>>,
}

impl GeminiClient {
    pub fn new(api_key: String, model: String) -> AppResult<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            // Previously `unwrap_or_default()`, which silently produced a client
            // with no timeout at all — exactly the wrong outcome for the one
            // dependency most likely to hang.
            .map_err(|e| AppError::Config(format!("failed to build Gemini HTTP client: {e}")))?;

        Ok(Self {
            http_client,
            api_key: Secret::new(api_key),
            model,
            usage_tx: None,
        })
    }

    /// Attaches the token-accounting channel.
    pub fn with_usage_channel(mut self, tx: UnboundedSender<UsageEvent>) -> Self {
        self.usage_tx = Some(tx);
        self
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    fn record_usage(&self, feature: AiFeature, usage: TokenUsage) {
        if usage.is_empty() {
            return;
        }

        if let Some(tx) = &self.usage_tx {
            // A closed channel means the writer task is gone; accounting is not
            // worth failing a turn over.
            let _ = tx.send(UsageEvent {
                model: self.model.clone(),
                feature,
                usage,
            });
        }
    }

    /// Reads `usageMetadata` from a response. Absent on some error shapes, in
    /// which case the call simply is not counted.
    fn extract_usage(res: &Value) -> TokenUsage {
        let Some(meta) = res.get("usageMetadata") else {
            return TokenUsage::default();
        };

        let field = |name: &str| meta.get(name).and_then(Value::as_u64).unwrap_or(0) as u32;

        TokenUsage::new(
            field("promptTokenCount"),
            // Thinking tokens are billed as output, so fold them in rather than
            // under-reporting spend.
            field("candidatesTokenCount") + field("thoughtsTokenCount"),
            field("totalTokenCount"),
        )
    }

    async fn call_api(&self, request_body: &Value) -> AppResult<Value> {
        // The key travels in a header, never the query string. As a URL
        // parameter it ended up inside `reqwest::Error`'s Display output and
        // was printed verbatim on every network failure.
        let url = format!("{GEMINI_BASE_URL}/{}:generateContent", self.model);

        let response = self
            .http_client
            .post(&url)
            .header("x-goog-api-key", self.api_key.expose())
            .json(request_body)
            .send()
            .await
            .map_err(|e| {
                AppError::AiUpstream(format!(
                    "request failed: {}",
                    redact_secrets(&e.to_string())
                ))
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let mut snippet = redact_secrets(&body);
            snippet.truncate(MAX_LOGGED_BODY);
            return Err(AppError::AiUpstream(format!(
                "gemini returned {status}: {snippet}"
            )));
        }

        response.json().await.map_err(|e| {
            AppError::AiParse(format!(
                "could not decode gemini response: {}",
                redact_secrets(&e.to_string())
            ))
        })
    }

    fn extract_text(res: &Value) -> AppResult<String> {
        // A blocked prompt returns 200 with no candidates but a `promptFeedback`
        // block, so surface that reason instead of a generic shape complaint.
        if let Some(reason) = res
            .get("promptFeedback")
            .and_then(|f| f.get("blockReason"))
            .and_then(|r| r.as_str())
        {
            return Err(AppError::AiUpstream(format!(
                "gemini blocked the prompt: {reason}"
            )));
        }

        res.get("candidates")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.get(0))
            .and_then(|p| p.get("text"))
            .and_then(|t| t.as_str())
            .map(str::to_string)
            .ok_or_else(|| AppError::AiParse("gemini response contained no text part".to_string()))
    }

    /// Strips markdown code fences the model sometimes adds despite the JSON
    /// response MIME type.
    fn strip_code_fence(raw: &str) -> &str {
        raw.trim()
            .trim_start_matches("```json")
            .trim_start_matches("```JSON")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
    }

    pub(crate) async fn generate_json<T: DeserializeOwned>(
        &self,
        feature: AiFeature,
        system_instruction: Option<&str>,
        user_prompt: &str,
    ) -> AppResult<T> {
        let mut body = json!({
            "contents": [{
                "parts": [{ "text": user_prompt }]
            }],
            "generationConfig": {
                "responseMimeType": "application/json"
            }
        });

        if let Some(sys) = system_instruction {
            body["systemInstruction"] = json!({ "parts": [{ "text": sys }] });
        }

        let response_json = self.call_api(&body).await?;

        // Recorded before parsing: the tokens were spent whether or not the
        // payload turns out to be usable.
        self.record_usage(feature, Self::extract_usage(&response_json));

        let raw_text = Self::extract_text(&response_json)?;
        let cleaned = Self::strip_code_fence(&raw_text);

        serde_json::from_str::<T>(cleaned).map_err(|e| {
            // The raw model output goes to the log, never to the learner.
            let mut snippet = cleaned.to_string();
            snippet.truncate(MAX_LOGGED_BODY);
            AppError::AiParse(format!("could not map gemini JSON: {e}; output: {snippet}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_json_code_fences() {
        assert_eq!(
            GeminiClient::strip_code_fence("```json\n{\"a\":1}\n```"),
            "{\"a\":1}"
        );
        assert_eq!(
            GeminiClient::strip_code_fence("```\n{\"a\":1}\n```"),
            "{\"a\":1}"
        );
        assert_eq!(GeminiClient::strip_code_fence("{\"a\":1}"), "{\"a\":1}");
    }

    #[test]
    fn extracts_text_from_a_well_formed_response() {
        let res = json!({
            "candidates": [{ "content": { "parts": [{ "text": "hello" }] } }]
        });
        assert_eq!(GeminiClient::extract_text(&res).unwrap(), "hello");
    }

    #[test]
    fn reports_block_reason_rather_than_shape_error() {
        let res = json!({ "promptFeedback": { "blockReason": "SAFETY" } });
        let err = GeminiClient::extract_text(&res).unwrap_err();
        assert!(matches!(err, AppError::AiUpstream(_)));
        assert!(err.to_string().contains("SAFETY"));
    }

    #[test]
    fn missing_candidates_is_a_parse_error() {
        let err = GeminiClient::extract_text(&json!({})).unwrap_err();
        assert!(matches!(err, AppError::AiParse(_)));
    }

    #[test]
    fn reads_token_counts_from_usage_metadata() {
        let res = json!({
            "usageMetadata": {
                "promptTokenCount": 120,
                "candidatesTokenCount": 45,
                "totalTokenCount": 165
            }
        });
        let usage = GeminiClient::extract_usage(&res);
        assert_eq!(usage.prompt_tokens, 120);
        assert_eq!(usage.output_tokens, 45);
        assert_eq!(usage.total_tokens, 165);
    }

    /// Thinking tokens are billed but reported separately; omitting them would
    /// under-report spend on reasoning models.
    #[test]
    fn folds_thinking_tokens_into_output() {
        let res = json!({
            "usageMetadata": {
                "promptTokenCount": 100,
                "candidatesTokenCount": 40,
                "thoughtsTokenCount": 60,
                "totalTokenCount": 200
            }
        });
        let usage = GeminiClient::extract_usage(&res);
        assert_eq!(usage.output_tokens, 100, "40 visible + 60 thinking");
    }

    #[test]
    fn missing_usage_metadata_is_not_counted() {
        assert!(GeminiClient::extract_usage(&json!({})).is_empty());
    }
}
