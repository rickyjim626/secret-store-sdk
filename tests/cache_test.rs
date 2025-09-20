//! Integration tests for caching functionality

use secret_store_sdk::{Auth, ClientBuilder, GetOpts};
use secrecy::ExposeSecret;
use wiremock::{matchers::{method, path}, Mock, MockServer, ResponseTemplate};
use serde_json::json;
use std::time::Duration;

/// Helper to create a test client with the given cache configuration
async fn create_test_client(server: &MockServer, enable_cache: bool, cache_ttl_secs: u64) -> secret_store_sdk::Client {
    #[cfg(feature = "danger-insecure-http")]
    let client = ClientBuilder::new(server.uri())
        .auth(Auth::bearer("test-token"))
        .enable_cache(enable_cache)
        .cache_ttl_secs(cache_ttl_secs)
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");
        
    #[cfg(not(feature = "danger-insecure-http"))]
    let client = ClientBuilder::new(&server.uri().replace("http://", "https://"))
        .auth(Auth::bearer("test-token"))
        .enable_cache(enable_cache)
        .cache_ttl_secs(cache_ttl_secs)
        .build()
        .expect("Failed to build client");
        
    client
}

#[tokio::test]
async fn test_cache_hit() {
    let server = MockServer::start().await;
    let client = create_test_client(&server, true, 10).await;
    
    // Mock should only be called once due to caching
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/cached-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "production",
                    "key": "cached-key",
                    "value": "cached-value",
                    "version": 1,
                    "format": "plaintext",
                    "updated_at": "2024-01-01T00:00:00Z"
                }))
                .append_header("ETag", "\"12345\"")
        )
        .expect(1)  // Should only be called once
        .mount(&server)
        .await;
    
    // First request - should hit the server
    let secret1 = client
        .get_secret("production", "cached-key", GetOpts::default())
        .await
        .expect("Failed to get secret");
    
    assert_eq!(secret1.value.expose_secret(), "cached-value");
    
    // Second request - should be served from cache
    let secret2 = client
        .get_secret("production", "cached-key", GetOpts::default())
        .await
        .expect("Failed to get cached secret");
    
    assert_eq!(secret2.value.expose_secret(), "cached-value");
    
    // Verify cache statistics
    let stats = client.cache_stats();
    assert_eq!(stats.hits(), 1);
    assert_eq!(stats.misses(), 1);
}

#[tokio::test]
async fn test_cache_disabled() {
    let server = MockServer::start().await;
    let client = create_test_client(&server, false, 0).await;
    
    // Mock should be called twice when cache is disabled
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/no-cache-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "production",
                    "key": "no-cache-key",
                    "value": "no-cache-value",
                    "version": 1,
                    "format": "plaintext",
                    "updated_at": "2024-01-01T00:00:00Z"
                }))
        )
        .expect(2)  // Should be called twice
        .mount(&server)
        .await;
    
    // Both requests should hit the server
    let secret1 = client
        .get_secret("production", "no-cache-key", GetOpts::default())
        .await
        .expect("Failed to get secret");
    
    let secret2 = client
        .get_secret("production", "no-cache-key", GetOpts::default())
        .await
        .expect("Failed to get secret");
    
    assert_eq!(secret1.value.expose_secret(), "no-cache-value");
    assert_eq!(secret2.value.expose_secret(), "no-cache-value");
}

#[tokio::test]
async fn test_cache_bypass() {
    let server = MockServer::start().await;
    let client = create_test_client(&server, true, 60).await;
    
    // First request to populate cache
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/bypass-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "production",
                    "key": "bypass-key",
                    "value": "original-value",
                    "version": 1,
                    "format": "plaintext",
                    "updated_at": "2024-01-01T00:00:00Z"
                }))
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    
    let secret1 = client
        .get_secret("production", "bypass-key", GetOpts::default())
        .await
        .expect("Failed to get secret");
    
    assert_eq!(secret1.value.expose_secret(), "original-value");
    
    // Second request with cache bypass
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/bypass-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "production",
                    "key": "bypass-key",
                    "value": "updated-value",
                    "version": 2,
                    "format": "plaintext",
                    "updated_at": "2024-01-01T00:01:00Z"
                }))
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    
    let opts = GetOpts {
        use_cache: false,
        ..Default::default()
    };
    
    let secret2 = client
        .get_secret("production", "bypass-key", opts)
        .await
        .expect("Failed to get secret with bypass");
    
    assert_eq!(secret2.value.expose_secret(), "updated-value");
}

#[tokio::test]
async fn test_cache_invalidation() {
    let server = MockServer::start().await;
    let client = create_test_client(&server, true, 60).await;
    
    // Initial request
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/invalidate-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "production",
                    "key": "invalidate-key",
                    "value": "original-value",
                    "version": 1,
                    "format": "plaintext",
                    "updated_at": "2024-01-01T00:00:00Z"
                }))
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    
    let secret1 = client
        .get_secret("production", "invalidate-key", GetOpts::default())
        .await
        .expect("Failed to get secret");
    
    assert_eq!(secret1.value.expose_secret(), "original-value");
    
    // Update the secret (should invalidate cache)
    Mock::given(method("PUT"))
        .and(path("/api/v2/secrets/production/invalidate-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "message": "Secret updated",
                    "namespace": "production",
                    "key": "invalidate-key",
                    "created_at": "2024-01-01T00:01:00Z",
                    "request_id": "req-123",
                    "version": 2
                }))
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    
    let _ = client
        .put_secret("production", "invalidate-key", "new-value", Default::default())
        .await
        .expect("Failed to put secret");
    
    // Next GET should fetch from server (cache invalidated)
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/invalidate-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "production",
                    "key": "invalidate-key",
                    "value": "new-value",
                    "version": 2,
                    "format": "plaintext",
                    "updated_at": "2024-01-01T00:01:00Z"
                }))
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    
    let secret2 = client
        .get_secret("production", "invalidate-key", GetOpts::default())
        .await
        .expect("Failed to get secret after invalidation");
    
    assert_eq!(secret2.value.expose_secret(), "new-value");
}

#[tokio::test]
async fn test_cache_clear() {
    let server = MockServer::start().await;
    let client = create_test_client(&server, true, 60).await;
    
    // Populate cache with multiple entries
    for i in 0..3 {
        Mock::given(method("GET"))
            .and(path(format!("/api/v2/secrets/production/clear-key-{}", i)))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({
                        "namespace": "production",
                        "key": format!("clear-key-{}", i),
                        "value": format!("value-{}", i),
                        "version": 1,
                        "format": "plaintext",
                        "updated_at": "2024-01-01T00:00:00Z"
                    }))
            )
            .expect(2)  // Will be called again after clear
            .mount(&server)
            .await;
        
        let _ = client
            .get_secret("production", &format!("clear-key-{}", i), GetOpts::default())
            .await
            .expect("Failed to get secret");
    }
    
    // Clear cache
    client.clear_cache();
    
    // All subsequent requests should hit the server
    for i in 0..3 {
        let secret = client
            .get_secret("production", &format!("clear-key-{}", i), GetOpts::default())
            .await
            .expect("Failed to get secret after clear");
        
        assert_eq!(secret.value.expose_secret(), &format!("value-{}", i));
    }
}

#[tokio::test]
async fn test_etag_cache_validation() {
    let server = MockServer::start().await;
    let client = create_test_client(&server, true, 60).await;
    
    // Initial request with ETag
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/etag-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "production",
                    "key": "etag-key",
                    "value": "etag-value",
                    "version": 1,
                    "format": "plaintext",
                    "updated_at": "2024-01-01T00:00:00Z"
                }))
                .append_header("ETag", "\"abc123\"")
                .append_header("Last-Modified", "Wed, 01 Jan 2024 00:00:00 GMT")
        )
        .expect(1)
        .mount(&server)
        .await;
    
    let secret = client
        .get_secret("production", "etag-key", GetOpts::default())
        .await
        .expect("Failed to get secret");
    
    assert_eq!(secret.value.expose_secret(), "etag-value");
    assert_eq!(secret.etag.as_ref().unwrap(), "\"abc123\"");
    
    // Subsequent request should use cache
    let cached = client
        .get_secret("production", "etag-key", GetOpts::default())
        .await
        .expect("Failed to get cached secret");
    
    assert_eq!(cached.value.expose_secret(), "etag-value");
}

#[tokio::test]
async fn test_cache_ttl_expiration() {
    let server = MockServer::start().await;
    let client = create_test_client(&server, true, 1).await;  // 1 second TTL
    
    // First request
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/ttl-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "production",
                    "key": "ttl-key",
                    "value": "ttl-value",
                    "version": 1,
                    "format": "plaintext",
                    "updated_at": "2024-01-01T00:00:00Z"
                }))
        )
        .expect(2)  // Should be called twice due to TTL expiration
        .mount(&server)
        .await;
    
    // First request - populates cache
    let secret1 = client
        .get_secret("production", "ttl-key", GetOpts::default())
        .await
        .expect("Failed to get secret");
    
    assert_eq!(secret1.value.expose_secret(), "ttl-value");
    
    // Wait for TTL to expire
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Second request - should hit server again
    let secret2 = client
        .get_secret("production", "ttl-key", GetOpts::default())
        .await
        .expect("Failed to get secret after TTL");
    
    assert_eq!(secret2.value.expose_secret(), "ttl-value");
}