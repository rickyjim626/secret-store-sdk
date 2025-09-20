use crate::{auth::Auth, cache::CacheConfig, errors::Result, Error, telemetry::TelemetryConfig};
use std::time::Duration;

/// Client configuration
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URL of the secret store service
    pub base_url: String,
    /// Authentication configuration
    pub auth: Auth,
    /// Request timeout
    pub timeout: Duration,
    /// Number of retries
    pub retries: u32,
    /// User agent suffix
    pub user_agent_suffix: Option<String>,
    /// Cache configuration
    pub cache_config: CacheConfig,
    /// Telemetry configuration
    pub telemetry_config: TelemetryConfig,
    /// Allow insecure HTTP (only with danger-insecure-http feature)
    pub allow_insecure_http: bool,
}

/// Builder for creating a configured Client
#[derive(Debug)]
pub struct ClientBuilder {
    base_url: String,
    auth: Option<Auth>,
    timeout_ms: u64,
    retries: u32,
    user_agent_suffix: Option<String>,
    cache_enabled: bool,
    cache_max_entries: u64,
    cache_ttl_secs: u64,
    telemetry_config: TelemetryConfig,
    allow_insecure_http: bool,
}

impl ClientBuilder {
    /// Create a new client builder with the given base URL
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL of the secret store service (e.g., `"https://secret.example.com"`)
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            auth: None,
            timeout_ms: crate::DEFAULT_TIMEOUT_MS,
            retries: crate::DEFAULT_RETRIES,
            user_agent_suffix: None,
            cache_enabled: true,
            cache_max_entries: crate::DEFAULT_CACHE_MAX_ENTRIES,
            cache_ttl_secs: crate::DEFAULT_CACHE_TTL_SECS,
            telemetry_config: TelemetryConfig::default(),
            allow_insecure_http: false,
        }
    }

    /// Set the authentication method
    pub fn auth(mut self, auth: Auth) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Set the request timeout in milliseconds
    pub fn timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Set the number of retries for failed requests
    pub fn retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    /// Add a custom user agent suffix
    pub fn user_agent_extra(mut self, suffix: impl Into<String>) -> Self {
        self.user_agent_suffix = Some(suffix.into());
        self
    }

    /// Enable or disable caching (enabled by default)
    pub fn enable_cache(mut self, enabled: bool) -> Self {
        self.cache_enabled = enabled;
        self
    }

    /// Set the maximum number of cache entries
    pub fn cache_max_entries(mut self, max_entries: u64) -> Self {
        self.cache_max_entries = max_entries;
        self
    }

    /// Set the default cache TTL in seconds
    pub fn cache_ttl_secs(mut self, ttl_secs: u64) -> Self {
        self.cache_ttl_secs = ttl_secs;
        self
    }

    /// Configure telemetry/metrics
    #[cfg(feature = "metrics")]
    pub fn with_telemetry(mut self, config: TelemetryConfig) -> Self {
        self.telemetry_config = config;
        self
    }

    /// Enable telemetry with default settings
    #[cfg(feature = "metrics")]
    pub fn enable_telemetry(mut self) -> Self {
        self.telemetry_config.enabled = true;
        self
    }

    /// Allow insecure HTTP connections (requires danger-insecure-http feature)
    #[cfg(feature = "danger-insecure-http")]
    pub fn allow_insecure_http(mut self) -> Self {
        self.allow_insecure_http = true;
        self
    }

    /// Build the client with the configured options
    pub fn build(self) -> Result<crate::Client> {
        // Validate base URL
        let url = self.base_url.trim_end_matches('/');
        
        // Check for insecure HTTP
        if url.starts_with("http://") && !self.allow_insecure_http {
            #[cfg(feature = "danger-insecure-http")]
            return Err(Error::Config(
                "HTTP URLs are not allowed by default. Use .allow_insecure_http() to enable (dangerous!)".to_string()
            ));
            
            #[cfg(not(feature = "danger-insecure-http"))]
            return Err(Error::Config(
                "HTTP URLs are not allowed. Enable the 'danger-insecure-http' feature and use .allow_insecure_http() (dangerous!)".to_string()
            ));
        }

        // Require authentication
        let auth = self.auth.ok_or_else(|| {
            Error::Config("Authentication is required. Use .auth() to set authentication method".to_string())
        })?;

        // Validate URL format
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(Error::Config("Base URL must start with http:// or https://".to_string()));
        }

        let config = ClientConfig {
            base_url: url.to_string(),
            auth,
            timeout: Duration::from_millis(self.timeout_ms),
            retries: self.retries,
            user_agent_suffix: self.user_agent_suffix,
            cache_config: CacheConfig {
                enabled: self.cache_enabled,
                max_entries: self.cache_max_entries,
                default_ttl_secs: self.cache_ttl_secs,
            },
            telemetry_config: self.telemetry_config,
            allow_insecure_http: self.allow_insecure_http,
        };

        crate::client::Client::new(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_requires_auth() {
        let result = ClientBuilder::new("https://example.com").build();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Config(_)));
    }

    #[test]
    fn test_builder_validates_url() {
        let result = ClientBuilder::new("not-a-url")
            .auth(Auth::bearer("token"))
            .build();
        assert!(result.is_err());
    }

    #[test]
    #[cfg(not(feature = "danger-insecure-http"))]
    fn test_builder_rejects_http() {
        let result = ClientBuilder::new("http://example.com")
            .auth(Auth::bearer("token"))
            .build();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Config(_)));
    }
}