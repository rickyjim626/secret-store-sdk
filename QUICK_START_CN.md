# XJP Secret Store SDK 快速开始指南

## 🚀 5分钟快速上手

### 1. 添加依赖

在您的 `Cargo.toml` 中添加：

```toml
[dependencies]
secret-store-sdk = "0.1.0"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1"
```

### 2. 基本使用

```rust
use secret_store_sdk::{ClientBuilder, Auth, GetOpts, PutOpts};
use secrecy::ExposeSecret;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建客户端 - 使用您的实际服务器地址和 API Key
    let client = ClientBuilder::new("http://34.92.201.151:8080")
        .auth(Auth::api_key("sk-prod-8e9f874dfc324f0f6dca2eee9e4ecdcc5ca57e74b9e4131e"))
        .timeout_ms(30000)
        .retries(3)
        .enable_cache(true)
        .allow_insecure_http() // 仅用于测试环境
        .build()?;

    // 存储密钥
    let put_result = client.put_secret(
        "my-app",               // 命名空间
        "database-password",    // 密钥名
        "my-secret-password",   // 密钥值
        PutOpts {
            ttl_seconds: Some(3600), // 1小时后过期
            metadata: Some(serde_json::json!({
                "owner": "开发团队",
                "environment": "生产"
            })),
            ..Default::default()
        }
    ).await?;

    println!("✅ 密钥已存储: {}", put_result.message);

    // 读取密钥
    let secret = client.get_secret(
        "my-app",
        "database-password",
        GetOpts::default()
    ).await?;

    println!("📖 读取密钥成功:");
    println!("  值: {}", secret.value.expose_secret());
    println!("  版本: {}", secret.version);
    println!("  更新时间: {}", secret.updated_at);

    // 列出所有密钥
    let list_result = client.list_secrets(
        "my-app",
        secret_store_sdk::ListOpts::default()
    ).await?;

    println!("📋 命名空间中的密钥:");
    for secret_info in list_result.secrets {
        println!("  - {}", secret_info.key);
    }

    Ok(())
}
```

### 3. 环境变量配置 (可选)

创建 `.env` 文件：

```bash
XJP_SECRET_STORE_URL=http://34.92.201.151:8080
XJP_SECRET_STORE_API_KEY=sk-prod-8e9f874dfc324f0f6dca2eee9e4ecdcc5ca57e74b9e4131e
```

然后可以简化代码：

```rust
// 从环境变量创建客户端
let client = ClientBuilder::from_env()?;
```

### 4. 常用操作示例

#### 批量操作

```rust
use secret_store_sdk::{BatchOp, BatchKeys, ExportFormat};

// 批量创建密钥
let operations = vec![
    BatchOp::put("key1", "value1"),
    BatchOp::put("key2", "value2"),
    BatchOp::put("key3", "value3"),
];

let batch_result = client.batch_operate(
    "my-app",
    operations,
    false, // 非事务性
    None   // 无幂等性密钥
).await?;

println!("批量操作: 成功 {}, 失败 {}",
    batch_result.results.succeeded.len(),
    batch_result.results.failed.len()
);

// 批量读取
let keys = BatchKeys::Keys(vec!["key1".to_string(), "key2".to_string()]);
let batch_get = client.batch_get("my-app", keys, ExportFormat::Json).await?;
```

#### 导出环境变量

```rust
use secret_store_sdk::{ExportEnvOpts, ExportFormat};

// 导出为 .env 格式
let export_opts = ExportEnvOpts {
    format: ExportFormat::Dotenv,
    ..Default::default()
};

let export = client.export_env("my-app", export_opts).await?;
if let secret_store_sdk::EnvExport::Text(dotenv_content) = export {
    std::fs::write(".env", dotenv_content)?;
    println!("✅ 环境变量已导出到 .env 文件");
}
```

### 5. 错误处理

```rust
use secret_store_sdk::Error;

match client.get_secret("my-app", "missing-key", GetOpts::default()).await {
    Ok(secret) => println!("密钥值: {}", secret.value.expose_secret()),
    Err(Error::Http { status: 404, .. }) => println!("密钥不存在"),
    Err(Error::Http { status: 401, .. }) => println!("认证失败，请检查 API Key"),
    Err(Error::Http { status: 403, .. }) => println!("权限不足"),
    Err(e) => println!("其他错误: {}", e),
}
```

### 6. 生产环境最佳实践

```rust
let client = ClientBuilder::new("https://your-production-server.com") // 使用 HTTPS
    .auth(Auth::api_key(&std::env::var("SECRET_STORE_API_KEY")?)) // 从环境变量读取
    .timeout_ms(30000)      // 30秒超时
    .retries(3)             // 最多重试3次
    .enable_cache(true)     // 启用缓存提升性能
    .cache_ttl_secs(300)    // 缓存5分钟
    .build()?;
```

## 🔧 特性开关

在 `Cargo.toml` 中启用需要的特性：

```toml
[dependencies]
secret-store-sdk = { version = "0.1.0", features = ["metrics", "blocking"] }
```

可用特性：
- `metrics` - 启用性能指标收集
- `blocking` - 启用同步 API
- `danger-insecure-http` - 允许 HTTP 连接 (仅用于开发)

## 📚 完整示例

运行包含的示例：

```bash
# 运行实时后端演示
cargo run --example live_backend_demo --features danger-insecure-http

# 运行完整测试
cargo test --test live_backend_test --features danger-insecure-http -- --nocapture
```

## 🆘 需要帮助？

- 查看完整文档: [README.md](README.md)
- 运行示例: `examples/` 目录
- 提交问题: GitHub Issues

---

**🎉 现在您已经准备好使用 XJP Secret Store SDK 了！**