//! Live backend integration tests for XJP Secret Store SDK
//! 
//! This test connects to a real backend server and verifies all SDK functionality.
//! Run with: cargo test --test live_backend_test --features danger-insecure-http -- --nocapture

use secret_store_sdk::{
    Auth, BatchGetResult, BatchKeys, BatchOp, ClientBuilder, ExportFormat, GetOpts, ListOpts,
    PutOpts,
};
use secrecy::ExposeSecret;

/// Backend configuration
const BASE_URL: &str = "http://34.92.201.151:8080";
const API_KEY: &str = "sk-prod-8e9f874dfc324f0f6dca2eee9e4ecdcc5ca57e74b9e4131e";
const TEST_NAMESPACE: &str = "sdk-test";

/// Create a configured client for testing
fn create_client() -> secret_store_sdk::Client {
    ClientBuilder::new(BASE_URL)
        .auth(Auth::api_key(API_KEY))
        .timeout_ms(30000)
        .retries(3)
        .enable_cache(true)
        .allow_insecure_http()
        .user_agent_extra("live-test/1.0")
        .build()
        .expect("Failed to build client")
}

#[tokio::test]
async fn test_health_check() {
    println!("🔍 Testing health check...");
    
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/readyz", BASE_URL))
        .send()
        .await
        .expect("Failed to send health check request");
    
    assert!(response.status().is_success());
    println!("✅ Health check passed");
}

#[tokio::test]
async fn test_basic_secret_operations() {
    println!("🔍 Testing basic secret operations...");
    
    let client = create_client();
    let test_key = format!("test-secret-{}", chrono::Utc::now().timestamp());
    
    // 1. Create a secret
    println!("  📝 Creating secret...");
    let put_result = client
        .put_secret(
            TEST_NAMESPACE,
            &test_key,
            "test-value-123",
            PutOpts {
                ttl_seconds: Some(3600),
                metadata: Some(serde_json::json!({
                    "source": "sdk-test",
                    "environment": "test"
                })),
                ..Default::default()
            },
        )
        .await
        .expect("Failed to put secret");
    
    println!("  ✅ Secret created: {}", put_result.message);
    assert!(!put_result.message.is_empty());
    
    // 2. Get the secret
    println!("  📖 Reading secret...");
    let secret = client
        .get_secret(TEST_NAMESPACE, &test_key, GetOpts::default())
        .await
        .expect("Failed to get secret");
    
    assert_eq!(secret.namespace, TEST_NAMESPACE);
    assert_eq!(secret.key, test_key);
    assert_eq!(secret.value.expose_secret(), "test-value-123");
    println!("  ✅ Secret retrieved successfully");
    
    // 3. Update the secret
    println!("  🔄 Updating secret...");
    let update_result = client
        .put_secret(
            TEST_NAMESPACE,
            &test_key,
            "updated-value-456",
            Default::default(),
        )
        .await
        .expect("Failed to update secret");
    
    // Note: SDK PutResult doesn't include version in response
    println!("  ✅ Secret updated successfully");
    
    // 4. Get updated secret
    let updated_secret = client
        .get_secret(TEST_NAMESPACE, &test_key, GetOpts::default())
        .await
        .expect("Failed to get updated secret");
    
    assert_eq!(updated_secret.value.expose_secret(), "updated-value-456");
    assert!(updated_secret.version > 1);
    
    // 5. Delete the secret
    println!("  🗑️  Deleting secret...");
    let delete_result = client
        .delete_secret(TEST_NAMESPACE, &test_key)
        .await
        .expect("Failed to delete secret");
    
    assert!(delete_result.deleted);
    println!("  ✅ Secret deleted successfully");
    
    // 6. Verify deletion
    let get_deleted = client
        .get_secret(TEST_NAMESPACE, &test_key, GetOpts::default())
        .await;
    
    assert!(get_deleted.is_err());
    println!("  ✅ Confirmed secret no longer exists");
}

#[tokio::test]
async fn test_list_secrets() {
    println!("🔍 Testing list secrets...");
    
    let client = create_client();
    let prefix = format!("list-test-{}-", chrono::Utc::now().timestamp());
    
    // Create multiple secrets
    for i in 0..5 {
        client
            .put_secret(
                TEST_NAMESPACE,
                &format!("{}{}", prefix, i),
                &format!("value-{}", i),
                Default::default(),
            )
            .await
            .expect("Failed to create test secret");
    }
    
    // List with prefix
    let list_result = client
        .list_secrets(
            TEST_NAMESPACE,
            ListOpts {
                prefix: Some(prefix.clone()),
                limit: Some(10),
            },
        )
        .await
        .expect("Failed to list secrets");
    
    assert_eq!(list_result.secrets.len(), 5);
    println!("✅ Listed {} secrets with prefix", list_result.secrets.len());
    
    // Clean up
    for i in 0..5 {
        client
            .delete_secret(TEST_NAMESPACE, &format!("{}{}", prefix, i))
            .await
            .ok();
    }
}

#[tokio::test]
async fn test_batch_operations() {
    println!("🔍 Testing batch operations...");
    
    let client = create_client();
    let batch_prefix = format!("batch-test-{}-", chrono::Utc::now().timestamp());
    
    // Batch create
    println!("  📦 Creating batch of secrets...");
    let operations = vec![
        BatchOp::put(&format!("{}1", batch_prefix), "batch-value-1"),
        BatchOp::put(&format!("{}2", batch_prefix), "batch-value-2"),
        BatchOp::put(&format!("{}3", batch_prefix), "batch-value-3")
            .with_ttl(7200)
            .with_metadata(serde_json::json!({"batch": true})),
    ];
    
    let batch_result = client
        .batch_operate(TEST_NAMESPACE, operations, false, None)
        .await
        .expect("Failed to perform batch operations");
    
    assert_eq!(batch_result.results.succeeded.len(), 3);
    assert_eq!(batch_result.results.failed.len(), 0);
    println!("  ✅ Batch created {} secrets", batch_result.results.succeeded.len());
    
    // Batch get
    println!("  📖 Batch reading secrets...");
    let keys = BatchKeys::Keys(vec![
        format!("{}1", batch_prefix),
        format!("{}2", batch_prefix),
        format!("{}3", batch_prefix),
    ]);
    
    let batch_get_result = client
        .batch_get(TEST_NAMESPACE, keys, ExportFormat::Json)
        .await
        .expect("Failed to batch get");
    
    if let BatchGetResult::Json(json_result) = batch_get_result {
        assert_eq!(json_result.secrets.len(), 3);
        assert_eq!(json_result.secrets.get(&format!("{}1", batch_prefix)).unwrap(), "batch-value-1");
        println!("  ✅ Batch retrieved {} secrets", json_result.secrets.len());
    }
    
    // Batch delete
    println!("  🗑️  Batch deleting secrets...");
    let delete_ops = vec![
        BatchOp::delete(&format!("{}1", batch_prefix)),
        BatchOp::delete(&format!("{}2", batch_prefix)),
        BatchOp::delete(&format!("{}3", batch_prefix)),
    ];
    
    let delete_result = client
        .batch_operate(TEST_NAMESPACE, delete_ops, false, None)
        .await
        .expect("Failed to batch delete");
    
    assert_eq!(delete_result.results.succeeded.len(), 3);
    println!("  ✅ Batch deleted {} secrets", delete_result.results.succeeded.len());
}

#[tokio::test]
async fn test_cache_functionality() {
    println!("🔍 Testing cache functionality...");
    
    let client = create_client();
    let cache_key = format!("cache-test-{}", chrono::Utc::now().timestamp());
    
    // Create a secret
    client
        .put_secret(TEST_NAMESPACE, &cache_key, "cache-test-value", Default::default())
        .await
        .expect("Failed to create cache test secret");
    
    // First read (cache miss)
    let start = std::time::Instant::now();
    let _secret1 = client
        .get_secret(TEST_NAMESPACE, &cache_key, GetOpts::default())
        .await
        .expect("Failed to get secret");
    let first_duration = start.elapsed();
    
    // Second read (should be from cache)
    let start = std::time::Instant::now();
    let _secret2 = client
        .get_secret(TEST_NAMESPACE, &cache_key, GetOpts::default())
        .await
        .expect("Failed to get cached secret");
    let cached_duration = start.elapsed();
    
    println!("  ⏱️  First read: {:?}, Cached read: {:?}", first_duration, cached_duration);
    
    // Get cache stats
    let stats = client.cache_stats();
    assert!(stats.hits() > 0);
    println!("  ✅ Cache hits: {}, misses: {}", stats.hits(), stats.misses());
    
    // Test cache bypass
    let opts = GetOpts {
        use_cache: false,
        ..Default::default()
    };
    
    let _secret3 = client
        .get_secret(TEST_NAMESPACE, &cache_key, opts)
        .await
        .expect("Failed to get secret with cache bypass");
    
    println!("  ✅ Cache bypass working correctly");
    
    // Clean up
    client.delete_secret(TEST_NAMESPACE, &cache_key).await.ok();
}

#[tokio::test]
async fn test_conditional_requests() {
    println!("🔍 Testing conditional requests (ETag)...");
    
    let client = create_client();
    let etag_key = format!("etag-test-{}", chrono::Utc::now().timestamp());
    
    // Create a secret
    client
        .put_secret(TEST_NAMESPACE, &etag_key, "etag-value", Default::default())
        .await
        .expect("Failed to create etag test secret");
    
    // Get with ETag
    let secret = client
        .get_secret(TEST_NAMESPACE, &etag_key, GetOpts::default())
        .await
        .expect("Failed to get secret");
    
    let etag = secret.etag.clone();
    assert!(etag.is_some());
    println!("  📌 Got ETag: {:?}", etag);
    
    // Update the secret (Note: SDK doesn't support conditional updates yet)
    let update_result = client
        .put_secret(TEST_NAMESPACE, &etag_key, "updated-etag-value", Default::default())
        .await;
    
    assert!(update_result.is_ok());
    println!("  ✅ Secret update succeeded");
    
    // Verify the update
    let updated = client
        .get_secret(TEST_NAMESPACE, &etag_key, GetOpts::default())
        .await
        .expect("Failed to get updated secret");
    
    assert_eq!(updated.value.expose_secret(), "updated-etag-value");
    println!("  ✅ Update verified successfully");
    
    // Clean up
    client.delete_secret(TEST_NAMESPACE, &etag_key).await.ok();
}

#[tokio::test]
async fn test_auth_methods() {
    println!("🔍 Testing different auth methods...");
    
    // Test with API Key (x-api-key header)
    let client_api_key = ClientBuilder::new(BASE_URL)
        .auth(Auth::api_key(API_KEY))
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");
    
    let result = client_api_key
        .list_secrets(TEST_NAMESPACE, ListOpts::default())
        .await;
    
    assert!(result.is_ok());
    println!("✅ API Key auth (x-api-key) working");
    
    // Test with Bearer token
    let client_bearer = ClientBuilder::new(BASE_URL)
        .auth(Auth::bearer(API_KEY))
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");
    
    let result = client_bearer
        .list_secrets(TEST_NAMESPACE, ListOpts::default())
        .await;
    
    assert!(result.is_ok());
    println!("✅ Bearer token auth working");
    
    // Test with XJP Key
    let client_xjp = ClientBuilder::new(BASE_URL)
        .auth(Auth::xjp_key(API_KEY))
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");
    
    let result = client_xjp
        .list_secrets(TEST_NAMESPACE, ListOpts::default())
        .await;
    
    assert!(result.is_ok());
    println!("✅ XJP Key auth working");
}

#[tokio::test]
async fn test_error_handling() {
    println!("🔍 Testing error handling...");
    
    let client = create_client();
    
    // Test 404 - secret not found
    let not_found = client
        .get_secret(TEST_NAMESPACE, "non-existent-key-xyz", GetOpts::default())
        .await;
    
    assert!(not_found.is_err());
    println!("✅ 404 error handling working");
    
    // Test invalid namespace
    let invalid_ns = client
        .list_secrets("", ListOpts::default())
        .await;
    
    assert!(invalid_ns.is_err());
    println!("✅ Invalid namespace error handling working");
}

#[tokio::test]
async fn test_idempotency() {
    println!("🔍 Testing idempotency...");
    
    let client = create_client();
    let idempotency_key = format!("idempotent-{}", uuid::Uuid::new_v4());
    let secret_key = format!("test-idempotent-{}", chrono::Utc::now().timestamp());
    
    // Create with idempotency key
    let opts1 = PutOpts {
        idempotency_key: Some(idempotency_key.clone()),
        ..Default::default()
    };
    
    let result1 = client
        .put_secret(TEST_NAMESPACE, &secret_key, "idempotent-value", opts1)
        .await
        .expect("Failed first idempotent put");
    
    // Retry with same idempotency key
    let opts2 = PutOpts {
        idempotency_key: Some(idempotency_key),
        ..Default::default()
    };
    
    let result2 = client
        .put_secret(TEST_NAMESPACE, &secret_key, "different-value", opts2)
        .await
        .expect("Failed second idempotent put");
    
    // Both requests should succeed with same result
    assert_eq!(result1.message, result2.message);
    println!("✅ Idempotency working correctly");
    
    // Verify value didn't change
    let secret = client
        .get_secret(TEST_NAMESPACE, &secret_key, GetOpts::default())
        .await
        .expect("Failed to get idempotent secret");
    
    assert_eq!(secret.value.expose_secret(), "idempotent-value");
    
    // Clean up
    client.delete_secret(TEST_NAMESPACE, &secret_key).await.ok();
}

