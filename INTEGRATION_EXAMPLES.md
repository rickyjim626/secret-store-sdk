# 集成示例：在实际项目中使用 XJP Secret Store SDK

## 🏗️ 项目集成模式

### 1. Web 服务集成

#### Axum Web 框架示例

```rust
use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use secret_store_sdk::{Client, ClientBuilder, Auth, GetOpts};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower::ServiceBuilder;

// 应用状态
#[derive(Clone)]
struct AppState {
    secret_client: Arc<Client>,
}

// API 响应
#[derive(Serialize)]
struct ApiResponse {
    message: String,
    data: Option<serde_json::Value>,
}

// 配置请求
#[derive(Deserialize)]
struct ConfigRequest {
    environment: String,
}

#[tokio::main]
async fn main() {
    // 初始化 Secret Store 客户端
    let secret_client = Arc::new(
        ClientBuilder::new(&std::env::var("SECRET_STORE_URL").unwrap())
            .auth(Auth::api_key(&std::env::var("SECRET_STORE_API_KEY").unwrap()))
            .timeout_ms(10000)
            .retries(3)
            .enable_cache(true)
            .build()
            .expect("Failed to create secret store client")
    );

    let state = AppState { secret_client };

    // 构建路由
    let app = Router::new()
        .route("/config/:env", get(get_config))
        .route("/config", post(update_config))
        .route("/health", get(health_check))
        .layer(
            ServiceBuilder::new()
                .layer(Extension(state))
        );

    // 启动服务器
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// 获取环境配置
async fn get_config(
    Extension(state): Extension<AppState>,
    Path(env): Path<String>,
) -> Result<Json<ApiResponse>, StatusCode> {
    match state.secret_client.get_secret(&env, "database-url", GetOpts::default()).await {
        Ok(secret) => Ok(Json(ApiResponse {
            message: "配置获取成功".to_string(),
            data: Some(serde_json::json!({
                "database_url": secret.value.expose_secret(),
                "version": secret.version,
                "updated_at": secret.updated_at
            })),
        })),
        Err(e) => {
            eprintln!("获取配置失败: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// 更新配置
async fn update_config(
    Extension(state): Extension<AppState>,
    Json(payload): Json<ConfigRequest>,
) -> Result<Json<ApiResponse>, StatusCode> {
    // 这里可以添加更新逻辑
    Ok(Json(ApiResponse {
        message: format!("环境 {} 的配置已更新", payload.environment),
        data: None,
    }))
}

// 健康检查
async fn health_check(
    Extension(state): Extension<AppState>,
) -> Result<Json<ApiResponse>, StatusCode> {
    match state.secret_client.readyz().await {
        Ok(_) => Ok(Json(ApiResponse {
            message: "服务健康".to_string(),
            data: Some(serde_json::json!({"status": "ok"})),
        })),
        Err(_) => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}
```

### 2. 配置管理服务

```rust
use secret_store_sdk::{Client, ClientBuilder, Auth, GetOpts, PutOpts, ListOpts};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::time::{interval, Duration};

#[derive(Debug, Serialize, Deserialize)]
struct AppConfig {
    database: DatabaseConfig,
    redis: RedisConfig,
    api_keys: HashMap<String, String>,
    feature_flags: HashMap<String, bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DatabaseConfig {
    url: String,
    max_connections: u32,
    timeout_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct RedisConfig {
    url: String,
    pool_size: u32,
}

pub struct ConfigManager {
    client: Client,
    cache: tokio::sync::RwLock<Option<AppConfig>>,
    environment: String,
}

impl ConfigManager {
    pub async fn new(environment: String) -> Result<Self, Box<dyn std::error::Error>> {
        let client = ClientBuilder::new(&std::env::var("SECRET_STORE_URL")?)
            .auth(Auth::api_key(&std::env::var("SECRET_STORE_API_KEY")?))
            .enable_cache(true)
            .cache_ttl_secs(300) // 5分钟缓存
            .build()?;

        Ok(Self {
            client,
            cache: tokio::sync::RwLock::new(None),
            environment,
        })
    }

    // 加载完整配置
    pub async fn load_config(&self) -> Result<AppConfig, Box<dyn std::error::Error>> {
        // 批量获取所有配置
        let secrets = self.client.list_secrets(&self.environment, ListOpts::default()).await?;

        let mut database_url = String::new();
        let mut redis_url = String::new();
        let mut api_keys = HashMap::new();
        let mut feature_flags = HashMap::new();

        // 批量获取密钥值
        for secret_info in secrets.secrets {
            let secret = self.client.get_secret(
                &self.environment,
                &secret_info.key,
                GetOpts::default()
            ).await?;

            match secret_info.key.as_str() {
                "database-url" => database_url = secret.value.expose_secret().to_string(),
                "redis-url" => redis_url = secret.value.expose_secret().to_string(),
                key if key.starts_with("api-key-") => {
                    let service = key.strip_prefix("api-key-").unwrap();
                    api_keys.insert(service.to_string(), secret.value.expose_secret().to_string());
                }
                key if key.starts_with("feature-") => {
                    let feature = key.strip_prefix("feature-").unwrap();
                    let enabled = secret.value.expose_secret() == "true";
                    feature_flags.insert(feature.to_string(), enabled);
                }
                _ => {}
            }
        }

        let config = AppConfig {
            database: DatabaseConfig {
                url: database_url,
                max_connections: 20,
                timeout_seconds: 30,
            },
            redis: RedisConfig {
                url: redis_url,
                pool_size: 10,
            },
            api_keys,
            feature_flags,
        };

        // 更新缓存
        *self.cache.write().await = Some(config.clone());

        Ok(config)
    }

    // 获取缓存的配置
    pub async fn get_config(&self) -> Result<AppConfig, Box<dyn std::error::Error>> {
        {
            let cache = self.cache.read().await;
            if let Some(config) = cache.as_ref() {
                return Ok(config.clone());
            }
        }

        // 缓存未命中，重新加载
        self.load_config().await
    }

    // 更新特定配置
    pub async fn update_feature_flag(&self, feature: &str, enabled: bool) -> Result<(), Box<dyn std::error::Error>> {
        let key = format!("feature-{}", feature);
        let value = if enabled { "true" } else { "false" };

        self.client.put_secret(
            &self.environment,
            &key,
            value,
            PutOpts {
                metadata: Some(serde_json::json!({
                    "type": "feature_flag",
                    "updated_by": "config_manager",
                    "updated_at": chrono::Utc::now().to_rfc3339()
                })),
                ..Default::default()
            }
        ).await?;

        // 清除缓存以强制重新加载
        *self.cache.write().await = None;

        Ok(())
    }

    // 启动配置监控 (定期刷新)
    pub async fn start_refresh_task(&self) {
        let mut interval = interval(Duration::from_secs(300)); // 每5分钟刷新

        loop {
            interval.tick().await;
            if let Err(e) = self.load_config().await {
                eprintln!("配置刷新失败: {}", e);
            } else {
                println!("配置已刷新");
            }
        }
    }
}

// 使用示例
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_manager = ConfigManager::new("production".to_string()).await?;

    // 启动后台刷新任务
    let manager_clone = config_manager.clone();
    tokio::spawn(async move {
        manager_clone.start_refresh_task().await;
    });

    // 加载初始配置
    let config = config_manager.load_config().await?;
    println!("应用配置已加载: {:?}", config);

    // 应用主逻辑
    loop {
        // 获取当前配置
        let current_config = config_manager.get_config().await?;

        // 检查功能开关
        if current_config.feature_flags.get("new_feature").unwrap_or(&false) {
            println!("新功能已启用");
        }

        // 模拟一些工作
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}
```

### 3. Kubernetes 集成

#### Kubernetes Secret 同步器

```rust
use secret_store_sdk::{Client, ClientBuilder, Auth, ListOpts, GetOpts};
use k8s_openapi::api::core::v1::Secret as K8sSecret;
use kube::{Api, Client as KubeClient, ResourceExt};
use std::collections::BTreeMap;
use tokio::time::{interval, Duration};

pub struct SecretSyncer {
    secret_client: Client,
    kube_client: KubeClient,
    namespace: String,
}

impl SecretSyncer {
    pub async fn new(namespace: String) -> Result<Self, Box<dyn std::error::Error>> {
        let secret_client = ClientBuilder::new(&std::env::var("SECRET_STORE_URL")?)
            .auth(Auth::api_key(&std::env::var("SECRET_STORE_API_KEY")?))
            .build()?;

        let kube_client = KubeClient::try_default().await?;

        Ok(Self {
            secret_client,
            kube_client,
            namespace,
        })
    }

    pub async fn sync_secrets(&self, secret_namespace: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("开始同步密钥到 Kubernetes namespace: {}", self.namespace);

        // 获取所有密钥
        let secrets = self.secret_client.list_secrets(secret_namespace, ListOpts::default()).await?;

        for secret_info in secrets.secrets {
            // 获取密钥值
            let secret = self.secret_client.get_secret(
                secret_namespace,
                &secret_info.key,
                GetOpts::default()
            ).await?;

            // 创建 Kubernetes Secret
            let mut data = BTreeMap::new();
            data.insert(
                secret_info.key.clone(),
                k8s_openapi::ByteString(secret.value.expose_secret().as_bytes().to_vec())
            );

            let k8s_secret = K8sSecret {
                metadata: kube::api::ObjectMeta {
                    name: Some(format!("xjp-secret-{}", secret_info.key.replace("_", "-"))),
                    namespace: Some(self.namespace.clone()),
                    labels: Some({
                        let mut labels = BTreeMap::new();
                        labels.insert("managed-by".to_string(), "xjp-secret-store-sdk".to_string());
                        labels.insert("source-namespace".to_string(), secret_namespace.to_string());
                        labels
                    }),
                    annotations: Some({
                        let mut annotations = BTreeMap::new();
                        annotations.insert("xjp.io/version".to_string(), secret.version.to_string());
                        annotations.insert("xjp.io/updated-at".to_string(), secret.updated_at.to_string());
                        annotations
                    }),
                    ..Default::default()
                },
                data: Some(data),
                ..Default::default()
            };

            // 应用到 Kubernetes
            let secrets_api: Api<K8sSecret> = Api::namespaced(self.kube_client.clone(), &self.namespace);

            match secrets_api.create(&kube::api::PostParams::default(), &k8s_secret).await {
                Ok(_) => println!("✅ 已创建 K8s Secret: {}", secret_info.key),
                Err(kube::Error::Api(kube::core::ErrorResponse { code: 409, .. })) => {
                    // Secret 已存在，更新它
                    let name = k8s_secret.metadata.name.as_ref().unwrap();
                    match secrets_api.replace(name, &kube::api::PostParams::default(), &k8s_secret).await {
                        Ok(_) => println!("🔄 已更新 K8s Secret: {}", secret_info.key),
                        Err(e) => eprintln!("❌ 更新 K8s Secret 失败 {}: {}", secret_info.key, e),
                    }
                }
                Err(e) => eprintln!("❌ 创建 K8s Secret 失败 {}: {}", secret_info.key, e),
            }
        }

        Ok(())
    }

    pub async fn start_sync_loop(&self, secret_namespace: &str) {
        let mut interval = interval(Duration::from_secs(60)); // 每分钟同步一次

        loop {
            interval.tick().await;
            if let Err(e) = self.sync_secrets(secret_namespace).await {
                eprintln!("同步失败: {}", e);
            }
        }
    }
}

// Deployment YAML 示例
const DEPLOYMENT_YAML: &str = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: secret-syncer
  namespace: default
spec:
  replicas: 1
  selector:
    matchLabels:
      app: secret-syncer
  template:
    metadata:
      labels:
        app: secret-syncer
    spec:
      containers:
      - name: syncer
        image: your-registry/secret-syncer:latest
        env:
        - name: SECRET_STORE_URL
          value: "https://your-secret-store.com"
        - name: SECRET_STORE_API_KEY
          valueFrom:
            secretKeyRef:
              name: secret-store-credentials
              key: api-key
        - name: KUBE_NAMESPACE
          valueFrom:
            fieldRef:
              fieldPath: metadata.namespace
      serviceAccountName: secret-syncer
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: secret-syncer
  namespace: default
---
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: secret-syncer
  namespace: default
rules:
- apiGroups: [""]
  resources: ["secrets"]
  verbs: ["get", "list", "create", "update", "patch"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata:
  name: secret-syncer
  namespace: default
subjects:
- kind: ServiceAccount
  name: secret-syncer
  namespace: default
roleRef:
  kind: Role
  name: secret-syncer
  apiGroup: rbac.authorization.k8s.io
"#;
```

### 4. CI/CD 集成

#### GitHub Actions 工作流

```yaml
# .github/workflows/deploy.yml
name: Deploy with Secret Store

on:
  push:
    branches: [ main ]

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v4

    - name: Setup Rust
      uses: actions-rs/toolchain@v1
      with:
        toolchain: stable

    - name: Build deployment tool
      run: |
        cat > deploy_tool.rs << 'EOF'
        use secret_store_sdk::{ClientBuilder, Auth, GetOpts, BatchKeys, ExportFormat};
        use std::fs;

        #[tokio::main]
        async fn main() -> Result<(), Box<dyn std::error::Error>> {
            let client = ClientBuilder::new(&std::env::var("SECRET_STORE_URL")?)
                .auth(Auth::api_key(&std::env::var("SECRET_STORE_API_KEY")?))
                .build()?;

            // 导出生产环境配置
            let export = client.batch_get(
                "production",
                BatchKeys::All,
                ExportFormat::Dotenv
            ).await?;

            if let secret_store_sdk::BatchGetResult::Text(env_content) = export {
                fs::write(".env.production", env_content)?;
                println!("生产环境配置已导出");
            }

            Ok(())
        }
        EOF

        # 创建临时 Cargo.toml
        cat > Cargo.toml << 'EOF'
        [package]
        name = "deploy_tool"
        version = "0.1.0"
        edition = "2021"

        [dependencies]
        secret-store-sdk = { git = "https://github.com/your-org/secret-store-sdk.git" }
        tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
        EOF

        cargo run --bin deploy_tool

    - name: Deploy to production
      env:
        SECRET_STORE_URL: ${{ secrets.SECRET_STORE_URL }}
        SECRET_STORE_API_KEY: ${{ secrets.SECRET_STORE_API_KEY }}
      run: |
        # 使用导出的环境变量进行部署
        source .env.production
        # 您的部署命令
        echo "部署到生产环境..."
```

### 5. 监控和告警集成

```rust
use secret_store_sdk::{Client, ClientBuilder, Auth};
use tokio::time::{interval, Duration};
use serde_json::json;

pub struct SecretMonitor {
    client: Client,
    alert_webhook: String,
}

impl SecretMonitor {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let client = ClientBuilder::new(&std::env::var("SECRET_STORE_URL")?)
            .auth(Auth::api_key(&std::env::var("SECRET_STORE_API_KEY")?))
            .build()?;

        let alert_webhook = std::env::var("ALERT_WEBHOOK_URL")?;

        Ok(Self {
            client,
            alert_webhook,
        })
    }

    pub async fn check_secret_health(&self) -> Result<(), Box<dyn std::error::Error>> {
        // 检查关键密钥
        let critical_secrets = vec![
            ("production", "database-url"),
            ("production", "api-key"),
            ("production", "jwt-secret"),
        ];

        for (namespace, key) in critical_secrets {
            match self.client.get_secret(namespace, key, Default::default()).await {
                Ok(secret) => {
                    // 检查密钥是否即将过期
                    if let Some(expires_at) = secret.expires_at {
                        let now = time::OffsetDateTime::now_utc();
                        let time_until_expiry = expires_at - now;

                        if time_until_expiry < time::Duration::days(7) {
                            self.send_alert(&format!(
                                "⚠️ 密钥即将过期: {}/{} 将在 {} 过期",
                                namespace, key, expires_at
                            )).await?;
                        }
                    }

                    // 检查密钥版本是否太旧
                    if secret.version == 1 &&
                       (time::OffsetDateTime::now_utc() - secret.updated_at) > time::Duration::days(90) {
                        self.send_alert(&format!(
                            "🔄 密钥需要轮换: {}/{} 已 90 天未更新",
                            namespace, key
                        )).await?;
                    }
                }
                Err(e) => {
                    self.send_alert(&format!(
                        "❌ 无法访问关键密钥: {}/{} - {}",
                        namespace, key, e
                    )).await?;
                }
            }
        }

        // 检查服务健康状态
        match self.client.readyz().await {
            Ok(health) => {
                for (check_name, check_result) in &health.checks {
                    if check_result.status != "ok" {
                        self.send_alert(&format!(
                            "🚨 Secret Store 健康检查失败: {} - {}",
                            check_name, check_result.status
                        )).await?;
                    }
                }
            }
            Err(e) => {
                self.send_alert(&format!("🚨 Secret Store 服务不可用: {}", e)).await?;
            }
        }

        Ok(())
    }

    async fn send_alert(&self, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();
        let payload = json!({
            "text": message,
            "channel": "#alerts",
            "username": "Secret Store Monitor"
        });

        client.post(&self.alert_webhook)
            .json(&payload)
            .send()
            .await?;

        println!("告警已发送: {}", message);
        Ok(())
    }

    pub async fn start_monitoring(&self) {
        let mut interval = interval(Duration::from_secs(3600)); // 每小时检查一次

        loop {
            interval.tick().await;
            if let Err(e) = self.check_secret_health().await {
                eprintln!("监控检查失败: {}", e);
            }
        }
    }
}
```

## 🚀 最佳实践总结

### 1. 项目结构建议

```
your-project/
├── src/
│   ├── config/
│   │   ├── mod.rs
│   │   └── secrets.rs      # Secret Store 集成
│   ├── services/
│   └── main.rs
├── .env.example            # 环境变量模板
├── docker-compose.yml      # 开发环境
└── k8s/                   # Kubernetes 清单
    ├── deployment.yml
    └── secret-syncer.yml
```

### 2. 环境变量管理

```bash
# 开发环境
SECRET_STORE_URL=http://localhost:8080
SECRET_STORE_API_KEY=dev-key

# 生产环境
SECRET_STORE_URL=https://secret-store.prod.com
SECRET_STORE_API_KEY=prod-key
```

### 3. 错误处理模式

```rust
// 自定义错误类型
#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("Secret Store 错误: {0}")]
    SecretStore(#[from] secret_store_sdk::Error),

    #[error("配置错误: {0}")]
    Config(String),
}

// 统一错误处理
pub async fn get_config_safely(key: &str) -> Result<String, AppError> {
    match secret_client.get_secret("app", key, Default::default()).await {
        Ok(secret) => Ok(secret.value.expose_secret().to_string()),
        Err(secret_store_sdk::Error::Http { status: 404, .. }) => {
            Err(AppError::Config(format!("配置 {} 不存在", key)))
        }
        Err(e) => Err(AppError::SecretStore(e)),
    }
}
```

---

**🎯 现在您已经有了完整的 SDK 和集成指南！**

您可以选择适合的发布方式，并使用这些示例来帮助其他开发者快速集成您的 SDK。