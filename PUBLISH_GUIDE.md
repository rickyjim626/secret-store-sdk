# XJP Secret Store SDK 发布指南

## 📦 发布选项

### 选项 1: 发布到 crates.io (推荐)

这是 Rust 生态系统的官方包管理器，最容易让其他开发者使用。

#### 准备步骤

1. **注册 crates.io 账户**
   ```bash
   # 创建账户: https://crates.io/
   # 获取 API token: https://crates.io/me
   cargo login <your-api-token>
   ```

2. **验证包配置**
   ```bash
   # 检查 Cargo.toml 配置
   cargo check --all-features
   cargo test --all-features
   cargo clippy --all-features
   ```

3. **发布**
   ```bash
   # 干运行检查
   cargo publish --dry-run

   # 正式发布
   cargo publish
   ```

#### 优点
- ✅ 用户可以直接 `cargo add secret-store-sdk`
- ✅ 自动文档生成 (docs.rs)
- ✅ 版本管理
- ✅ 依赖解析

#### 缺点
- ❌ 公开可见
- ❌ 无法撤回已发布版本

### 选项 2: 私有 Git 仓库

适合企业内部或私有项目使用。

#### 设置步骤

1. **创建 Git 仓库**
   ```bash
   # 推送到您的 Git 仓库
   git remote add origin https://github.com/your-org/secret-store-sdk.git
   git push -u origin main

   # 创建版本标签
   git tag v0.1.0
   git push origin v0.1.0
   ```

2. **用户使用**
   ```toml
   [dependencies]
   secret-store-sdk = { git = "https://github.com/your-org/secret-store-sdk.git", tag = "v0.1.0" }
   ```

#### 优点
- ✅ 私有控制
- ✅ 可以撤回/修改
- ✅ 企业访问控制

#### 缺点
- ❌ 用户需要 Git 访问权限
- ❌ 没有自动文档

### 选项 3: 企业内部包管理

设置私有 Cargo registry。

#### 设置 (使用 Kellnr)

1. **部署 Kellnr**
   ```bash
   # 使用 Docker
   docker run -d -p 8000:8000 --name kellnr kellnr/kellnr:latest
   ```

2. **配置 .cargo/config.toml**
   ```toml
   [registries]
   internal = { index = "https://your-kellnr-instance.com/git/index" }

   [registry]
   default = "internal"
   ```

3. **发布到内部 registry**
   ```bash
   cargo publish --registry internal
   ```

## 🔧 当前 Cargo.toml 配置检查

您的 `Cargo.toml` 已经配置得很好：

```toml
[package]
name = "secret-store-sdk"           # ✅ 清晰的包名
version = "0.1.0"                   # ✅ 初始版本
edition = "2021"                    # ✅ 最新 Rust edition
rust-version = "1.75"               # ✅ 最低 Rust 版本
authors = ["XJP Team"]              # ✅ 作者信息
description = "Rust SDK for XJP Secret Store Service"  # ✅ 描述
license = "MIT OR Apache-2.0"       # ✅ 开源许可
repository = "https://github.com/rickyjim626/secret-store-sdk"  # ✅ 仓库地址
documentation = "https://docs.rs/secret-store-sdk"     # ✅ 文档地址
keywords = ["secret", "vault", "encryption", "configuration", "sdk"]  # ✅ 关键词
categories = ["api-bindings", "cryptography", "config"]  # ✅ 分类
```

### 需要更新的配置 (可选)

如果要发布到 crates.io，可以考虑更新：

```toml
[package]
# 添加更详细的描述
description = """
XJP Secret Store SDK for Rust - A comprehensive SDK for secure secret management.
Supports caching, batch operations, multiple auth methods, and real-time backend integration.
"""

# 添加更多关键词
keywords = ["secret", "vault", "encryption", "config", "security"]

# 添加排除文件 (减少包大小)
exclude = [
    "tests/",
    "examples/live_backend_*",
    "*.md",
    ".github/",
    "scripts/",
]

# 添加包含文件 (如果需要特定文件)
include = [
    "src/**/*",
    "examples/basic_*.rs",
    "Cargo.toml",
    "LICENSE*",
    "README.md",
]
```

## 🚀 发布流程

### 1. 预发布检查清单

- [ ] 所有测试通过: `cargo test --all-features`
- [ ] 代码格式化: `cargo fmt`
- [ ] Clippy 检查: `cargo clippy --all-features -- -D warnings`
- [ ] 文档生成: `cargo doc --all-features`
- [ ] 示例运行: `cargo run --example live_backend_demo --features danger-insecure-http`
- [ ] 安全审计: `cargo audit` (需要安装 cargo-audit)

### 2. 版本发布

```bash
# 1. 更新版本号
sed -i 's/version = "0.1.0"/version = "0.1.1"/' Cargo.toml

# 2. 更新 CHANGELOG.md (如果有)
echo "## [0.1.1] - $(date +%Y-%m-%d)" >> CHANGELOG.md

# 3. 提交更改
git add .
git commit -m "Bump version to 0.1.1"

# 4. 创建标签
git tag v0.1.1
git push origin main --tags

# 5. 发布 (如果选择 crates.io)
cargo publish
```

### 3. 自动化发布 (GitHub Actions)

创建 `.github/workflows/release.yml`:

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v4

    - name: Setup Rust
      uses: actions-rs/toolchain@v1
      with:
        toolchain: stable
        override: true

    - name: Cache dependencies
      uses: actions/cache@v3
      with:
        path: target
        key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

    - name: Run tests
      run: cargo test --all-features

    - name: Publish to crates.io
      run: cargo publish --token ${{ secrets.CARGO_TOKEN }}
```

## 👥 用户集成指南

### 给其他开发者的使用说明

#### 基本安装 (crates.io)

```toml
[dependencies]
secret-store-sdk = "0.1.0"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

#### Git 安装

```toml
[dependencies]
secret-store-sdk = { git = "https://github.com/your-org/secret-store-sdk.git", tag = "v0.1.0" }
```

#### 本地开发安装

```toml
[dependencies]
secret-store-sdk = { path = "../path/to/secret-store-sdk" }
```

### 环境配置模板

为用户提供 `.env.example`:

```bash
# XJP Secret Store 配置
XJP_SECRET_STORE_URL=https://your-secret-store.example.com
XJP_SECRET_STORE_API_KEY=your-api-key-here

# 可选配置
XJP_SECRET_STORE_TIMEOUT_MS=30000
XJP_SECRET_STORE_RETRIES=3
XJP_SECRET_STORE_CACHE_ENABLED=true
XJP_SECRET_STORE_CACHE_TTL_SECS=300
```

### Docker 集成示例

```dockerfile
FROM rust:1.75 as builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/your-app /usr/local/bin/your-app

ENV XJP_SECRET_STORE_URL=https://secret-store.example.com
CMD ["your-app"]
```

## 📋 集成测试

为用户提供集成测试模板：

```rust
// tests/integration_test.rs
use secret_store_sdk::{ClientBuilder, Auth, GetOpts, PutOpts};

#[tokio::test]
async fn test_sdk_integration() {
    let client = ClientBuilder::new(&std::env::var("XJP_SECRET_STORE_URL").unwrap())
        .auth(Auth::api_key(&std::env::var("XJP_SECRET_STORE_API_KEY").unwrap()))
        .build()
        .unwrap();

    // 测试基本功能
    let test_key = format!("test-{}", uuid::Uuid::new_v4());

    // 创建
    client.put_secret("test", &test_key, "test-value", PutOpts::default())
        .await
        .unwrap();

    // 读取
    let secret = client.get_secret("test", &test_key, GetOpts::default())
        .await
        .unwrap();

    assert_eq!(secret.value.expose_secret(), "test-value");

    // 清理
    client.delete_secret("test", &test_key).await.unwrap();
}
```

## 🔒 安全发布清单

- [ ] 检查代码中没有硬编码的密钥或敏感信息
- [ ] 确保示例使用环境变量或占位符
- [ ] 验证 HTTPS 强制执行 (生产环境)
- [ ] 检查依赖项的安全漏洞: `cargo audit`
- [ ] 确保错误消息不泄露敏感信息

## 📊 发布后监控

1. **监控使用情况** (如果使用 crates.io)
   - 下载统计: https://crates.io/crates/secret-store-sdk
   - 依赖图: https://deps.rs/crate/secret-store-sdk

2. **用户反馈**
   - GitHub Issues
   - crates.io 评论
   - 社区论坛

3. **维护任务**
   - 定期更新依赖项
   - 安全补丁
   - 性能优化
   - 新功能开发

---

## 🎯 推荐发布策略

**对于您的情况，我推荐：**

1. **短期**: 使用私有 Git 仓库 + 标签版本管理
2. **中期**: 如果决定开源，发布到 crates.io
3. **长期**: 设置 CI/CD 自动化发布流程

这样您可以先内部使用和测试，再考虑是否公开发布。