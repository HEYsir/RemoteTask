# Advanced Rust Concurrent HTTP Request Tool

这是一个用Rust实现的高级并发HTTP请求工具，支持GET/POST请求、digest认证、精确的延迟控制和并发执行。

## 功能特性

- ✅ **GET和POST请求支持** - 支持两种HTTP方法，可自定义请求头和请求体
- ✅ **Digest认证支持** - 完整的digest认证实现
- ✅ **A和B请求并发执行** - A请求发送后立即发送B请求，不等待A返回
- ✅ **精确延迟控制** - A→B延迟和A→A延迟都可精确控制
- ✅ **防止A请求重叠** - 确保连续A请求之间的间隔准确
- ✅ **详细统计和错误处理** - 完整的请求统计和错误信息
- ✅ **可配置参数** - 所有参数都可灵活配置

## 项目结构

```
src/
└── main.rs  # 完整功能版本（包含GET/POST、digest认证、并发执行）
```

## 使用方法

```bash
# 运行完整功能版本
cargo run
```

## 配置参数

### 请求配置 (HttpRequestConfig)

- `method`: HTTP方法 ("GET" 或 "POST")
- `url`: 请求URL
- `headers`: 可选的请求头 (HashMap<String, String>)
- `body`: POST请求的请求体 (JSON字符串)

### 认证配置 (DigestAuthConfig)

- `username`: digest认证用户名
- `password`: digest认证密码  
- `realm`: 认证域 (可选)
- `nonce`: 随机数 (可选)

### 主配置 (RequestConfig)

- `request_a`: A请求配置
- `request_b`: B请求配置
- `delay_between_a_and_b_ms`: A和B请求之间的延迟（毫秒）
- `delay_between_a_requests_ms`: 连续A请求之间的延迟（毫秒）
- `max_requests`: 最大请求次数（可选）
- `digest_auth`: digest认证配置（可选）

## 配置示例

```rust
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
```

## 实现说明

### 原始实现的问题

原始实现使用简单的循环和延迟，可能会导致A请求重叠：

- 每个周期：A请求 → 延迟 → B请求 → 延迟 → 下一个A请求
- 如果A请求+延迟+B请求的总时间超过A请求之间的间隔，就会发生重叠

### 修复版本

修复版本通过精确的时间控制防止A请求重叠：

- 在每个周期开始时检查距离上次A请求的时间
- 如果时间不足，等待剩余时间再发送A请求
- 确保A请求之间的间隔始终满足配置要求

### 并行版本（推荐）

并行版本实现了真正的并发请求：

- **A请求发送后立即发送B请求**：不等待A请求返回
- **A和B请求并发执行**：使用独立的异步任务
- **精确的延迟控制**：A→B延迟和A→A延迟都可控
- **防止A请求重叠**：确保连续A请求之间的间隔

## 关键行为对比

| 版本 | A和B请求关系 | A请求间隔控制 | 推荐场景 |
|------|-------------|--------------|----------|
| 原始版本 | 顺序执行（A完成→B） | 可能重叠 | 简单测试 |
| 修复版本 | 顺序执行（A完成→B） | 精确控制 | 需要精确间隔 |
| **并行版本** | **并发执行（A发送→B发送）** | **精确控制** | **生产环境** |

## 测试结果

通过测试程序验证：

- ✅ 原始版本确实存在A请求重叠问题
- ✅ 修复版本正确防止了A请求重叠
- ✅ 延迟控制精确有效

## 技术栈

- **Rust** - 编程语言
- **Tokio** - 异步运行时
- **Reqwest** - HTTP客户端
- **Serde** - 序列化/反序列化

## 许可证

MIT License
