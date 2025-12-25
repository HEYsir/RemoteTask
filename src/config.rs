use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// HTTP 请求配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequestConfig {
    pub method: String, // "GET" or "POST"
    pub url: String,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<String>, // JSON string for POST requests
}

/// Digest 认证配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestAuthConfig {
    pub username: String,
    pub password: String,
    pub realm: Option<String>,
    pub nonce: Option<String>,
}

/// 动态生成字段配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedField {
    pub name: String,          // 字段名
    pub generator: String,     // 生成器类型："random", "timestamp", "counter", "uuid"
    pub field_type: String,    // 字段类型："header" 或 "body"
    pub value: Option<String>, // 生成的值（可选，用于固定值）
}

/// 主配置结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestConfig {
    pub request_a: HttpRequestConfig,
    pub request_b: HttpRequestConfig,
    pub delay_between_a_and_b_ms: u64,
    pub delay_between_a_requests_ms: u64,
    pub max_requests: Option<usize>,
    pub digest_auth: Option<DigestAuthConfig>,
    pub generated_fields: Option<Vec<GeneratedField>>,
}

impl Default for RequestConfig {
    fn default() -> Self {
        Self {
            request_a: HttpRequestConfig {
                method: "GET".to_string(),
                url: "https://httpbin.org/get".to_string(),
                headers: None,
                body: None,
            },
            request_b: HttpRequestConfig {
                method: "GET".to_string(),
                url: "https://httpbin.org/get".to_string(),
                headers: None,
                body: None,
            },
            delay_between_a_and_b_ms: 100,
            delay_between_a_requests_ms: 1000,
            max_requests: None,
            digest_auth: None,
            generated_fields: None,
        }
    }
}
