//! Telemetry and observability utilities

use std::sync::Arc;

#[cfg(feature = "metrics")]
use opentelemetry::{
    metrics::{Counter, Histogram, UpDownCounter},
    KeyValue,
};

/// Telemetry configuration
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// Whether telemetry is enabled
    pub enabled: bool,
    /// Service name for metrics
    pub service_name: String,
    /// Service version for metrics
    pub service_version: String,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            service_name: "xjp-secret-store-sdk".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// SDK metrics collector
#[derive(Clone)]
pub struct Metrics {
    #[cfg(feature = "metrics")]
    pub(crate) requests_total: Counter<u64>,

    #[cfg(feature = "metrics")]
    pub(crate) request_duration: Histogram<f64>,

    #[cfg(feature = "metrics")]
    pub(crate) errors_total: Counter<u64>,

    #[cfg(feature = "metrics")]
    pub(crate) cache_hits: Counter<u64>,

    #[cfg(feature = "metrics")]
    pub(crate) cache_misses: Counter<u64>,

    #[cfg(feature = "metrics")]
    pub(crate) active_connections: UpDownCounter<i64>,

    #[cfg(feature = "metrics")]
    pub(crate) retry_attempts: Counter<u64>,
}

impl Metrics {
    /// Create new metrics instance
    #[cfg(feature = "metrics")]
    pub fn new(config: &TelemetryConfig) -> Self {
        use opentelemetry::global;

        let meter = global::meter(config.service_name.clone());

        let requests_total = meter
            .u64_counter("secret_store_sdk.requests_total")
            .with_description("Total number of requests made")
            .init();

        let request_duration = meter
            .f64_histogram("secret_store_sdk.request_duration_seconds")
            .with_description("Request duration in seconds")
            .init();

        let errors_total = meter
            .u64_counter("secret_store_sdk.errors_total")
            .with_description("Total number of errors")
            .init();

        let cache_hits = meter
            .u64_counter("secret_store_sdk.cache_hits_total")
            .with_description("Total number of cache hits")
            .init();

        let cache_misses = meter
            .u64_counter("secret_store_sdk.cache_misses_total")
            .with_description("Total number of cache misses")
            .init();

        let active_connections = meter
            .i64_up_down_counter("secret_store_sdk.active_connections")
            .with_description("Number of active connections")
            .init();

        let retry_attempts = meter
            .u64_counter("secret_store_sdk.retry_attempts_total")
            .with_description("Total number of retry attempts")
            .init();

        Self {
            requests_total,
            request_duration,
            errors_total,
            cache_hits,
            cache_misses,
            active_connections,
            retry_attempts,
        }
    }

    /// Create a no-op metrics instance when feature is disabled
    #[cfg(not(feature = "metrics"))]
    #[allow(dead_code)]
    pub fn new(_config: &TelemetryConfig) -> Self {
        Self {}
    }

    /// Record a request
    #[cfg(feature = "metrics")]
    pub fn record_request(&self, method: &str, path: &str, status: u16, duration_secs: f64) {
        let labels = &[
            KeyValue::new("method", method.to_string()),
            KeyValue::new("path", path.to_string()),
            KeyValue::new("status", status.to_string()),
        ];

        self.requests_total.add(1, labels);
        self.request_duration.record(duration_secs, labels);

        if status >= 400 {
            self.errors_total.add(
                1,
                &[
                    KeyValue::new("type", if status >= 500 { "server" } else { "client" }),
                    KeyValue::new("status", status.to_string()),
                ],
            );
        }
    }

    /// Record a request (no-op when metrics disabled)
    #[cfg(not(feature = "metrics"))]
    #[allow(dead_code)]
    pub fn record_request(&self, _method: &str, _path: &str, _status: u16, _duration_secs: f64) {}

    /// Record a cache hit
    #[cfg(feature = "metrics")]
    pub fn record_cache_hit(&self, namespace: &str) {
        self.cache_hits
            .add(1, &[KeyValue::new("namespace", namespace.to_string())]);
    }

    /// Record a cache hit (no-op)
    #[cfg(not(feature = "metrics"))]
    #[allow(dead_code)]
    pub fn record_cache_hit(&self, _namespace: &str) {}

    /// Record a cache miss
    #[cfg(feature = "metrics")]
    pub fn record_cache_miss(&self, namespace: &str) {
        self.cache_misses
            .add(1, &[KeyValue::new("namespace", namespace.to_string())]);
    }

    /// Record a cache miss (no-op)
    #[cfg(not(feature = "metrics"))]
    #[allow(dead_code)]
    pub fn record_cache_miss(&self, _namespace: &str) {}

    /// Increment active connections
    #[cfg(feature = "metrics")]
    pub fn inc_active_connections(&self) {
        self.active_connections.add(1, &[]);
    }

    /// Increment active connections (no-op)
    #[cfg(not(feature = "metrics"))]
    #[allow(dead_code)]
    pub fn inc_active_connections(&self) {}

    /// Decrement active connections
    #[cfg(feature = "metrics")]
    pub fn dec_active_connections(&self) {
        self.active_connections.add(-1, &[]);
    }

    /// Decrement active connections (no-op)
    #[cfg(not(feature = "metrics"))]
    #[allow(dead_code)]
    pub fn dec_active_connections(&self) {}

    /// Record a retry attempt
    #[cfg(feature = "metrics")]
    pub fn record_retry(&self, attempt: u32, reason: &str) {
        self.retry_attempts.add(
            1,
            &[
                KeyValue::new("attempt", attempt.to_string()),
                KeyValue::new("reason", reason.to_string()),
            ],
        );
    }

    /// Record a retry attempt (no-op)
    #[cfg(not(feature = "metrics"))]
    #[allow(dead_code)]
    pub fn record_retry(&self, _attempt: u32, _reason: &str) {}
}

impl std::fmt::Debug for Metrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Metrics")
            .field("enabled", &cfg!(feature = "metrics"))
            .finish()
    }
}

/// Global telemetry instance holder
static TELEMETRY: std::sync::OnceLock<Arc<Metrics>> = std::sync::OnceLock::new();

/// Initialize global telemetry
#[cfg(feature = "metrics")]
pub fn init_telemetry(config: TelemetryConfig) -> Arc<Metrics> {
    let metrics = Arc::new(Metrics::new(&config));
    TELEMETRY.get_or_init(|| metrics.clone()).clone()
}

/// Get global telemetry instance
#[allow(dead_code)]
pub fn telemetry() -> Option<Arc<Metrics>> {
    TELEMETRY.get().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_config_default() {
        let config = TelemetryConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.service_name, "xjp-secret-store-sdk");
    }

    #[test]
    fn test_metrics_creation() {
        let config = TelemetryConfig {
            enabled: true,
            ..Default::default()
        };

        let _metrics = Metrics::new(&config);
        // Just ensure it compiles and creates successfully
    }
}
