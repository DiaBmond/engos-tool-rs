use reqwest::Client;
use serde_json::{json, Value};
use std::env;
use std::future::Future;
use std::time::Duration;

pub trait LineMessagingPort: Send + Sync {
    fn reply_text(&self, reply_token: &str, text: &str) -> impl Future<Output = Result<(), String>> + Send;
    fn reply_custom(&self, reply_token: &str, messages: Vec<Value>) -> impl Future<Output = Result<(), String>> + Send;
    fn push_text(&self, user_id: &str, text: &str) -> impl Future<Output = Result<(), String>> + Send;
}

#[derive(Clone)]
pub struct LineClient {
    http_client: Client,
    access_token: String,
}

impl LineClient {
    pub fn new(access_token: String) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15)) 
            .build()
            .unwrap_or_default();

        Self {
            http_client,
            access_token,
        }
    }

    pub fn from_env() -> Self {
        let access_token = env::var("LINE_CHANNEL_ACCESS_TOKEN")
            .expect("The LINE_CHANNEL_ACCESS_TOKEN file was not found in the .env file");
        Self::new(access_token)
    }

    async fn send_request(&self, endpoint: &str, payload: &Value) -> Result<(), String> {
        let url = format!("https://api.line.me/v2/bot/message/{}", endpoint);

        let response = self
            .http_client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(payload)
            .send()
            .await
            .map_err(|e| format!("The HTTP request to LINE failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("LINE API Error: {}", error_text));
        }

        Ok(())
    }
}

impl LineMessagingPort for LineClient {
    async fn reply_text(&self, reply_token: &str, text: &str) -> Result<(), String> {
        let payload = json!({
            "replyToken": reply_token,
            "messages": [
                {
                    "type": "text",
                    "text": text
                }
            ]
        });

        self.send_request("reply", &payload).await
    }

    async fn reply_custom(&self, reply_token: &str, messages: Vec<Value>) -> Result<(), String> {
        let payload = json!({
            "replyToken": reply_token,
            "messages": messages
        });

        self.send_request("reply", &payload).await
    }

    async fn push_text(&self, user_id: &str, text: &str) -> Result<(), String> {
        let payload = json!({
            "to": user_id,
            "messages": [
                {
                    "type": "text",
                    "text": text
                }
            ]
        });

        self.send_request("push", &payload).await
    }
}