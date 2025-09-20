//! Example of version management functionality

use anyhow::Result;
use secrecy::ExposeSecret;
use secret_store_sdk::{Auth, ClientBuilder, PutOpts};

#[tokio::main]
async fn main() -> Result<()> {
    // Setup
    env_logger::init();
    dotenv::dotenv().ok();
    
    let api_key = std::env::var("XJP_API_KEY")?;
    let base_url = std::env::var("XJP_BASE_URL")?;
    
    // Create client
    let client = ClientBuilder::new(base_url)
        .auth(Auth::api_key(api_key))
        .build()?;
    
    let namespace = "test";
    let key = "version-demo";
    
    // Create initial version
    println!("Creating initial secret...");
    let result = client.put_secret(namespace, key, "version 1", PutOpts::default()).await?;
    println!("Created: {:?}\n", result);
    
    // Update to create version 2
    println!("Updating secret (creating version 2)...");
    let result = client.put_secret(namespace, key, "version 2", PutOpts::default()).await?;
    println!("Updated: {:?}\n", result);
    
    // Update to create version 3
    println!("Updating secret (creating version 3)...");
    let result = client.put_secret(namespace, key, "version 3", PutOpts::default()).await?;
    println!("Updated: {:?}\n", result);
    
    // List all versions
    println!("Listing all versions...");
    let versions = client.list_versions(namespace, key).await?;
    println!("Found {} versions:", versions.total);
    for version in &versions.versions {
        println!("  - Version {}: created at {} by {}", 
            version.version, 
            version.created_at,
            version.created_by
        );
    }
    println!();
    
    // Get specific version
    println!("Getting version 2...");
    let version_2 = client.get_version(namespace, key, 2).await?;
    println!("Version 2 value: {}", version_2.value.expose_secret());
    println!();
    
    // Get current version
    println!("Getting current version...");
    let current = client.get_secret(namespace, key, Default::default()).await?;
    println!("Current version ({}) value: {}", current.version, current.value.expose_secret());
    println!();
    
    // Rollback to version 2
    println!("Rolling back to version 2...");
    let rollback_result = client.rollback(namespace, key, 2).await?;
    println!("Rollback result: {:?}", rollback_result);
    println!();
    
    // Verify rollback
    println!("Getting current version after rollback...");
    let current = client.get_secret(namespace, key, Default::default()).await?;
    println!("Current version ({}) value: {}", current.version, current.value.expose_secret());
    
    Ok(())
}