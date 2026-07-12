use reqwest::Client;
use serde_json::Value;
use std::env;
use std::time::Duration;

#[derive(Clone)]
pub struct GeminiClient {
    http_client: Client,
    api_key: String,
    model: String,
}

impl GeminiClient {
    pub fn new(api_key: String, model: String) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            http_client,
            api_key,
            model,
        }
    }

    pub fn from_env() -> Self {
        let api_key = env::var("GEMINI_API_KEY")
            .expect("The GEMINI_API_KEY was not found in the .env file.");
        let model = env::var("GEMINI_MODEL")
            .unwrap_or_else(|_| "gemini-3.5-flash".to_string());

        Self::new(api_key, model)
    }

    pub(crate) async fn call_api(&self, request_body: &Value) -> Result<Value, String> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let response = self
            .http_client
            .post(&url)
            .json(request_body)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Gemini API Error: {}", error_text));
        }

        let json_res: Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

        Ok(json_res)
    }
}