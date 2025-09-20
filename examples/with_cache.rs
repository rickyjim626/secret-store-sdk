//! Cache usage example for XJP Secret Store SDK

use secrecy::ExposeSecret;
use secret_store_sdk::{Auth, Client, ClientBuilder, GetOpts, PutOpts};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Example 1: Basic cache behavior
    println!("=== Example 1: Basic cache behavior ===");
    basic_cache_example().await?;

    // Example 2: Conditional requests (ETag/304)
    println!("\n=== Example 2: Conditional requests (ETag/304) ===");
    conditional_request_example().await?;

    // Example 3: Cache performance comparison
    println!("\n=== Example 3: Cache performance comparison ===");
    performance_comparison().await?;

    // Example 4: Cache management
    println!("\n=== Example 4: Cache management ===");
    cache_management_example().await?;

    // Example 5: Custom cache configuration
    println!("\n=== Example 5: Custom cache configuration ===");
    custom_cache_config_example().await?;

    Ok(())
}

async fn basic_cache_example() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_client_with_cache(true, 10000, 300)?;
    let namespace = "cache-example";
    let key = "frequently-accessed-config";

    // Ensure the secret exists
    client
        .put_secret(namespace, key, "config-value", PutOpts::default())
        .await?;

    // Clear cache to start fresh
    client.clear_cache();

    // First request - cache miss
    println!("First request (cache miss):");
    let start = Instant::now();
    let secret1 = client
        .get_secret(namespace, key, GetOpts::default())
        .await?;
    let duration1 = start.elapsed();
    println!(
        "  Value: {} (v{})",
        secret1.value.expose_secret(),
        secret1.version
    );
    println!("  Time: {:?}", duration1);

    // Second request - cache hit
    println!("\nSecond request (cache hit):");
    let start = Instant::now();
    let secret2 = client
        .get_secret(namespace, key, GetOpts::default())
        .await?;
    let duration2 = start.elapsed();
    println!(
        "  Value: {} (v{})",
        secret2.value.expose_secret(),
        secret2.version
    );
    println!("  Time: {:?}", duration2);

    // Third request - bypass cache
    println!("\nThird request (bypass cache):");
    let opts = GetOpts {
        use_cache: false,
        ..Default::default()
    };
    let start = Instant::now();
    let secret3 = client.get_secret(namespace, key, opts).await?;
    let duration3 = start.elapsed();
    println!(
        "  Value: {} (v{})",
        secret3.value.expose_secret(),
        secret3.version
    );
    println!("  Time: {:?}", duration3);

    // Show cache statistics
    let stats = client.cache_stats();
    println!("\nCache statistics:");
    println!("  Hits: {}", stats.hits());
    println!("  Misses: {}", stats.misses());
    println!("  Hit rate: {:.1}%", stats.hit_rate());

    println!(
        "\nPerformance improvement: {:.1}x faster with cache",
        duration1.as_secs_f64() / duration2.as_secs_f64()
    );

    Ok(())
}

async fn conditional_request_example() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_client_with_cache(true, 1000, 60)?;
    let namespace = "etag-example";
    let key = "versioned-secret";

    // Create a secret
    client
        .put_secret(namespace, key, "initial-value", PutOpts::default())
        .await?;

    // Get the secret (will have ETag)
    let secret = client
        .get_secret(namespace, key, GetOpts::default())
        .await?;
    let etag = secret.etag.clone().expect("Should have ETag");
    let last_modified = secret
        .last_modified
        .clone()
        .expect("Should have Last-Modified");

    println!("Initial fetch:");
    println!("  Version: {}", secret.version);
    println!("  ETag: {}", etag);
    println!("  Last-Modified: {}", last_modified);

    // Clear cache to force conditional request
    client.clear_cache();

    // Make conditional request with ETag
    println!("\nConditional request with ETag:");
    let opts = GetOpts {
        use_cache: true, // Still use cache for storing 304 result
        if_none_match: Some(etag.clone()),
        ..Default::default()
    };

    // This would return 304 from server but SDK handles it transparently
    let secret2 = client.get_secret(namespace, key, opts).await?;
    println!(
        "  Got value (from 304 response): {}",
        secret2.value.expose_secret()
    );

    // Update the secret
    println!("\nUpdating secret...");
    client
        .put_secret(namespace, key, "updated-value", PutOpts::default())
        .await?;

    // Fetch again with old ETag - should get new value
    let opts = GetOpts {
        use_cache: false,
        if_none_match: Some(etag),
        ..Default::default()
    };
    let secret3 = client.get_secret(namespace, key, opts).await?;
    println!("  New version: {} (ETag changed)", secret3.version);
    println!("  New value: {}", secret3.value.expose_secret());

    Ok(())
}

async fn performance_comparison() -> Result<(), Box<dyn std::error::Error>> {
    let namespace = "perf-test";

    // Create secrets for testing
    println!("Setting up test data...");
    let client_setup = create_client_with_cache(false, 0, 0)?;
    for i in 0..10 {
        client_setup
            .put_secret(
                namespace,
                &format!("perf-key-{}", i),
                &format!("value-{}", i),
                PutOpts::default(),
            )
            .await?;
    }

    // Test without cache
    println!("\nTesting without cache:");
    let client_no_cache = create_client_with_cache(false, 0, 0)?;
    let start = Instant::now();
    for _ in 0..3 {
        for i in 0..10 {
            let _ = client_no_cache
                .get_secret(namespace, &format!("perf-key-{}", i), GetOpts::default())
                .await?;
        }
    }
    let duration_no_cache = start.elapsed();
    println!("  30 requests took: {:?}", duration_no_cache);

    // Test with cache
    println!("\nTesting with cache:");
    let client_with_cache = create_client_with_cache(true, 10000, 300)?;
    let start = Instant::now();
    for _ in 0..3 {
        for i in 0..10 {
            let _ = client_with_cache
                .get_secret(namespace, &format!("perf-key-{}", i), GetOpts::default())
                .await?;
        }
    }
    let duration_with_cache = start.elapsed();
    println!("  30 requests took: {:?}", duration_with_cache);

    let stats = client_with_cache.cache_stats();
    println!("\n  Cache hits: {}", stats.hits());
    println!("  Cache misses: {}", stats.misses());
    println!(
        "  Performance improvement: {:.1}x faster",
        duration_no_cache.as_secs_f64() / duration_with_cache.as_secs_f64()
    );

    Ok(())
}

async fn cache_management_example() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_client_with_cache(true, 100, 300)?;
    let namespace = "cache-mgmt";

    // Fill cache with multiple entries
    println!("Filling cache with secrets...");
    for i in 0..5 {
        let key = format!("cache-key-{}", i);
        client
            .put_secret(namespace, &key, &format!("value-{}", i), PutOpts::default())
            .await?;
        client
            .get_secret(namespace, &key, GetOpts::default())
            .await?;
    }

    let stats = client.cache_stats();
    println!("Initial cache state:");
    println!("  Entries cached: {}", stats.insertions());

    // Invalidate specific entry
    println!("\nInvalidating cache-key-2...");
    client.invalidate_cache(namespace, "cache-key-2").await;

    // Verify it's no longer cached
    let _ = client
        .get_secret(namespace, "cache-key-2", GetOpts::default())
        .await?;
    println!("  Cache misses increased: {}", stats.misses());

    // Update a secret (should auto-invalidate)
    println!("\nUpdating cache-key-3 (auto-invalidates cache)...");
    client
        .put_secret(namespace, "cache-key-3", "new-value-3", PutOpts::default())
        .await?;

    // Fetch should get new value
    let updated = client
        .get_secret(namespace, "cache-key-3", GetOpts::default())
        .await?;
    println!("  Got updated value: {}", updated.value.expose_secret());

    // Clear entire cache
    println!("\nClearing entire cache...");
    let stats_before_clear = client.cache_stats().clone();
    client.clear_cache();

    // All fetches should miss now
    for i in 0..5 {
        let _ = client
            .get_secret(namespace, &format!("cache-key-{}", i), GetOpts::default())
            .await?;
    }

    println!(
        "  All entries cleared, new misses: {}",
        client.cache_stats().misses() - stats_before_clear.misses()
    );

    Ok(())
}

async fn custom_cache_config_example() -> Result<(), Box<dyn std::error::Error>> {
    // Example 1: Short-lived cache for frequently changing data
    println!("Short-lived cache (10 second TTL):");
    let client_short = create_client_with_cache(true, 1000, 10)?;

    let namespace = "cache-config";
    client_short
        .put_secret(namespace, "short-ttl", "value", PutOpts::default())
        .await?;
    client_short
        .get_secret(namespace, "short-ttl", GetOpts::default())
        .await?;

    println!("  Cached entry will expire in 10 seconds");

    // Example 2: Large cache for read-heavy workload
    println!("\nLarge cache (100k entries):");
    let _client_large = create_client_with_cache(true, 100_000, 3600)?;
    println!("  Configured for high-volume read operations");

    // Example 3: No cache for sensitive operations
    println!("\nNo cache (security-sensitive):");
    let _client_no_cache = create_client_with_cache(false, 0, 0)?;
    println!("  Every request goes to server");

    // Example 4: Custom TTL per secret
    println!("\nCache with secret-specific TTL:");
    let client = create_client_with_cache(true, 10000, 300)?;

    // Secret with TTL will be cached for min(secret_ttl, cache_ttl)
    let opts = PutOpts {
        ttl_seconds: Some(60), // 1 minute TTL on secret itself
        ..Default::default()
    };
    client
        .put_secret(namespace, "ttl-secret", "expires-soon", opts)
        .await?;

    let secret = client
        .get_secret(namespace, "ttl-secret", GetOpts::default())
        .await?;
    if let Some(expires_at) = secret.expires_at {
        println!("  Secret expires at: {}", expires_at);
        println!("  Cache will respect this expiration");
    }

    Ok(())
}

// Helper functions

fn create_client_with_cache(
    enable_cache: bool,
    max_entries: u64,
    ttl_secs: u64,
) -> Result<Client, Box<dyn std::error::Error>> {
    let base_url = std::env::var("XJP_SECRET_STORE_URL")
        .unwrap_or_else(|_| "https://secret.example.com".to_string());
    let api_key =
        std::env::var("XJP_SECRET_STORE_API_KEY").unwrap_or_else(|_| "demo-api-key".to_string());

    let client = ClientBuilder::new(base_url)
        .auth(Auth::bearer(api_key))
        .enable_cache(enable_cache)
        .cache_max_entries(max_entries)
        .cache_ttl_secs(ttl_secs)
        .user_agent_extra("cache-examples/1.0")
        .build()?;

    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_config() {
        // Test that we can create clients with different cache configs
        let _client1 = create_client_with_cache(true, 1000, 60);
        let _client2 = create_client_with_cache(false, 0, 0);
        let _client3 = create_client_with_cache(true, 100_000, 3600);
    }
}
