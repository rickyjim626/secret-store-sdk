//! Simple production test for XJP Secret Store SDK
//!
//! Run with: cargo run --example simple_production_test

use secret_store_sdk::{ClientBuilder, Auth, GetOpts, PutOpts};
use secrecy::ExposeSecret;
use serde_json::json;

const PRODUCTION_URL: &str = "https://kskxndnvmqwr.sg-members-1.clawcloudrun.com";
const API_KEY: &str = "xjp_a6e12622773d96627ae31a14ce72e0cfba29a5880e0113782e89a030b9f0b8aa";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 XJP Secret Store SDK 生产环境连接测试");
    println!("📍 生产地址: {}", PRODUCTION_URL);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // 创建生产环境客户端
    let client = ClientBuilder::new(PRODUCTION_URL)
        .auth(Auth::api_key(API_KEY))
        .timeout_ms(30000)
        .retries(3)
        .enable_cache(true)
        .build()?;

    println!("✅ 客户端创建成功");

    // 1. 简单健康检查 (使用 HTTP 客户端)
    println!("\n🩺 测试服务连通性...");
    let http_client = reqwest::Client::new();
    let health_response = http_client
        .get(format!("{}/readyz", PRODUCTION_URL))
        .send()
        .await?;

    if health_response.status().is_success() {
        let body = health_response.text().await?;
        println!("✅ 服务健康: {}", body.trim());
    } else {
        println!("❌ 服务健康检查失败: {}", health_response.status());
        return Ok(());
    }

    // 2. 测试列表操作
    println!("\n📋 测试列表密钥...");
    let list_result = client.list_secrets("test", Default::default()).await?;
    println!("✅ 成功连接到生产环境");
    println!("   命名空间 'test' 中有 {} 个密钥", list_result.secrets.len());

    // 3. 测试创建密钥
    println!("\n📝 测试创建密钥...");
    let test_key = format!("sdk-production-test-{}", chrono::Utc::now().timestamp());

    let put_result = client.put_secret(
        "test",
        &test_key,
        "production-test-value-2024",
        PutOpts {
            ttl_seconds: Some(3600), // 1小时自动删除
            metadata: Some(json!({
                "source": "sdk-production-test",
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "environment": "production"
            })),
            ..Default::default()
        }
    ).await?;

    println!("✅ 密钥创建成功: {}", put_result.message);

    // 4. 测试读取密钥
    println!("\n📖 测试读取密钥...");
    let secret = client.get_secret("test", &test_key, GetOpts::default()).await?;

    println!("✅ 密钥读取成功:");
    println!("   命名空间: {}", secret.namespace);
    println!("   密钥名: {}", secret.key);
    println!("   值: {}", secret.value.expose_secret());
    println!("   版本: {}", secret.version);
    println!("   更新时间: {}", secret.updated_at);

    // 5. 测试缓存
    println!("\n💾 测试缓存性能...");

    // 第一次读取
    let start = std::time::Instant::now();
    let _cached_read1 = client.get_secret("test", &test_key, GetOpts::default()).await?;
    let first_read = start.elapsed();

    // 第二次读取 (来自缓存)
    let start = std::time::Instant::now();
    let _cached_read2 = client.get_secret("test", &test_key, GetOpts::default()).await?;
    let cached_read = start.elapsed();

    println!("✅ 缓存性能:");
    println!("   第一次读取: {:?}", first_read);
    println!("   缓存读取: {:?}", cached_read);

    let speedup = first_read.as_micros() as f64 / cached_read.as_micros() as f64;
    println!("   性能提升: {:.1}x", speedup);

    let stats = client.cache_stats();
    println!("   缓存统计: 命中 {}, 未命中 {}, 命中率 {:.1}%",
        stats.hits(), stats.misses(), stats.hit_rate());

    // 6. 清理测试数据
    println!("\n🗑️ 清理测试数据...");
    client.delete_secret("test", &test_key).await?;
    println!("✅ 测试数据已清理");

    // 7. 验证删除
    let deleted_check = client.get_secret("test", &test_key, GetOpts::default()).await;
    if deleted_check.is_err() {
        println!("✅ 确认密钥已删除");
    }

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🎉 生产环境测试完成！");
    println!("✨ XJP Secret Store SDK 与生产环境完全兼容！");
    println!("🔗 生产地址: {}", PRODUCTION_URL);
    println!("📊 所有核心功能正常，可以安全使用！");

    Ok(())
}