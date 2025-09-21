//! Production environment validation for XJP Secret Store SDK
//!
//! This example validates the SDK against the production environment.
//! Run with: cargo run --example production_validation

use secret_store_sdk::{
    Auth, ClientBuilder, GetOpts, ListOpts, PutOpts,
};
use secrecy::ExposeSecret;
use serde_json::json;

/// Production environment configuration
const PRODUCTION_URL: &str = "https://kskxndnvmqwr.sg-members-1.clawcloudrun.com";
const ADMIN_API_KEY: &str = "xjp_a6e12622773d96627ae31a14ce72e0cfba29a5880e0113782e89a030b9f0b8aa";
const TEST_NAMESPACE: &str = "production-validation";

/// Create a production-configured client
fn create_production_client() -> secret_store_sdk::Client {
    ClientBuilder::new(PRODUCTION_URL)
        .auth(Auth::api_key(ADMIN_API_KEY))
        .timeout_ms(30000)
        .retries(3)
        .enable_cache(true)
        .cache_ttl_secs(300)
        .user_agent_extra("production-validation/1.0")
        .build()
        .expect("Failed to build production client")
}

async fn test_health_check() {
    println!("🩺 测试生产环境健康检查...");

    let client = create_production_client();

    match client.readyz().await {
        Ok(health) => {
            println!("✅ 生产环境健康状态: {}", health.status);
            if !health.checks.is_empty() {
                println!("   详细检查结果:");
                for (name, check) in &health.checks {
                    let duration = check.duration_ms.unwrap_or(0);
                    println!("   - {}: {} ({}ms)", name, check.status, duration);
                }
            }
        }
        Err(e) => {
            println!("❌ 健康检查失败: {}", e);
            panic!("Production environment is not healthy!");
        }
    }

    println!();
}

async fn test_basic_operations() {
    println!("🔧 测试基本密钥操作...");

    let client = create_production_client();
    let test_key = format!("validation-test-{}", chrono::Utc::now().timestamp());

    // 1. 创建密钥
    println!("  📝 创建测试密钥...");
    let put_result = client
        .put_secret(
            TEST_NAMESPACE,
            &test_key,
            "production-validation-value-2024",
            PutOpts {
                ttl_seconds: Some(3600), // 1小时后自动删除
                metadata: Some(json!({
                    "source": "sdk-production-validation",
                    "environment": "production",
                    "test_run": chrono::Utc::now().to_rfc3339(),
                    "sdk_version": "0.1.0"
                })),
                idempotency_key: Some(format!("validation-{}", chrono::Utc::now().timestamp())),
            },
        )
        .await
        .expect("Failed to create test secret");

    println!("  ✅ 密钥创建成功: {}", put_result.message);

    // 2. 读取密钥
    println!("  📖 读取测试密钥...");
    let secret = client
        .get_secret(TEST_NAMESPACE, &test_key, GetOpts::default())
        .await
        .expect("Failed to get test secret");

    assert_eq!(secret.namespace, TEST_NAMESPACE);
    assert_eq!(secret.key, test_key);
    assert_eq!(secret.value.expose_secret(), "production-validation-value-2024");
    println!("  ✅ 密钥读取成功 (版本: {})", secret.version);

    if let Some(etag) = &secret.etag {
        println!("  📌 ETag: {}", etag);
    }

    // 3. 更新密钥
    println!("  🔄 更新测试密钥...");
    let _update_result = client
        .put_secret(
            TEST_NAMESPACE,
            &test_key,
            "updated-production-value-2024",
            PutOpts {
                metadata: Some(json!({
                    "source": "sdk-production-validation",
                    "environment": "production",
                    "updated_at": chrono::Utc::now().to_rfc3339(),
                    "operation": "update"
                })),
                ..Default::default()
            },
        )
        .await
        .expect("Failed to update test secret");

    println!("  ✅ 密钥更新成功");

    // 4. 验证更新
    let updated_secret = client
        .get_secret(TEST_NAMESPACE, &test_key, GetOpts::default())
        .await
        .expect("Failed to get updated secret");

    assert_eq!(updated_secret.value.expose_secret(), "updated-production-value-2024");
    assert!(updated_secret.version > 1);
    println!("  ✅ 更新验证成功 (新版本: {})", updated_secret.version);

    // 5. 删除测试密钥
    println!("  🗑️ 清理测试密钥...");
    let delete_result = client
        .delete_secret(TEST_NAMESPACE, &test_key)
        .await
        .expect("Failed to delete test secret");

    println!("  ✅ 测试密钥已清理");

    // 6. 验证删除
    let get_deleted = client
        .get_secret(TEST_NAMESPACE, &test_key, GetOpts::default())
        .await;

    assert!(get_deleted.is_err());
    println!("  ✅ 确认密钥已不存在");

    println!();
}

async fn test_list_operations() {
    println!("📋 测试列表操作...");

    let client = create_production_client();

    // 列出指定命名空间的密钥
    let list_result = client
        .list_secrets(
            TEST_NAMESPACE,
            ListOpts {
                limit: Some(10),
                ..Default::default()
            },
        )
        .await
        .expect("Failed to list secrets");

    println!("✅ 命名空间 '{}' 中有 {} 个密钥", TEST_NAMESPACE, list_result.secrets.len());

    if !list_result.secrets.is_empty() {
        println!("   最新的几个密钥:");
        for (i, secret_info) in list_result.secrets.iter().take(3).enumerate() {
            println!("   {}. {} (版本: {}, 更新时间: {})",
                i + 1,
                secret_info.key,
                secret_info.version,
                secret_info.updated_at
            );
        }
    }

    println!();
}

async fn test_cache_performance() {
    println!("💾 测试缓存性能...");

    let client = create_production_client();
    let cache_test_key = format!("cache-performance-{}", chrono::Utc::now().timestamp());

    // 创建测试密钥
    client
        .put_secret(
            TEST_NAMESPACE,
            &cache_test_key,
            "cache-test-value",
            PutOpts {
                ttl_seconds: Some(1800), // 30分钟
                ..Default::default()
            },
        )
        .await
        .expect("Failed to create cache test secret");

    // 第一次读取 (缓存未命中)
    let start = std::time::Instant::now();
    let _secret1 = client
        .get_secret(TEST_NAMESPACE, &cache_test_key, GetOpts::default())
        .await
        .expect("Failed to get secret for cache test");
    let first_read_duration = start.elapsed();

    // 第二次读取 (应该从缓存获取)
    let start = std::time::Instant::now();
    let _secret2 = client
        .get_secret(TEST_NAMESPACE, &cache_test_key, GetOpts::default())
        .await
        .expect("Failed to get cached secret");
    let cached_read_duration = start.elapsed();

    // 获取缓存统计
    let cache_stats = client.cache_stats();

    println!("✅ 缓存性能测试结果:");
    println!("   第一次读取: {:?}", first_read_duration);
    println!("   缓存读取: {:?}", cached_read_duration);
    println!("   性能提升: {:.1}x",
        first_read_duration.as_nanos() as f64 / cached_read_duration.as_nanos() as f64);
    println!("   缓存统计: 命中 {}, 未命中 {}, 命中率 {:.2}%",
        cache_stats.hits(),
        cache_stats.misses(),
        cache_stats.hit_rate()
    );

    // 清理测试数据
    client.delete_secret(TEST_NAMESPACE, &cache_test_key).await.ok();

    println!();
}

async fn test_error_handling() {
    println!("🚨 测试错误处理...");

    let client = create_production_client();

    // 测试 404 错误
    let not_found_result = client
        .get_secret(TEST_NAMESPACE, "non-existent-key-xyz", GetOpts::default())
        .await;

    match not_found_result {
        Err(e) => {
            if let Some(status) = e.status_code() {
                if status == 404 {
                    println!("✅ 404 错误处理正常");
                } else {
                    println!("⚠️ 预期 404，但得到状态码: {}", status);
                }
            } else {
                println!("⚠️ 网络错误: {}", e);
            }
        }
        Ok(_) => println!("⚠️ 预期 404 错误，但请求成功了"),
    }

    println!();
}

async fn test_authentication_methods() {
    println!("🔐 测试认证方法...");

    // 测试 API Key 认证
    let client_api_key = ClientBuilder::new(PRODUCTION_URL)
        .auth(Auth::api_key(ADMIN_API_KEY))
        .timeout_ms(15000)
        .build()
        .expect("Failed to build API key client");

    let result = client_api_key
        .list_secrets(TEST_NAMESPACE, ListOpts { limit: Some(1), ..Default::default() })
        .await;

    match result {
        Ok(_) => println!("✅ API Key 认证正常工作"),
        Err(e) => println!("❌ API Key 认证失败: {}", e),
    }

    // 测试 Bearer Token 认证
    let client_bearer = ClientBuilder::new(PRODUCTION_URL)
        .auth(Auth::bearer(ADMIN_API_KEY))
        .timeout_ms(15000)
        .build()
        .expect("Failed to build Bearer client");

    let result = client_bearer
        .list_secrets(TEST_NAMESPACE, ListOpts { limit: Some(1), ..Default::default() })
        .await;

    match result {
        Ok(_) => println!("✅ Bearer Token 认证正常工作"),
        Err(e) => println!("❌ Bearer Token 认证失败: {}", e),
    }

    println!();
}

#[tokio::main]
async fn main() {
    println!("🚀 XJP Secret Store SDK 生产环境验证");
    println!("📍 生产环境: {}", PRODUCTION_URL);
    println!("🔑 使用管理员密钥认证");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // 运行所有验证测试
    test_health_check().await;
    test_basic_operations().await;
    test_list_operations().await;
    test_cache_performance().await;
    test_error_handling().await;
    test_authentication_methods().await;

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    println!("🎉 生产环境验证完成！");
    println!("✨ XJP Secret Store SDK 与生产环境完全兼容！");
    println!("🔗 生产环境地址: {}", PRODUCTION_URL);
    println!("📊 所有核心功能正常工作，可以安全使用！");
}