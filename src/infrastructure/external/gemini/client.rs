use reqwest::Client;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
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
            .unwrap_or_else(|_| "gemini-2.5-flash".to_string());

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

    fn extract_text_from_response(&self, res: &Value) -> Result<String, String> {
        res.get("candidates")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.get(0))
            .and_then(|p| p.get("text"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("Invalid response structure from Gemini: {}", res))
    }

    pub(crate) async fn generate_json<T: DeserializeOwned>(
        &self,
        system_instruction: Option<&str>,
        user_prompt: &str,
    ) -> Result<T, String> {
        let mut body = json!({
            "contents": [{
                "parts": [{ "text": user_prompt }]
            }],
            "generationConfig": {
                "responseMimeType": "application/json"
            }
        });

        if let Some(sys) = system_instruction {
            body["systemInstruction"] = json!({
                "parts": [{ "text": sys }]
            });
        }

        let response_json = self.call_api(&body).await?;
        let raw_text = self.extract_text_from_response(&response_json)?;

        let cleaned_text = raw_text
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        serde_json::from_str::<T>(cleaned_text)
            .map_err(|e| format!("Failed to map Gemini JSON to Struct: {} \nRaw AI output: {}", e, cleaned_text))
    }
}