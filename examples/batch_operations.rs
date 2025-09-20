//! Batch operations example for XJP Secret Store SDK

use secret_store_sdk::{
    Auth, BatchGetResult, BatchKeys, BatchOp, Client, ClientBuilder, ExportFormat,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the client
    let client = create_client()?;

    // Example 1: Batch get specific keys
    println!("=== Example 1: Batch get specific keys ===");
    batch_get_keys_example(&client).await?;

    // Example 2: Batch get all keys
    println!("\n=== Example 2: Batch get all keys ===");
    batch_get_all_example(&client).await?;

    // Example 3: Batch operations (put/delete)
    println!("\n=== Example 3: Batch operations ===");
    batch_operations_example(&client).await?;

    // Example 4: Transactional batch operations
    println!("\n=== Example 4: Transactional batch operations ===");
    transactional_batch_example(&client).await?;

    Ok(())
}

fn create_client() -> Result<Client, Box<dyn std::error::Error>> {
    let base_url = std::env::var("XJP_SECRET_STORE_URL")
        .unwrap_or_else(|_| "https://secret.example.com".to_string());
    let api_key = std::env::var("XJP_SECRET_STORE_API_KEY")
        .unwrap_or_else(|_| "demo-api-key".to_string());

    let client = ClientBuilder::new(base_url)
        .auth(Auth::bearer(api_key))
        .user_agent_extra("batch-examples/1.0")
        .build()?;

    Ok(client)
}

async fn batch_get_keys_example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    // First, create some secrets to fetch
    let namespace = "batch-example";
    
    for i in 1..=5 {
        client
            .put_secret(
                namespace,
                &format!("batch-key-{}", i),
                &format!("batch-value-{}", i),
                Default::default(),
            )
            .await?;
    }

    // Batch get specific keys as JSON
    let keys = BatchKeys::Keys(vec![
        "batch-key-1".to_string(),
        "batch-key-3".to_string(),
        "batch-key-5".to_string(),
    ]);

    let result = client
        .batch_get(namespace, keys, ExportFormat::Json)
        .await?;

    match result {
        BatchGetResult::Json(json_result) => {
            println!("Got {} secrets:", json_result.total);
            for (key, value) in &json_result.secrets {
                println!("  {}: {}", key, value);
            }
        }
        _ => println!("Unexpected result format"),
    }

    Ok(())
}

async fn batch_get_all_example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let namespace = "batch-example";

    // Get all secrets in different formats
    
    // 1. As JSON
    println!("\nAll secrets as JSON:");
    let result = client
        .batch_get(namespace, BatchKeys::All, ExportFormat::Json)
        .await?;
    
    if let BatchGetResult::Json(json) = result {
        println!("Total secrets: {}", json.total);
    }

    // 2. As dotenv format
    println!("\nAll secrets as .env format:");
    let result = client
        .batch_get(namespace, BatchKeys::All, ExportFormat::Dotenv)
        .await?;
    
    if let BatchGetResult::Text(dotenv) = result {
        println!("{}", dotenv);
        // Could write to file: std::fs::write(".env", dotenv)?;
    }

    // 3. As shell script
    println!("\nAll secrets as shell exports:");
    let result = client
        .batch_get(namespace, BatchKeys::All, ExportFormat::Shell)
        .await?;
    
    if let BatchGetResult::Text(shell) = result {
        // First few lines only
        for line in shell.lines().take(3) {
            println!("{}", line);
        }
        println!("... (truncated)");
    }

    Ok(())
}

async fn batch_operations_example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let namespace = "batch-ops";

    // Create a batch of operations
    let operations = vec![
        // Create new secrets
        BatchOp::put("db-host", "localhost")
            .with_ttl(3600)
            .with_metadata(serde_json::json!({"service": "postgres"})),
        BatchOp::put("db-port", "5432"),
        BatchOp::put("db-name", "myapp"),
        
        // Update existing secret
        BatchOp::put("api-version", "v2"),
        
        // Delete old secrets
        BatchOp::delete("deprecated-key-1"),
        BatchOp::delete("deprecated-key-2"),
    ];

    // Execute non-transactional (partial success allowed)
    let result = client
        .batch_operate(
            namespace, 
            operations,
            false, // non-transactional
            Some("batch-example-001".to_string()),
        )
        .await?;

    println!("Batch operation results:");
    println!("  Total operations: {}", result.results.total);
    println!("  Succeeded: {}", result.results.succeeded.len());
    println!("  Failed: {}", result.results.failed.len());
    println!("  Success rate: {:.2}%", result.success_rate * 100.0);

    // Show successful operations
    for op in &result.results.succeeded {
        println!("  ✓ {} {}", op.action, op.key);
    }
    
    // Show failed operations
    for op in &result.results.failed {
        println!("  ✗ {} {}: {:?}", op.action, op.key, op.error);
    }

    Ok(())
}

async fn transactional_batch_example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let namespace = "transactional-example";

    // Prepare a transactional batch - all or nothing
    let operations = vec![
        BatchOp::put("transaction-1", "value1"),
        BatchOp::put("transaction-2", "value2"),
        BatchOp::put("transaction-3", "value3"),
        // This might fail if the key doesn't exist
        BatchOp::delete("must-exist-key"),
    ];

    println!("Attempting transactional batch operation...");

    match client
        .batch_operate(
            namespace,
            operations.clone(),
            true, // transactional
            None,
        )
        .await
    {
        Ok(result) => {
            println!("Transaction succeeded!");
            println!("All {} operations completed", result.results.succeeded.len());
        }
        Err(e) => {
            println!("Transaction failed: {}", e);
            println!("No operations were applied due to transactional mode");
        }
    }

    // Retry without the potentially failing operation
    println!("\nRetrying without the delete operation...");
    
    let safe_operations = vec![
        BatchOp::put("transaction-1", "value1"),
        BatchOp::put("transaction-2", "value2"),
        BatchOp::put("transaction-3", "value3"),
    ];

    let result = client
        .batch_operate(
            namespace,
            safe_operations,
            true,
            Some("retry-transaction-001".to_string()),
        )
        .await?;

    println!("Transaction succeeded with {} operations", result.results.succeeded.len());

    Ok(())
}

/// Example of building complex batch operations programmatically
#[allow(dead_code)]
async fn complex_batch_example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let namespace = "complex-batch";

    // Build operations dynamically
    let mut operations = Vec::new();

    // Add configuration for multiple services
    let services = ["web", "api", "worker"];
    let environments = ["dev", "staging", "prod"];

    for service in &services {
        for env in &environments {
            let key = format!("{}-{}-endpoint", service, env);
            let value = format!("https://{}-{}.example.com", service, env);
            
            operations.push(
                BatchOp::put(&key, &value)
                    .with_metadata(serde_json::json!({
                        "service": service,
                        "environment": env,
                        "managed_by": "terraform"
                    }))
            );
        }
    }

    // Add database configs with TTL
    for i in 1..=3 {
        operations.push(
            BatchOp::put(
                &format!("temp-db-password-{}", i),
                &format!("temp-pass-{}", uuid::Uuid::new_v4()),
            )
            .with_ttl(86400) // 24 hours
        );
    }

    // Execute the batch
    let result = client
        .batch_operate(namespace, operations, false, None)
        .await?;

    println!("Created {} configuration entries", result.results.succeeded.len());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_op_builder() {
        let op = BatchOp::put("key", "value")
            .with_ttl(3600)
            .with_metadata(serde_json::json!({"test": true}));

        assert_eq!(op.action, "put");
        assert_eq!(op.key, "key");
        assert_eq!(op.value, Some("value".to_string()));
        assert_eq!(op.ttl_seconds, Some(3600));
        assert!(op.metadata.is_some());
    }

    #[test]
    fn test_batch_delete() {
        let op = BatchOp::delete("old-key");
        assert_eq!(op.action, "delete");
        assert_eq!(op.key, "old-key");
        assert!(op.value.is_none());
    }
}