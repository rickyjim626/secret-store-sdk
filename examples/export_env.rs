//! Environment export example for XJP Secret Store SDK

use secret_store_sdk::{Auth, Client, ClientBuilder, EnvExport, ExportFormat, PutOpts};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the client
    let client = create_client()?;

    // Setup: Create some environment variables
    setup_env_secrets(&client).await?;

    // Example 1: Export as JSON
    println!("=== Example 1: Export as JSON ===");
    export_json_example(&client).await?;

    // Example 2: Export as .env file
    println!("\n=== Example 2: Export as .env file ===");
    export_dotenv_example(&client).await?;

    // Example 3: Export as shell script
    println!("\n=== Example 3: Export as shell script ===");
    export_shell_example(&client).await?;

    // Example 4: Export as docker-compose
    println!("\n=== Example 4: Export as docker-compose ===");
    export_docker_compose_example(&client).await?;

    // Example 5: Using exports in application
    println!("\n=== Example 5: Using exports in application ===");
    use_exports_example(&client).await?;

    Ok(())
}

fn create_client() -> Result<Client, Box<dyn std::error::Error>> {
    let base_url = std::env::var("XJP_SECRET_STORE_URL")
        .unwrap_or_else(|_| "https://secret.example.com".to_string());
    let api_key = std::env::var("XJP_SECRET_STORE_API_KEY")
        .unwrap_or_else(|_| "demo-api-key".to_string());

    let client = ClientBuilder::new(base_url)
        .auth(Auth::bearer(api_key))
        .user_agent_extra("env-export-examples/1.0")
        .build()?;

    Ok(client)
}

async fn setup_env_secrets(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let namespace = "env-example";
    
    // Create typical environment variables
    let env_vars = vec![
        ("DATABASE_URL", "postgresql://user:pass@localhost:5432/myapp", "Database connection string"),
        ("REDIS_URL", "redis://localhost:6379", "Redis connection string"),
        ("API_KEY", "sk_live_abcdef123456", "External API key"),
        ("LOG_LEVEL", "info", "Application log level"),
        ("PORT", "8080", "Server port"),
        ("NODE_ENV", "production", "Node environment"),
        ("AWS_REGION", "us-east-1", "AWS region"),
        ("FEATURE_FLAG_NEW_UI", "true", "Feature flag for new UI"),
        ("MAX_CONNECTIONS", "100", "Maximum database connections"),
        ("CACHE_TTL", "3600", "Cache TTL in seconds"),
    ];

    let env_count = env_vars.len();
    for (key, value, description) in env_vars {
        let opts = PutOpts {
            metadata: Some(serde_json::json!({
                "description": description,
                "category": categorize_env_var(key)
            })),
            ..Default::default()
        };
        
        client.put_secret(namespace, key, value, opts).await?;
    }

    println!("Created {} environment variables", env_count);
    Ok(())
}

fn categorize_env_var(key: &str) -> &'static str {
    if key.contains("DATABASE") || key.contains("REDIS") {
        "database"
    } else if key.contains("API") || key.contains("KEY") {
        "credentials"
    } else if key.contains("FLAG") {
        "feature_flags"
    } else {
        "config"
    }
}

async fn export_json_example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let namespace = "env-example";
    
    let export = client.export_env(namespace, ExportFormat::Json).await?;
    
    if let EnvExport::Json(json_export) = export {
        println!("Namespace: {}", json_export.namespace);
        println!("Total variables: {}", json_export.total);
        println!("ETag: {}", json_export.etag);
        println!("\nEnvironment variables:");
        
        // Sort for consistent output
        let mut sorted: Vec<_> = json_export.environment.into_iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        
        for (key, value) in sorted.iter() {
            // Mask sensitive values
            let display_value = if key.contains("KEY") || key.contains("PASSWORD") {
                mask_value(value)
            } else {
                value.clone()
            };
            println!("  {} = {}", key, display_value);
        }

        // Parse as configuration
        let config: AppConfig = parse_config(sorted.into_iter().collect());
        println!("\nParsed configuration:");
        println!("  Database: {}", mask_value(&config.database_url));
        println!("  Port: {}", config.port);
        println!("  Log level: {}", config.log_level);
    }

    Ok(())
}

async fn export_dotenv_example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let namespace = "env-example";
    
    let export = client.export_env(namespace, ExportFormat::Dotenv).await?;
    
    if let EnvExport::Text(dotenv_content) = export {
        println!("Generated .env file:");
        println!("---");
        // Show first few lines
        for line in dotenv_content.lines().take(5) {
            println!("{}", line);
        }
        println!("...");
        println!("---");
        
        // Save to file (in real app)
        // std::fs::write(".env.production", &dotenv_content)?;
        println!("\n(Would save {} bytes to .env.production)", dotenv_content.len());
    }

    Ok(())
}

async fn export_shell_example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let namespace = "env-example";
    
    let export = client.export_env(namespace, ExportFormat::Shell).await?;
    
    if let EnvExport::Text(shell_script) = export {
        println!("Generated shell script:");
        println!("---");
        // Show first few lines
        for line in shell_script.lines().take(5) {
            if line.contains("API_KEY") {
                println!("export API_KEY='****'");
            } else {
                println!("{}", line);
            }
        }
        println!("...");
        println!("---");
        
        // Could be used as: source env.sh
        println!("\n(Would save as env.sh for sourcing)");
    }

    Ok(())
}

async fn export_docker_compose_example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let namespace = "env-example";
    
    let export = client.export_env(namespace, ExportFormat::DockerCompose).await?;
    
    if let EnvExport::Text(docker_compose) = export {
        println!("Docker Compose environment section:");
        println!("---");
        // Docker compose format includes proper indentation
        for line in docker_compose.lines().take(10) {
            println!("{}", line);
        }
        println!("...");
        println!("---");
        
        // Could be included in docker-compose.yml
        println!("\n(Would include in docker-compose.yml under 'environment:')");
    }

    Ok(())
}

async fn use_exports_example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let namespace = "env-example";
    
    // Get environment for different deployment scenarios
    
    // 1. Local development - write .env file
    println!("Setting up local development environment...");
    let local_env = client.export_env(namespace, ExportFormat::Dotenv).await?;
    if let EnvExport::Text(content) = local_env {
        // In real app: std::fs::write(".env.local", content)?;
        println!("  Would create .env.local with {} variables", content.lines().count());
    }

    // 2. CI/CD - export as shell
    println!("\nPreparing CI/CD environment...");
    let ci_env = client.export_env(namespace, ExportFormat::Shell).await?;
    if let EnvExport::Text(_script) = ci_env {
        // In CI: Execute script to set environment
        println!("  Would execute shell script to set environment");
    }

    // 3. Container deployment - get as JSON for programmatic use
    println!("\nConfiguring container deployment...");
    let container_env = client.export_env(namespace, ExportFormat::Json).await?;
    if let EnvExport::Json(json) = container_env {
        // Build container environment
        let env_list: Vec<String> = json.environment
            .into_iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        
        println!("  Would pass {} environment variables to container", env_list.len());
        
        // Example: docker run -e VAR1=val1 -e VAR2=val2 ...
        // Or kubernetes ConfigMap/Secret
    }

    Ok(())
}

// Helper structures
#[derive(Debug)]
struct AppConfig {
    database_url: String,
    port: u16,
    log_level: String,
}

fn parse_config(env: HashMap<String, String>) -> AppConfig {
    AppConfig {
        database_url: env.get("DATABASE_URL").cloned().unwrap_or_default(),
        port: env.get("PORT")
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080),
        log_level: env.get("LOG_LEVEL").cloned().unwrap_or_else(|| "info".to_string()),
    }
}

fn mask_value(value: &str) -> String {
    if value.len() <= 8 {
        "****".to_string()
    } else {
        format!("{}...{}", &value[..4], "****")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_categorize_env_var() {
        assert_eq!(categorize_env_var("DATABASE_URL"), "database");
        assert_eq!(categorize_env_var("API_KEY"), "credentials");
        assert_eq!(categorize_env_var("FEATURE_FLAG_X"), "feature_flags");
        assert_eq!(categorize_env_var("PORT"), "config");
    }

    #[test]
    fn test_mask_value() {
        assert_eq!(mask_value("secret"), "****");
        assert_eq!(mask_value("very-long-secret-key"), "very...****");
    }
}