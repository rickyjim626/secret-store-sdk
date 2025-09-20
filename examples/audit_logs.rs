//! Example of querying audit logs

use anyhow::Result;
use secret_store_sdk::{AuditQuery, Auth, ClientBuilder};

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

    // Example 1: Query all audit logs
    println!("=== All Audit Logs ===");
    let query = AuditQuery::default();
    let audit_logs = client.audit(query).await?;

    println!("Total audit logs: {}", audit_logs.total);
    println!(
        "Showing {} of {} entries\n",
        audit_logs.entries.len(),
        audit_logs.total
    );

    for entry in &audit_logs.entries {
        println!(
            "ID: {} | Time: {} | Actor: {:?} | Action: {}",
            entry.id, entry.timestamp, entry.actor, entry.action
        );
        if let Some(ns) = &entry.namespace {
            println!("  Namespace: {}", ns);
        }
        if let Some(key) = &entry.key_name {
            println!("  Key: {}", key);
        }
        println!("  Success: {} | IP: {:?}", entry.success, entry.ip_address);
        if let Some(error) = &entry.error {
            println!("  Error: {}", error);
        }
        println!();
    }

    // Example 2: Query with filters
    println!("\n=== Filtered Audit Logs (Failed Operations) ===");
    let query = AuditQuery {
        success: Some(false),
        limit: Some(10),
        ..Default::default()
    };
    let failed_ops = client.audit(query).await?;

    println!("Failed operations: {}", failed_ops.total);
    for entry in &failed_ops.entries {
        println!(
            "Failed: {} - {} by {:?} - Error: {:?}",
            entry.action,
            entry.namespace.as_deref().unwrap_or("N/A"),
            entry.actor,
            entry.error
        );
    }

    // Example 3: Query by namespace and time range
    println!("\n=== Audit Logs for 'production' Namespace ===");
    let query = AuditQuery {
        namespace: Some("production".to_string()),
        from: Some("2024-01-01T00:00:00Z".to_string()),
        limit: Some(20),
        ..Default::default()
    };

    match client.audit(query).await {
        Ok(logs) => {
            println!("Found {} audit logs for production namespace", logs.total);
            for entry in &logs.entries {
                println!(
                    "{} - {} by {:?}",
                    entry.timestamp, entry.action, entry.actor
                );
            }
        }
        Err(e) => {
            println!("Query failed: {}", e);
        }
    }

    // Example 4: Query specific actions with pagination
    println!("\n=== Secret Write Operations (Paginated) ===");
    let mut offset = 0;
    let limit = 5;
    let mut total_shown = 0;

    loop {
        let query = AuditQuery {
            action: Some("put".to_string()),
            limit: Some(limit),
            offset: Some(offset),
            ..Default::default()
        };

        let page = client.audit(query).await?;

        if page.entries.is_empty() {
            break;
        }

        println!(
            "\nPage {} (showing {}-{} of {}):",
            offset / limit + 1,
            offset + 1,
            offset + page.entries.len(),
            page.total
        );

        for entry in &page.entries {
            println!(
                "  {} - {}:{} by {:?}",
                entry.timestamp,
                entry.namespace.as_deref().unwrap_or(""),
                entry.key_name.as_deref().unwrap_or(""),
                entry.actor
            );
            total_shown += 1;
        }

        if !page.has_more || total_shown >= 15 {
            // Show max 3 pages
            break;
        }

        offset += limit;
    }

    // Example 5: Query by actor
    println!("\n=== Audit Logs by Actor ===");
    let query = AuditQuery {
        actor: Some("admin".to_string()),
        limit: Some(10),
        ..Default::default()
    };

    let admin_logs = client.audit(query).await?;
    println!("Admin performed {} actions", admin_logs.total);

    // Group actions by type
    let mut action_counts = std::collections::HashMap::new();
    for entry in &admin_logs.entries {
        *action_counts.entry(entry.action.as_str()).or_insert(0) += 1;
    }

    println!("Action summary:");
    for (action, count) in action_counts {
        println!("  {}: {} times", action, count);
    }

    Ok(())
}
