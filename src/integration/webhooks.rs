// Auto-patched by Alloma
// Timestamp: 2025-06-02 00:19:37
// User: kartik4091

#![allow(warnings)]

use std::collections::HashMap;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use tokio::sync::RwLock;
use std::sync::Arc;

pub struct WebhookManager {
    webhooks: Arc<RwLock<HashMap<String, Webhook>>>,
    config: WebhookConfig,
    client: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub max_retries: u32,
    pub timeout: std::time::Duration,
    pub batch_size: usize,
    pub verify_ssl: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Webhook {
    pub id: String,
    pub url: String,
    pub events: Vec<String>,
    pub headers: HashMap<String, String>,
    pub secret: Option<String>,
    pub enabled: bool,
    pub retry_policy: RetryPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: EventMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    pub source: String,
    pub version: String,
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff_factor: f32,
    pub jitter: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookResponse {
    pub status_code: u16,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
    pub timing: ResponseTiming,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseTiming {
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: chrono::DateTime<chrono::Utc>,
    pub duration: std::time::Duration,
}

impl WebhookManager {
    pub fn new(config: WebhookConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .danger_accept_invalid_certs(!config.verify_ssl)
            .build()
            .unwrap();

        WebhookManager {
            webhooks: Arc::new(RwLock::new(HashMap::new())),
            config,
            client,
        }
    }

    pub async fn register_webhook(&self, webhook: Webhook) -> Result<(), WebhookError> {
        let mut webhooks = self.webhooks.write().await;
        webhooks.insert(webhook.id.clone(), webhook);
        Ok(())
    }

    pub async fn trigger_event(&self, event: WebhookEvent) -> Result<Vec<WebhookResponse>, WebhookError> {
        let webhooks = self.webhooks.read().await;
        let mut responses = Vec::new();

        for webhook in webhooks.values() {
            if webhook.enabled && webhook.events.contains(&event.event_type) {
                match self.send_webhook(webhook, &event).await {
                    Ok(response) => responses.push(response),
                    Err(e) => {
                        // Handle error but continue with other webhooks
                        eprintln!("Webhook error: {}", e);
                    }
                }
            }
        }

        Ok(responses)
    }

    pub async fn batch_trigger(&self, events: Vec<WebhookEvent>) -> Result<HashMap<String, Vec<WebhookResponse>>, WebhookError> {
        let mut results = HashMap::new();

        for chunk in events.chunks(self.config.batch_size) {
            for event in chunk {
                let responses = self.trigger_event(event.clone()).await?;
                results.insert(event.event_type.clone(), responses);
            }
        }

        Ok(results)
    }

    async fn send_webhook(&self, webhook: &Webhook, event: &WebhookEvent) -> Result<WebhookResponse, WebhookError> {
        let mut attempts = 0;
        let start_time = chrono::Utc::now();

        loop {
            let request = self.build_request(webhook, event)?;
            match self.client.execute(request).await {
                Ok(response) => {
                    let end_time = chrono::Utc::now();
                    return Ok(WebhookResponse {
                        status_code: response.status().as_u16(),
                        headers: response.headers()
                            .iter()
                            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                            .collect(),
                        body: response.text().await.ok(),
                        timing: ResponseTiming {
                            start_time,
                            end_time,
                            duration: end_time - start_time,
                        },
                    });
                }
                Err(e) => {
                    attempts += 1;
                    if attempts >= webhook.retry_policy.max_attempts {
                        return Err(WebhookError::RequestFailed(e.to_string()));
                    }
                    let delay = self.calculate_backoff(&webhook.retry_policy, attempts);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    fn build_request(&self, webhook: &Webhook, event: &WebhookEvent) -> Result<reqwest::Request, WebhookError> {
        let mut builder = self.client.post(&webhook.url)
            .json(&event);

        // Add headers
        for (key, value) in &webhook.headers {
            builder = builder.header(key, value);
        }

        // Add signature if secret is present
        if let Some(secret) = &webhook.secret {
            let signature = self.generate_signature(secret, event)?;
            builder = builder.header("X-Webhook-Signature", signature);
        }

        builder.build().map_err(|e| WebhookError::RequestBuildFailed(e.to_string()))
    }

    fn generate_signature(&self, secret: &str, event: &WebhookEvent) -> Result<String, WebhookError> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let payload = serde_json::to_string(event)
            .map_err(|e| WebhookError::SignatureError(e.to_string()))?;

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| WebhookError::SignatureError(e.to_string()))?;

        mac.update(payload.as_bytes());
        let result = mac.finalize();
        Ok(hex::encode(result.into_bytes()))
    }

    fn calculate_backoff(&self, policy: &RetryPolicy, attempt: u32) -> std::time::Duration {
        let base = policy.backoff_factor * (2_f32.powf(attempt as f32) - 1.0);
        let jitter = if policy.jitter {
            rand::random::<f32>()
        } else {
            0.0
        };
        std::time::Duration::from_secs_f32(base + jitter)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("Failed to send webhook request: {0}")]
    RequestFailed(String),

    #[error("Failed to build request: {0}")]
    RequestBuildFailed(String),

    #[error("Failed to generate signature: {0}")]
    SignatureError(String),

    #[error("Invalid webhook configuration: {0}")]
    ConfigurationError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_webhook_registration() {
        let config = WebhookConfig {
            max_retries: 3,
            timeout: std::time::Duration::from_secs(30),
            batch_size: 10,
            verify_ssl: true,
        };

        let manager = WebhookManager::new(config);

        let webhook = Webhook {
            id: "test".to_string(),
            url: "https://example.com/webhook".to_string(),
            events: vec!["test.event".to_string()],
            headers: HashMap::new(),
            secret: None,
            enabled: true,
            retry_policy: RetryPolicy {
                max_attempts: 3,
                backoff_factor: 1.5,
                jitter: true,
            },
        };

        let result = manager.register_webhook(webhook).await;
        assert!(result.is_ok());
    }
}