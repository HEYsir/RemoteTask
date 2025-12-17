use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::{Instant, sleep};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequestConfig {
    pub method: String, // "GET" or "POST"
    pub url: String,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<String>, // JSON string for POST requests
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestAuthConfig {
    pub username: String,
    pub password: String,
    pub realm: Option<String>,
    pub nonce: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestConfig {
    pub request_a: HttpRequestConfig,
    pub request_b: HttpRequestConfig,
    pub delay_between_a_and_b_ms: u64,
    pub delay_between_a_requests_ms: u64,
    pub max_requests: Option<usize>,
    pub digest_auth: Option<DigestAuthConfig>,
}

#[derive(Debug, Clone)]
pub struct RequestStats {
    pub total_requests: usize,
    pub successful_requests: usize,
    pub failed_requests: usize,
    pub last_error: Option<String>,
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
        }
    }
}

impl RequestStats {
    pub fn new() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            last_error: None,
        }
    }
}

// Simple digest authentication implementation
fn build_digest_auth_header(
    method: &str,
    uri: &str,
    username: &str,
    password: &str,
    realm: &str,
    nonce: &str,
) -> String {
    use base64::prelude::*;
    use md5::{Digest, Md5};

    // HA1 = MD5(username:realm:password)
    let ha1_data = format!("{}:{}:{}", username, realm, password);
    let ha1 = format!("{:x}", Md5::digest(ha1_data.as_bytes()));

    // HA2 = MD5(method:uri)
    let ha2_data = format!("{}:{}", method, uri);
    let ha2 = format!("{:x}", Md5::digest(ha2_data.as_bytes()));

    // Response = MD5(HA1:nonce:HA2)
    let response_data = format!("{}:{}:{}", ha1, nonce, ha2);
    let response = format!("{:x}", Md5::digest(response_data.as_bytes()));

    format!(
        "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
        username, realm, nonce, uri, response
    )
}

async fn send_request_with_auth(
    client: &Client,
    config: &HttpRequestConfig,
    auth_config: Option<&DigestAuthConfig>,
) -> Result<(), String> {
    let start_time = Instant::now();

    let mut request_builder = match config.method.to_uppercase().as_str() {
        "GET" => client.get(&config.url),
        "POST" => client.post(&config.url),
        method => return Err(format!("Unsupported HTTP method: {}", method)),
    };

    // Add headers if provided
    if let Some(headers) = &config.headers {
        for (key, value) in headers {
            request_builder = request_builder.header(key, value);
        }
    }

    // Add body for POST requests
    if config.method.to_uppercase() == "POST" {
        if let Some(body) = &config.body {
            request_builder = request_builder.body(body.clone());
        }
    }

    // Add digest authentication if configured
    if let Some(auth) = auth_config {
        let realm = auth.realm.as_deref().unwrap_or("");
        let nonce = auth.nonce.as_deref().unwrap_or("123456");
        let auth_header = build_digest_auth_header(
            &config.method,
            &config.url,
            &auth.username,
            &auth.password,
            realm,
            nonce,
        );
        request_builder = request_builder.header("Authorization", &auth_header);
    }

    match request_builder.send().await {
        Ok(response) => {
            let duration = start_time.elapsed();
            if response.status().is_success() {
                println!(
                    "✅ {} request to {} succeeded in {:.2}ms (Status: {})",
                    config.method,
                    config.url,
                    duration.as_millis(),
                    response.status()
                );
                Ok(())
            } else {
                let error_msg = format!(
                    "❌ {} request to {} failed with status: {} in {:.2}ms",
                    config.method,
                    config.url,
                    response.status(),
                    duration.as_millis()
                );
                Err(error_msg)
            }
        }
        Err(e) => {
            let duration = start_time.elapsed();
            let error_msg = format!(
                "❌ {} request to {} failed with error: {} in {:.2}ms",
                config.method,
                config.url,
                e,
                duration.as_millis()
            );
            Err(error_msg)
        }
    }
}

async fn send_request_async(
    client: Client,
    config: HttpRequestConfig,
    request_type: String,
    stats: Arc<Mutex<RequestStats>>,
    auth_config: Option<DigestAuthConfig>,
) {
    let result = send_request_with_auth(&client, &config, auth_config.as_ref()).await;

    let mut stats_guard = stats.lock().await;
    stats_guard.total_requests += 1;

    match &result {
        Ok(_) => {
            stats_guard.successful_requests += 1;
        }
        Err(e) => {
            stats_guard.failed_requests += 1;
            stats_guard.last_error = Some(e.clone());
        }
    }
}

async fn run_concurrent_requests(config: RequestConfig) -> RequestStats {
    let stats = Arc::new(Mutex::new(RequestStats::new()));
    let config = Arc::new(config);

    let stats_clone = Arc::clone(&stats);
    let config_clone = Arc::clone(&config);

    let request_task = tokio::spawn(async move {
        let mut request_count = 0;
        let mut last_a_request_time = Instant::now();

        loop {
            // Check if we've reached the maximum number of requests
            if let Some(max) = config_clone.max_requests {
                if request_count >= max {
                    println!("🎯 Reached maximum request count of {}", max);
                    break;
                }
            }

            request_count += 1;
            println!("\n--- Request Cycle {} ---", request_count);

            // Calculate time since last A request to ensure proper spacing
            let time_since_last_a = last_a_request_time.elapsed();
            let required_delay = Duration::from_millis(config_clone.delay_between_a_requests_ms);

            if time_since_last_a < required_delay {
                let remaining_delay = required_delay - time_since_last_a;
                println!(
                    "⏳ Waiting {}ms to ensure proper A request spacing",
                    remaining_delay.as_millis()
                );
                sleep(remaining_delay).await;
            }

            // Update last A request time
            last_a_request_time = Instant::now();

            // Create separate clients for A and B requests
            let client_a = Client::new();
            let client_b = Client::new();

            let stats_a = Arc::clone(&stats_clone);
            let stats_b = Arc::clone(&stats_clone);

            let config_a = config_clone.request_a.clone();
            let config_b = config_clone.request_b.clone();

            let auth_config = config_clone.digest_auth.clone();

            // Send request A (doesn't wait for completion)
            let a_handle = tokio::spawn(send_request_async(
                client_a,
                config_a,
                "A".to_string(),
                stats_a,
                auth_config.clone(),
            ));

            // Wait before sending request B (but don't wait for A to complete)
            sleep(Duration::from_millis(config_clone.delay_between_a_and_b_ms)).await;

            // Send request B (concurrent with A)
            let b_handle = tokio::spawn(send_request_async(
                client_b,
                config_b,
                "B".to_string(),
                stats_b,
                auth_config,
            ));

            // Wait for both requests to complete before next cycle
            let _ = tokio::try_join!(a_handle, b_handle);
        }
    });

    // Wait for the request task to complete
    println!("🚀 Concurrent HTTP requests started!");
    println!("Features:");
    println!("  ✅ GET and POST requests supported");
    println!("  ✅ Digest authentication supported");
    println!("  ✅ A and B requests sent concurrently");
    println!("  ✅ Precise delay control");
    println!("Press Ctrl+C to stop...");

    match request_task.await {
        Ok(_) => println!("\n✅ All request cycles completed!"),
        Err(e) => println!("\n❌ Request task failed: {}", e),
    }

    // Return final stats
    let stats_guard = stats.lock().await;
    stats_guard.clone()
}

#[tokio::main]
async fn main() {
    println!("🌐 Advanced Rust Concurrent HTTP Request Tool");
    println!("==============================================");
    println!("📝 Features: GET/POST requests, Digest auth, concurrent execution");
    println!();

    // Example configuration with POST requests and digest auth
    let config = RequestConfig {
        request_a: HttpRequestConfig {
            method: "POST".to_string(),
            url: "https://httpbin.org/post".to_string(),
            headers: Some({
                let mut headers = HashMap::new();
                headers.insert("Content-Type".to_string(), "application/json".to_string());
                headers
            }),
            body: Some(r#"{"message": "Hello from request A"}"#.to_string()),
        },
        request_b: HttpRequestConfig {
            method: "GET".to_string(),
            url: "https://httpbin.org/get".to_string(),
            headers: None,
            body: None,
        },
        delay_between_a_and_b_ms: 500,
        delay_between_a_requests_ms: 3000,
        max_requests: Some(3),
        digest_auth: Some(DigestAuthConfig {
            username: "testuser".to_string(),
            password: "testpass".to_string(),
            realm: Some("testrealm".to_string()),
            nonce: Some("123456".to_string()),
        }),
    };

    println!("📋 Configuration:");
    println!("  Request A:");
    println!("    Method: {}", config.request_a.method);
    println!("    URL: {}", config.request_a.url);
    println!("    Body: {:?}", config.request_a.body);
    println!("  Request B:");
    println!("    Method: {}", config.request_b.method);
    println!("    URL: {}", config.request_b.url);
    println!("  Delays:");
    println!("    A→B: {}ms", config.delay_between_a_and_b_ms);
    println!("    A→A: {}ms", config.delay_between_a_requests_ms);
    println!("  Authentication:");
    if let Some(auth) = &config.digest_auth {
        println!("    Username: {}", auth.username);
        println!("    Realm: {}", auth.realm.as_deref().unwrap_or("default"));
    } else {
        println!("    None");
    }
    println!();

    // Run the concurrent requests
    let stats = run_concurrent_requests(config).await;

    println!("\n📊 Final Statistics:");
    println!("  Total requests: {}", stats.total_requests);
    println!("  Successful: {}", stats.successful_requests);
    println!("  Failed: {}", stats.failed_requests);
    if let Some(error) = &stats.last_error {
        println!("  Last error: {}", error);
    }
}
