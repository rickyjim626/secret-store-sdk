//! Example showing how to use OpenTelemetry metrics with XJP Secret Store SDK
//!
//! This example demonstrates:
//! - Enabling metrics collection in the SDK
//! - Setting up Prometheus exporter
//! - Performing operations that generate metrics
//! - Exporting metrics in Prometheus format
//!
//! Run with:
//! ```sh
//! cargo run --example metrics --features metrics
//! ```

#[cfg(feature = "metrics")]
use secret_store_sdk::{Auth, ClientBuilder, GetOpts};

#[cfg(not(feature = "metrics"))]
fn main() {
    eprintln!("This example requires the 'metrics' feature to be enabled.");
    eprintln!("Run with: cargo run --example metrics --features metrics");
}

#[cfg(feature = "metrics")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use secret_store_sdk::telemetry::TelemetryConfig;
    
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    #[cfg(feature = "metrics")]
    {
        // Initialize simple Prometheus registry
        println!("Initializing metrics...");
        
        let api_key = std::env::var("XJP_API_KEY")?;
        let base_url = std::env::var("XJP_BASE_URL")?;
        
        // Create telemetry config
        let telemetry_config = TelemetryConfig {
            enabled: true,
            service_name: "xjp-secret-store-example".to_string(),
            service_version: "0.1.0".to_string(),
        };
        
        // Create client with telemetry enabled
        let client = ClientBuilder::new(&base_url)
            .auth(Auth::api_key(api_key))
            .with_telemetry(telemetry_config)
            .enable_cache(true)
            .build()?;
        
        println!("Performing operations to generate metrics...\n");
        
        // Perform various operations to generate metrics
        
        // 1. Successful requests
        println!("1. Making successful requests:");
        for i in 0..5 {
            let key = format!("metric-test-{}", i);
            match client.get_secret("test", &key, GetOpts::default()).await {
                Ok(_) => println!("   ✓ Got secret: test/{}", key),
                Err(_) => println!("   ✗ Failed to get: test/{}", key),
            }
        }
        
        // 2. Cache hits
        println!("\n2. Testing cache (should generate cache hits):");
        let _ = client.get_secret("test", "cached-key", GetOpts::default()).await;
        for _ in 0..3 {
            match client.get_secret("test", "cached-key", GetOpts::default()).await {
                Ok(_) => println!("   ✓ Cache hit for test/cached-key"),
                Err(_) => println!("   ✗ Cache miss"),
            }
        }
        
        // 3. Force cache misses
        println!("\n3. Testing cache misses:");
        for i in 0..3 {
            let opts = GetOpts {
                use_cache: false,
                ..Default::default()
            };
            let key = format!("no-cache-{}", i);
            match client.get_secret("test", &key, opts).await {
                Ok(_) => println!("   ✓ Got secret (no cache): test/{}", key),
                Err(_) => println!("   ✗ Failed: test/{}", key),
            }
        }
        
        // 4. Generate some errors
        println!("\n4. Testing error scenarios:");
        for i in 0..3 {
            let key = format!("error-test-{}", i);
            match client.get_secret("invalid-namespace!", &key, GetOpts::default()).await {
                Ok(_) => println!("   ✓ Unexpected success"),
                Err(e) => println!("   ✗ Expected error: {}", e),
            }
        }
        
        println!("\n5. Metrics Summary:");
        println!("   Note: OpenTelemetry metrics are collected internally.");
        println!("   To export metrics, integrate with a metrics backend like Prometheus.");
        println!("   The SDK tracks:");
        println!("   - Total requests by method/path/status");
        println!("   - Request duration histograms");
        println!("   - Cache hits/misses");
        println!("   - Active connections");
        println!("   - Retry attempts");
        
        // Get cache statistics
        let cache_stats = client.cache_stats();
        println!("\n6. Cache Statistics:");
        println!("   Hits: {}", cache_stats.hits());
        println!("   Misses: {}", cache_stats.misses());
        println!("   Hit rate: {:.2}%", cache_stats.hit_rate() * 100.0);
        
        println!("\nExample completed successfully!");
    }
    
    Ok(())
}