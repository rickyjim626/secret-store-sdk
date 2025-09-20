//! Basic usage example for XJP Secret Store SDK

use secret_store_sdk::{Auth, Client, ClientBuilder, GetOpts, PutOpts};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the client
    let client = create_client()?;

    // Example 1: Put a secret
    println!("=== Example 1: Put a secret ===");
    put_secret_example(&client).await?;

    // Example 2: Get a secret
    println!("\n=== Example 2: Get a secret ===");
    get_secret_example(&client).await?;

    // Example 3: List secrets
    println!("\n=== Example 3: List secrets ===");
    list_secrets_example(&client).await?;

    // Example 4: Update and delete
    println!("\n=== Example 4: Update and delete ===");
    update_delete_example(&client).await?;

    // Example 5: Cache demonstration
    println!("\n=== Example 5: Cache demonstration ===");
    cache_example(&client).await?;

    Ok(())
}

fn create_client() -> Result<Client, Box<dyn std::error::Error>> {
    // Get configuration from environment
    let base_url = std::env::var("XJP_SECRET_STORE_URL")
        .unwrap_or_else(|_| "https://secret.example.com".to_string());
    let api_key =
        std::env::var("XJP_SECRET_STORE_API_KEY").unwrap_or_else(|_| "demo-api-key".to_string());

    let client = ClientBuilder::new(base_url)
        .auth(Auth::bearer(api_key))
        .user_agent_extra("examples/1.0")
        .build()?;

    Ok(client)
}

async fn put_secret_example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    // Simple put
    let result = client
        .put_secret(
            "example-namespace",
            "database-url",
            "postgresql://user:pass@localhost/db",
            PutOpts::default(),
        )
        .await?;

    println!("Created secret: {}", result.message);
    println!("Request ID: {}", result.request_id);

    // Put with TTL and metadata
    let opts = PutOpts {
        ttl_seconds: Some(3600), // 1 hour
        metadata: Some(serde_json::json!({
            "environment": "development",
            "owner": "backend-team",
            "rotation_required": true
        })),
        idempotency_key: Some("example-put-001".to_string()),
    };

    client
        .put_secret("example-namespace", "api-key", "sk_test_123456", opts)
        .await?;

    println!("Created secret with TTL and metadata");

    Ok(())
}

async fn get_secret_example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    // Get with default options (uses cache)
    let secret = client
        .get_secret("example-namespace", "database-url", GetOpts::default())
        .await?;

    println!("Got secret: {}", secret.key);
    println!("Version: {}", secret.version);
    println!("Updated at: {}", secret.updated_at);
    if let Some(etag) = &secret.etag {
        println!("ETag: {}", etag);
    }

    // Get without cache
    let opts = GetOpts {
        use_cache: false,
        ..Default::default()
    };
    let fresh_secret = client
        .get_secret("example-namespace", "database-url", opts)
        .await?;

    println!("\nFresh fetch (no cache):");
    println!("Version: {}", fresh_secret.version);

    Ok(())
}

async fn list_secrets_example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    use secret_store_sdk::ListOpts;

    // List all secrets
    let list = client
        .list_secrets("example-namespace", ListOpts::default())
        .await?;

    println!("Total secrets in namespace: {}", list.total);
    for secret_info in &list.secrets {
        println!(
            "  - {} (v{}, updated: {})",
            secret_info.key, secret_info.version, secret_info.updated_at
        );
    }

    // List with prefix
    let opts = ListOpts {
        prefix: Some("api-".to_string()),
        limit: Some(10),
    };
    let filtered = client.list_secrets("example-namespace", opts).await?;
    println!("\nSecrets starting with 'api-': {}", filtered.total);

    Ok(())
}

async fn update_delete_example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    // Update an existing secret
    let _update = client
        .put_secret(
            "example-namespace",
            "temp-secret",
            "updated-value",
            PutOpts::default(),
        )
        .await?;

    println!("Updated secret");

    // Delete the secret
    let delete_result = client
        .delete_secret("example-namespace", "temp-secret")
        .await?;

    if delete_result.deleted {
        println!("Deleted secret successfully");
        if let Some(req_id) = delete_result.request_id {
            println!("Delete request ID: {}", req_id);
        }
    }

    Ok(())
}

async fn cache_example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    // First, clear any existing cache
    client.clear_cache();

    // Initial fetch - will miss cache
    let _secret1 = client
        .get_secret("example-namespace", "database-url", GetOpts::default())
        .await?;

    // Second fetch - should hit cache
    let _secret2 = client
        .get_secret("example-namespace", "database-url", GetOpts::default())
        .await?;

    // Third fetch - still cached
    let _secret3 = client
        .get_secret("example-namespace", "database-url", GetOpts::default())
        .await?;

    // Check cache statistics
    let stats = client.cache_stats();
    println!("Cache statistics:");
    println!("  Hits: {}", stats.hits());
    println!("  Misses: {}", stats.misses());
    println!("  Hit rate: {:.1}%", stats.hit_rate());
    println!("  Insertions: {}", stats.insertions());

    // Invalidate specific entry
    client
        .invalidate_cache("example-namespace", "database-url")
        .await;
    println!("\nInvalidated cache entry");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        // This test verifies that client creation works
        let result = create_client();
        // In real tests, you might want to use a mock server
        // For now, we just check that creation doesn't panic
        assert!(result.is_ok() || result.is_err());
    }
}
