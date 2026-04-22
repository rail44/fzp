use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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
    /// Unix timestamp in milliseconds until which requests should be paused (shared across all tasks)
    rate_limit_until_ms: AtomicU64,
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
            rate_limit_until_ms: AtomicU64::new(0),
        }
    }

    async fn wait_for_rate_limit(&self) {
        let until_ms = self.rate_limit_until_ms.load(Ordering::Relaxed);
        if until_ms == 0 {
            return;
        }
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        if now_ms < until_ms {
            let wait = Duration::from_millis(until_ms - now_ms);
            tokio::time::sleep(wait).await;
        }
    }

    fn update_rate_limit(&self, resp: &reqwest::Response) {
        // Try X-RateLimit-Reset (Unix timestamp in ms, used by OpenRouter)
        if let Some(reset_ms) = resp
            .headers()
            .get("x-ratelimit-reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
        {
            self.rate_limit_until_ms.fetch_max(reset_ms, Ordering::Relaxed);
            return;
        }

        // Fall back to Retry-After (seconds)
        if let Some(secs) = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
        {
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let until_ms = now_ms + secs.min(60) * 1000;
            self.rate_limit_until_ms.fetch_max(until_ms, Ordering::Relaxed);
            return;
        }

        // No header: set a fallback pause of 2 seconds from now
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.rate_limit_until_ms.fetch_max(now_ms + 2000, Ordering::Relaxed);
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

        let max_retries: u32 = 10;
        for attempt in 1..=max_retries {
            // Wait if a rate limit is globally active
            self.wait_for_rate_limit().await;

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

            let is_rate_limited = status.as_u16() == 429;
            let retryable = is_rate_limited || status.is_server_error();
            if retryable && attempt < max_retries {
                if is_rate_limited {
                    self.update_rate_limit(&resp);
                    self.wait_for_rate_limit().await;
                } else {
                    let delay = Duration::from_millis(500 * 2u64.pow((attempt - 1).min(3)));
                    warn!(status = status.as_u16(), attempt, "retrying after {:?}", delay);
                    tokio::time::sleep(delay).await;
                }
                continue;
            }

            let body = resp.text().await.unwrap_or_default();
            bail!("API error {status}: {body}");
        }

        unreachable!()
    }
}
