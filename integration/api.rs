// Auto-patched by Alloma
// Timestamp: 2025-06-02 00:16:05
// User: kartik4091

#![allow(warnings)]

use std::collections::HashMap;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use tokio::sync::RwLock;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ApiClient {
    config: ApiConfig,
    client: reqwest::Client,
    cache: Arc<RwLock<HashMap<String, CachedResponse>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub base_url: String,
    pub timeout: std::time::Duration,
    pub retry_attempts: u32,
    pub api_key: Option<String>,
    pub cache_ttl: std::time::Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequest {
    pub method: HttpMethod,
    pub endpoint: String,
    pub headers: HashMap<String, String>,
    pub query_params: HashMap<String, String>,
    pub body: Option<serde_json::Value>,
    pub timeout: Option<std::time::Duration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Option<serde_json::Value>,
    pub timing: ResponseTiming,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseTiming {
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: chrono::DateTime<chrono::Utc>,
    pub duration: std::time::Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
}

#[derive(Debug, Clone)]
struct CachedResponse {
    response: ApiResponse,
    expires_at: chrono::DateTime<chrono::Utc>,
}

impl ApiClient {
    pub fn new(config: ApiConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .unwrap();

        ApiClient {
            config,
            client,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn send(&self, request: ApiRequest) -> Result<ApiResponse, ApiError> {
        // Check cache first
        if request.method == HttpMethod::GET {
            if let Some(cached) = self.get_from_cache(&request).await? {
                return Ok(cached);
            }
        }

        let start_time = chrono::Utc::now();
        let mut attempts = 0;

        loop {
            match self.execute_request(&request).await {
                Ok(mut response) => {
                    let end_time = chrono::Utc::now();
                    response.timing = ResponseTiming {
                        start_time,
                        end_time,
                        duration: end_time - start_time,
                    };

                    // Cache if it's a GET request
                    if request.method == HttpMethod::GET {
                        self.cache_response(&request, &response).await?;
                    }

                    return Ok(response);
                }
                Err(e) => {
                    attempts += 1;
                    if attempts >= self.config.retry_attempts {
                        return Err(e);
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempts))).await;
                }
            }
        }
    }

    async fn execute_request(&self, request: &ApiRequest) -> Result<ApiResponse, ApiError> {
        let url = format!("{}{}", self.config.base_url, request.endpoint);
        let mut builder = self.client.request(request.method.clone().into(), &url);

        // Add headers
        for (key, value) in &request.headers {
            builder = builder.header(key, value);
        }

        // Add API key if present
        if let Some(api_key) = &self.config.api_key {
            builder = builder.header("Authorization", format!("Bearer {}", api_key));
        }

        // Add query parameters
        if !request.query_params.is_empty() {
            builder = builder.query(&request.query_params);
        }

        // Add body if present
        if let Some(body) = &request.body {
            builder = builder.json(body);
        }

        // Send request
        let response = builder.send().await.map_err(|e| ApiError::RequestFailed(e.to_string()))?;

        // Build response
        Ok(ApiResponse {
            status: response.status().as_u16(),
            headers: response.headers()
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                .collect(),
            body: response.json().await.ok(),
            timing: ResponseTiming {
                start_time: chrono::Utc::now(),
                end_time: chrono::Utc::now(),
                duration: std::time::Duration::from_secs(0),
            },
        })
    }

    async fn get_from_cache(&self, request: &ApiRequest) -> Result<Option<ApiResponse>, ApiError> {
        let cache = self.cache.read().await;
        let cache_key = self.create_cache_key(request);

        if let Some(cached) = cache.get(&cache_key) {
            if cached.expires_at > chrono::Utc::now() {
                return Ok(Some(cached.response.clone()));
            }
        }

        Ok(None)
    }

    async fn cache_response(&self, request: &ApiRequest, response: &ApiResponse) -> Result<(), ApiError> {
        let mut cache = self.cache.write().await;
        let cache_key = self.create_cache_key(request);

        cache.insert(cache_key, CachedResponse {
            response: response.clone(),
            expires_at: chrono::Utc::now() + self.config.cache_ttl,
        });

        Ok(())
    }

    fn create_cache_key(&self, request: &ApiRequest) -> String {
        format!("{}{}{:?}", 
            request.method, 
            request.endpoint,
            request.query_params
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Request failed: {0}")]
    RequestFailed(String),
    
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    
    #[error("Cache error: {0}")]
    CacheError(String),
}

impl From<HttpMethod> for reqwest::Method {
    fn from(method: HttpMethod) -> Self {
        match method {
            HttpMethod::GET => reqwest::Method::GET,
            HttpMethod::POST => reqwest::Method::POST,
            HttpMethod::PUT => reqwest::Method::PUT,
            HttpMethod::DELETE => reqwest::Method::DELETE,
            HttpMethod::PATCH => reqwest::Method::PATCH,
            HttpMethod::HEAD => reqwest::Method::HEAD,
            HttpMethod::OPTIONS => reqwest::Method::OPTIONS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_api_client() {
        let config = ApiConfig {
            base_url: "https://api.example.com".to_string(),
            timeout: std::time::Duration::from_secs(30),
            retry_attempts: 3,
            api_key: Some("test-key".to_string()),
            cache_ttl: std::time::Duration::from_secs(300),
        };

        let client = ApiClient::new(config);

        let request = ApiRequest {
            method: HttpMethod::GET,
            endpoint: "/test".to_string(),
            headers: HashMap::new(),
            query_params: HashMap::new(),
            body: None,
            timeout: None,
        };

        // This will fail since we're using a fake URL
        let result = client.send(request).await;
        assert!(result.is_err());
    }
}