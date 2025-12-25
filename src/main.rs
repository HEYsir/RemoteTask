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

async fn send_request_with_auth(
    client: &Client,
    config: &HttpRequestConfig,
    auth_config: Option<&DigestAuthConfig>,
    header_fields: Option<&HashMap<String, String>>,
) -> Result<(), String> {
    let start_time = Instant::now();

    let mut request_builder = match config.method.to_uppercase().as_str() {
        "GET" => client.get(&config.url),
        "POST" => client.post(&config.url),
        "PUT" => client.put(&config.url),
        method => return Err(format!("Unsupported HTTP method: {}", method)),
    };

    // Add headers if provided
    if let Some(headers) = &config.headers {
        for (key, value) in headers {
            request_builder = request_builder.header(key, value);
        }
    }

    // Add generated header fields if provided
    if let Some(fields) = header_fields {
        println!("📤 Injecting header fields: {:?}", fields);
        for (key, value) in fields {
            request_builder = request_builder.header(key, value);
        }
    }

    // Add body for POST requests
    if config.method.to_uppercase() == "POST" {
        if let Some(body) = &config.body {
            request_builder = request_builder.body(body.clone());
        }
    } else if config.method.to_uppercase() == "PUT" {
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
    _request_type: String,
    stats: Arc<Mutex<RequestStats>>,
    auth_config: Option<DigestAuthConfig>,
    header_fields: Option<HashMap<String, String>>,
) {
    let result = send_request_with_auth(
        &client,
        &config,
        auth_config.as_ref(),
        header_fields.as_ref(),
    )
    .await;

    let mut stats_guard = stats.lock().await;
    stats_guard.total_requests += 1;

    match &result {
        Ok(_) => {
            stats_guard.successful_requests += 1;
        }
        Err(e) => {
            stats_guard.failed_requests += 1;
            println!("🎯 request failed:  {}", e);
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

            // Separate fields by type (header vs body)
            let (header_fields, body_fields) =
                separate_fields_by_type(&config_clone.generated_fields, request_count);

            if !header_fields.is_empty() {
                println!("🎲 Generated header fields: {:?}", header_fields);
            }
            if !body_fields.is_empty() {
                println!("📝 Generated body fields: {:?}", body_fields);
            }

            // Create dynamic body content for A and B requests
            let config_a = {
                let mut config = config_clone.request_a.clone();
                if !body_fields.is_empty() {
                    config.body = generate_dynamic_body(&config.body, &body_fields);
                    println!("📝 Dynamic body for A: {:?}", config.body);
                }
                config
            };

            let config_b = {
                let mut config = config_clone.request_b.clone();
                if !body_fields.is_empty() {
                    config.body = generate_dynamic_body(&config.body, &body_fields);
                    println!("📝 Dynamic body for B: {:?}", config.body);
                }
                config
            };

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

            let auth_config = config_clone.digest_auth.clone();
            let header_fields_clone = header_fields.clone();

            // Send request A with header fields
            let a_handle = tokio::spawn(send_request_async(
                client_a,
                config_a,
                "A".to_string(),
                stats_a,
                auth_config.clone(),
                Some(header_fields),
            ));

            // Wait before sending request B
            sleep(Duration::from_millis(config_clone.delay_between_a_and_b_ms)).await;

            // Send request B with the same header fields
            let b_handle = tokio::spawn(send_request_async(
                client_b,
                config_b,
                "B".to_string(),
                stats_b,
                auth_config,
                Some(header_fields_clone),
            ));

            // Wait for both requests to complete
            let _ = tokio::try_join!(a_handle, b_handle);
        }
    });

    // Wait for the request task to complete
    println!("🚀 Concurrent HTTP requests started!");
    println!("Features:");
    println!("  ✅ GET and POST requests supported");
    println!("  ✅ Digest authentication supported");
    println!("  ✅ A and B requests with shared generated fields");
    println!("  ✅ Header and body field generation support");
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
    println!("📝 Features: GET/POST requests, Digest auth, field generation");
    println!();

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
    println!("  Generated Fields:");
    if let Some(fields) = &config.generated_fields {
        for field in fields {
            println!(
                "    {}: {} ({})",
                field.name, field.generator, field.field_type
            );
        }
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
