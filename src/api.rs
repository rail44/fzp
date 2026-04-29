use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::warn;

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<Message>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<&'a ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<ProviderRouting>,
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
    json_schema: JsonSchemaSpec,
}

#[derive(Serialize)]
struct JsonSchemaSpec {
    name: &'static str,
    strict: bool,
    schema: serde_json::Value,
}

#[derive(Serialize)]
struct ProviderRouting {
    require_parameters: bool,
}

#[derive(Serialize, Deserialize, Clone)]
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
    api_key: String,
    auth_header: String,
    model: String,
    response_format: Option<ResponseFormat>,
    /// Unix timestamp in milliseconds until which requests should be paused (shared across all tasks)
    rate_limit_until_ms: AtomicU64,
}

impl ApiClient {
    pub fn new(
        base_url: &str,
        api_key: String,
        model: String,
        output_schema: Option<serde_json::Value>,
    ) -> Self {
        let base = base_url.trim_end_matches('/');
        let endpoint = format!("{base}/chat/completions");
        let auth_header = format!("Bearer {api_key}");
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("failed to build HTTP client");
        let response_format = output_schema.map(|schema| ResponseFormat {
            kind: "json_schema",
            json_schema: JsonSchemaSpec {
                name: "fzp_output",
                strict: true,
                schema,
            },
        });
        Self {
            client,
            endpoint,
            api_key,
            auth_header,
            model,
            response_format,
            rate_limit_until_ms: AtomicU64::new(0),
        }
    }

    /// Replace the configured API key with `***` in the given text. Used before
    /// emitting upstream error bodies so a 4xx echo of the request can never
    /// leak the secret to logs or stderr.
    fn redact(&self, text: &str) -> String {
        if self.api_key.is_empty() {
            return text.to_string();
        }
        text.replace(&self.api_key, "***")
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
            model: &self.model,
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
            response_format: self.response_format.as_ref(),
            provider: self
                .response_format
                .is_some()
                .then_some(ProviderRouting { require_parameters: true }),
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
            let body = self.redact(&body);
            bail!("API error {status}: {body}");
        }

        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_client(api_key: &str) -> ApiClient {
        ApiClient::new(
            "https://example.com",
            api_key.to_string(),
            "m".to_string(),
            None,
        )
    }

    #[test]
    fn redact_replaces_api_key() {
        let client = make_client("sk-secret-123");
        let out = client.redact(r#"{"error": "bad token: sk-secret-123"}"#);
        assert_eq!(out, r#"{"error": "bad token: ***"}"#);
    }

    #[test]
    fn redact_passes_through_when_key_absent() {
        let client = make_client("");
        let out = client.redact("anything goes here");
        assert_eq!(out, "anything goes here");
    }

    #[test]
    fn redact_handles_multiple_occurrences() {
        let client = make_client("k");
        let out = client.redact("k and k and k");
        assert_eq!(out, "*** and *** and ***");
    }

    #[test]
    fn request_serializes_without_response_format_by_default() {
        let client = ApiClient::new(
            "https://example.com",
            "k".to_string(),
            "m".to_string(),
            None,
        );
        let request = ChatRequest {
            model: &client.model,
            messages: vec![],
            temperature: 0.0,
            response_format: client.response_format.as_ref(),
            provider: client
                .response_format
                .is_some()
                .then_some(ProviderRouting { require_parameters: true }),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(!json.contains("response_format"));
        assert!(!json.contains("provider"));
    }

    #[test]
    fn request_serializes_response_format_when_schema_set() {
        let schema = serde_json::json!({"type": "object"});
        let client = ApiClient::new(
            "https://example.com",
            "k".to_string(),
            "m".to_string(),
            Some(schema),
        );
        let request = ChatRequest {
            model: &client.model,
            messages: vec![],
            temperature: 0.0,
            response_format: client.response_format.as_ref(),
            provider: client
                .response_format
                .is_some()
                .then_some(ProviderRouting { require_parameters: true }),
        };
        let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
        assert_eq!(v["response_format"]["type"], "json_schema");
        assert_eq!(v["response_format"]["json_schema"]["name"], "fzp_output");
        assert_eq!(v["response_format"]["json_schema"]["strict"], true);
        assert_eq!(v["response_format"]["json_schema"]["schema"]["type"], "object");
        assert_eq!(v["provider"]["require_parameters"], true);
    }
}
