use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::{Instant, sleep};

// Import modules
mod http_client;
mod logger;
use http_client::{AuthConfig, AuthType, HttpClient, HttpClientConfig};
use logger::{LogLevel, set_log_level};

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
pub struct GeneratedField {
    pub name: String,          // 字段名
    pub generator: String,     // 生成器类型："random", "timestamp", "counter", "uuid"
    pub field_type: String,    // 字段类型："header" 或 "body"
    pub value: Option<String>, // 生成的值（可选，用于固定值）
}

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
            generated_fields: None,
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

// Field generator functions
fn generate_field(field: &GeneratedField, cycle: usize) -> String {
    match field.generator.as_str() {
        "random" => {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let random_num: u32 = rng.gen_range(1000..9999);
            format!("random_{}_{}", cycle, random_num)
        }
        "timestamp" => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis();
            format!("timestamp_{}_{}", cycle, now)
        }
        "counter" => format!("counter_{}", cycle),
        "uuid" => {
            use uuid::Uuid;
            Uuid::new_v4().to_string()
        }
        "fixed" => field.value.clone().unwrap_or_else(|| "default".to_string()),
        _ => field.value.clone().unwrap_or_else(|| "unknown".to_string()),
    }
}

// Separate fields by type (header vs body)
fn separate_fields_by_type(
    generated_fields: &Option<Vec<GeneratedField>>,
    cycle: usize,
) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut header_fields = HashMap::new();
    let mut body_fields = HashMap::new();

    if let Some(field_configs) = generated_fields {
        for field_config in field_configs {
            let value = generate_field(field_config, cycle);
            if field_config.field_type == "body" {
                body_fields.insert(field_config.name.clone(), value);
            } else {
                header_fields.insert(field_config.name.clone(), value);
            }
        }
    }

    (header_fields, body_fields)
}

// Generate dynamic body content
fn generate_dynamic_body(
    base_body: &Option<String>,
    body_fields: &HashMap<String, String>,
) -> Option<String> {
    if let Some(body) = base_body {
        let mut dynamic_body = body.clone();

        // Replace placeholders in the body with generated values
        for (field_name, field_value) in body_fields {
            let placeholder = format!("{{{}}}", field_name);
            dynamic_body = dynamic_body.replace(&placeholder, field_value);
        }

        Some(dynamic_body)
    } else {
        // If no base body, create a JSON object with body fields
        if !body_fields.is_empty() {
            let mut json_body = String::from("{");
            let mut first = true;
            for (key, value) in body_fields {
                if !first {
                    json_body.push(',');
                }
                json_body.push_str(&format!("\"{}\":\"{}\"", key, value));
                first = false;
            }
            json_body.push('}');
            Some(json_body)
        } else {
            None
        }
    }
}

// Helper function to create a basic reqwest client
fn create_reqwest_client() -> Result<reqwest::Client, anyhow::Error> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to create reqwest client: {}", e))
}

// Helper function to add headers to a request
fn add_headers_to_request(
    mut request: reqwest::RequestBuilder,
    headers: &Option<HashMap<String, String>>,
) -> reqwest::RequestBuilder {
    if let Some(headers) = headers {
        for (key, value) in headers {
            request = request.header(key, value);
        }
    }
    request
}

// Helper function to handle Basic authentication
fn add_basic_auth(
    mut request: reqwest::RequestBuilder,
    auth_config: &AuthConfig,
) -> reqwest::RequestBuilder {
    let auth_value = format!("{}:{}", auth_config.username, auth_config.password);
    let encoded = base64::prelude::BASE64_STANDARD.encode(auth_value.as_bytes());
    request.header("Authorization", format!("Basic {}", encoded))
}

// Helper function to handle Digest authentication
async fn handle_digest_auth(
    request: reqwest::RequestBuilder,
    url: &str,
    auth_config: &AuthConfig,
) -> Result<reqwest::RequestBuilder, anyhow::Error> {
    // First attempt without auth to get challenge
    let response_result = request.try_clone().unwrap().send().await;

    let mut final_request = request;

    if let Ok(response) = response_result {
        if response.status().as_u16() == 401 {
            // Create a new HttpClient to handle digest auth
            let digest_client = HttpClient::new(HttpClientConfig {
                timeout: Duration::from_secs(30),
                user_agent: "RemoteTask-HTTP-Client/1.0".to_string(),
                auth: Some(auth_config.clone()),
            })?;

            // Use post_json method which handles digest auth internally
            let temp_response = digest_client.post_json(url, "", None).await?;

            // Extract Authorization header if present
            if let Some(auth_header) = temp_response.headers().get("Authorization") {
                let auth_string = auth_header.to_str().unwrap_or("");
                final_request = final_request.header("Authorization", auth_string);
            }
        }
    }

    Ok(final_request)
}

// Helper function to add authentication to request
async fn add_authentication(
    request: reqwest::RequestBuilder,
    url: &str,
    auth_config: Option<&AuthConfig>,
) -> Result<reqwest::RequestBuilder, anyhow::Error> {
    if let Some(auth_cfg) = auth_config {
        match auth_cfg.auth_type {
            AuthType::Basic => Ok(add_basic_auth(request, auth_cfg)),
            AuthType::Digest => handle_digest_auth(request, url, auth_cfg).await,
        }
    } else {
        Ok(request)
    }
}

// Helper function to handle HTTP client creation and error reporting
async fn create_http_client(
    auth_config: Option<&AuthConfig>,
    stats: &Arc<Mutex<RequestStats>>,
) -> Option<HttpClient> {
    let http_client_config = HttpClientConfig {
        timeout: Duration::from_secs(30),
        user_agent: "RemoteTask-HTTP-Client/1.0".to_string(),
        auth: auth_config.cloned(),
    };

    match HttpClient::new(http_client_config) {
        Ok(client) => Some(client),
        Err(e) => {
            let mut stats_guard = stats.lock().await;
            stats_guard.total_requests += 1;
            stats_guard.failed_requests += 1;
            let error_msg = format!("❌ Failed to create HTTP client: {}", e);
            log_error!("🎯 request failed: {}", error_msg);
            stats_guard.last_error = Some(error_msg);
            None
        }
    }
}

// Helper function to handle response and update statistics
async fn handle_response(
    result: Result<reqwest::Response, anyhow::Error>,
    config: &HttpRequestConfig,
    start_time: Instant,
    stats: &Arc<Mutex<RequestStats>>,
) {
    let duration = start_time.elapsed();
    let mut stats_guard = stats.lock().await;
    stats_guard.total_requests += 1;

    match &result {
        Ok(response) => {
            if response.status().is_success() {
                stats_guard.successful_requests += 1;
                log_info!(
                    "✅ {} request to {} succeeded in {:.2}ms (Status: {})",
                    config.method,
                    config.url,
                    duration.as_millis(),
                    response.status()
                );
            } else {
                stats_guard.failed_requests += 1;
                let error_msg = format!(
                    "❌ {} request to {} failed with status: {} in {:.2}ms",
                    config.method,
                    config.url,
                    response.status(),
                    duration.as_millis()
                );
                log_error!("🎯 request failed:  {}", error_msg);
                stats_guard.last_error = Some(error_msg);
            }
        }
        Err(e) => {
            stats_guard.failed_requests += 1;
            let error_msg = format!(
                "❌ {} request to {} failed with error: {} in {:.2}ms",
                config.method,
                config.url,
                e,
                duration.as_millis()
            );
            log_error!("🎯 request failed:  {}", error_msg);
            stats_guard.last_error = Some(error_msg);
        }
    }
}

// Main function for sending POST requests using HttpClient
async fn send_post_request(
    config: &HttpRequestConfig,
    auth_config: Option<&AuthConfig>,
    stats: &Arc<Mutex<RequestStats>>,
) -> Result<reqwest::Response, anyhow::Error> {
    if let Some(body) = &config.body {
        if let Some(http_client) = create_http_client(auth_config, stats).await {
            // Convert HashMap headers to Vec of tuples for http_client
            let headers = config.headers.as_ref().map(|headers| {
                headers
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect::<Vec<_>>()
            });

            http_client
                .post_json(&config.url, body, headers)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        } else {
            Err(anyhow::anyhow!(
                "Failed to create HTTP client for POST request"
            ))
        }
    } else {
        Err(anyhow::anyhow!("POST request requires a body"))
    }
}

// Main function for sending PUT/GET requests using reqwest client
async fn send_standard_request(
    config: &HttpRequestConfig,
    auth_config: Option<&AuthConfig>,
) -> Result<reqwest::Response, anyhow::Error> {
    let client = create_reqwest_client()?;

    let method = config.method.to_uppercase();
    let mut request = match method.as_str() {
        "PUT" => {
            if let Some(body) = &config.body {
                client.put(&config.url).body(body.clone())
            } else {
                return Err(anyhow::anyhow!("PUT request requires a body"));
            }
        }
        "GET" => client.get(&config.url),
        _ => return Err(anyhow::anyhow!("Unsupported HTTP method: {}", method)),
    };

    // Add Content-Type header for JSON if it's a PUT request
    if method == "PUT" {
        request = request.header("Content-Type", "application/json");
    }

    // Add custom headers
    request = add_headers_to_request(request, &config.headers);

    // Add authentication
    request = add_authentication(request, &config.url, auth_config).await?;

    request.send().await.map_err(|e| anyhow::anyhow!("{}", e))
}

// Refactored main request function
async fn send_request_async(
    config: HttpRequestConfig,
    auth_config: Option<&AuthConfig>,
    _request_type: String,
    stats: Arc<Mutex<RequestStats>>,
) {
    let start_time = Instant::now();
    let method = config.method.to_uppercase();

    let result = match method.as_str() {
        "POST" => send_post_request(&config, auth_config, &stats).await,
        "PUT" | "GET" => send_standard_request(&config, auth_config).await,
        _ => Err(anyhow::anyhow!("Unsupported HTTP method: {}", method)),
    };

    handle_response(result, &config, start_time, &stats).await;
}

// New function for sending requests with shared HttpClient (authentication reuse)
async fn send_request_with_shared_client(
    config: HttpRequestConfig,
    http_client: Arc<HttpClient>,
    _request_type: String,
    stats: Arc<Mutex<RequestStats>>,
) {
    let start_time = Instant::now();
    let method = config.method.to_uppercase();

    // Convert HashMap headers to Vec of tuples for http_client
    let headers = config.headers.as_ref().map(|headers| {
        headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<Vec<_>>()
    });

    let result = match method.as_str() {
        "POST" => {
            if let Some(body) = &config.body {
                http_client
                    .post_json(&config.url, body, headers)
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            } else {
                Err(anyhow::anyhow!("POST request requires a body"))
            }
        }
        "PUT" | "GET" => http_client
            .send_request(&method, &config.url, config.body.clone(), headers)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e)),
        _ => Err(anyhow::anyhow!("Unsupported HTTP method: {}", method)),
    };

    handle_response(result, &config, start_time, &stats).await;
}

async fn run_concurrent_requests(config: RequestConfig) -> RequestStats {
    let stats = Arc::new(Mutex::new(RequestStats::new()));
    let config = Arc::new(config);

    let stats_clone = Arc::clone(&stats);
    let config_clone = Arc::clone(&config);

    let request_task = tokio::spawn(async move {
        let mut request_count = 0;
        let mut last_a_request_time = Instant::now();

        // Note: The new HttpClient is not being used since we're using standard reqwest clients
        // This preserves the authentication configuration for future use if needed

        loop {
            // Check if we've reached the maximum number of requests
            if let Some(max) = config_clone.max_requests {
                if request_count >= max {
                    log_info!("🎯 Reached maximum request count of {}", max);
                    break;
                }
            }

            request_count += 1;
            log_debug!("\n--- Request Cycle {} ---", request_count);

            // Separate fields by type (header vs body)
            let (header_fields, body_fields) =
                separate_fields_by_type(&config_clone.generated_fields, request_count);

            if !header_fields.is_empty() {
                log_trace!("🎲 Generated header fields: {:?}", header_fields);
            }
            if !body_fields.is_empty() {
                log_trace!("📝 Generated body fields: {:?}", body_fields);
            }

            // Create dynamic body content for A and B requests
            let config_a = {
                let mut config = config_clone.request_a.clone();
                if !body_fields.is_empty() {
                    config.body = generate_dynamic_body(&config.body, &body_fields);
                    log_trace!("📝 Dynamic body for A: {:?}", config.body);
                }
                config
            };

            let config_b = {
                let mut config = config_clone.request_b.clone();
                if !body_fields.is_empty() {
                    config.body = generate_dynamic_body(&config.body, &body_fields);
                    log_trace!("📝 Dynamic body for B: {:?}", config.body);
                }
                config
            };

            // Calculate time since last A request to ensure proper spacing
            let time_since_last_a = last_a_request_time.elapsed();
            let required_delay = Duration::from_millis(config_clone.delay_between_a_requests_ms);

            if time_since_last_a < required_delay {
                let remaining_delay = required_delay - time_since_last_a;
                log_trace!(
                    "⏳ Waiting {}ms to ensure proper A request spacing",
                    remaining_delay.as_millis()
                );
                sleep(remaining_delay).await;
            }

            // Update last A request time
            last_a_request_time = Instant::now();

            let stats_a = Arc::clone(&stats_clone);
            let stats_b = Arc::clone(&stats_clone);

            // Create shared HttpClient for authentication reuse
            let http_client = {
                let auth_config = config_clone
                    .digest_auth
                    .as_ref()
                    .map(|digest_auth| AuthConfig {
                        username: digest_auth.username.clone(),
                        password: digest_auth.password.clone(),
                        auth_type: AuthType::Digest,
                    });

                let http_client_config = HttpClientConfig {
                    timeout: Duration::from_secs(30),
                    user_agent: "RemoteTask-HTTP-Client/1.0".to_string(),
                    auth: auth_config,
                };

                match HttpClient::new(http_client_config) {
                    Ok(client) => Arc::new(client),
                    Err(e) => {
                        log_error!("Failed to create HTTP client: {}", e);
                        return;
                    }
                }
            };

            // Send request A with shared HttpClient (authentication reuse)
            let a_handle = {
                let http_client_clone = Arc::clone(&http_client);
                let config_a_clone = config_a.clone();
                let stats_a_clone = Arc::clone(&stats_a);

                tokio::spawn(async move {
                    send_request_with_shared_client(
                        config_a_clone,
                        http_client_clone,
                        "A".to_string(),
                        stats_a_clone,
                    )
                    .await;
                })
            };

            // Wait before sending request B
            sleep(Duration::from_millis(config_clone.delay_between_a_and_b_ms)).await;

            // Send request B with shared HttpClient (authentication reuse)
            let b_handle = {
                let http_client_clone = Arc::clone(&http_client);
                let config_b_clone = config_b.clone();
                let stats_b_clone = Arc::clone(&stats_b);

                tokio::spawn(async move {
                    send_request_with_shared_client(
                        config_b_clone,
                        http_client_clone,
                        "B".to_string(),
                        stats_b_clone,
                    )
                    .await;
                })
            };

            // Wait for both requests to complete
            let _ = tokio::try_join!(a_handle, b_handle);
        }
    });

    // Wait for the request task to complete
    log_info!("🚀 Concurrent HTTP requests started!");
    log_trace!("Features:");
    log_trace!("  ✅ GET and POST requests supported");
    log_trace!("  ✅ Digest authentication with smart auth handling");
    log_trace!("  ✅ Cookie-based session management");
    log_trace!("  ✅ A and B requests with shared generated fields");
    log_trace!("  ✅ Header and body field generation support");
    log_trace!("  ✅ Precise delay control");
    log_info!("Press Ctrl+C to stop...");

    match request_task.await {
        Ok(_) => log_info!("\n✅ All request cycles completed!"),
        Err(e) => log_error!("\n❌ Request task failed: {}", e),
    }

    // Return final stats
    let stats_guard = stats.lock().await;
    stats_guard.clone()
}

#[tokio::main]
async fn main() {
    // Set default log level to Info
    set_log_level(LogLevel::Info);

    log_info!("🌐 Advanced Rust Concurrent HTTP Request Tool");
    log_info!("==============================================");
    log_info!("📝 Features: GET/POST requests, Digest auth, field generation");
    log_info!("");

    // Example configuration with POST requests and digest auth
    let config = RequestConfig {
        request_a: HttpRequestConfig {
            method: "POST".to_string(),
            url: "https://10.41.131.87/ISAPI/System/AlgoPackageScheduling/AddTask?format=json".to_string(),
            headers: Some({
                let mut headers = HashMap::new();
                headers.insert("Content-Type".to_string(), "application/json".to_string());
                headers
            }),
            body: Some(r#"
            {
                "taskName": "{{taskID}}",
                "customInfo": "test",
                "taskID": "{{taskID}}",
                "nodeID": "1",
                "algoPackageID": "NTkmMCYxJjEyJjEuMC4xNSYxJjEmdmlkZW8mJjA=",
                "algoID": "smokeAndFireDetection",
                "DataSource": {
                    "sourceType": "video",
                    "pollingTime": 10,
                    "StreamList": [
                        {
                            "cameraIndexCode": "24e3fdcd1b2441f6a3be07b945e391ce",
                            "rule": "{\"pictureAddTarget\":true,\"pictureAddRule\":true,\"rulesParam\":[{\"algorithmID\":\"NTkmMCYxJjEyJjEuMC4xNSYxJjEmdmlkZW8mJjA=_smokeAndFireDetection\",\"commonParams\":{\"TimeList\":[{\"day\":\"monday\",\"timeRange\":[{\"startTime\":\"00:00:00\",\"endTime\":\"23:59:59\"}]},{\"day\":\"tuesday\",\"timeRange\":[{\"startTime\":\"00:00:00\",\"endTime\":\"23:59:59\"}]},{\"day\":\"wednesday\",\"timeRange\":[{\"startTime\":\"00:00:00\",\"endTime\":\"23:59:59\"}]},{\"day\":\"thursday\",\"timeRange\":[{\"startTime\":\"00:00:00\",\"endTime\":\"23:59:59\"}]},{\"day\":\"friday\",\"timeRange\":[{\"startTime\":\"00:00:00\",\"endTime\":\"23:59:59\"}]},{\"day\":\"saturday\",\"timeRange\":[{\"startTime\":\"00:00:00\",\"endTime\":\"23:59:59\"}]},{\"day\":\"sunday\",\"timeRange\":[{\"startTime\":\"00:00:00\",\"endTime\":\"23:59:59\"}]}]},\"ruleCustomName\":\"新增规则\",\"eventType\":\"smokeAndFireDetection\",\"scene\":\"smokeAndFire\",\"ruleId\":\"574a60848fb7434483a33670e89b5338\",\"ruleID\":\"574a60848fb7434483a33670e89b5338\",\"paramsInfo\":{\"smokeSensitivity\":2,\"Region\":[{\"x\":0.023876,\"y\":0.245614},{\"x\":0.325843,\"y\":0.250627},{\"x\":0.30618,\"y\":0.954887},{\"x\":0.01264,\"y\":0.9599}],\"sensitivity\":2,\"uploadMode\":\"filter\",\"alarmDelayConfirmSensitivity\":4}}],\"shieldRegion\":[]}",
                            "RTSPURL": "rtsp://admin:backend15@10.14.99.155:554/ISAPI/streaming/channels/101"
                        }
                    ]
                },
                "Destination": [
                    {
                        "type": "ServerHCS",
                        "addressType": "ipaddress",
                        "ipV4Address": "10.41.131.62",
                        "portNo": 6011,
                        "accessKey": "3Yht1fJl6nkg735uQ3oB8Gx89Jj0Xz1aoOIx819PIPpgklb60V89WBa5S07Zr5C",
                        "secretKey": "o6n66M48S3v1cK3fsm639sNA4i9739S5AUD54a88672756bGuW0jPEo99AF4IYA",
                        "poolID": "196661251",
                        "downloadPort": 6120,
                        "SSLEnabled": false
                    },
                    {
                        "type":  "ServerClient",	
                        "protocolType":  "HTTP",	
                        "addressType":  "ipaddress",	
                        "ipV4Address":  "10.14.99.71",	
                        "portNo":  8000,	
                        "URI":  "/httpalarm"
                    }
                ]
            }"#.to_string()),
        },
        request_b: HttpRequestConfig {
            method: "PUT".to_string(),
            url: "http://10.41.131.87/ISAPI/System/AlgoPackageScheduling/DeleteTask?format=json".to_string(),
            headers: None,
            body: Some(r#"
            {
                "TaskIDList": [
                    {
                        "taskID":"{{taskID}}"
                    }
                ]
            }"#.to_string()),
        },
        delay_between_a_and_b_ms: 500,
        delay_between_a_requests_ms: 3000,
        max_requests: Some(1),
        digest_auth: Some(DigestAuthConfig {
            username: "admin".to_string(),
            password: "backend15".to_string(),
            realm: None,
            nonce: None,
        }),
        generated_fields: Some(vec![
            // GeneratedField {
            //     name: "session_id".to_string(),
            //     generator: "random".to_string(),
            //     field_type: "body".to_string(),
            //     value: None,
            // },
            // GeneratedField {
            //     name: "request_id".to_string(),
            //     generator: "counter".to_string(),
            //     field_type: "body".to_string(),
            //     value: None,
            // },
            // GeneratedField {
            //     name: "timestamp".to_string(),
            //     generator: "timestamp".to_string(),
            //     field_type: "body".to_string(),
            //     value: None,
            // },
            GeneratedField {
                name: "taskID".to_string(),
                generator: "uuid".to_string(),
                field_type: "body".to_string(),
                value: None,
            },
        ]),
    };

    log_info!("📋 Configuration:");
    log_info!("  Request A:");
    log_info!("    Method: {}", config.request_a.method);
    log_info!("    URL: {}", config.request_a.url);
    log_trace!("    Body: {:?}", config.request_a.body);
    log_info!("  Request B:");
    log_info!("    Method: {}", config.request_b.method);
    log_info!("    URL: {}", config.request_b.url);
    log_info!("  Delays:");
    log_info!("    A→B: {}ms", config.delay_between_a_and_b_ms);
    log_info!("    A→A: {}ms", config.delay_between_a_requests_ms);
    log_info!("  Authentication:");
    if let Some(auth) = &config.digest_auth {
        log_info!("    Username: {}", auth.username);
        log_info!("    Realm: {}", auth.realm.as_deref().unwrap_or("default"));
    } else {
        log_info!("    None");
    }
    log_info!("  Generated Fields:");
    if let Some(fields) = &config.generated_fields {
        for field in fields {
            log_info!(
                "    {}: {} ({})",
                field.name,
                field.generator,
                field.field_type
            );
        }
    } else {
        log_info!("    None");
    }
    log_info!("");

    // Run the concurrent requests
    let stats = run_concurrent_requests(config).await;

    log_info!("\n📊 Final Statistics:");
    log_info!("  Total requests: {}", stats.total_requests);
    log_info!("  Successful: {}", stats.successful_requests);
    log_info!("  Failed: {}", stats.failed_requests);
    if let Some(error) = &stats.last_error {
        log_error!("  Last error: {}", error);
    }
}
