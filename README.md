# XJP Secret Store SDK for Rust

A comprehensive Rust SDK for interacting with the XJP Secret Store service, providing secure storage and retrieval of secrets, configuration values, and sensitive data.

## Features

- 🔐 **Multiple Authentication Methods**: Bearer token, API key, XJP key, and dynamic token providers
- ⚡ **High Performance**: Built-in caching with ETag/304 support for optimal performance
- 🔄 **Automatic Retries**: Exponential backoff with jitter for transient failures
- 🛡️ **Secure by Default**: Enforces HTTPS, proper secret handling with zeroization
- 📦 **Batch Operations**: Efficient bulk operations with transactional support
- 🌍 **Environment Export**: Export secrets in multiple formats (JSON, dotenv, shell, docker-compose)
- 📊 **Comprehensive Monitoring**: Cache statistics and optional OpenTelemetry support
- ⏱️ **Version Management**: Track and rollback secret versions
- 🔍 **Audit Trail**: Query audit logs for compliance and debugging

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
secret-store-sdk = "0.1"
```

### Feature Flags

- `rustls-tls` (default): Use rustls for TLS
- `native-tls`: Use native system TLS implementation
- `blocking`: Enable blocking/synchronous API
- `metrics`: Enable OpenTelemetry metrics
- `wasm`: WebAssembly support for browser/edge environments
- `danger-insecure-http`: Allow insecure HTTP connections (development only)

## Quick Start

```rust
use xjp_secret_store::{Client, ClientBuilder, Auth};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Build client with API key authentication
    let client = ClientBuilder::new("https://secret.example.com")
        .auth(Auth::bearer("your-api-key"))
        .build()?;

    // Get a secret
    let secret = client.get_secret("production", "database-url", Default::default()).await?;
    println!("Secret version: {}", secret.version);

    // Put a secret with TTL
    let opts = PutOpts {
        ttl_seconds: Some(3600), // 1 hour
        metadata: Some(serde_json::json!({"env": "prod"})),
        ..Default::default()
    };
    client.put_secret("production", "api-key", "secret-value", opts).await?;

    Ok(())
}
```

## Authentication

The SDK supports multiple authentication methods in priority order:

### Bearer Token (Highest Priority)
```rust
let client = ClientBuilder::new(base_url)
    .auth(Auth::bearer("your-bearer-token"))
    .build()?;
```

### API Key
```rust
let client = ClientBuilder::new(base_url)
    .auth(Auth::api_key("your-api-key"))
    .build()?;
```

### Dynamic Token Provider
```rust
use xjp_secret_store::{TokenProvider, SecretString};

#[derive(Clone)]
struct MyTokenProvider {
    // your implementation
}

#[async_trait]
impl TokenProvider for MyTokenProvider {
    async fn get_token(&self) -> Result<SecretString, Box<dyn Error + Send + Sync>> {
        // Fetch token from your auth service
        Ok(SecretString::new("dynamic-token"))
    }
    
    async fn refresh_token(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Refresh the token
        Ok(())
    }
    
    fn clone_box(&self) -> Box<dyn TokenProvider> {
        Box::new(self.clone())
    }
}

let client = ClientBuilder::new(base_url)
    .auth(Auth::token_provider(MyTokenProvider { }))
    .build()?;
```

## Core Operations

### Get Secret
```rust
use xjp_secret_store::GetOpts;

// Simple get
let secret = client.get_secret("namespace", "key", GetOpts::default()).await?;

// Get with cache disabled
let opts = GetOpts {
    use_cache: false,
    ..Default::default()
};
let secret = client.get_secret("namespace", "key", opts).await?;

// Conditional get with ETag
let opts = GetOpts {
    if_none_match: Some(previous_etag),
    ..Default::default()
};
match client.get_secret("namespace", "key", opts).await {
    Ok(secret) => println!("Secret updated: {}", secret.version),
    Err(e) if e.status_code() == Some(304) => println!("Not modified"),
    Err(e) => return Err(e.into()),
}
```

### Put Secret
```rust
use xjp_secret_store::PutOpts;

// Simple put
client.put_secret("namespace", "key", "value", PutOpts::default()).await?;

// Put with options
let opts = PutOpts {
    ttl_seconds: Some(3600), // 1 hour TTL
    metadata: Some(serde_json::json!({
        "owner": "team-a",
        "classification": "internal"
    })),
    idempotency_key: Some("unique-operation-id".to_string()),
};
client.put_secret("namespace", "key", "value", opts).await?;
```

### List Secrets
```rust
use xjp_secret_store::ListOpts;

// List all secrets
let list = client.list_secrets("namespace", ListOpts::default()).await?;

// List with prefix and limit
let opts = ListOpts {
    prefix: Some("app-".to_string()),
    limit: Some(50),
};
let list = client.list_secrets("namespace", opts).await?;
```

## Batch Operations

### Batch Get
```rust
use xjp_secret_store::{BatchKeys, ExportFormat};

// Get specific keys
let keys = BatchKeys::Keys(vec!["key1".to_string(), "key2".to_string()]);
let result = client.batch_get("namespace", keys, ExportFormat::Json).await?;

// Get all keys
let result = client.batch_get("namespace", BatchKeys::All, ExportFormat::Json).await?;

// Export as dotenv format
let result = client.batch_get("namespace", BatchKeys::All, ExportFormat::Dotenv).await?;
match result {
    BatchGetResult::Text(dotenv_content) => {
        std::fs::write(".env", dotenv_content)?;
    }
    _ => {}
}
```

### Batch Operations
```rust
use xjp_secret_store::BatchOp;

let operations = vec![
    BatchOp::put("key1", "value1").with_ttl(3600),
    BatchOp::put("key2", "value2").with_metadata(json!({"env": "prod"})),
    BatchOp::delete("old-key"),
];

// Execute with transaction
let result = client.batch_operate(
    "namespace", 
    operations, 
    true, // transactional
    Some("idempotency-key".to_string())
).await?;

println!("Succeeded: {}, Failed: {}", result.succeeded, result.failed);
```

## Environment Export

```rust
use xjp_secret_store::ExportFormat;

// Export as JSON
let export = client.export_env("namespace", ExportFormat::Json).await?;
if let EnvExport::Json(json) = export {
    for (key, value) in json.environment {
        println!("{} = {}", key, value);
    }
}

// Export as shell script
let export = client.export_env("namespace", ExportFormat::Shell).await?;
if let EnvExport::Text(shell_script) = export {
    std::fs::write("env.sh", shell_script)?;
}
```

## Version Management

The SDK provides comprehensive version management capabilities for secrets:

### List Secret Versions
```rust
// List all versions of a secret
let versions = client.list_versions("namespace", "key").await?;
println!("Found {} versions:", versions.total);

for version in &versions.versions {
    println!("Version {}: created at {} by {}", 
        version.version,
        version.created_at,
        version.created_by
    );
    if version.is_current {
        println!("  ^ This is the current version");
    }
}
```

### Get Specific Version
```rust
// Get a specific version of a secret
let secret_v2 = client.get_version("namespace", "key", 2).await?;
println!("Version 2 value: {}", secret_v2.value.expose_secret());
println!("Version 2 metadata: {:?}", secret_v2.metadata);
```

### Rollback to Previous Version
```rust
// Rollback a secret to a specific version
let rollback_result = client.rollback("namespace", "key", 2).await?;
println!("Rolled back from version {} to {}", 
    rollback_result.from_version, 
    rollback_result.to_version
);

// The rolled back version becomes the new current version
let current = client.get_secret("namespace", "key", Default::default()).await?;
println!("Current version is now: {}", current.version);
```

### Version History Example
```rust
// Create multiple versions
client.put_secret("namespace", "api-key", "v1-secret", Default::default()).await?;
tokio::time::sleep(Duration::from_secs(1)).await;

let opts = PutOpts {
    metadata: Some(json!({"reason": "rotation"})),
    ..Default::default()
};
client.put_secret("namespace", "api-key", "v2-secret", opts).await?;

// Check version history
let versions = client.list_versions("namespace", "api-key").await?;
assert_eq!(versions.total, 2);

// Rollback if needed
if need_rollback {
    client.rollback("namespace", "api-key", 1).await?;
}
```

## Namespace Management

```rust
// List all namespaces
let namespaces = client.list_namespaces().await?;

// Get namespace details
let info = client.get_namespace("production").await?;
println!("Namespace has {} secrets", info.secret_count);

// Initialize namespace with template
use xjp_secret_store::NamespaceTemplate;
let template = NamespaceTemplate {
    template: "web-app".to_string(),
    params: json!({
        "environment": "staging",
        "region": "us-east-1"
    }),
};
client.init_namespace("new-namespace", template).await?;
```

## Audit Logs

The SDK provides comprehensive audit log querying capabilities for compliance and debugging:

### Basic Audit Query
```rust
use xjp_secret_store::AuditQuery;

// Query all audit logs
let query = AuditQuery::default();
let audit_logs = client.audit(query).await?;

println!("Total audit entries: {}", audit_logs.total);
for entry in &audit_logs.entries {
    println!("{}: {} by {:?} - Success: {}", 
        entry.timestamp,
        entry.action,
        entry.actor,
        entry.success
    );
}
```

### Filtered Queries
```rust
// Query failed operations
let query = AuditQuery {
    success: Some(false),
    limit: Some(20),
    ..Default::default()
};
let failed_ops = client.audit(query).await?;

// Query by namespace and time range
let query = AuditQuery {
    namespace: Some("production".to_string()),
    from: Some("2024-01-01T00:00:00Z".to_string()),
    to: Some("2024-01-31T23:59:59Z".to_string()),
    ..Default::default()
};
let prod_logs = client.audit(query).await?;

// Query specific actions by actor
let query = AuditQuery {
    actor: Some("ci-pipeline".to_string()),
    action: Some("put".to_string()),
    ..Default::default()
};
let ci_writes = client.audit(query).await?;
```

### Pagination
```rust
// Paginate through audit logs
let mut all_entries = Vec::new();
let mut offset = 0;
let limit = 100;

loop {
    let query = AuditQuery {
        limit: Some(limit),
        offset: Some(offset),
        ..Default::default()
    };
    
    let page = client.audit(query).await?;
    all_entries.extend(page.entries);
    
    if !page.has_more {
        break;
    }
    
    offset += limit;
}

println!("Retrieved {} total audit entries", all_entries.len());
```

### Audit Entry Fields
Each audit entry contains:
- `id`: Unique identifier
- `timestamp`: When the action occurred
- `actor`: Who performed the action (optional)
- `action`: What action was performed (get, put, delete, etc.)
- `namespace`: Affected namespace (optional)
- `key_name`: Affected key (optional)
- `success`: Whether the action succeeded
- `ip_address`: Client IP address (optional)
- `user_agent`: Client user agent (optional)
- `error`: Error message if failed (optional)

## Caching

The SDK includes an intelligent caching layer:

```rust
// Configure caching
let client = ClientBuilder::new(base_url)
    .auth(Auth::bearer(token))
    .enable_cache(true)
    .cache_max_entries(10000)
    .cache_ttl_secs(300) // 5 minutes
    .build()?;

// Get cache statistics
let stats = client.cache_stats();
println!("Cache hit rate: {:.2}%", stats.hit_rate());
println!("Hits: {}, Misses: {}", stats.hits(), stats.misses());

// Clear cache
client.clear_cache();

// Invalidate specific entry
client.invalidate_cache("namespace", "key").await;
```

## Error Handling

The SDK provides detailed error information:

```rust
match client.get_secret("ns", "key", Default::default()).await {
    Ok(secret) => println!("Got secret v{}", secret.version),
    Err(Error::Http { status, category, message, request_id }) => {
        eprintln!("HTTP {}: {} - {} (request: {:?})", 
                 status, category, message, request_id);
        
        // Handle specific errors
        match status {
            401 => println!("Authentication failed"),
            403 => println!("Permission denied"),
            404 => println!("Secret not found"),
            429 => println!("Rate limited, retry later"),
            _ => println!("Server error"),
        }
    }
    Err(Error::Network(msg)) => eprintln!("Network error: {}", msg),
    Err(Error::Timeout) => eprintln!("Request timed out"),
    Err(e) => eprintln!("Other error: {}", e),
}
```

## Configuration

### Timeouts and Retries
```rust
let client = ClientBuilder::new(base_url)
    .auth(Auth::bearer(token))
    .timeout_ms(30000) // 30 seconds
    .retries(3) // up to 3 retries
    .build()?;
```

### Custom User Agent
```rust
let client = ClientBuilder::new(base_url)
    .auth(Auth::bearer(token))
    .user_agent_extra("my-app/1.0")
    .build()?;
```

### Allow Insecure HTTP (Development Only)
```rust
#[cfg(feature = "danger-insecure-http")]
let client = ClientBuilder::new("http://localhost:8080")
    .auth(Auth::bearer(token))
    .allow_insecure_http()
    .build()?;
```

## Observability with OpenTelemetry

The SDK supports OpenTelemetry metrics when the `metrics` feature is enabled:

### Enable Metrics

```rust
// Add to Cargo.toml
[dependencies]
secret-store-sdk = { version = "0.1", features = ["metrics"] }
```

### Configure Telemetry

```rust
use xjp_secret_store::{ClientBuilder, Auth, telemetry::TelemetryConfig};

// Configure telemetry
let telemetry_config = TelemetryConfig {
    enabled: true,
    service_name: "my-service".to_string(),
    service_version: "1.0.0".to_string(),
};

// Create client with telemetry
let client = ClientBuilder::new(base_url)
    .auth(Auth::bearer(token))
    .with_telemetry(telemetry_config)
    .build()?;

// Or simply enable with defaults
let client = ClientBuilder::new(base_url)
    .auth(Auth::bearer(token))
    .enable_telemetry()
    .build()?;
```

### Available Metrics

The SDK exposes the following metrics:

- **xjp_secret_store.requests_total**: Total number of requests (labels: method, path, status)
- **xjp_secret_store.request_duration_seconds**: Request duration histogram
- **xjp_secret_store.errors_total**: Total number of errors (labels: type, status)
- **xjp_secret_store.cache_hits_total**: Cache hit counter (label: namespace)
- **xjp_secret_store.cache_misses_total**: Cache miss counter (label: namespace)
- **xjp_secret_store.active_connections**: Current active connections (UpDownCounter)
- **xjp_secret_store.retry_attempts_total**: Retry attempts counter (labels: attempt, reason)

### Integration Example

```rust
// Initialize OpenTelemetry with Prometheus exporter
let exporter = opentelemetry_prometheus::exporter()
    .init();

// Set global meter provider
opentelemetry::global::set_meter_provider(
    exporter.meter_provider().unwrap()
);

// Create SDK client with telemetry
let client = ClientBuilder::new(base_url)
    .auth(Auth::bearer(token))
    .enable_telemetry()
    .build()?;

// Use the client - metrics are automatically collected
let secret = client.get_secret("prod", "api-key", Default::default()).await?;

// Export metrics (e.g., for Prometheus scraping)
let metrics = exporter.registry().gather();
```

See the [metrics example](examples/metrics.rs) for a complete working implementation.

## Best Practices

1. **Enable Caching**: For read-heavy workloads, keep caching enabled to reduce API calls
2. **Use Batch Operations**: For multiple operations, use batch APIs to reduce round trips
3. **Handle 304 Not Modified**: Leverage ETags for conditional requests
4. **Set Appropriate TTLs**: Use TTLs for temporary secrets
5. **Use Idempotency Keys**: For critical write operations, use idempotency keys
6. **Monitor Cache Stats**: Regularly check cache hit rates to optimize performance
7. **Secure Token Storage**: Never hardcode tokens; use environment variables or secure vaults

## Migration Guide

### From Node.js SDK
```javascript
// Node.js
const client = new SecretStoreClient({
  baseUrl: 'https://api.example.com',
  apiKey: 'key',
});
const secret = await client.getSecret('ns', 'key');
```

```rust
// Rust
let client = ClientBuilder::new("https://api.example.com")
    .auth(Auth::api_key("key"))
    .build()?;
let secret = client.get_secret("ns", "key", Default::default()).await?;
```

### Field Mappings
- List responses: `ver` (API) → `version` (SDK)
- All timestamps are parsed to `time::OffsetDateTime`
- Metadata is `serde_json::Value` for flexibility

## Contributing

1. Clone the repository
2. Run tests: `cargo test`
3. Run benchmarks: `cargo bench`
4. Format code: `cargo fmt`
5. Check lints: `cargo clippy`

## License

MIT OR Apache-2.0