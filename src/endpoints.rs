//! API endpoint URL construction

use crate::util::encode_path;

/// API v2 base path
pub const API_V2_BASE: &str = "/api/v2";

/// Endpoint builder
#[derive(Clone)]
pub struct Endpoints {
    base_url: String,
}

impl Endpoints {
    /// Create a new endpoints builder
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Get the full URL for a path
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    // Discovery
    #[allow(dead_code)]
    pub fn discovery(&self) -> String {
        self.url(API_V2_BASE)
    }

    // Secrets
    pub fn get_secret(&self, namespace: &str, key: &str) -> String {
        self.url(&format!(
            "{}/secrets/{}/{}",
            API_V2_BASE,
            encode_path(namespace),
            encode_path(key)
        ))
    }

    pub fn put_secret(&self, namespace: &str, key: &str) -> String {
        self.get_secret(namespace, key)
    }

    pub fn delete_secret(&self, namespace: &str, key: &str) -> String {
        self.get_secret(namespace, key)
    }

    pub fn list_secrets(&self, namespace: &str) -> String {
        self.url(&format!(
            "{}/secrets/{}",
            API_V2_BASE,
            encode_path(namespace)
        ))
    }

    // Batch
    #[allow(dead_code)]
    pub fn batch_get(&self, namespace: &str) -> String {
        self.url(&format!(
            "{}/secrets/{}/batch",
            API_V2_BASE,
            encode_path(namespace)
        ))
    }

    #[allow(dead_code)]
    pub fn batch_operate(&self, namespace: &str) -> String {
        self.batch_get(namespace)
    }

    // Versions
    #[allow(dead_code)]
    pub fn list_versions(&self, namespace: &str, key: &str) -> String {
        self.url(&format!(
            "{}/secrets/{}/{}/versions",
            API_V2_BASE,
            encode_path(namespace),
            encode_path(key)
        ))
    }

    #[allow(dead_code)]
    pub fn get_version(&self, namespace: &str, key: &str, version: i32) -> String {
        self.url(&format!(
            "{}/secrets/{}/{}/versions/{}",
            API_V2_BASE,
            encode_path(namespace),
            encode_path(key),
            version
        ))
    }

    #[allow(dead_code)]
    pub fn rollback(&self, namespace: &str, key: &str, version: i32) -> String {
        self.url(&format!(
            "{}/secrets/{}/{}/rollback/{}",
            API_V2_BASE,
            encode_path(namespace),
            encode_path(key),
            version
        ))
    }

    // Namespaces
    #[allow(dead_code)]
    pub fn list_namespaces(&self) -> String {
        self.url(&format!("{}/namespaces", API_V2_BASE))
    }

    pub fn create_namespace(&self) -> String {
        self.url(&format!("{}/namespaces", API_V2_BASE))
    }

    #[allow(dead_code)]
    pub fn get_namespace(&self, namespace: &str) -> String {
        self.url(&format!(
            "{}/namespaces/{}",
            API_V2_BASE,
            encode_path(namespace)
        ))
    }

    #[allow(dead_code)]
    pub fn init_namespace(&self, namespace: &str) -> String {
        self.url(&format!(
            "{}/namespaces/{}/init",
            API_V2_BASE,
            encode_path(namespace)
        ))
    }

    pub fn delete_namespace(&self, namespace: &str) -> String {
        self.url(&format!(
            "{}/namespaces/{}",
            API_V2_BASE,
            encode_path(namespace)
        ))
    }

    // Environment
    #[allow(dead_code)]
    pub fn export_env(&self, namespace: &str) -> String {
        self.url(&format!("{}/env/{}", API_V2_BASE, encode_path(namespace)))
    }

    // Audit
    #[allow(dead_code)]
    pub fn audit(&self) -> String {
        self.url(&format!("{}/audit", API_V2_BASE))
    }

    // Health
    #[allow(dead_code)]
    pub fn livez(&self) -> String {
        self.url(&format!("{}/livez", API_V2_BASE))
    }

    #[allow(dead_code)]
    pub fn readyz(&self) -> String {
        self.url(&format!("{}/readyz", API_V2_BASE))
    }

    // API Keys
    pub fn list_api_keys(&self) -> String {
        self.url(&format!("{}/api-keys", API_V2_BASE))
    }

    pub fn create_api_key(&self) -> String {
        self.list_api_keys()
    }

    pub fn get_api_key(&self, key_id: &str) -> String {
        self.url(&format!("{}/api-keys/{}", API_V2_BASE, encode_path(key_id)))
    }

    pub fn revoke_api_key(&self, key_id: &str) -> String {
        self.get_api_key(key_id)
    }

    // Metrics
    pub fn metrics(&self) -> String {
        self.url(&format!("{}/metrics", API_V2_BASE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoints() {
        let endpoints = Endpoints::new("https://api.example.com");

        assert_eq!(
            endpoints.get_secret("prod", "db-pass"),
            "https://api.example.com/api/v2/secrets/prod/db-pass"
        );

        assert_eq!(
            endpoints.list_secrets("test namespace"),
            "https://api.example.com/api/v2/secrets/test%20namespace"
        );

        assert_eq!(endpoints.discovery(), "https://api.example.com/api/v2");
    }

    #[test]
    fn test_trailing_slash() {
        let endpoints = Endpoints::new("https://api.example.com/");
        assert_eq!(endpoints.discovery(), "https://api.example.com/api/v2");
    }
}
