use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::{sleep, sleep_until};

// 定义必要的类型
type Response = String; // 或者使用具体的响应类型，比如 reqwest::Response
type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone, Default, Debug)]
struct RequestStats {
    total_requests: u32,
    successful_requests: u32,
    failed_requests: u32,
    last_error: Option<String>,
}

impl RequestStats {
    fn new() -> Self {
        Self::default()
    }
}

#[derive(Clone)]
struct RequestConfig {
    url_a: String,
    url_b: String,
    max_requests: Option<u32>,
    delay_between_a_and_b_ms: u64,
    delay_between_a_requests_ms: u64,
    wait_for_a_completion: bool,
}

// 假设的 HTTP 客户端（根据你的实际实现调整）
#[derive(Clone)]
struct Client;

impl Client {
    fn new() -> Self {
        Client
    }
}

// 发送请求的函数（根据你的实际实现调整）
async fn send_request(
    _client: &Client,
    _url: &str,
    _request_type: &str,
) -> Result<Response, Error> {
    // 这里应该是实际的 HTTP 请求逻辑
    // 示例：返回一个简单的字符串
    Ok("Response received".to_string())
}

// 辅助函数：更新统计信息
async fn update_stats(
    stats: &Arc<Mutex<RequestStats>>,
    result: &Result<Response, Error>,
    request_type: &str,
) {
    let mut stats_guard = stats.lock().await;
    stats_guard.total_requests += 1;

    match result {
        Ok(_) => {
            stats_guard.successful_requests += 1;
            println!("✅ {} request succeeded", request_type);
        }
        Err(e) => {
            stats_guard.failed_requests += 1;
            stats_guard.last_error = Some(e.to_string());
            println!("❌ {} request failed: {}", request_type, e);
        }
    }
}

async fn run_concurrent_requests(config: RequestConfig) -> RequestStats {
    let client = Client::new();
    let stats = Arc::new(Mutex::new(RequestStats::new()));
    let config = Arc::new(config);

    let stats_clone = Arc::clone(&stats);
    let config_clone = Arc::clone(&config);

    let handle = tokio::spawn(async move {
        let mut request_count = 0;
        let mut last_a_start_time = Instant::now();
        let mut pending_a_requests = Vec::new();

        loop {
            // 检查是否达到最大请求数
            if let Some(max) = config_clone.max_requests {
                if request_count >= max {
                    println!("🎯 Reached maximum request count of {}", max);
                    break;
                }
            }

            request_count += 1;
            println!("\n--- Request Cycle {} ---", request_count);

            let cycle_start = Instant::now();

            // 计算下一个A请求应该开始的时间
            let next_a_time = if request_count > 1 {
                last_a_start_time + Duration::from_millis(config_clone.delay_between_a_requests_ms)
            } else {
                Instant::now() // 第一次立即开始
            };

            // 如果不是第一次请求，需要等待到下一个A请求的时间
            if request_count > 1 {
                let now = Instant::now();
                if now < next_a_time {
                    sleep_until(next_a_time.into()).await;
                }
            }

            last_a_start_time = Instant::now();

            if config_clone.wait_for_a_completion {
                // 模式1：等待A完成
                let a_result = send_request(&client, &config_clone.url_a, "A").await;
                update_stats(&stats_clone, &a_result, "A").await;

                // 计算A和B之间的实际等待时间
                let elapsed = last_a_start_time.elapsed();
                let remaining_delay =
                    if elapsed < Duration::from_millis(config_clone.delay_between_a_and_b_ms) {
                        Duration::from_millis(config_clone.delay_between_a_and_b_ms) - elapsed
                    } else {
                        Duration::ZERO
                    };

                // 等待配置的A-B延时后发送B请求
                if remaining_delay > Duration::ZERO {
                    sleep(remaining_delay).await;
                }

                // 发送B请求
                let b_result = send_request(&client, &config_clone.url_b, "B").await;
                update_stats(&stats_clone, &b_result, "B").await;
            } else {
                // 模式2：不等待A完成

                // 启动A请求（后台执行）
                let a_handle = tokio::spawn({
                    let client = client.clone();
                    let url = config_clone.url_a.clone();
                    let stats = stats_clone.clone();
                    async move {
                        let result = send_request(&client, &url, "A").await;
                        update_stats(&stats, &result, "A").await;
                        result
                    }
                });

                // 记录A请求以便后续清理（如果需要）
                pending_a_requests.push(a_handle);

                // 计算A和B之间的实际等待时间
                let elapsed = last_a_start_time.elapsed();
                let remaining_delay =
                    if elapsed < Duration::from_millis(config_clone.delay_between_a_and_b_ms) {
                        Duration::from_millis(config_clone.delay_between_a_and_b_ms) - elapsed
                    } else {
                        Duration::ZERO
                    };

                // 等待配置的A-B延时后发送B请求
                if remaining_delay > Duration::ZERO {
                    sleep(remaining_delay).await;
                }

                // 发送B请求
                let b_result = send_request(&client, &config_clone.url_b, "B").await;
                update_stats(&stats_clone, &b_result, "B").await;

                // 清理已完成的任务（避免内存泄漏）
                pending_a_requests.retain(|handle| !handle.is_finished());
            }

            println!("✅ Cycle {} completed", request_count);
        }

        // 等待所有未完成的A请求完成（可选）
        if !config_clone.wait_for_a_completion {
            println!("🔄 Waiting for pending A requests to complete...");
            for handle in pending_a_requests {
                let _ = handle.await;
            }
        }
    });

    // 等待用户中断
    println!("🚀 Concurrent requests started!");
    println!(
        "Configuration: wait_for_a_completion = {}",
        config.wait_for_a_completion
    );
    println!("Press Ctrl+C to stop...");

    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl+C");

    println!("\n🛑 Stopping concurrent requests...");

    // 取消任务
    handle.abort();
    let _ = handle.await; // 等待任务完全停止

    // 返回最终统计
    let stats_guard = stats.lock().await;
    stats_guard.clone()
}

// 使用示例
#[tokio::main]
async fn main() {
    let config = RequestConfig {
        url_a: "http://example.com/a".to_string(),
        url_b: "http://example.com/b".to_string(),
        max_requests: Some(10),
        delay_between_a_and_b_ms: 100,
        delay_between_a_requests_ms: 500,
        wait_for_a_completion: false, // 设置为 true 则等待A完成
    };

    let stats = run_concurrent_requests(config).await;
    println!("Final stats: {:?}", stats);
}
