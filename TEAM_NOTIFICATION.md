# 📢 团队通知：XJP Secret Store SDK 可以使用了！

各位开发者，

我们的 **XJP Secret Store SDK for Rust** 已经开发完成并经过全面测试，现在可以在项目中使用了！

## 🎯 **一分钟上手**

### **1. 添加依赖**
```toml
# 在您的 Cargo.toml 中添加
[dependencies]
secret-store-sdk = { git = "https://github.com/rickyjim626/secret-store-sdk.git" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

### **2. 基本使用**
```rust
use secret_store_sdk::{ClientBuilder, Auth};

let client = ClientBuilder::new("http://34.92.201.151:8080")
    .auth(Auth::api_key("sk-prod-8e9f874dfc324f0f6dca2eee9e4ecdcc5ca57e74b9e4131e"))
    .allow_insecure_http() // 当前测试环境
    .build()?;

// 读取密钥
let secret = client.get_secret("my-app", "database-url", Default::default()).await?;
```

## 🔧 **服务器信息**

- **服务器地址**: `http://34.92.201.151:8080`
- **API Key**: `sk-prod-8e9f874dfc324f0f6dca2eee9e4ecdcc5ca57e74b9e4131e`
- **支持的认证**: API Key、Bearer Token、XJP Key

## 📋 **已验证功能**

✅ 基本密钥操作 (增删改查)
✅ 批量操作
✅ 缓存系统
✅ 自动重试
✅ 错误处理
✅ 多种认证方式

## 📚 **文档位置**

| 文档 | 用途 |
|------|------|
| [QUICK_START_CN.md](QUICK_START_CN.md) | 🚀 快速上手 (推荐先看这个) |
| [README.md](README.md) | 📖 完整文档 |
| [INTEGRATION_EXAMPLES.md](INTEGRATION_EXAMPLES.md) | 🔧 集成示例 |

## 🧪 **快速测试**

克隆仓库并运行演示：

```bash
git clone https://github.com/rickyjim626/secret-store-sdk.git
cd secret-store-sdk
cargo run --example live_backend_demo --features danger-insecure-http
```

## 💡 **最佳实践**

1. **开发环境**: 使用环境变量配置
2. **错误处理**: 参考文档中的错误处理模式
3. **性能优化**: 启用缓存，使用批量操作
4. **安全**: 生产环境切换到 HTTPS

## 🆘 **需要帮助？**

- 📖 先查看快速开始文档
- 💬 技术讨论可以找我
- 🐛 问题反馈请提 GitHub Issue

## 🎯 **推荐接入顺序**

1. **第一步**: 阅读快速开始文档 (5分钟)
2. **第二步**: 运行演示程序验证连接
3. **第三步**: 在测试项目中试用基本功能
4. **第四步**: 根据需要参考集成示例

---

**准备好开始使用了吗？从 [快速开始文档](QUICK_START_CN.md) 开始吧！** 🚀