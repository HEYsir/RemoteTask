use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::{Instant, sleep};

use crate::config::{HttpRequestConfig, RequestConfig};
use crate::field_generator::FieldGenerator;
use crate::http_client::{AuthConfig, AuthType, HttpClient, HttpClientConfig};
use crate::stats::{RequestStats, StatsHandler};

// Import logger macros from crate root
use crate::{log_debug, log_error, log_info, log_trace};

/// 请求处理器
pub struct RequestHandler;

impl RequestHandler {
    /// 使用共享HttpClient发送请求（认证复用）
    pub async fn send_request_with_shared_client(
        config: HttpRequestConfig,
        http_client: Arc<HttpClient>,
        _request_type: String,
        stats: Arc<Mutex<RequestStats>>,
    ) {
        let start_time = Instant::now();
        let method = config.method.to_uppercase();

        // 转换HashMap头为Vec元组用于http_client
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

        StatsHandler::handle_response(result, &config, start_time, &stats).await;
    }

    /// 运行并发请求
    pub async fn run_concurrent_requests(config: RequestConfig) -> RequestStats {
        let stats = Arc::new(Mutex::new(RequestStats::new()));
        let config = Arc::new(config);

        let stats_clone = Arc::clone(&stats);
        let config_clone = Arc::clone(&config);

        let request_task = tokio::spawn(async move {
            let mut request_count = 0;
            let mut last_a_request_time = Instant::now();

            loop {
                // 检查是否达到最大请求数
                if let Some(max) = config_clone.max_requests {
                    if request_count >= max {
                        log_info!("🎯 Reached maximum request count of {}", max);
                        break;
                    }
                }

                request_count += 1;
                log_debug!("\n--- Request Cycle {} ---", request_count);

                // 按类型分离字段（header vs body）
                let (header_fields, body_fields) = FieldGenerator::separate_fields_by_type(
                    &config_clone.generated_fields,
                    request_count,
                );

                if !header_fields.is_empty() {
                    log_trace!("🎲 Generated header fields: {:?}", header_fields);
                }
                if !body_fields.is_empty() {
                    log_trace!("📝 Generated body fields: {:?}", body_fields);
                }

                // 为A和B请求创建动态body内容
                let config_a = {
                    let mut config = config_clone.request_a.clone();
                    if !body_fields.is_empty() {
                        config.body =
                            FieldGenerator::generate_dynamic_body(&config.body, &body_fields);
                        log_trace!("📝 Dynamic body for A: {:?}", config.body);
                    }
                    config
                };

                let config_b = {
                    let mut config = config_clone.request_b.clone();
                    if !body_fields.is_empty() {
                        config.body =
                            FieldGenerator::generate_dynamic_body(&config.body, &body_fields);
                        log_trace!("📝 Dynamic body for B: {:?}", config.body);
                    }
                    config
                };

                // 计算距离上次A请求的时间以确保适当间隔
                let time_since_last_a = last_a_request_time.elapsed();
                let required_delay =
                    Duration::from_millis(config_clone.delay_between_a_requests_ms);

                if time_since_last_a < required_delay {
                    let remaining_delay = required_delay - time_since_last_a;
                    log_trace!(
                        "⏳ Waiting {}ms to ensure proper A request spacing",
                        remaining_delay.as_millis()
                    );
                    sleep(remaining_delay).await;
                }

                // 更新上次A请求时间
                last_a_request_time = Instant::now();

                let stats_a = Arc::clone(&stats_clone);
                let stats_b = Arc::clone(&stats_clone);

                // 创建共享HttpClient用于认证复用
                let http_client = {
                    let auth_config =
                        config_clone
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

                // 使用共享HttpClient发送请求A（认证复用）
                let a_handle = {
                    let http_client_clone = Arc::clone(&http_client);
                    let config_a_clone = config_a.clone();
                    let stats_a_clone = Arc::clone(&stats_a);

                    tokio::spawn(async move {
                        Self::send_request_with_shared_client(
                            config_a_clone,
                            http_client_clone,
                            "A".to_string(),
                            stats_a_clone,
                        )
                        .await;
                    })
                };

                // 发送请求B前等待
                sleep(Duration::from_millis(config_clone.delay_between_a_and_b_ms)).await;

                // 使用共享HttpClient发送请求B（认证复用）
                let b_handle = {
                    let http_client_clone = Arc::clone(&http_client);
                    let config_b_clone = config_b.clone();
                    let stats_b_clone = Arc::clone(&stats_b);

                    tokio::spawn(async move {
                        Self::send_request_with_shared_client(
                            config_b_clone,
                            http_client_clone,
                            "B".to_string(),
                            stats_b_clone,
                        )
                        .await;
                    })
                };

                // 等待两个请求完成
                let _ = tokio::try_join!(a_handle, b_handle);
            }
        });

        // 等待请求任务完成
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

        // 返回最终统计信息
        let stats_guard = stats.lock().await;
        stats_guard.clone()
    }
}
