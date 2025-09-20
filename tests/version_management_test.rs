//! Integration tests for version management functionality

use secret_store_sdk::{Auth, ClientBuilder};
use secrecy::ExposeSecret;
use wiremock::{matchers::{method, path}, Mock, MockServer, ResponseTemplate};
use serde_json::json;

#[tokio::test]
async fn test_list_versions() {
    let server = MockServer::start().await;
    
    #[cfg(feature = "danger-insecure-http")]
    let client = ClientBuilder::new(server.uri())
        .auth(Auth::bearer("test-token"))
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");
        
    #[cfg(not(feature = "danger-insecure-http"))]
    let client = ClientBuilder::new(&server.uri().replace("http://", "https://"))
        .auth(Auth::bearer("test-token"))
        .build()
        .expect("Failed to build client");
    
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/versioned-key/versions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "total": 3,
                    "namespace": "production",
                    "key": "versioned-key",
                    "request_id": "req-list-versions",
                    "versions": [
                        {
                            "version": 3,
                            "created_at": "2024-01-03T00:00:00Z",
                            "created_by": "user3",
                            "is_current": true,
                            "comment": "update"
                        },
                        {
                            "version": 2,
                            "created_at": "2024-01-02T00:00:00Z",
                            "created_by": "user2",
                            "is_current": false,
                            "comment": "rotation"
                        },
                        {
                            "version": 1,
                            "created_at": "2024-01-01T00:00:00Z",
                            "created_by": "user1",
                            "is_current": false,
                            "comment": null
                        }
                    ]
                }))
        )
        .expect(1)
        .mount(&server)
        .await;
    
    let versions = client
        .list_versions("production", "versioned-key")
        .await
        .expect("Failed to list versions");
    
    assert_eq!(versions.total, 3);
    assert_eq!(versions.namespace, "production");
    assert_eq!(versions.key, "versioned-key");
    assert_eq!(versions.versions.len(), 3);
    
    // Check current version
    let current = versions.versions.iter().find(|v| v.is_current).unwrap();
    assert_eq!(current.version, 3);
    assert_eq!(current.created_by, "user3");
    
    // Check ordering (newest first)
    assert_eq!(versions.versions[0].version, 3);
    assert_eq!(versions.versions[1].version, 2);
    assert_eq!(versions.versions[2].version, 1);
}

#[tokio::test]
async fn test_get_specific_version() {
    let server = MockServer::start().await;
    
    #[cfg(feature = "danger-insecure-http")]
    let client = ClientBuilder::new(server.uri())
        .auth(Auth::bearer("test-token"))
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");
        
    #[cfg(not(feature = "danger-insecure-http"))]
    let client = ClientBuilder::new(&server.uri().replace("http://", "https://"))
        .auth(Auth::bearer("test-token"))
        .build()
        .expect("Failed to build client");
    
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/multi-version/versions/2"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "production",
                    "key": "multi-version",
                    "value": "version-2-value",
                    "version": 2,
                    "format": "plaintext",
                    "expires_at": null,
                    "metadata": {"reason": "rotation"},
                    "updated_at": "2024-01-02T00:00:00Z"
                }))
        )
        .expect(1)
        .mount(&server)
        .await;
    
    let secret = client
        .get_version("production", "multi-version", 2)
        .await
        .expect("Failed to get specific version");
    
    assert_eq!(secret.namespace, "production");
    assert_eq!(secret.key, "multi-version");
    assert_eq!(secret.value.expose_secret(), "version-2-value");
    assert_eq!(secret.version, 2);
    assert_eq!(
        secret.metadata.get("reason").unwrap().as_str().unwrap(),
        "rotation"
    );
}

#[tokio::test]
async fn test_rollback_version() {
    let server = MockServer::start().await;
    
    #[cfg(feature = "danger-insecure-http")]
    let client = ClientBuilder::new(server.uri())
        .auth(Auth::bearer("test-token"))
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");
        
    #[cfg(not(feature = "danger-insecure-http"))]
    let client = ClientBuilder::new(&server.uri().replace("http://", "https://"))
        .auth(Auth::bearer("test-token"))
        .build()
        .expect("Failed to build client");
    
    Mock::given(method("POST"))
        .and(path("/api/v2/secrets/production/rollback-key/rollback/2"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "message": "Rollback successful",
                    "namespace": "production",
                    "key": "rollback-key",
                    "from_version": 3,
                    "to_version": 2,
                    "request_id": "req-rollback"
                }))
        )
        .expect(1)
        .mount(&server)
        .await;
    
    let result = client
        .rollback("production", "rollback-key", 2)
        .await
        .expect("Failed to rollback version");
    
    assert_eq!(result.from_version, 3);
    assert_eq!(result.to_version, 2);
}

#[tokio::test]
async fn test_version_not_found() {
    let server = MockServer::start().await;
    
    #[cfg(feature = "danger-insecure-http")]
    let client = ClientBuilder::new(server.uri())
        .auth(Auth::bearer("test-token"))
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");
        
    #[cfg(not(feature = "danger-insecure-http"))]
    let client = ClientBuilder::new(&server.uri().replace("http://", "https://"))
        .auth(Auth::bearer("test-token"))
        .build()
        .expect("Failed to build client");
    
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/missing-key/versions/99"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({
                    "status": 404,
                    "error": "not_found",
                    "message": "Version not found"
                }))
        )
        .expect(1)
        .mount(&server)
        .await;
    
    let result = client.get_version("production", "missing-key", 99).await;
    
    match result {
        Err(secret_store_sdk::Error::Http { status: 404, .. }) => (),
        _ => panic!("Expected 404 error, got: {:?}", result),
    }
}

#[tokio::test]
async fn test_rollback_caching_behavior() {
    let server = MockServer::start().await;
    
    #[cfg(feature = "danger-insecure-http")]
    let client = ClientBuilder::new(server.uri())
        .auth(Auth::bearer("test-token"))
        .enable_cache(true)
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");
        
    #[cfg(not(feature = "danger-insecure-http"))]
    let client = ClientBuilder::new(&server.uri().replace("http://", "https://"))
        .auth(Auth::bearer("test-token"))
        .enable_cache(true)
        .build()
        .expect("Failed to build client");
    
    // First, get the current version (v3) and cache it
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/cached-rollback"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "production",
                    "key": "cached-rollback",
                    "value": "current-value-v3",
                    "version": 3,
                    "format": "plaintext",
                    "metadata": null,
                    "updated_at": "2024-01-01T00:00:00Z"
                }))
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    
    let secret = client
        .get_secret("production", "cached-rollback", Default::default())
        .await
        .expect("Failed to get secret");
    
    assert_eq!(secret.version, 3);
    assert_eq!(secret.value.expose_secret(), "current-value-v3");
    
    // Perform rollback to v1
    Mock::given(method("POST"))
        .and(path("/api/v2/secrets/production/cached-rollback/rollback/1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "message": "Rollback successful",
                    "namespace": "production",
                    "key": "cached-rollback",
                    "from_version": 3,
                    "to_version": 1,
                    "request_id": "req-rollback-cached"
                }))
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    
    client
        .rollback("production", "cached-rollback", 1)
        .await
        .expect("Failed to rollback");
    
    // After rollback, cache should be invalidated
    // Next get should fetch from server
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/cached-rollback"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "namespace": "production",
                    "key": "cached-rollback",
                    "value": "rolled-back-value-v1",
                    "version": 4,
                    "format": "plaintext",
                    "metadata": null,
                    "updated_at": "2024-01-01T00:00:00Z"
                }))
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    
    let updated = client
        .get_secret("production", "cached-rollback", Default::default())
        .await
        .expect("Failed to get secret after rollback");
    
    assert_eq!(updated.version, 4);
    assert_eq!(updated.value.expose_secret(), "rolled-back-value-v1");
}

#[tokio::test]
async fn test_version_history_pagination() {
    // This test simulates a secret with many versions
    let server = MockServer::start().await;
    
    #[cfg(feature = "danger-insecure-http")]
    let client = ClientBuilder::new(server.uri())
        .auth(Auth::bearer("test-token"))
        .allow_insecure_http()
        .build()
        .expect("Failed to build client");
        
    #[cfg(not(feature = "danger-insecure-http"))]
    let client = ClientBuilder::new(&server.uri().replace("http://", "https://"))
        .auth(Auth::bearer("test-token"))
        .build()
        .expect("Failed to build client");
    
    // Create a list of 100 versions (in practice, the API might paginate)
    let mut versions = Vec::new();
    for i in (1..=100).rev() {
        versions.push(json!({
            "version": i,
            "created_at": format!("2024-01-{:02}T00:00:00Z", (i % 31) + 1),
            "created_by": format!("user{}", i % 10),
            "is_current": i == 100,
            "comment": if i % 5 == 0 { 
                Some("scheduled rotation")
            } else { 
                None 
            }
        }));
    }
    
    Mock::given(method("GET"))
        .and(path("/api/v2/secrets/production/many-versions/versions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "total": 100,
                    "namespace": "production",
                    "key": "many-versions",
                    "request_id": "req-list-many",
                    "versions": versions
                }))
        )
        .expect(1)
        .mount(&server)
        .await;
    
    let version_list = client
        .list_versions("production", "many-versions")
        .await
        .expect("Failed to list many versions");
    
    assert_eq!(version_list.total, 100);
    assert_eq!(version_list.versions.len(), 100);
    
    // Verify newest version is current
    assert!(version_list.versions[0].is_current);
    assert_eq!(version_list.versions[0].version, 100);
    
    // Count versions with comment
    let with_comment = version_list
        .versions
        .iter()
        .filter(|v| v.comment.is_some())
        .count();
    
    assert_eq!(with_comment, 20); // Versions divisible by 5
}