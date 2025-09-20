//! Data models for the XJP Secret Store SDK
//!
//! This module contains all the data structures used for API requests and responses.
//! The models are designed to provide a safe and ergonomic interface while mapping
//! cleanly to the underlying API.
//!
//! # Key Types
//!
//! * [`Secret`] - The main type representing a secret with its value and metadata
//! * [`GetOpts`], [`PutOpts`], [`ListOpts`] - Options for various operations
//! * [`BatchOp`] - Batch operation definitions
//! * [`ExportFormat`] - Supported export formats for environment variables

use secrecy::SecretString;
use serde::{Deserialize, Serialize};

/// A secret value with metadata
///
/// This is the main type returned when retrieving secrets from the store.
/// The secret value itself is protected using [`SecretString`] to prevent
/// accidental exposure in logs or debug output.
///
/// # Example
///
/// ```no_run
/// # use secret_store_sdk::{Client, ClientBuilder, Auth};
/// # use secrecy::ExposeSecret;
/// # async fn example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
/// let secret = client.get_secret("prod", "api-key", Default::default()).await?;
///
/// // Access the protected value
/// let value = secret.value.expose_secret();
///
/// // Check metadata
/// if let Some(owner) = secret.metadata.get("owner") {
///     println!("Secret owned by: {}", owner);
/// }
///
/// // Use ETag for conditional requests
/// if let Some(etag) = &secret.etag {
///     println!("ETag: {}", etag);
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Secret {
    /// Namespace the secret belongs to
    pub namespace: String,
    /// Key name
    pub key: String,
    /// Secret value (protected)
    pub value: SecretString,
    /// Version number
    pub version: i32,
    /// Optional expiration time
    pub expires_at: Option<time::OffsetDateTime>,
    /// JSON metadata
    pub metadata: serde_json::Value,
    /// Last update time
    pub updated_at: time::OffsetDateTime,
    /// ETag from response header
    pub etag: Option<String>,
    /// Last-Modified from response header
    pub last_modified: Option<String>,
    /// Request ID from response header
    pub request_id: Option<String>,
}

/// Secret key info in list responses
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecretKeyInfo {
    /// Key name
    pub key: String,
    /// Version number (mapped from "ver" in API)
    #[serde(rename = "ver")]
    pub version: i32,
    /// Last update time
    pub updated_at: String,
    /// Optional KID
    pub kid: Option<String>,
}

/// Options for getting a secret
///
/// Controls caching behavior and conditional requests when retrieving secrets.
///
/// # Example
///
/// ```
/// use secret_store_sdk::GetOpts;
///
/// // Use defaults (cache enabled)
/// let opts = GetOpts::default();
///
/// // Disable cache for this request
/// let opts = GetOpts {
///     use_cache: false,
///     ..Default::default()
/// };
///
/// // Conditional request with ETag
/// let opts = GetOpts {
///     if_none_match: Some("\"123abc\"".to_string()),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone)]
pub struct GetOpts {
    /// Whether to use cache (default: true)
    pub use_cache: bool,
    /// If-None-Match header value for conditional requests
    pub if_none_match: Option<String>,
    /// If-Modified-Since header value for conditional requests
    pub if_modified_since: Option<String>,
}

impl Default for GetOpts {
    fn default() -> Self {
        Self {
            use_cache: true,
            if_none_match: None,
            if_modified_since: None,
        }
    }
}

/// Options for putting a secret
///
/// Allows setting TTL, metadata, and idempotency key when creating or updating secrets.
///
/// # Example
///
/// ```
/// use secret_store_sdk::PutOpts;
/// use serde_json::json;
///
/// // Simple put with defaults
/// let opts = PutOpts::default();
///
/// // Put with TTL and metadata
/// let opts = PutOpts {
///     ttl_seconds: Some(3600), // 1 hour
///     metadata: Some(json!({
///         "environment": "production",
///         "rotation_policy": "30d",
///         "owner": "backend-team"
///     })),
///     idempotency_key: Some("deploy-12345".to_string()),
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct PutOpts {
    /// TTL in seconds (secret will be auto-deleted after this time)
    pub ttl_seconds: Option<i64>,
    /// JSON metadata to attach to the secret
    pub metadata: Option<serde_json::Value>,
    /// Idempotency key to ensure exactly-once semantics
    pub idempotency_key: Option<String>,
}

/// Result of put operation
#[derive(Debug, Clone, Deserialize)]
pub struct PutResult {
    /// Success message
    pub message: String,
    /// Namespace
    pub namespace: String,
    /// Key
    pub key: String,
    /// Creation timestamp
    pub created_at: String,
    /// Request ID
    pub request_id: String,
}

/// Result of delete operation
#[derive(Debug, Clone)]
pub struct DeleteResult {
    /// Whether the secret was deleted
    pub deleted: bool,
    /// Request ID if available
    pub request_id: Option<String>,
}

/// Options for listing secrets
#[derive(Debug, Clone, Default)]
pub struct ListOpts {
    /// Key prefix to filter by
    pub prefix: Option<String>,
    /// Maximum number of results
    pub limit: Option<usize>,
}

/// Result of list operation
#[derive(Debug, Clone, Deserialize)]
pub struct ListSecretsResult {
    /// Namespace
    pub namespace: String,
    /// List of secrets
    pub secrets: Vec<SecretKeyInfo>,
    /// Total count
    pub total: usize,
    /// Limit used
    pub limit: usize,
    /// Whether there are more results
    pub has_more: bool,
    /// Request ID
    pub request_id: String,
}

/// Export format for batch operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// JSON format
    Json,
    /// .env file format
    Dotenv,
    /// Shell export format
    Shell,
    /// Docker compose format
    DockerCompose,
}

impl ExportFormat {
    /// Get the format string for API parameter
    pub fn as_str(&self) -> &'static str {
        match self {
            ExportFormat::Json => "json",
            ExportFormat::Dotenv => "dotenv",
            ExportFormat::Shell => "shell",
            ExportFormat::DockerCompose => "docker-compose",
        }
    }
}

/// Keys for batch get operation
#[derive(Debug, Clone)]
pub enum BatchKeys {
    /// Specific keys
    Keys(Vec<String>),
    /// All keys (wildcard)
    All,
}

// Implementation of batch and advanced operations types

/// Result of batch get operation
#[derive(Debug, Clone)]
pub enum BatchGetResult {
    /// JSON format with all secrets
    Json(BatchGetJsonResult),
    /// Text format (dotenv, shell, docker-compose)
    Text(String),
}

/// Batch get result in JSON format
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BatchGetJsonResult {
    /// Namespace
    pub namespace: String,
    /// Map of key to secret value
    pub secrets: std::collections::HashMap<String, String>,
    /// List of missing keys
    #[serde(default)]
    pub missing: Vec<String>,
    /// Total number of secrets
    pub total: usize,
    /// Request ID
    pub request_id: String,
}

/// Batch operation
#[derive(Debug, Clone, Serialize)]
pub struct BatchOp {
    /// Action type: "put" or "delete"
    pub action: String,
    /// Secret key
    pub key: String,
    /// Value (required for "put" action)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// TTL in seconds (optional for "put" action)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<i64>,
    /// Metadata (optional for "put" action)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl BatchOp {
    /// Create a put operation
    pub fn put(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            action: "put".to_string(),
            key: key.into(),
            value: Some(value.into()),
            ttl_seconds: None,
            metadata: None,
        }
    }

    /// Create a delete operation
    pub fn delete(key: impl Into<String>) -> Self {
        Self {
            action: "delete".to_string(),
            key: key.into(),
            value: None,
            ttl_seconds: None,
            metadata: None,
        }
    }

    /// Set TTL for a put operation
    pub fn with_ttl(mut self, ttl_seconds: i64) -> Self {
        self.ttl_seconds = Some(ttl_seconds);
        self
    }

    /// Set metadata for a put operation
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Result of batch operations
#[derive(Debug, Clone, Deserialize)]
pub struct BatchOperateResult {
    /// Namespace
    pub namespace: String,
    /// Results summary
    pub results: BatchResultSummary,
    /// Success rate
    pub success_rate: f64,
}

/// Batch results summary
#[derive(Debug, Clone, Deserialize)]
pub struct BatchResultSummary {
    /// Successful operations
    pub succeeded: Vec<BatchOperationResult>,
    /// Failed operations  
    pub failed: Vec<BatchOperationResult>,
    /// Total operations
    pub total: usize,
}

/// Individual operation result in batch
#[derive(Debug, Clone, Deserialize)]
pub struct BatchOperationResult {
    /// Key affected
    pub key: String,
    /// Action performed
    pub action: String,
    /// Whether the operation succeeded
    pub success: bool,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Environment export result
#[derive(Debug, Clone)]
pub enum EnvExport {
    /// JSON format
    Json(EnvJsonExport),
    /// Text format (dotenv, shell, docker-compose)
    Text(String),
}

/// Environment export in JSON format
#[derive(Debug, Clone, Deserialize)]
pub struct EnvJsonExport {
    /// Namespace
    pub namespace: String,
    /// Environment variables
    pub environment: std::collections::HashMap<String, String>,
    /// ETag
    pub etag: String,
    /// Total count
    pub total: usize,
    /// Request ID
    pub request_id: String,
}

/// List of namespaces
#[derive(Debug, Clone, Deserialize)]
pub struct ListNamespacesResult {
    /// List of namespaces
    pub namespaces: Vec<NamespaceListItem>,
    /// Total count
    pub total: usize,
    /// Request ID
    pub request_id: String,
}

/// Namespace list item
#[derive(Debug, Clone, Deserialize)]
pub struct NamespaceListItem {
    /// Namespace name
    pub name: String,
    /// Creation time
    pub created_at: String,
    /// Last updated time
    pub updated_at: String,
    /// Number of secrets
    pub secret_count: usize,
}

/// Namespace detailed information
#[derive(Debug, Clone, Deserialize)]
pub struct NamespaceInfo {
    /// Namespace name
    pub name: String,
    /// Creation time
    pub created_at: String,
    /// Last updated time
    pub updated_at: String,
    /// Number of secrets
    pub secret_count: usize,
    /// Total size in bytes
    pub total_size: usize,
    /// Metadata
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// Request ID
    pub request_id: String,
}

/// Namespace template for initialization
#[derive(Debug, Clone, Serialize, Default)]
pub struct NamespaceTemplate {
    /// Template name
    pub template: String,
    /// Additional parameters
    #[serde(flatten)]
    pub params: serde_json::Value,
}

/// Result of namespace initialization
#[derive(Debug, Clone, Deserialize)]
pub struct InitNamespaceResult {
    /// Success message
    pub message: String,
    /// Namespace
    pub namespace: String,
    /// Number of secrets created
    pub secrets_created: usize,
    /// Request ID
    pub request_id: String,
}

/// List of secret versions
#[derive(Debug, Clone, Deserialize)]
pub struct VersionList {
    /// Namespace
    pub namespace: String,
    /// Key
    pub key: String,
    /// List of versions
    pub versions: Vec<VersionInfo>,
    /// Total count
    pub total: usize,
    /// Request ID
    pub request_id: String,
}

/// Version information
#[derive(Debug, Clone, Deserialize)]
pub struct VersionInfo {
    /// Version number
    pub version: i32,
    /// Creation time
    pub created_at: String,
    /// Actor who created this version
    pub created_by: String,
    /// Comment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Whether this is the current version
    pub is_current: bool,
}

/// Result of rollback operation
#[derive(Debug, Clone, Deserialize)]
pub struct RollbackResult {
    /// Success message
    pub message: String,
    /// Namespace
    pub namespace: String,
    /// Key
    pub key: String,
    /// New version (after rollback)
    pub from_version: i32,
    /// Rolled back to version
    pub to_version: i32,
    /// Request ID
    pub request_id: String,
}

/// Audit query parameters
#[derive(Debug, Clone, Serialize, Default)]
pub struct AuditQuery {
    /// Filter by namespace
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Filter by actor
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// Filter by action
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Start time (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// End time (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// Filter by success/failure
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    /// Limit
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// Offset
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}

/// Audit log results
#[derive(Debug, Clone, Deserialize)]
pub struct AuditResult {
    /// List of audit entries (mapped from "logs" in API response)
    #[serde(rename = "logs")]
    pub entries: Vec<AuditEntry>,
    /// Total count (without limit)
    pub total: usize,
    /// Applied limit
    pub limit: usize,
    /// Applied offset
    pub offset: usize,
    /// Whether more results are available
    pub has_more: bool,
    /// Request ID
    pub request_id: String,
}

/// Audit log entry
#[derive(Debug, Clone, Deserialize)]
pub struct AuditEntry {
    /// Unique ID
    pub id: i64,
    /// Timestamp
    pub timestamp: String,
    /// Actor (user/service)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// Action performed
    pub action: String,
    /// Namespace
    #[serde(rename = "namespace", skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Key name
    #[serde(rename = "key_name", skip_serializing_if = "Option::is_none")]
    pub key_name: Option<String>,
    /// Whether the action succeeded
    #[serde(rename = "success")]
    pub success: bool,
    /// IP address
    #[serde(rename = "ip_address", skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    /// User agent
    #[serde(rename = "user_agent", skip_serializing_if = "Option::is_none")]  
    pub user_agent: Option<String>,
    /// Error message if failed
    #[serde(rename = "error", skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Service discovery information
#[derive(Debug, Clone, Deserialize)]
pub struct Discovery {
    /// Service name
    pub service: String,
    /// Service version
    pub version: String,
    /// API version
    pub api_version: String,
    /// Supported features
    pub features: Vec<String>,
    /// Build information
    pub build: BuildInfo,
    /// Endpoints
    pub endpoints: EndpointInfo,
}

/// Build information
#[derive(Debug, Clone, Deserialize)]
pub struct BuildInfo {
    /// Git commit hash
    pub commit: String,
    /// Build timestamp
    pub timestamp: String,
    /// Rust version
    pub rust_version: String,
}

/// Endpoint information
#[derive(Debug, Clone, Deserialize)]
pub struct EndpointInfo {
    /// Base URL
    pub base_url: String,
    /// Health check URL
    pub health_url: String,
    /// Metrics URL
    pub metrics_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_format() {
        assert_eq!(ExportFormat::Json.as_str(), "json");
        assert_eq!(ExportFormat::Dotenv.as_str(), "dotenv");
        assert_eq!(ExportFormat::Shell.as_str(), "shell");
        assert_eq!(ExportFormat::DockerCompose.as_str(), "docker-compose");
    }
}