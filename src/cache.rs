use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Whether caching is enabled
    pub enabled: bool,
    /// Maximum number of entries in the cache
    pub max_entries: u64,
    /// Default TTL for cache entries in seconds
    pub default_ttl_secs: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: crate::DEFAULT_CACHE_MAX_ENTRIES,
            default_ttl_secs: crate::DEFAULT_CACHE_TTL_SECS,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    inner: Arc<CacheStatsInner>,
}

#[derive(Debug, Default)]
struct CacheStatsInner {
    hits: AtomicU64,
    misses: AtomicU64,
    insertions: AtomicU64,
    evictions: AtomicU64,
    expirations: AtomicU64,
}

impl CacheStats {
    /// Create new cache statistics
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(CacheStatsInner::default()),
        }
    }

    /// Get the number of cache hits
    pub fn hits(&self) -> u64 {
        self.inner.hits.load(Ordering::Relaxed)
    }

    /// Get the number of cache misses
    pub fn misses(&self) -> u64 {
        self.inner.misses.load(Ordering::Relaxed)
    }

    /// Get the number of cache insertions
    pub fn insertions(&self) -> u64 {
        self.inner.insertions.load(Ordering::Relaxed)
    }

    /// Get the number of cache evictions
    pub fn evictions(&self) -> u64 {
        self.inner.evictions.load(Ordering::Relaxed)
    }

    /// Get the number of expired entries
    pub fn expirations(&self) -> u64 {
        self.inner.expirations.load(Ordering::Relaxed)
    }

    /// Get the hit rate as a percentage (0.0-100.0)
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits();
        let total = hits + self.misses();
        if total == 0 {
            0.0
        } else {
            (hits as f64 / total as f64) * 100.0
        }
    }

    /// Reset all statistics to zero
    pub fn reset(&self) {
        self.inner.hits.store(0, Ordering::Relaxed);
        self.inner.misses.store(0, Ordering::Relaxed);
        self.inner.insertions.store(0, Ordering::Relaxed);
        self.inner.evictions.store(0, Ordering::Relaxed);
        self.inner.expirations.store(0, Ordering::Relaxed);
    }

    // Internal methods for updating stats
    pub(crate) fn record_hit(&self) {
        let _ = self.inner.hits.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_miss(&self) {
        let _ = self.inner.misses.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_insertion(&self) {
        let _ = self.inner.insertions.fetch_add(1, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub(crate) fn record_eviction(&self) {
        let _ = self.inner.evictions.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_expiration(&self) {
        let _ = self.inner.expirations.fetch_add(1, Ordering::Relaxed);
    }
}

/// Cached secret entry
#[derive(Debug, Clone)]
pub(crate) struct CachedSecret {
    pub value: secrecy::SecretString,
    pub version: i32,
    pub expires_at: Option<time::OffsetDateTime>,
    pub metadata: serde_json::Value,
    pub updated_at: time::OffsetDateTime,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub cache_expires_at: time::OffsetDateTime,
}

impl CachedSecret {
    /// Check if the cache entry has expired
    pub fn is_expired(&self) -> bool {
        let now = time::OffsetDateTime::now_utc();

        // Check cache expiry
        if now >= self.cache_expires_at {
            return true;
        }

        // Check secret expiry
        if let Some(expires_at) = self.expires_at {
            if now >= expires_at {
                return true;
            }
        }

        false
    }

    /// Convert to a Secret model
    pub fn into_secret(self, namespace: String, key: String) -> crate::models::Secret {
        crate::models::Secret {
            namespace,
            key,
            value: self.value,
            version: self.version,
            expires_at: self.expires_at,
            metadata: self.metadata,
            updated_at: self.updated_at,
            etag: self.etag,
            last_modified: self.last_modified,
            request_id: None, // Cache hits don't have request IDs
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_entries, crate::DEFAULT_CACHE_MAX_ENTRIES);
        assert_eq!(config.default_ttl_secs, crate::DEFAULT_CACHE_TTL_SECS);
    }

    #[test]
    fn test_cache_stats() {
        let stats = CacheStats::new();

        // Initial state
        assert_eq!(stats.hits(), 0);
        assert_eq!(stats.misses(), 0);
        assert_eq!(stats.hit_rate(), 0.0);

        // Record some activity
        stats.record_hit();
        stats.record_hit();
        stats.record_miss();

        assert_eq!(stats.hits(), 2);
        assert_eq!(stats.misses(), 1);
        assert_eq!(stats.hit_rate(), 66.66666666666666);

        // Reset
        stats.reset();
        assert_eq!(stats.hits(), 0);
        assert_eq!(stats.misses(), 0);
    }

    #[test]
    fn test_cached_secret_expiry() {
        use time::Duration;

        let now = time::OffsetDateTime::now_utc();

        // Not expired
        let cached = CachedSecret {
            value: secrecy::SecretString::new("value".to_string()),
            version: 1,
            expires_at: None,
            metadata: serde_json::Value::Null,
            updated_at: now,
            etag: None,
            last_modified: None,
            cache_expires_at: now + Duration::minutes(5),
        };
        assert!(!cached.is_expired());

        // Cache expired
        let cached = CachedSecret {
            value: secrecy::SecretString::new("value".to_string()),
            version: 1,
            expires_at: None,
            metadata: serde_json::Value::Null,
            updated_at: now,
            etag: None,
            last_modified: None,
            cache_expires_at: now - Duration::minutes(1),
        };
        assert!(cached.is_expired());

        // Secret expired
        let cached = CachedSecret {
            value: secrecy::SecretString::new("value".to_string()),
            version: 1,
            expires_at: Some(now - Duration::minutes(1)),
            metadata: serde_json::Value::Null,
            updated_at: now,
            etag: None,
            last_modified: None,
            cache_expires_at: now + Duration::minutes(5),
        };
        assert!(cached.is_expired());
    }
}
