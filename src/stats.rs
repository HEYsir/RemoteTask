use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::config::HttpRequestConfig;

// Import logger macros from crate root
use crate::{log_error, log_info};

/// 请求统计信息
#[derive(Debug, Clone)]
pub struct RequestStats {
    pub total_requests: usize,
    pub successful_requests: usize,
    pub failed_requests: usize,
    pub last_error: Option<String>,
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

/// 统计处理器
pub struct StatsHandler;

impl StatsHandler {
    /// 处理响应并更新统计信息
    pub async fn handle_response(
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

    /// 打印最终统计信息
    pub fn print_final_stats(stats: &RequestStats) {
        log_info!("\n📊 Final Statistics:");
        log_info!("  Total requests: {}", stats.total_requests);
        log_info!("  Successful: {}", stats.successful_requests);
        log_info!("  Failed: {}", stats.failed_requests);
        if let Some(error) = &stats.last_error {
            log_error!("  Last error: {}", error);
        }
    }
}
