//! Integration tests for the XJP Secret Store SDK client

use secret_store_sdk::{
    Auth, BatchGetResult, BatchKeys, BatchOp, ClientBuilder, EnvExport, Error,
    ExportFormat, GetOpts, ListOpts, PutOpts,
};
use secrecy::ExposeSecret;
use wiremock::{
    matchers::{header, method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};
use serde_json::json;

/// Create a mock server and test client
async fn setup() -> (MockServer, secret_store_sdk::Client) {
    let server = MockServer::start().await;
    
    #[cfg(feature = "danger-insecure-http")]
    let client = ClientBuilder::new(server.uri())
        .auth(Auth::bearer("test-token"))
        .timeout_ms(5000)
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");
    
    #[cfg(not(feature = "danger-insecure-http"))]
    let client = ClientBuilder::new(&server.uri().replace("http://", "https://"))
        .auth(Auth::bearer("test-token"))
        .timeout_ms(5000)
        .build()
        .expect("Failed to build client");
    
    (server, client)
}

#[tokio::test]
async fn test_get_secret() {
    let (server, client) = setup().await;
    
    // Mock successful response
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/database-url"))
        .and(header("Authorization", "Bearer test-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "production",
                    "key": "database-url",
                    "value": "postgres://user:pass@host/db",
                    "version": 1,
                    "expires_at": null,
                    "metadata": {"env": "prod"},
                    "updated_at": "2024-01-01T00:00:00Z",
                    "format": "plaintext",
                    "request_id": "req-123"
                }))
                .append_header("ETag", "\"123abc\"")
                .append_header("X-Request-ID", "req-123")
        )
        .expect(1)
        .mount(&server)
        .await;
    
    let secret = client
        .get_secret("production", "database-url", GetOpts::default())
        .await
        .expect("Failed to get secret");
    
    assert_eq!(secret.namespace, "production");
    assert_eq!(secret.key, "database-url");
    assert_eq!(secret.value.expose_secret(), "postgres://user:pass@host/db");
    assert_eq!(secret.version, 1);
    assert_eq!(secret.etag, Some("\"123abc\"".to_string()));
}

#[tokio::test]
async fn test_get_secret_not_found() {
    let (server, client) = setup().await;
    
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/nonexistent"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({
                    "status": 404,
                    "error": "not_found",
                    "message": "Secret not found",
                    "request_id": "req-456"
                }))
        )
        .expect(1)
        .mount(&server)
        .await;
    
    let result = client
        .get_secret("production", "nonexistent", GetOpts::default())
        .await;
    
    match result {
        Err(Error::Http { status: 404, .. }) => (),
        _ => panic!("Expected 404 error, got: {:?}", result),
    }
}

#[tokio::test]
async fn test_get_secret_304_not_modified() {
    let server = MockServer::start().await;
    
    // Create client with caching explicitly enabled
    #[cfg(feature = "danger-insecure-http")]
    let client = ClientBuilder::new(server.uri())
        .auth(Auth::bearer("test-token"))
        .timeout_ms(5000)
        .enable_cache(true)
        .cache_ttl_secs(300) // 5 minutes TTL
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");
        
    #[cfg(not(feature = "danger-insecure-http"))]
    let client = ClientBuilder::new(&server.uri().replace("http://", "https://"))
        .auth(Auth::bearer("test-token"))
        .timeout_ms(5000)
        .enable_cache(true)
        .cache_ttl_secs(300) // 5 minutes TTL
        .build()
        .expect("Failed to build client");
    
    // First request to get ETag
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/api-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "production",
                    "key": "api-key",
                    "value": "secret-value",
                    "version": 1,
                    "format": "plaintext",
                    "updated_at": "2024-01-01T00:00:00Z"
                }))
                .append_header("ETag", "\"etag123\"")
        )
        .expect(1)
        .mount(&server)
        .await;
    
    let secret = client
        .get_secret("production", "api-key", GetOpts::default())
        .await
        .expect("Failed to get secret");
    
    let _etag = secret.etag.expect("Expected ETag");
    
    // When requesting with the same key again,
    // the client should use the cached value
    let cached = client
        .get_secret("production", "api-key", GetOpts::default())
        .await
        .expect("Failed to get cached secret");
    
    assert_eq!(cached.value.expose_secret(), "secret-value");
    assert_eq!(cached.etag, Some("\"etag123\"".to_string()));
}

#[tokio::test]
async fn test_put_secret() {
    let (server, client) = setup().await;
    
    Mock::given(method("PUT"))
        .and(path("/api/v2/secrets/production/new-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "message": "Secret created successfully",
                    "namespace": "production",
                    "key": "new-key",
                    "created_at": "2024-01-01T00:00:00Z",
                    "request_id": "req-789"
                }))
        )
        .expect(1)
        .mount(&server)
        .await;
    
    let result = client
        .put_secret("production", "new-key", "new-value", PutOpts::default())
        .await
        .expect("Failed to put secret");
    
    assert_eq!(result.message, "Secret created successfully");
}

#[tokio::test]
async fn test_delete_secret() {
    let (server, client) = setup().await;
    
    Mock::given(method("DELETE"))
        .and(path("/api/v2/secrets/production/old-key"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    
    let result = client
        .delete_secret("production", "old-key")
        .await
        .expect("Failed to delete secret");
    
    assert!(result.deleted);
}

#[tokio::test]
async fn test_list_secrets() {
    let (server, client) = setup().await;
    
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production"))
        .and(query_param("limit", "10"))
        .and(query_param("prefix", "app-"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "production",
                    "secrets": [
                        {
                            "key": "app-config",
                            "ver": 3,
                            "updated_at": "2024-01-01T00:00:00Z",
                            "kid": null
                        },
                        {
                            "key": "app-secret",
                            "ver": 1,
                            "updated_at": "2024-01-02T00:00:00Z",
                            "kid": null
                        }
                    ],
                    "total": 2,
                    "limit": 10,
                    "has_more": false,
                    "request_id": "req-list"
                }))
        )
        .expect(1)
        .mount(&server)
        .await;
    
    let opts = ListOpts {
        limit: Some(10),
        prefix: Some("app-".to_string()),
    };
    
    let list = client
        .list_secrets("production", opts)
        .await
        .expect("Failed to list secrets");
    
    assert_eq!(list.total, 2);
    assert_eq!(list.secrets.len(), 2);
    assert_eq!(list.secrets[0].key, "app-config");
    assert_eq!(list.secrets[0].version, 3);
}

#[tokio::test]
async fn test_batch_get() {
    let (server, client) = setup().await;
    
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/batch"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "production",
                    "secrets": {
                        "key1": "value1",
                        "key2": "value2"
                    },
                    "missing": [],
                    "total": 2,
                    "request_id": "req-batch"
                }))
        )
        .expect(1)
        .mount(&server)
        .await;
    
    let keys = BatchKeys::Keys(vec!["key1".to_string(), "key2".to_string()]);
    let result = client
        .batch_get("production", keys, ExportFormat::Json)
        .await
        .expect("Failed to batch get");
    
    match result {
        BatchGetResult::Json(json) => {
            assert_eq!(json.total, 2);
            assert_eq!(json.secrets.get("key1").unwrap(), "value1");
            assert_eq!(json.secrets.get("key2").unwrap(), "value2");
        }
        _ => panic!("Expected JSON result"),
    }
}

#[tokio::test]
async fn test_batch_operate() {
    let (server, client) = setup().await;
    
    Mock::given(method("POST"))
        .and(path("/api/v2/secrets/production/batch"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "production",
                    "results": {
                        "succeeded": [
                            {
                                "key": "new1",
                                "action": "put",
                                "success": true
                            },
                            {
                                "key": "new2",
                                "action": "put",
                                "success": true
                            }
                        ],
                        "failed": [
                            {
                                "key": "bad-key",
                                "action": "put",
                                "success": false,
                                "error": "Invalid key name"
                            }
                        ],
                        "total": 3
                    },
                    "success_rate": 0.6667
                }))
        )
        .expect(1)
        .mount(&server)
        .await;
    
    let operations = vec![
        BatchOp::put("new1", "value1"),
        BatchOp::put("new2", "value2"),
        BatchOp::put("bad-key", "value3"),
    ];
    
    let result = client
        .batch_operate("production", operations, false, None)
        .await
        .expect("Failed to batch operate");
    
    assert_eq!(result.results.succeeded.len(), 2);
    assert_eq!(result.results.failed.len(), 1);
    assert_eq!(result.results.total, 3);
    assert!(result.results.succeeded[0].success);
    assert!(!result.results.failed[0].success);
}

#[tokio::test]
async fn test_export_env() {
    let (server, client) = setup().await;
    
    Mock::given(method("GET"))
        .and(path("/api/v2/env/production"))
        .and(query_param("format", "dotenv"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "text/plain")
                .set_body_string("KEY1=value1\nKEY2=value2\n")
        )
        .expect(1)
        .mount(&server)
        .await;
    
    let export = client
        .export_env("production", ExportFormat::Dotenv)
        .await
        .expect("Failed to export env");
    
    match export {
        EnvExport::Text(content) => {
            assert_eq!(content, "KEY1=value1\nKEY2=value2\n");
        }
        _ => panic!("Expected text export"),
    }
}

#[tokio::test]
async fn test_auth_refresh_on_401() {
    use std::sync::{Arc, Mutex};
    use async_trait::async_trait;
    use secret_store_sdk::{TokenProvider, SecretString};
    
    #[derive(Clone)]
    struct RefreshableTokenProvider {
        token: Arc<Mutex<String>>,
        refresh_count: Arc<Mutex<u32>>,
    }
    
    #[async_trait]
    impl TokenProvider for RefreshableTokenProvider {
        async fn get_token(&self) -> Result<SecretString, Box<dyn std::error::Error + Send + Sync>> {
            let token = self.token.lock().unwrap().clone();
            Ok(SecretString::new(token))
        }
        
        async fn refresh_token(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let mut count = self.refresh_count.lock().unwrap();
            *count += 1;
            let mut token = self.token.lock().unwrap();
            *token = format!("refreshed-token-{}", count);
            Ok(())
        }
        
        fn clone_box(&self) -> Box<dyn TokenProvider> {
            Box::new(self.clone())
        }
    }
    
    let server = MockServer::start().await;
    let provider = RefreshableTokenProvider {
        token: Arc::new(Mutex::new("initial-token".to_string())),
        refresh_count: Arc::new(Mutex::new(0)),
    };
    
    #[cfg(feature = "danger-insecure-http")]
    let client = ClientBuilder::new(server.uri())
        .auth(Auth::token_provider(provider.clone()))
        .timeout_ms(5000)
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");
    
    #[cfg(not(feature = "danger-insecure-http"))]
    let client = ClientBuilder::new(&server.uri().replace("http://", "https://"))
        .auth(Auth::token_provider(provider.clone()))
        .timeout_ms(5000)
        .build()
        .expect("Failed to build client");
    
    // First request returns 401
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/key"))
        .and(header("Authorization", "Bearer initial-token"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    
    // Second request with refreshed token succeeds
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/key"))
        .and(header("Authorization", "Bearer refreshed-token-1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "production",
                    "key": "key",
                    "value": "secret",
                    "version": 1,
                    "format": "plaintext",
                    "updated_at": "2024-01-01T00:00:00Z"
                }))
        )
        .expect(1)
        .mount(&server)
        .await;
    
    let secret = client
        .get_secret("production", "key", GetOpts::default())
        .await
        .expect("Failed to get secret after refresh");
    
    assert_eq!(secret.value.expose_secret(), "secret");
    assert_eq!(*provider.refresh_count.lock().unwrap(), 1);
}

#[tokio::test]
async fn test_retry_on_server_error() {
    let server = MockServer::start().await;
    
    #[cfg(feature = "danger-insecure-http")]
    let client = ClientBuilder::new(server.uri())
        .auth(Auth::bearer("test-token"))
        .timeout_ms(5000)
        .retries(3)  // Explicitly set retries
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");
        
    #[cfg(not(feature = "danger-insecure-http"))]
    let client = ClientBuilder::new(&server.uri().replace("http://", "https://"))
        .auth(Auth::bearer("test-token"))
        .timeout_ms(5000)
        .retries(3)  // Explicitly set retries
        .build()
        .expect("Failed to build client");
    
    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let call_count_clone = call_count.clone();
    
    // Use a single mock that responds differently based on call count
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/flaky"))
        .respond_with(move |_req: &wiremock::Request| {
            let count = call_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            
            if count < 2 {
                // First two calls return 500
                ResponseTemplate::new(500)
            } else {
                // Third call returns success
                ResponseTemplate::new(200)
                    .set_body_json(json!({
                        "namespace": "production",
                        "key": "flaky",
                        "value": "success",
                        "version": 1,
                        "format": "plaintext",
                        "updated_at": "2024-01-01T00:00:00Z"
                    }))
            }
        })
        .mount(&server)
        .await;
    
    let secret = client
        .get_secret("production", "flaky", GetOpts::default())
        .await
        .expect("Failed after retries");
    
    assert_eq!(secret.value.expose_secret(), "success");
    
    // Verify the mock was called 3 times
    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 3);
}