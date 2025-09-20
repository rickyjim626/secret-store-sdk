//! XJP Secret Store Client Implementation
//!
//! This module contains the main `Client` struct that provides the core functionality
//! for interacting with the XJP Secret Store service.
//!
//! # Architecture
//!
//! The client is designed with the following key components:
//! - **HTTP Layer**: Built on `reqwest` for async HTTP operations
//! - **Caching Layer**: Uses `moka` for high-performance async caching with TTL support
//! - **Retry Logic**: Implements exponential backoff with jitter for transient failures
//! - **Authentication**: Supports multiple auth methods with automatic token refresh
//! - **Telemetry**: Optional OpenTelemetry integration for observability
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```no_run
//! use secret_store_sdk::{Client, ClientBuilder, Auth};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = ClientBuilder::new("https://secret.example.com")
//!     .auth(Auth::bearer("your-token"))
//!     .build()?;
//!
//! // Get a secret
//! let secret = client.get_secret("prod", "api-key", Default::default()).await?;
//! println!("Secret version: {}", secret.version);
//! # Ok(())
//! # }
//! ```
//!
//! ## With Caching and Retries
//!
//! ```no_run
//! use secret_store_sdk::{ClientBuilder, Auth};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = ClientBuilder::new("https://secret.example.com")
//!     .auth(Auth::bearer("your-token"))
//!     .enable_cache(true)
//!     .cache_ttl_secs(600) // 10 minute cache
//!     .retries(5) // Up to 5 retries
//!     .timeout_ms(30000) // 30 second timeout
//!     .build()?;
//! # Ok(())
//! # }
//! ```

use crate::{
    cache::{CacheStats, CachedSecret},
    config::ClientConfig,
    endpoints::Endpoints,
    errors::{Error, ErrorResponse, Result},
    models::*,
    util::{generate_request_id, header_str},
};

#[cfg(feature = "metrics")]
use crate::telemetry;
use backoff::{future::retry_notify, ExponentialBackoff};
use moka::future::Cache;
use reqwest::{Client as HttpClient, Method, Response, StatusCode};
use secrecy::SecretString;
use std::time::Duration;
use tracing::{debug, trace, warn};

const USER_AGENT_PREFIX: &str = "xjp-secret-store-sdk-rust";

/// XJP Secret Store client
///
/// The main client for interacting with the XJP Secret Store API.
/// Provides methods for managing secrets, including get, put, delete,
/// and batch operations. Supports caching, retries, and conditional requests.
#[derive(Clone)]
pub struct Client {
    pub(crate) config: ClientConfig,
    http: HttpClient,
    endpoints: Endpoints,
    cache: Option<Cache<String, CachedSecret>>,
    stats: CacheStats,
    #[cfg(feature = "metrics")]
    metrics: std::sync::Arc<telemetry::Metrics>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("base_url", &self.config.base_url)
            .field("timeout", &self.config.timeout)
            .field("retries", &self.config.retries)
            .field("cache_enabled", &self.config.cache_config.enabled)
            .finish()
    }
}

impl Client {
    /// Create a new client with the given configuration
    pub(crate) fn new(config: ClientConfig) -> Result<Self> {
        // Build user agent
        let user_agent = if let Some(suffix) = &config.user_agent_suffix {
            format!("{}/{} {}", USER_AGENT_PREFIX, crate::VERSION, suffix)
        } else {
            format!("{}/{}", USER_AGENT_PREFIX, crate::VERSION)
        };

        // Create HTTP client
        let mut http_builder = HttpClient::builder()
            .user_agent(user_agent)
            .timeout(config.timeout)
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(10)
            .http2_prior_knowledge();

        // Configure TLS
        #[cfg(not(feature = "danger-insecure-http"))]
        {
            http_builder = http_builder.https_only(true);
        }

        #[cfg(feature = "danger-insecure-http")]
        {
            if config.allow_insecure_http {
                http_builder = http_builder.danger_accept_invalid_certs(true);
            }
        }

        let http = http_builder
            .build()
            .map_err(|e| Error::Config(format!("Failed to build HTTP client: {}", e)))?;

        // Create cache if enabled
        let cache = if config.cache_config.enabled {
            Some(
                Cache::builder()
                    .max_capacity(config.cache_config.max_entries)
                    .time_to_live(Duration::from_secs(config.cache_config.default_ttl_secs))
                    .build(),
            )
        } else {
            None
        };

        // Initialize telemetry if enabled
        #[cfg(feature = "metrics")]
        let metrics = if config.telemetry_config.enabled {
            telemetry::init_telemetry(config.telemetry_config.clone())
        } else {
            std::sync::Arc::new(telemetry::Metrics::new(&config.telemetry_config))
        };

        Ok(Self {
            endpoints: Endpoints::new(&config.base_url),
            http,
            cache,
            stats: CacheStats::new(),
            #[cfg(feature = "metrics")]
            metrics,
            config,
        })
    }

    /// Get cache statistics
    ///
    /// Returns statistics about the cache including hit rate, number of hits/misses,
    /// and evictions. Useful for monitoring cache performance.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use secret_store_sdk::{Client, ClientBuilder, Auth};
    /// # async fn example(client: &Client) {
    /// let stats = client.cache_stats();
    /// println!("Cache hit rate: {:.2}%", stats.hit_rate());
    /// println!("Total hits: {}, misses: {}", stats.hits(), stats.misses());
    /// # }
    /// ```
    pub fn cache_stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Clear the cache
    ///
    /// Removes all entries from the cache and resets cache statistics.
    /// This is useful when you need to force fresh data retrieval.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use secret_store_sdk::{Client, ClientBuilder, Auth};
    /// # async fn example(client: &Client) {
    /// // Clear all cached secrets
    /// client.clear_cache();
    /// # }
    /// ```
    pub fn clear_cache(&self) {
        if let Some(cache) = &self.cache {
            cache.invalidate_all();
            self.stats.reset();
        }
    }

    /// Invalidate a specific cache entry
    ///
    /// Removes a single secret from the cache, forcing the next retrieval
    /// to fetch fresh data from the server.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The namespace of the secret
    /// * `key` - The key of the secret
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use secret_store_sdk::{Client, ClientBuilder, Auth};
    /// # async fn example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    /// // Invalidate a specific secret from cache
    /// client.invalidate_cache("production", "api-key").await;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn invalidate_cache(&self, namespace: &str, key: &str) {
        if let Some(cache) = &self.cache {
            let cache_key = format!("{}/{}", namespace, key);
            cache.invalidate(&cache_key).await;
        }
    }

    /// Get a secret from the store
    ///
    /// Retrieves a secret value from the specified namespace and key.
    /// Supports caching and conditional requests via ETags.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The namespace containing the secret
    /// * `key` - The key identifying the secret
    /// * `opts` - Options controlling cache usage and conditional requests
    ///
    /// # Returns
    ///
    /// The secret value with metadata on success, or an error if the secret
    /// doesn't exist or access is denied.
    ///
    /// # Errors
    ///
    /// * `Error::Http` with status 404 if the secret doesn't exist
    /// * `Error::Http` with status 403 if access is denied
    /// * `Error::Http` with status 401 if authentication fails
    /// * `Error::Network` for connection issues
    /// * `Error::Timeout` if the request times out
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use secret_store_sdk::{Client, ClientBuilder, Auth, GetOpts};
    /// # async fn example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    /// // Simple get with default options (cache enabled)
    /// let secret = client.get_secret("production", "database-url", GetOpts::default()).await?;
    /// println!("Secret version: {}", secret.version);
    ///
    /// // Get without cache
    /// let opts = GetOpts { use_cache: false, ..Default::default() };
    /// let fresh_secret = client.get_secret("production", "api-key", opts).await?;
    ///
    /// // Conditional get with ETag
    /// let opts = GetOpts {
    ///     if_none_match: Some(secret.etag.unwrap()),
    ///     ..Default::default()
    /// };
    /// match client.get_secret("production", "database-url", opts).await {
    ///     Ok(updated) => println!("Secret was updated"),
    ///     Err(e) if e.status_code() == Some(304) => println!("Not modified"),
    ///     Err(e) => return Err(e.into()),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_secret(&self, namespace: &str, key: &str, opts: GetOpts) -> Result<Secret> {
        let cache_key = format!("{}/{}", namespace, key);

        // Check cache if enabled and requested
        if opts.use_cache {
            if let Some(cached) = self.get_from_cache(&cache_key).await {
                return Ok(cached);
            }
        }

        // Build request
        let url = self.endpoints.get_secret(namespace, key);
        let mut request = self.build_request(Method::GET, &url)?;

        // Add conditional headers
        if let Some(etag) = &opts.if_none_match {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        if let Some(modified) = &opts.if_modified_since {
            request = request.header(reqwest::header::IF_MODIFIED_SINCE, modified);
        }

        // Execute with retry
        let response = self.execute_with_retry(request).await?;

        // Handle 304 Not Modified
        if response.status() == StatusCode::NOT_MODIFIED {
            // Try to return from cache if available
            if let Some(cached) = self.get_from_cache(&cache_key).await {
                return Ok(cached);
            }
            // If not in cache, this is an error
            return Err(Error::Other(
                "Server returned 304 but no cached entry found".to_string(),
            ));
        }

        // Parse response
        let secret = self.parse_get_response(response, namespace, key).await?;

        // Cache the secret if caching is enabled AND use_cache is true
        if self.config.cache_config.enabled && opts.use_cache {
            self.cache_secret(&cache_key, &secret).await;
        }

        Ok(secret)
    }

    /// Put a secret into the store
    ///
    /// Creates or updates a secret in the specified namespace.
    /// Automatically invalidates any cached value for this key.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The namespace to store the secret in
    /// * `key` - The key for the secret
    /// * `value` - The secret value (will be securely stored)
    /// * `opts` - Options including TTL, metadata, and idempotency key
    ///
    /// # Returns
    ///
    /// A `PutResult` containing the operation details and timestamp.
    ///
    /// # Security
    ///
    /// The secret value is transmitted over HTTPS and stored encrypted.
    /// The SDK uses the `secrecy` crate to prevent accidental exposure
    /// of secret values in logs or debug output.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use secret_store_sdk::{Client, ClientBuilder, Auth, PutOpts};
    /// # use serde_json::json;
    /// # async fn example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    /// // Simple put
    /// client.put_secret("production", "new-key", "secret-value", PutOpts::default()).await?;
    ///
    /// // Put with TTL and metadata
    /// let opts = PutOpts {
    ///     ttl_seconds: Some(3600), // Expires in 1 hour
    ///     metadata: Some(json!({
    ///         "owner": "backend-team",
    ///         "rotation_date": "2024-12-01"
    ///     })),
    ///     idempotency_key: Some("deploy-12345".to_string()),
    /// };
    /// client.put_secret("production", "api-key", "new-api-key", opts).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn put_secret(
        &self,
        namespace: &str,
        key: &str,
        value: impl Into<String>,
        opts: PutOpts,
    ) -> Result<PutResult> {
        // Invalidate cache for this key
        if let Some(cache) = &self.cache {
            let cache_key = format!("{}/{}", namespace, key);
            cache.invalidate(&cache_key).await;
        }

        // Build request body
        let mut body = serde_json::json!({
            "value": value.into(),
        });

        if let Some(ttl) = opts.ttl_seconds {
            body["ttl_seconds"] = serde_json::json!(ttl);
        }
        if let Some(metadata) = opts.metadata {
            body["metadata"] = metadata;
        }

        // Build request
        let url = self.endpoints.put_secret(namespace, key);
        let mut request = self.build_request(Method::PUT, &url)?;
        request = request.json(&body);

        // Add idempotency key if provided
        if let Some(idempotency_key) = &opts.idempotency_key {
            request = request.header("X-Idempotency-Key", idempotency_key);
        }

        // Execute with retry
        let response = self.execute_with_retry(request).await?;

        // Parse response
        self.parse_json_response(response).await
    }

    /// Delete a secret from the store
    pub async fn delete_secret(&self, namespace: &str, key: &str) -> Result<DeleteResult> {
        // Invalidate cache for this key
        if let Some(cache) = &self.cache {
            let cache_key = format!("{}/{}", namespace, key);
            cache.invalidate(&cache_key).await;
        }

        // Build request
        let url = self.endpoints.delete_secret(namespace, key);
        let request = self.build_request(Method::DELETE, &url)?;

        // Execute with retry
        let response = self.execute_with_retry(request).await?;
        let request_id = header_str(response.headers(), "x-request-id");

        // Check status
        let deleted = response.status() == StatusCode::NO_CONTENT;

        Ok(DeleteResult {
            deleted,
            request_id,
        })
    }

    /// List secrets in a namespace
    pub async fn list_secrets(&self, namespace: &str, opts: ListOpts) -> Result<ListSecretsResult> {
        // Build URL with query parameters
        let mut url = self.endpoints.list_secrets(namespace);

        let mut query_parts = Vec::new();
        if let Some(prefix) = &opts.prefix {
            query_parts.push(format!(
                "prefix={}",
                percent_encoding::utf8_percent_encode(prefix, percent_encoding::NON_ALPHANUMERIC)
            ));
        }
        if let Some(limit) = opts.limit {
            query_parts.push(format!("limit={}", limit));
        }

        if !query_parts.is_empty() {
            url.push('?');
            url.push_str(&query_parts.join("&"));
        }

        // Build and execute request
        let request = self.build_request(Method::GET, &url)?;
        let response = self.execute_with_retry(request).await?;

        // Parse response
        self.parse_json_response(response).await
    }

    /// Batch get secrets
    pub async fn batch_get(
        &self,
        namespace: &str,
        keys: BatchKeys,
        format: ExportFormat,
    ) -> Result<BatchGetResult> {
        let mut url = self.endpoints.batch_get(namespace);

        // Build query parameters
        match &keys {
            BatchKeys::Keys(key_list) => {
                let keys_param = key_list.join(",");
                url.push_str(&format!(
                    "?keys={}",
                    percent_encoding::utf8_percent_encode(
                        &keys_param,
                        percent_encoding::NON_ALPHANUMERIC
                    )
                ));
            }
            BatchKeys::All => {
                url.push_str("?wildcard=true");
            }
        }

        // Add format parameter
        let separator = if url.contains('?') { '&' } else { '?' };
        url.push_str(&format!("{}format={}", separator, format.as_str()));

        // Build and execute request
        let request = self.build_request(Method::GET, &url)?;
        let response = self.execute_with_retry(request).await?;

        // Check status
        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        // Parse response based on format
        match format {
            ExportFormat::Json => {
                let json_result: BatchGetJsonResult = response.json().await.map_err(Error::from)?;
                Ok(BatchGetResult::Json(json_result))
            }
            _ => {
                let text = response.text().await.map_err(Error::from)?;
                Ok(BatchGetResult::Text(text))
            }
        }
    }

    /// Batch operate on secrets
    pub async fn batch_operate(
        &self,
        namespace: &str,
        operations: Vec<BatchOp>,
        transactional: bool,
        idempotency_key: Option<String>,
    ) -> Result<BatchOperateResult> {
        // Invalidate cache for all affected keys
        if let Some(cache) = &self.cache {
            for op in &operations {
                let cache_key = format!("{}/{}", namespace, &op.key);
                cache.invalidate(&cache_key).await;
            }
        }

        // Build request body
        let body = serde_json::json!({
            "operations": operations,
            "transactional": transactional,
        });

        // Build request
        let url = self.endpoints.batch_operate(namespace);
        let mut request = self.build_request(Method::POST, &url)?;
        request = request.json(&body);

        // Add idempotency key if provided
        if let Some(key) = idempotency_key {
            request = request.header("Idempotency-Key", key);
        }

        // Execute with retry
        let response = self.execute_with_retry(request).await?;

        // Parse response
        self.parse_json_response(response).await
    }

    /// Export secrets as environment variables
    ///
    /// Exports all secrets from a namespace in the specified format.
    /// Supports conditional requests using ETag for efficient caching.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The namespace to export
    /// * `opts` - Export options including format and conditional request headers
    ///
    /// # Returns
    ///
    /// Returns `EnvExport::Json` for JSON format or `EnvExport::Text` for other formats.
    ///
    /// # Errors
    ///
    /// * Returns `Error::Http` with status 304 if content hasn't changed (when using if_none_match)
    /// * Returns other errors for authentication, network, or server issues
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use secret_store_sdk::{Client, ClientBuilder, Auth, ExportEnvOpts, ExportFormat};
    /// # async fn example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    /// // Simple export
    /// let opts = ExportEnvOpts {
    ///     format: ExportFormat::Dotenv,
    ///     ..Default::default()
    /// };
    /// let export = client.export_env("production", opts).await?;
    ///
    /// // Conditional export with ETag
    /// let opts = ExportEnvOpts {
    ///     format: ExportFormat::Json,
    ///     use_cache: true,
    ///     if_none_match: Some("previous-etag".to_string()),
    /// };
    /// match client.export_env("production", opts).await {
    ///     Ok(export) => println!("Content updated"),
    ///     Err(e) if e.status_code() == Some(304) => println!("Not modified"),
    ///     Err(e) => return Err(e.into()),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn export_env(&self, namespace: &str, opts: ExportEnvOpts) -> Result<EnvExport> {
        let mut url = self.endpoints.export_env(namespace);
        url.push_str(&format!("?format={}", opts.format.as_str()));

        // Build request
        let mut request = self.build_request(Method::GET, &url)?;

        // Add conditional header if provided
        if let Some(etag) = &opts.if_none_match {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }

        let response = self.execute_with_retry(request).await?;

        // Handle 304 Not Modified
        if response.status() == StatusCode::NOT_MODIFIED {
            return Err(Error::Http {
                status: 304,
                category: "not_modified".to_string(),
                message: "Environment export not modified".to_string(),
                request_id: header_str(response.headers(), "x-request-id"),
            });
        }

        // Check other error statuses
        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        // TODO: Implement caching if opts.use_cache is true
        // Cache key could be: namespace/env/{format}
        // Would need to extract ETag from response headers

        // Parse response based on format
        match opts.format {
            ExportFormat::Json => {
                let json_result: EnvJsonExport = response.json().await.map_err(Error::from)?;
                Ok(EnvExport::Json(json_result))
            }
            _ => {
                let text = response.text().await.map_err(Error::from)?;
                Ok(EnvExport::Text(text))
            }
        }
    }

    /// List all namespaces
    pub async fn list_namespaces(&self) -> Result<ListNamespacesResult> {
        let url = self.endpoints.list_namespaces();
        let request = self.build_request(Method::GET, &url)?;
        let response = self.execute_with_retry(request).await?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        self.parse_json_response(response).await
    }

    /// Get namespace information
    pub async fn get_namespace(&self, namespace: &str) -> Result<NamespaceInfo> {
        let url = self.endpoints.get_namespace(namespace);
        let request = self.build_request(Method::GET, &url)?;
        let response = self.execute_with_retry(request).await?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        self.parse_json_response(response).await
    }

    /// Initialize a namespace with a template
    ///
    /// Initializes a new namespace using a predefined template to create
    /// a set of initial secrets.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The namespace to initialize
    /// * `template` - The template configuration
    /// * `idempotency_key` - Optional idempotency key to prevent duplicate initialization
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use secret_store_sdk::{Client, ClientBuilder, Auth, NamespaceTemplate};
    /// # use serde_json::json;
    /// # async fn example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let template = NamespaceTemplate {
    ///     template: "web-app".to_string(),
    ///     params: json!({
    ///         "environment": "staging",
    ///         "region": "us-west-2"
    ///     }),
    /// };
    ///
    /// let result = client.init_namespace(
    ///     "staging-app",
    ///     template,
    ///     Some("init-staging-12345".to_string())
    /// ).await?;
    /// println!("Created {} secrets", result.secrets_created);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn init_namespace(
        &self,
        namespace: &str,
        template: NamespaceTemplate,
        idempotency_key: Option<String>,
    ) -> Result<InitNamespaceResult> {
        let url = self.endpoints.init_namespace(namespace);
        let mut request = self.build_request(Method::POST, &url)?;
        request = request.json(&template);

        // Add idempotency key if provided
        if let Some(key) = idempotency_key {
            request = request.header("X-Idempotency-Key", key);
        }

        let response = self.execute_with_retry(request).await?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        self.parse_json_response(response).await
    }

    /// Delete a namespace and all its secrets
    ///
    /// **Warning**: This operation is irreversible and will delete all secrets
    /// in the namespace. Use with extreme caution.
    ///
    /// This operation may take some time for namespaces with many secrets.
    /// The response includes the number of secrets that were deleted.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The namespace to delete
    ///
    /// # Returns
    ///
    /// A `DeleteNamespaceResult` containing deletion details.
    ///
    /// # Errors
    ///
    /// * `Error::Http` with status 404 if the namespace doesn't exist
    /// * `Error::Http` with status 403 if deletion is forbidden
    /// * `Error::Http` with status 409 if namespace has protection enabled
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use secret_store_sdk::{Client, ClientBuilder, Auth};
    /// # async fn example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let result = client.delete_namespace("test-namespace").await?;
    /// println!("Deleted {} secrets from namespace {}",
    ///     result.secrets_deleted,
    ///     result.namespace
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_namespace(&self, namespace: &str) -> Result<DeleteNamespaceResult> {
        // Clear all cached entries for this namespace
        if let Some(cache) = &self.cache {
            // TODO: Optimize to only clear entries for this specific namespace
            // For now, we'll invalidate all cache to ensure consistency
            cache.invalidate_all();
            debug!(
                "Cleared all cache entries due to namespace deletion: {}",
                namespace
            );
        }

        // Build request
        let url = self.endpoints.delete_namespace(namespace);
        let request = self.build_request(Method::DELETE, &url)?;

        // Execute with retry
        let response = self.execute_with_retry(request).await?;

        // Check status
        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        // Extract request ID from headers
        let request_id = header_str(response.headers(), "x-request-id");

        // Parse response
        let mut result: DeleteNamespaceResult = self.parse_json_response(response).await?;

        // Set request_id if not already in the response body
        if result.request_id.is_none() {
            result.request_id = request_id;
        }

        Ok(result)
    }

    /// Delete a namespace and all its secrets with idempotency support
    ///
    /// Same as `delete_namespace` but with idempotency key support for safe retries.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The namespace to delete
    /// * `idempotency_key` - Optional idempotency key to prevent duplicate deletion
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use secret_store_sdk::{Client, ClientBuilder, Auth};
    /// # async fn example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let result = client.delete_namespace_idempotent(
    ///     "test-namespace",
    ///     Some("delete-ns-12345".to_string())
    /// ).await?;
    /// println!("Deleted {} secrets", result.secrets_deleted);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_namespace_idempotent(
        &self,
        namespace: &str,
        idempotency_key: Option<String>,
    ) -> Result<DeleteNamespaceResult> {
        // Clear all cached entries for this namespace
        if let Some(cache) = &self.cache {
            cache.invalidate_all();
            debug!(
                "Cleared all cache entries due to namespace deletion: {}",
                namespace
            );
        }

        // Build request
        let url = self.endpoints.delete_namespace(namespace);
        let mut request = self.build_request(Method::DELETE, &url)?;

        // Add idempotency key if provided
        if let Some(key) = idempotency_key {
            request = request.header("X-Idempotency-Key", key);
        }

        // Execute with retry
        let response = self.execute_with_retry(request).await?;

        // Check status
        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        // Extract request ID from headers
        let request_id = header_str(response.headers(), "x-request-id");

        // Parse response
        let mut result: DeleteNamespaceResult = self.parse_json_response(response).await?;

        // Set request_id if not already in the response body
        if result.request_id.is_none() {
            result.request_id = request_id;
        }

        Ok(result)
    }

    /// List versions of a secret
    pub async fn list_versions(&self, namespace: &str, key: &str) -> Result<VersionList> {
        // Build and execute request
        let url = self.endpoints.list_versions(namespace, key);
        let request = self.build_request(Method::GET, &url)?;
        let response = self.execute_with_retry(request).await?;

        // Parse response
        self.parse_json_response(response).await
    }

    /// Get a specific version of a secret
    pub async fn get_version(&self, namespace: &str, key: &str, version: i32) -> Result<Secret> {
        // Build and execute request
        let url = self.endpoints.get_version(namespace, key, version);
        let request = self.build_request(Method::GET, &url)?;
        let response = self.execute_with_retry(request).await?;

        // Parse response (similar to get_secret)
        self.parse_get_response(response, namespace, key).await
    }

    /// Rollback a secret to a previous version
    pub async fn rollback(
        &self,
        namespace: &str,
        key: &str,
        version: i32,
    ) -> Result<RollbackResult> {
        // Invalidate cache for this key since we're changing it
        if let Some(cache) = &self.cache {
            let cache_key = format!("{}/{}", namespace, key);
            cache.invalidate(&cache_key).await;
        }

        // Build request with empty body (comment is optional)
        let url = self.endpoints.rollback(namespace, key, version);
        let mut request = self.build_request(Method::POST, &url)?;
        request = request.json(&serde_json::json!({}));

        // Execute with retry
        let response = self.execute_with_retry(request).await?;

        // Parse response
        self.parse_json_response(response).await
    }

    /// Query audit logs
    pub async fn audit(&self, query: AuditQuery) -> Result<AuditResult> {
        // Build URL with query parameters
        let mut url = self.endpoints.audit();
        let mut params = Vec::new();

        // Add query parameters
        if let Some(namespace) = &query.namespace {
            params.push(format!(
                "namespace={}",
                percent_encoding::utf8_percent_encode(
                    namespace,
                    percent_encoding::NON_ALPHANUMERIC
                )
            ));
        }
        if let Some(actor) = &query.actor {
            params.push(format!(
                "actor={}",
                percent_encoding::utf8_percent_encode(actor, percent_encoding::NON_ALPHANUMERIC)
            ));
        }
        if let Some(action) = &query.action {
            params.push(format!(
                "action={}",
                percent_encoding::utf8_percent_encode(action, percent_encoding::NON_ALPHANUMERIC)
            ));
        }
        if let Some(from) = &query.from {
            params.push(format!(
                "from={}",
                percent_encoding::utf8_percent_encode(from, percent_encoding::NON_ALPHANUMERIC)
            ));
        }
        if let Some(to) = &query.to {
            params.push(format!(
                "to={}",
                percent_encoding::utf8_percent_encode(to, percent_encoding::NON_ALPHANUMERIC)
            ));
        }
        if let Some(success) = query.success {
            params.push(format!("success={}", success));
        }
        if let Some(limit) = query.limit {
            params.push(format!("limit={}", limit));
        }
        if let Some(offset) = query.offset {
            params.push(format!("offset={}", offset));
        }

        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        // Build and execute request
        let request = self.build_request(Method::GET, &url)?;
        let response = self.execute_with_retry(request).await?;

        // Parse response
        self.parse_json_response(response).await
    }

    /// List all API keys
    ///
    /// Retrieves a list of all API keys associated with the current account.
    /// The response includes metadata about each key but not the key values themselves.
    ///
    /// # Returns
    ///
    /// A `ListApiKeysResult` containing the list of API keys and total count.
    ///
    /// # Errors
    ///
    /// * `Error::Http` with status 403 if not authorized to list keys
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use secret_store_sdk::{Client, ClientBuilder, Auth};
    /// # async fn example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let keys = client.list_api_keys().await?;
    /// for key in &keys.keys {
    ///     println!("Key {}: {} (active: {})", key.id, key.name, key.active);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_api_keys(&self) -> Result<ListApiKeysResult> {
        let url = self.endpoints.list_api_keys();
        let request = self.build_request(Method::GET, &url)?;
        let response = self.execute_with_retry(request).await?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        let request_id = header_str(response.headers(), "x-request-id");
        let mut result: ListApiKeysResult = self.parse_json_response(response).await?;

        if result.request_id.is_none() {
            result.request_id = request_id;
        }

        Ok(result)
    }

    /// Create a new API key
    ///
    /// Creates a new API key with the specified permissions and restrictions.
    /// The key value is only returned in the creation response and cannot be retrieved later.
    ///
    /// # Arguments
    ///
    /// * `request` - The API key creation request containing name, permissions, etc.
    /// * `idempotency_key` - Optional idempotency key to prevent duplicate creation
    ///
    /// # Returns
    ///
    /// An `ApiKeyInfo` containing the newly created key details including the key value.
    ///
    /// # Security
    ///
    /// The returned API key value should be stored securely. It cannot be retrieved
    /// again after this call.
    ///
    /// # Errors
    ///
    /// * `Error::Http` with status 403 if not authorized to create keys
    /// * `Error::Http` with status 400 for invalid permissions or parameters
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use secret_store_sdk::{Client, ClientBuilder, Auth, CreateApiKeyRequest};
    /// # use secrecy::ExposeSecret;
    /// # async fn example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let request = CreateApiKeyRequest {
    ///     name: "CI/CD Pipeline Key".to_string(),
    ///     expires_at: Some("2024-12-31T23:59:59Z".to_string()),
    ///     namespaces: vec!["production".to_string()],
    ///     permissions: vec!["read".to_string()],
    ///     metadata: None,
    /// };
    ///
    /// let key_info = client.create_api_key(request, Some("unique-key-123".to_string())).await?;
    /// if let Some(key) = &key_info.key {
    ///     println!("New API key: {}", key.expose_secret());
    ///     // Store this securely - it won't be available again!
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_api_key(
        &self,
        request: CreateApiKeyRequest,
        idempotency_key: Option<String>,
    ) -> Result<ApiKeyInfo> {
        let url = self.endpoints.create_api_key();
        let mut req = self.build_request(Method::POST, &url)?;
        req = req.json(&request);

        // Add idempotency key if provided
        if let Some(key) = idempotency_key {
            req = req.header("X-Idempotency-Key", key);
        }

        let response = self.execute_with_retry(req).await?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        self.parse_json_response(response).await
    }

    /// Get API key details
    ///
    /// Retrieves detailed information about a specific API key.
    /// Note that the key value itself is never returned for security reasons.
    ///
    /// # Arguments
    ///
    /// * `key_id` - The ID of the API key to retrieve
    ///
    /// # Returns
    ///
    /// An `ApiKeyInfo` with the key's metadata (without the key value).
    ///
    /// # Errors
    ///
    /// * `Error::Http` with status 404 if the key doesn't exist
    /// * `Error::Http` with status 403 if not authorized to view the key
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use secret_store_sdk::{Client, ClientBuilder, Auth};
    /// # async fn example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let key_info = client.get_api_key("key_123abc").await?;
    /// println!("Key {} last used: {:?}", key_info.name, key_info.last_used_at);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_api_key(&self, key_id: &str) -> Result<ApiKeyInfo> {
        let url = self.endpoints.get_api_key(key_id);
        let request = self.build_request(Method::GET, &url)?;
        let response = self.execute_with_retry(request).await?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        self.parse_json_response(response).await
    }

    /// Revoke an API key
    ///
    /// Revokes an API key, immediately invalidating it for future use.
    /// This operation is irreversible.
    ///
    /// # Arguments
    ///
    /// * `key_id` - The ID of the API key to revoke
    ///
    /// # Returns
    ///
    /// A `RevokeApiKeyResult` confirming the revocation.
    ///
    /// # Errors
    ///
    /// * `Error::Http` with status 404 if the key doesn't exist
    /// * `Error::Http` with status 403 if not authorized to revoke the key
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use secret_store_sdk::{Client, ClientBuilder, Auth};
    /// # async fn example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let result = client.revoke_api_key("key_123abc").await?;
    /// println!("Revoked key: {}", result.key_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn revoke_api_key(&self, key_id: &str) -> Result<RevokeApiKeyResult> {
        let url = self.endpoints.revoke_api_key(key_id);
        let request = self.build_request(Method::DELETE, &url)?;
        let response = self.execute_with_retry(request).await?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        let request_id = header_str(response.headers(), "x-request-id");
        let mut result: RevokeApiKeyResult = self.parse_json_response(response).await?;

        if result.request_id.is_none() {
            result.request_id = request_id;
        }

        Ok(result)
    }

    /// Get API discovery information
    pub async fn discovery(&self) -> Result<Discovery> {
        let url = self.endpoints.discovery();
        let request = self.build_request(Method::GET, &url)?;
        let response = self.execute_with_retry(request).await?;

        if !response.status().is_success() {
            return Err(self.parse_error_response(response).await);
        }

        self.parse_json_response(response).await
    }

    /// Check liveness
    ///
    /// Performs a simple liveness check against the service.
    /// Returns `Ok(())` if the service is alive and responding.
    ///
    /// This endpoint is typically used by Kubernetes liveness probes.
    /// It does not check dependencies and should respond quickly.
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not responding or returns
    /// a non-2xx status code.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use secret_store_sdk::{Client, ClientBuilder, Auth};
    /// # async fn example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    /// match client.livez().await {
    ///     Ok(()) => println!("Service is alive"),
    ///     Err(e) => eprintln!("Service is down: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn livez(&self) -> Result<()> {
        let url = self.endpoints.livez();
        let request = self.build_request(Method::GET, &url)?;

        // Execute without retry for health checks
        let response = self.execute_without_retry(request).await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(self.parse_error_response(response).await)
        }
    }

    /// Check readiness with detailed status
    ///
    /// Performs a comprehensive readiness check that may include
    /// checking dependencies (database, cache, etc.).
    ///
    /// This endpoint is typically used by Kubernetes readiness probes
    /// to determine if the service is ready to accept traffic.
    ///
    /// # Returns
    ///
    /// Returns a `HealthStatus` with details about the service health
    /// including individual component checks.
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not ready or if the
    /// request fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use secret_store_sdk::{Client, ClientBuilder, Auth};
    /// # async fn example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let health = client.readyz().await?;
    /// println!("Service status: {}", health.status);
    ///
    /// for (check, result) in &health.checks {
    ///     println!("  {}: {} ({}ms)",
    ///         check,
    ///         result.status,
    ///         result.duration_ms.unwrap_or(0)
    ///     );
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn readyz(&self) -> Result<HealthStatus> {
        let url = self.endpoints.readyz();
        let request = self.build_request(Method::GET, &url)?;

        // Execute without retry for health checks
        let response = self.execute_without_retry(request).await?;

        if response.status().is_success() {
            self.parse_json_response(response).await
        } else {
            Err(self.parse_error_response(response).await)
        }
    }

    /// Get service metrics
    ///
    /// Retrieves metrics from the service in Prometheus format.
    /// This endpoint may require special authentication using a metrics token.
    ///
    /// # Arguments
    ///
    /// * `metrics_token` - Optional metrics-specific authentication token.
    ///   If not provided, uses the client's default authentication.
    ///
    /// # Returns
    ///
    /// Returns the metrics as a raw string in Prometheus exposition format.
    ///
    /// # Errors
    ///
    /// * `Error::Http` with status 401 if authentication fails
    /// * `Error::Http` with status 403 if not authorized to view metrics
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use secret_store_sdk::{Client, ClientBuilder, Auth};
    /// # async fn example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    /// // Using default authentication
    /// let metrics = client.metrics(None).await?;
    /// println!("Metrics:\n{}", metrics);
    ///
    /// // Using specific metrics token
    /// let metrics = client.metrics(Some("metrics-token-xyz")).await?;
    /// println!("Metrics with token:\n{}", metrics);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn metrics(&self, metrics_token: Option<&str>) -> Result<String> {
        let url = self.endpoints.metrics();
        let mut request = self.build_request(Method::GET, &url)?;

        // Add metrics-specific token if provided
        if let Some(token) = metrics_token {
            request = request.header("X-Metrics-Token", token);
        }

        // Execute without retry for metrics endpoint
        let response = self.execute_without_retry(request).await?;

        if response.status().is_success() {
            response.text().await.map_err(Error::from)
        } else {
            Err(self.parse_error_response(response).await)
        }
    }

    // Helper methods

    /// Build a request with common headers
    fn build_request(&self, method: Method, url: &str) -> Result<reqwest::RequestBuilder> {
        let mut builder = self.http.request(method, url);

        // Generate and add request ID
        let request_id = generate_request_id();
        builder = builder.header("X-Request-ID", &request_id);

        // Add trace headers
        builder = builder
            .header("X-Trace-ID", &request_id)
            .header("X-Span-ID", uuid::Uuid::new_v4().to_string());

        Ok(builder)
    }

    /// Execute a request with retry logic
    async fn execute_with_retry(
        &self,
        request_builder: reqwest::RequestBuilder,
    ) -> Result<Response> {
        let mut token_refresh_count = 0;
        let max_retries = self.config.retries;
        let auth = &self.config.auth;

        // Extract method and URL for metrics
        #[cfg(feature = "metrics")]
        let (method, path) = {
            // Try to build a request to extract metadata
            if let Ok(req) = request_builder.try_clone().unwrap().build() {
                let method = req.method().to_string();
                let path = req.url().path().to_string();
                (method, path)
            } else {
                ("UNKNOWN".to_string(), "UNKNOWN".to_string())
            }
        };

        loop {
            // Get current auth header (may be refreshed)
            let (auth_header, auth_value) = auth
                .get_header()
                .await
                .map_err(|e| Error::Config(format!("Failed to get auth header: {}", e)))?;

            // Clone the base request and add current auth header
            let req_with_auth = request_builder
                .try_clone()
                .ok_or_else(|| Error::Other("Request cannot be cloned".to_string()))?
                .header(auth_header, auth_value);

            // Create backoff strategy for retries
            let mut backoff = ExponentialBackoff {
                initial_interval: Duration::from_millis(100),
                randomization_factor: 0.3,
                multiplier: 2.0,
                max_interval: Duration::from_secs(10),
                max_elapsed_time: None,
                ..Default::default()
            };
            backoff.max_elapsed_time = if max_retries > 0 {
                Some(Duration::from_secs(60))
            } else {
                Some(Duration::from_millis(0))
            };

            let retry_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let retry_count_clone = retry_count.clone();

            // Execute with backoff retry
            let result = retry_notify(
                backoff,
                || async {
                    let current_retry = retry_count.load(std::sync::atomic::Ordering::Relaxed);
                    // Clone request for this attempt
                    let req = req_with_auth
                        .try_clone()
                        .ok_or_else(|| {
                            backoff::Error::Permanent(Error::Other(
                                "Request cannot be cloned".to_string(),
                            ))
                        })?
                        .build()
                        .map_err(|e| {
                            backoff::Error::Permanent(Error::Other(format!(
                                "Failed to build request: {}",
                                e
                            )))
                        })?;

                    // Track active connections
                    #[cfg(feature = "metrics")]
                    self.metrics.inc_active_connections();

                    // Start timing request
                    #[cfg(feature = "metrics")]
                    let start_time = std::time::Instant::now();

                    let response_result = self.http.execute(req).await;

                    // Decrement active connections
                    #[cfg(feature = "metrics")]
                    self.metrics.dec_active_connections();

                    match response_result {
                        Ok(response) => {
                            let status = response.status();

                            // Handle 401 - but don't retry within backoff if we can refresh token
                            if status == StatusCode::UNAUTHORIZED
                                && token_refresh_count == 0
                                && auth.supports_refresh()
                            {
                                // Return a special error that we'll handle outside the backoff retry
                                return Err(backoff::Error::Permanent(Error::Http {
                                    status: 401,
                                    category: "auth_refresh_needed".to_string(),
                                    message: "Token refresh required".to_string(),
                                    request_id: header_str(response.headers(), "x-request-id"),
                                }));
                            }

                            // Check if error is retryable
                            if status.is_server_error()
                                || status == StatusCode::TOO_MANY_REQUESTS
                                || status == StatusCode::REQUEST_TIMEOUT
                            {
                                let error = self.parse_error_response(response).await;
                                if error.is_retryable() && current_retry < max_retries as usize {
                                    debug!("Retrying request due to: {:?}", error);
                                    #[cfg(feature = "metrics")]
                                    self.metrics.record_retry(
                                        (current_retry + 1) as u32,
                                        &status.to_string(),
                                    );
                                    return Err(backoff::Error::transient(error));
                                } else {
                                    return Err(backoff::Error::Permanent(error));
                                }
                            }

                            // Non-retryable HTTP errors
                            if !status.is_success() && status != StatusCode::NOT_MODIFIED {
                                let error = self.parse_error_response(response).await;
                                return Err(backoff::Error::Permanent(error));
                            }

                            // Record successful request metrics
                            #[cfg(feature = "metrics")]
                            {
                                let duration_secs = start_time.elapsed().as_secs_f64();
                                self.metrics.record_request(
                                    &method,
                                    &path,
                                    status.as_u16(),
                                    duration_secs,
                                );
                            }

                            Ok(response)
                        }
                        Err(e) => {
                            let error = Error::from(e);
                            if error.is_retryable() && current_retry < max_retries as usize {
                                debug!("Retrying request due to network error: {:?}", error);
                                #[cfg(feature = "metrics")]
                                self.metrics
                                    .record_retry((current_retry + 1) as u32, "network_error");
                                Err(backoff::Error::transient(error))
                            } else {
                                Err(backoff::Error::Permanent(error))
                            }
                        }
                    }
                },
                |err, dur| {
                    let count =
                        retry_count_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    debug!("Retry {} after {:?} due to: {:?}", count, dur, err);
                },
            )
            .await;

            match result {
                Ok(response) => return Ok(response),
                Err(Error::Http {
                    status: 401,
                    category,
                    ..
                }) if category == "auth_refresh_needed" && token_refresh_count == 0 => {
                    // Try to refresh token once
                    warn!("Got 401, attempting token refresh");
                    auth.refresh()
                        .await
                        .map_err(|e| Error::Config(format!("Token refresh failed: {}", e)))?;
                    token_refresh_count += 1;
                    // Continue to retry with new token
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Execute a request without retry logic (for health checks)
    async fn execute_without_retry(
        &self,
        request_builder: reqwest::RequestBuilder,
    ) -> Result<Response> {
        // Get auth header
        let (auth_header, auth_value) = self
            .config
            .auth
            .get_header()
            .await
            .map_err(|e| Error::Config(format!("Failed to get auth header: {}", e)))?;

        // Add auth header
        let request = request_builder
            .header(auth_header, auth_value)
            .build()
            .map_err(|e| Error::Other(format!("Failed to build request: {}", e)))?;

        // Execute request
        self.http.execute(request).await.map_err(Error::from)
    }

    /// Parse error response from server
    async fn parse_error_response(&self, response: Response) -> Error {
        let status = response.status().as_u16();
        let request_id = header_str(response.headers(), "x-request-id");

        // Try to parse JSON error response
        match response.json::<ErrorResponse>().await {
            Ok(error_resp) => Error::from_response(
                error_resp.status,
                &error_resp.error,
                &error_resp.message,
                request_id,
            ),
            Err(_) => Error::Http {
                status,
                category: "unknown".to_string(),
                message: format!("HTTP error {}", status),
                request_id,
            },
        }
    }

    /// Parse JSON response
    async fn parse_json_response<T: serde::de::DeserializeOwned>(
        &self,
        response: Response,
    ) -> Result<T> {
        response.json().await.map_err(Error::from)
    }

    /// Parse get secret response
    async fn parse_get_response(
        &self,
        response: Response,
        namespace: &str,
        key: &str,
    ) -> Result<Secret> {
        let headers = response.headers().clone();

        // Extract headers
        let etag = header_str(&headers, "etag");
        let last_modified = header_str(&headers, "last-modified");
        let request_id = header_str(&headers, "x-request-id");

        // Parse body
        #[derive(serde::Deserialize)]
        struct GetResponse {
            value: String,
            version: i32,
            expires_at: Option<String>,
            metadata: Option<serde_json::Value>,
            updated_at: String,
        }

        let body: GetResponse = response.json().await.map_err(Error::from)?;

        // Parse timestamps
        let updated_at = time::OffsetDateTime::parse(
            &body.updated_at,
            &time::format_description::well_known::Rfc3339,
        )
        .map_err(|e| Error::Deserialize(format!("Invalid updated_at timestamp: {}", e)))?;

        let expires_at = body
            .expires_at
            .as_ref()
            .map(|s| {
                time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
                    .map_err(|e| Error::Deserialize(format!("Invalid expires_at timestamp: {}", e)))
            })
            .transpose()?;

        Ok(Secret {
            namespace: namespace.to_string(),
            key: key.to_string(),
            value: SecretString::new(body.value),
            version: body.version,
            expires_at,
            metadata: body.metadata.unwrap_or(serde_json::Value::Null),
            updated_at,
            etag,
            last_modified,
            request_id,
        })
    }

    /// Get secret from cache
    async fn get_from_cache(&self, cache_key: &str) -> Option<Secret> {
        let cache = self.cache.as_ref()?;

        match cache.get(cache_key).await {
            Some(cached) => {
                // Check if expired
                if cached.is_expired() {
                    trace!("Cache entry expired for key: {}", cache_key);
                    cache.invalidate(cache_key).await;
                    self.stats.record_expiration();
                    self.stats.record_miss();
                    None
                } else {
                    debug!("Cache hit for key: {}", cache_key);
                    self.stats.record_hit();

                    // Record cache hit metric
                    #[cfg(feature = "metrics")]
                    {
                        let (namespace, _) = cache_key.split_once('/').unwrap_or(("", cache_key));
                        self.metrics.record_cache_hit(namespace);
                    }

                    let (namespace, key) = cache_key.split_once('/').unwrap_or(("", cache_key));
                    Some(cached.into_secret(namespace.to_string(), key.to_string()))
                }
            }
            None => {
                trace!("Cache miss for key: {}", cache_key);
                self.stats.record_miss();

                // Record cache miss metric
                #[cfg(feature = "metrics")]
                {
                    let (namespace, _) = cache_key.split_once('/').unwrap_or(("", cache_key));
                    self.metrics.record_cache_miss(namespace);
                }

                None
            }
        }
    }

    /// Cache a secret
    async fn cache_secret(&self, cache_key: &str, secret: &Secret) {
        let Some(cache) = &self.cache else { return };

        // Determine TTL from Cache-Control or use default
        let ttl = if let Some(_etag) = &secret.etag {
            // If we have an etag, use a longer TTL since we can validate
            Duration::from_secs(self.config.cache_config.default_ttl_secs * 2)
        } else {
            Duration::from_secs(self.config.cache_config.default_ttl_secs)
        };

        let cache_expires_at = time::OffsetDateTime::now_utc() + ttl;

        let cached = CachedSecret {
            value: secret.value.clone(),
            version: secret.version,
            expires_at: secret.expires_at,
            metadata: secret.metadata.clone(),
            updated_at: secret.updated_at,
            etag: secret.etag.clone(),
            last_modified: secret.last_modified.clone(),
            cache_expires_at,
        };

        cache.insert(cache_key.to_string(), cached).await;
        self.stats.record_insertion();
        debug!("Cached secret for key: {} with TTL: {:?}", cache_key, ttl);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{auth::Auth, ClientBuilder};
    use secrecy::ExposeSecret;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Helper function to create test client that works with HTTP URLs
    fn create_test_client(base_url: &str) -> Client {
        #[cfg(feature = "danger-insecure-http")]
        {
            ClientBuilder::new(base_url)
                .auth(Auth::bearer("test-token"))
                .allow_insecure_http()
                .build()
                .unwrap()
        }
        #[cfg(not(feature = "danger-insecure-http"))]
        {
            // In tests without the feature, we'll just use a dummy HTTPS URL
            // The actual URL doesn't matter since we're mocking
            ClientBuilder::new(&base_url.replace("http://", "https://"))
                .auth(Auth::bearer("test-token"))
                .build()
                .unwrap()
        }
    }

    #[test]
    fn test_client_creation() {
        let client = ClientBuilder::new("https://example.com")
            .auth(Auth::bearer("test-token"))
            .build();
        assert!(client.is_ok());
    }

    #[test]
    fn test_cache_key_format() {
        let cache_key = format!("{}/{}", "namespace", "key");
        assert_eq!(cache_key, "namespace/key");
    }

    #[tokio::test]
    async fn test_get_secret_success() {
        let mock_server = MockServer::start().await;

        // Mock successful response
        let response_body = serde_json::json!({
            "value": "secret-value",
            "version": 1,
            "expires_at": null,
            "metadata": {"env": "prod"},
            "updated_at": "2024-01-01T00:00:00Z"
        });

        Mock::given(method("GET"))
            .and(path("/api/v2/secrets/test-namespace/test-key"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(&response_body)
                    .insert_header("etag", "\"abc123\"")
                    .insert_header("x-request-id", "req-123"),
            )
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());

        let result = client
            .get_secret("test-namespace", "test-key", GetOpts::default())
            .await;
        if let Err(ref e) = result {
            eprintln!("Error: {:?}", e);
        }
        assert!(result.is_ok());

        let secret = result.unwrap();
        assert_eq!(secret.namespace, "test-namespace");
        assert_eq!(secret.key, "test-key");
        assert_eq!(secret.version, 1);
        assert_eq!(secret.etag, Some("\"abc123\"".to_string()));
    }

    #[tokio::test]
    async fn test_get_secret_404() {
        let mock_server = MockServer::start().await;

        let error_body = serde_json::json!({
            "error": "not_found",
            "message": "Secret not found",
            "timestamp": "2024-01-01T00:00:00Z",
            "status": 404
        });

        Mock::given(method("GET"))
            .and(path("/api/v2/secrets/test-namespace/missing-key"))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(&error_body)
                    .insert_header("x-request-id", "req-456"),
            )
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());

        let result = client
            .get_secret("test-namespace", "missing-key", GetOpts::default())
            .await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.status_code(), Some(404));
        assert_eq!(err.request_id(), Some("req-456"));
    }

    #[tokio::test]
    async fn test_get_secret_with_cache() {
        let mock_server = MockServer::start().await;

        let response_body = serde_json::json!({
            "value": "cached-value",
            "version": 2,
            "expires_at": null,
            "metadata": null,
            "updated_at": "2024-01-01T00:00:00Z"
        });

        // First request
        Mock::given(method("GET"))
            .and(path("/api/v2/secrets/cache-ns/cache-key"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(&response_body)
                    .insert_header("etag", "\"etag123\""),
            )
            .expect(1) // Should only be called once
            .mount(&mock_server)
            .await;

        #[cfg(feature = "danger-insecure-http")]
        let client = ClientBuilder::new(mock_server.uri())
            .auth(Auth::bearer("test-token"))
            .enable_cache(true)
            .allow_insecure_http()
            .build()
            .unwrap();

        #[cfg(not(feature = "danger-insecure-http"))]
        let client = ClientBuilder::new(&mock_server.uri().replace("http://", "https://"))
            .auth(Auth::bearer("test-token"))
            .enable_cache(true)
            .build()
            .unwrap();

        // First request - should hit server
        let secret1 = client
            .get_secret("cache-ns", "cache-key", GetOpts::default())
            .await
            .unwrap();
        assert_eq!(secret1.version, 2);

        // Small delay to ensure cache is populated
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Second request - should hit cache
        let secret2 = client
            .get_secret("cache-ns", "cache-key", GetOpts::default())
            .await
            .unwrap();
        assert_eq!(secret2.version, 2);

        // Verify cache hit
        let stats = client.cache_stats();
        assert_eq!(stats.hits(), 1);
        assert_eq!(stats.misses(), 1);
    }

    #[tokio::test]
    async fn test_get_secret_304_not_modified() {
        let mock_server = MockServer::start().await;

        let response_body = serde_json::json!({
            "value": "initial-value",
            "version": 1,
            "expires_at": null,
            "metadata": null,
            "updated_at": "2024-01-01T00:00:00Z"
        });

        // Mount both mocks at once with more specific one first
        // Second request with etag - return 304 (more specific, so should match first)
        Mock::given(method("GET"))
            .and(path("/api/v2/secrets/test-ns/test-key"))
            .and(header("Authorization", "Bearer test-token"))
            .and(header("if-none-match", "etag-v1"))
            .respond_with(ResponseTemplate::new(304))
            .expect(1)
            .mount(&mock_server)
            .await;

        // First request - return data (less specific)
        Mock::given(method("GET"))
            .and(path("/api/v2/secrets/test-ns/test-key"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(&response_body)
                    .insert_header("etag", "\"etag-v1\""),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        #[cfg(feature = "danger-insecure-http")]
        let client = ClientBuilder::new(mock_server.uri())
            .auth(Auth::bearer("test-token"))
            .enable_cache(true)
            .allow_insecure_http()
            .build()
            .unwrap();

        #[cfg(not(feature = "danger-insecure-http"))]
        let client = ClientBuilder::new(&mock_server.uri().replace("http://", "https://"))
            .auth(Auth::bearer("test-token"))
            .enable_cache(true)
            .build()
            .unwrap();

        // First request
        let secret1 = client
            .get_secret("test-ns", "test-key", GetOpts::default())
            .await
            .unwrap();
        assert_eq!(secret1.etag, Some("\"etag-v1\"".to_string()));

        // Clear cache to force second request to hit server
        client.clear_cache();

        // Second request with etag
        let opts = GetOpts {
            use_cache: false, // Disable cache to ensure we hit the server
            if_none_match: Some("etag-v1".to_string()), // Without quotes
            if_modified_since: None,
        };
        // This should return error since cache was cleared and server returns 304
        let result = client.get_secret("test-ns", "test-key", opts).await;
        assert!(result.is_err());

        // The error should indicate that we got 304 but have no cache
        if let Err(e) = result {
            match &e {
                Error::Other(msg) => {
                    assert!(msg.contains("304"));
                    assert!(msg.contains("no cached entry"));
                }
                _ => panic!("Expected Error::Other, got {:?}", e),
            }
        }
    }

    #[tokio::test]
    async fn test_put_secret_success() {
        let mock_server = MockServer::start().await;

        let response_body = serde_json::json!({
            "message": "Secret created",
            "namespace": "test-ns",
            "key": "new-key",
            "created_at": "2024-01-01T00:00:00Z",
            "request_id": "req-789"
        });

        Mock::given(method("PUT"))
            .and(path("/api/v2/secrets/test-ns/new-key"))
            .respond_with(ResponseTemplate::new(201).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());

        let opts = PutOpts {
            ttl_seconds: Some(3600),
            metadata: Some(serde_json::json!({"env": "test"})),
            idempotency_key: None,
        };

        let result = client
            .put_secret("test-ns", "new-key", "new-value", opts)
            .await;
        assert!(result.is_ok());

        let put_result = result.unwrap();
        assert_eq!(put_result.namespace, "test-ns");
        assert_eq!(put_result.key, "new-key");
    }

    #[tokio::test]
    async fn test_delete_secret_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/api/v2/secrets/test-ns/delete-key"))
            .respond_with(ResponseTemplate::new(204).insert_header("x-request-id", "req-delete"))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());

        let result = client.delete_secret("test-ns", "delete-key").await;
        assert!(result.is_ok());

        let delete_result = result.unwrap();
        assert!(delete_result.deleted);
        assert_eq!(delete_result.request_id, Some("req-delete".to_string()));
    }

    #[tokio::test]
    async fn test_retry_on_server_error() {
        let mock_server = MockServer::start().await;

        let error_body = serde_json::json!({
            "error": "internal",
            "message": "Internal server error",
            "timestamp": "2024-01-01T00:00:00Z",
            "status": 500
        });

        // First two requests fail, third succeeds
        Mock::given(method("GET"))
            .and(path("/api/v2/secrets/test-ns/retry-key"))
            .respond_with(ResponseTemplate::new(500).set_body_json(&error_body))
            .up_to_n_times(2)
            .mount(&mock_server)
            .await;

        let success_body = serde_json::json!({
            "value": "success-after-retry",
            "version": 1,
            "expires_at": null,
            "metadata": null,
            "updated_at": "2024-01-01T00:00:00Z"
        });

        Mock::given(method("GET"))
            .and(path("/api/v2/secrets/test-ns/retry-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&success_body))
            .mount(&mock_server)
            .await;

        #[cfg(feature = "danger-insecure-http")]
        let client = ClientBuilder::new(mock_server.uri())
            .auth(Auth::bearer("test-token"))
            .retries(3)
            .allow_insecure_http()
            .build()
            .unwrap();

        #[cfg(not(feature = "danger-insecure-http"))]
        let client = ClientBuilder::new(&mock_server.uri().replace("http://", "https://"))
            .auth(Auth::bearer("test-token"))
            .retries(3)
            .build()
            .unwrap();

        let result = client
            .get_secret("test-ns", "retry-key", GetOpts::default())
            .await;
        assert!(result.is_ok()); // Should succeed after retries
    }

    #[tokio::test]
    async fn test_list_secrets() {
        let mock_server = MockServer::start().await;

        let response_body = serde_json::json!({
            "namespace": "test-ns",
            "secrets": [
                {"key": "key1", "ver": 1, "updated_at": "2024-01-01T00:00:00Z", "kid": null},
                {"key": "key2", "ver": 2, "updated_at": "2024-01-01T00:00:00Z", "kid": "kid123"}
            ],
            "total": 2,
            "limit": 10,
            "has_more": false,
            "request_id": "req-list"
        });

        Mock::given(method("GET"))
            .and(path("/api/v2/secrets/test-ns"))
            .and(wiremock::matchers::query_param("prefix", "key"))
            .and(wiremock::matchers::query_param("limit", "10"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());

        let opts = ListOpts {
            prefix: Some("key".to_string()),
            limit: Some(10),
        };

        let result = client.list_secrets("test-ns", opts).await;
        assert!(result.is_ok());

        let list_result = result.unwrap();
        assert_eq!(list_result.namespace, "test-ns");
        assert_eq!(list_result.secrets.len(), 2);
        assert_eq!(list_result.total, 2);
    }

    #[tokio::test]
    async fn test_list_versions() {
        let mock_server = MockServer::start().await;

        let response_body = serde_json::json!({
            "namespace": "test-ns",
            "key": "versioned-key",
            "versions": [
                {
                    "version": 3,
                    "created_at": "2024-01-03T00:00:00Z",
                    "created_by": "user1",
                    "is_current": true
                },
                {
                    "version": 2,
                    "created_at": "2024-01-02T00:00:00Z",
                    "created_by": "user1",
                    "is_current": false
                },
                {
                    "version": 1,
                    "created_at": "2024-01-01T00:00:00Z",
                    "created_by": "user1",
                    "is_current": false
                }
            ],
            "total": 3,
            "request_id": "req-versions"
        });

        Mock::given(method("GET"))
            .and(path("/api/v2/secrets/test-ns/versioned-key/versions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());

        let result = client.list_versions("test-ns", "versioned-key").await;
        assert!(result.is_ok());

        let version_list = result.unwrap();
        assert_eq!(version_list.namespace, "test-ns");
        assert_eq!(version_list.key, "versioned-key");
        assert_eq!(version_list.versions.len(), 3);
        assert_eq!(version_list.total, 3);
        assert!(version_list.versions[0].is_current);
    }

    #[tokio::test]
    async fn test_get_version() {
        let mock_server = MockServer::start().await;

        let response_body = serde_json::json!({
            "value": "version-2-value",
            "version": 2,
            "expires_at": null,
            "metadata": {"note": "version 2"},
            "updated_at": "2024-01-02T00:00:00Z"
        });

        Mock::given(method("GET"))
            .and(path("/api/v2/secrets/test-ns/versioned-key/versions/2"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(&response_body)
                    .insert_header("etag", "\"etag-v2\""),
            )
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());

        let result = client.get_version("test-ns", "versioned-key", 2).await;
        assert!(result.is_ok());

        let secret = result.unwrap();
        assert_eq!(secret.namespace, "test-ns");
        assert_eq!(secret.key, "versioned-key");
        assert_eq!(secret.version, 2);
        assert_eq!(secret.value.expose_secret(), "version-2-value");
    }

    #[tokio::test]
    async fn test_rollback() {
        let mock_server = MockServer::start().await;

        let response_body = serde_json::json!({
            "message": "Secret successfully rolled back to version 2",
            "namespace": "test-ns",
            "key": "versioned-key",
            "from_version": 4,
            "to_version": 2,
            "request_id": "req-rollback"
        });

        Mock::given(method("POST"))
            .and(path("/api/v2/secrets/test-ns/versioned-key/rollback/2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());

        let result = client.rollback("test-ns", "versioned-key", 2).await;
        assert!(result.is_ok());

        let rollback_result = result.unwrap();
        assert_eq!(rollback_result.namespace, "test-ns");
        assert_eq!(rollback_result.key, "versioned-key");
        assert_eq!(rollback_result.from_version, 4);
        assert_eq!(rollback_result.to_version, 2);
    }

    #[tokio::test]
    async fn test_audit_logs() {
        let mock_server = MockServer::start().await;

        let response_body = serde_json::json!({
            "logs": [
                {
                    "id": 123,
                    "timestamp": "2024-01-01T12:00:00Z",
                    "actor": "user1",
                    "action": "put",
                    "namespace": "production",
                    "key_name": "api-key",
                    "success": true,
                    "ip_address": "192.168.1.1",
                    "user_agent": "SDK/1.0"
                },
                {
                    "id": 124,
                    "timestamp": "2024-01-01T12:05:00Z",
                    "actor": "user2",
                    "action": "get",
                    "namespace": "production",
                    "key_name": "db-pass",
                    "success": false,
                    "error": "not found"
                }
            ],
            "total": 2,
            "limit": 10,
            "offset": 0,
            "has_more": false,
            "request_id": "req-audit"
        });

        Mock::given(method("GET"))
            .and(path("/api/v2/audit"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());

        let query = AuditQuery::default();
        let result = client.audit(query).await;
        assert!(result.is_ok());

        let audit_result = result.unwrap();
        assert_eq!(audit_result.entries.len(), 2);
        assert_eq!(audit_result.total, 2);
        assert!(!audit_result.has_more);

        // Check first entry
        let first = &audit_result.entries[0];
        assert_eq!(first.id, 123);
        assert_eq!(first.action, "put");
        assert!(first.success);
        assert_eq!(first.namespace, Some("production".to_string()));
    }

    #[tokio::test]
    async fn test_audit_logs_with_filters() {
        let mock_server = MockServer::start().await;

        let response_body = serde_json::json!({
            "logs": [
                {
                    "id": 200,
                    "timestamp": "2024-01-02T10:00:00Z",
                    "actor": "admin",
                    "action": "delete",
                    "namespace": "test",
                    "key_name": "temp-key",
                    "success": false,
                    "error": "permission denied"
                }
            ],
            "total": 1,
            "limit": 5,
            "offset": 0,
            "has_more": false,
            "request_id": "req-audit-filtered"
        });

        Mock::given(method("GET"))
            .and(path("/api/v2/audit"))
            .and(wiremock::matchers::query_param("namespace", "test"))
            .and(wiremock::matchers::query_param("success", "false"))
            .and(wiremock::matchers::query_param("limit", "5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let client = create_test_client(&mock_server.uri());

        let query = AuditQuery {
            namespace: Some("test".to_string()),
            success: Some(false),
            limit: Some(5),
            ..Default::default()
        };

        let result = client.audit(query).await;
        assert!(result.is_ok());

        let audit_result = result.unwrap();
        assert_eq!(audit_result.entries.len(), 1);
        assert_eq!(audit_result.entries[0].action, "delete");
        assert!(!audit_result.entries[0].success);
        assert_eq!(
            audit_result.entries[0].error,
            Some("permission denied".to_string())
        );
    }
}
