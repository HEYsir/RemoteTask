use std::collections::HashMap;

// Import modules
mod config;
mod field_generator;
mod http_client;
mod logger;
mod request_handler;
mod stats;

use config::{DigestAuthConfig, GeneratedField, HttpRequestConfig, RequestConfig};
use logger::{LogLevel, set_log_level};
use request_handler::RequestHandler;
use stats::StatsHandler;

#[tokio::main]
async fn main() {
    // 设置默认日志级别为Info
    set_log_level(LogLevel::Info);

    log_info!("🌐 Advanced Rust Concurrent HTTP Request Tool");
    log_info!("==============================================");
    log_info!("📝 Features: GET/POST requests, Digest auth, field generation");
    log_info!("");

    // 示例配置，包含POST请求和digest认证
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
            GeneratedField {
                name: "taskID".to_string(),
                generator: "uuid".to_string(),
                field_type: "body".to_string(),
                value: None,
            },
        ]),
    };

    // 打印配置信息
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

    // 运行并发请求
    let stats = RequestHandler::run_concurrent_requests(config).await;

    // 打印最终统计信息
    StatsHandler::print_final_stats(&stats);
}
