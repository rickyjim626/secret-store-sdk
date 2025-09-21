# 🎉 XJP Secret Store SDK for Rust 现已可用！

亲爱的开发者们，

我们很高兴地宣布 **XJP Secret Store SDK for Rust** 现已正式可用！这是一个高性能、功能丰富的 Rust SDK，让您可以轻松安全地管理应用程序密钥。

## 🚀 **立即开始使用**

### **第一步：添加依赖**

在您的 `Cargo.toml` 中添加：

```toml
[dependencies]
secret-store-sdk = { git = "https://github.com/rickyjim626/secret-store-sdk.git", tag = "v0.1.0" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1"
```

### **第二步：设置环境变量**

创建 `.env` 文件：

```bash
XJP_SECRET_STORE_URL=http://34.92.201.151:8080
XJP_SECRET_STORE_API_KEY=sk-prod-8e9f874dfc324f0f6dca2eee9e4ecdcc5ca57e74b9e4131e
```

### **第三步：开始编码**

```rust
use secret_store_sdk::{ClientBuilder, Auth, GetOpts, PutOpts};
use secrecy::ExposeSecret;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建客户端
    let client = ClientBuilder::new("http://34.92.201.151:8080")
        .auth(Auth::api_key("sk-prod-8e9f874dfc324f0f6dca2eee9e4ecdcc5ca57e74b9e4131e"))
        .timeout_ms(30000)
        .retries(3)
        .enable_cache(true)
        .allow_insecure_http() // 仅测试环境
        .build()?;

    // 存储密钥
    client.put_secret(
        "my-app",
        "database-url",
        "postgresql://user:pass@localhost:5432/db",
        PutOpts::default()
    ).await?;

    // 读取密钥
    let secret = client.get_secret("my-app", "database-url", GetOpts::default()).await?;
    println!("数据库URL: {}", secret.value.expose_secret());

    Ok(())
}
```

## ✨ **核心功能**

- 🔐 **安全第一**: 密钥值自动保护，防止意外泄露
- ⚡ **高性能**: 内置缓存，~10μs 读取速度
- 🔄 **自动重试**: 智能重试机制，处理网络抖动
- 📦 **批量操作**: 高效的批量读写操作
- 🌐 **多种认证**: 支持 API Key、Bearer Token、XJP Key
- 🎯 **生产就绪**: 完整的错误处理和监控支持

## 🎯 **适用场景**

- **Web 应用**: 数据库连接字符串、API 密钥管理
- **微服务**: 服务间认证凭据管理
- **CI/CD**: 部署密钥和环境配置
- **Kubernetes**: 自动同步 Secret 资源
- **配置管理**: 集中化的应用配置存储

## 📚 **完整文档**

| 文档 | 描述 | 链接 |
|------|------|------|
| 📖 **完整文档** | 详细的 API 文档和示例 | [README.md](README.md) |
| ⚡ **快速开始** | 5分钟上手指南 | [QUICK_START_CN.md](QUICK_START_CN.md) |
| 🔧 **集成示例** | 实际项目集成案例 | [INTEGRATION_EXAMPLES.md](INTEGRATION_EXAMPLES.md) |
| 🚀 **部署指南** | 发布和部署说明 | [PUBLISH_GUIDE.md](PUBLISH_GUIDE.md) |

## 🛠️ **快速验证**

运行我们的演示程序验证连接：

```bash
git clone https://github.com/rickyjim626/secret-store-sdk.git
cd secret-store-sdk
cargo run --example live_backend_demo --features danger-insecure-http
```

您将看到完整的功能演示，包括创建、读取、更新、删除密钥等操作。

## 🤝 **获得帮助**

- 📧 **技术支持**: [提交 GitHub Issue](https://github.com/rickyjim626/secret-store-sdk/issues)
- 💬 **技术讨论**: [GitHub Discussions](https://github.com/rickyjim626/secret-store-sdk/discussions)
- 📖 **详细文档**: 查看项目 README 文件
- 💡 **最佳实践**: 参考集成示例文档

## 🎯 **快速集成检查表**

- [ ] 添加 SDK 依赖到 `Cargo.toml`
- [ ] 设置环境变量 (URL 和 API Key)
- [ ] 运行演示程序验证连接
- [ ] 在您的应用中集成基本功能
- [ ] 根据需要配置缓存和重试策略
- [ ] 设置错误处理和监控

## 🔧 **常见集成模式**

### **Web 服务集成**
```rust
// Axum/Actix-web 等框架
let client = Arc::new(create_secret_client());
// 在请求处理中使用
```

### **配置管理**
```rust
// 应用启动时加载配置
let config = load_app_config_from_secrets().await?;
```

### **Kubernetes 集成**
```rust
// 自动同步到 K8s Secrets
sync_secrets_to_kubernetes().await?;
```

## 🚨 **重要提醒**

1. **安全性**:
   - 生产环境必须使用 HTTPS
   - 不要在代码中硬编码 API Key
   - 使用环境变量或安全的密钥管理

2. **性能**:
   - 启用缓存以提高读取性能
   - 使用批量操作处理多个密钥
   - 合理设置超时和重试参数

3. **监控**:
   - 监控缓存命中率
   - 设置健康检查
   - 记录错误和异常情况

## 💡 **示例项目**

查看 `examples/` 目录中的完整示例：

- `basic_usage.rs` - 基础 CRUD 操作
- `batch_operations.rs` - 批量操作
- `with_cache.rs` - 缓存使用
- `live_backend_demo.rs` - 完整功能演示

## 🎉 **开始您的密钥管理之旅**

现在就开始使用 XJP Secret Store SDK，让您的应用程序密钥管理变得简单、安全、高效！

如有任何问题或建议，欢迎随时联系我们。

---

**Happy Coding! 🚀**

*XJP Team*