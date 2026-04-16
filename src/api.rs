use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::warn;

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
}

#[derive(Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

pub trait ChatClient: Send + Sync + 'static {
    fn chat(&self, system_prompt: &str, user_message: &str) -> impl std::future::Future<Output = Result<String>> + Send;
}

pub struct ApiClient {
    client: Client,
    endpoint: String,
    auth_header: String,
    model: String,
}

impl ApiClient {
    pub fn new(base_url: &str, api_key: String, model: String) -> Self {
        let base = base_url.trim_end_matches('/');
        let endpoint = format!("{base}/chat/completions");
        let auth_header = format!("Bearer {api_key}");
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            endpoint,
            auth_header,
            model,
        }
    }

}

impl ChatClient for ApiClient {
    async fn chat(&self, system_prompt: &str, user_message: &str) -> Result<String> {
        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                Message {
                    role: "user".to_string(),
                    content: user_message.to_string(),
                },
            ],
            temperature: 0.0,
        };

        let max_retries = 3;
        for attempt in 1..=max_retries {
            let resp = self
                .client
                .post(&self.endpoint)
                .header("Authorization", &self.auth_header)
                .json(&request)
                .send()
                .await
                .context("failed to send request")?;

            let status = resp.status();
            if status.is_success() {
                let body: ChatResponse = resp.json().await.context("failed to parse response")?;
                let content = body
                    .choices
                    .into_iter()
                    .next()
                    .map(|c| c.message.content)
                    .unwrap_or_default();
                return Ok(content.trim().to_string());
            }

            let retryable = status.as_u16() == 429 || status.is_server_error();
            if retryable && attempt < max_retries {
                let delay = Duration::from_millis(500 * 2u64.pow(attempt as u32 - 1));
                warn!(status = status.as_u16(), attempt, "retrying after {:?}", delay);
                tokio::time::sleep(delay).await;
                continue;
            }

            let body = resp.text().await.unwrap_or_default();
            bail!("API error {status}: {body}");
        }

        unreachable!()
    }
}
