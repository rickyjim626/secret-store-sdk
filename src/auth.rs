//! Authentication support for the XJP Secret Store SDK
//!
//! This module provides flexible authentication options with automatic token refresh support.
//! The SDK supports multiple authentication methods in priority order:
//!
//! 1. **Bearer Token** - Highest priority, typically for user access tokens
//! 2. **API Key** - For service-to-service authentication
//! 3. **XJP Key** - Legacy authentication method
//! 4. **Token Provider** - Dynamic tokens with refresh capability
//!
//! # Examples
//!
//! ## Static Authentication
//!
//! ```
//! use secret_store_sdk::Auth;
//!
//! // Bearer token (highest priority)
//! let auth = Auth::bearer("your-access-token");
//!
//! // API key
//! let auth = Auth::api_key("your-api-key");
//!
//! // XJP key (legacy)
//! let auth = Auth::xjp_key("your-xjp-key");
//! ```
//!
//! ## Dynamic Token Provider
//!
//! ```
//! use secret_store_sdk::{Auth, TokenProvider, SecretString};
//! use async_trait::async_trait;
//! use std::sync::{Arc, Mutex};
//!
//! #[derive(Clone)]
//! struct MyTokenProvider {
//!     current_token: Arc<Mutex<String>>,
//! }
//!
//! #[async_trait]
//! impl TokenProvider for MyTokenProvider {
//!     async fn get_token(&self) -> Result<SecretString, Box<dyn std::error::Error + Send + Sync>> {
//!         let token = self.current_token.lock().unwrap().clone();
//!         Ok(SecretString::new(token))
//!     }
//!
//!     async fn refresh_token(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!         // Fetch new token from auth service
//!         let new_token = "refreshed-token";
//!         *self.current_token.lock().unwrap() = new_token.to_string();
//!         Ok(())
//!     }
//!
//!     fn clone_box(&self) -> Box<dyn TokenProvider> {
//!         Box::new(self.clone())
//!     }
//! }
//!
//! let provider = MyTokenProvider {
//!     current_token: Arc::new(Mutex::new("initial-token".to_string())),
//! };
//! let auth = Auth::token_provider(provider);
//! ```

use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use std::fmt;

/// Authentication method for the secret store API
///
/// The authentication method determines how requests are authorized.
/// Methods are tried in priority order if multiple are configured.
///
/// # Security
///
/// All authentication credentials are stored using [`SecretString`] to prevent
/// accidental exposure in logs or debug output.
#[derive(Clone)]
pub enum Auth {
    /// Bearer token authentication (highest priority)
    ///
    /// Used for OAuth2/JWT tokens. Sent as `Authorization: Bearer <token>`
    Bearer(SecretString),
    /// API key authentication
    ///
    /// Used for service accounts. Sent as `X-API-Key: <key>`
    ApiKey(SecretString),
    /// XJP key authentication (legacy)
    ///
    /// Legacy authentication method. Sent as `XJP-KEY: <key>`
    XjpKey(SecretString),
    /// Dynamic token provider for refreshable tokens
    ///
    /// Supports automatic token refresh on 401 responses
    TokenProvider(Box<dyn TokenProvider>),
}

impl Auth {
    /// Create a bearer token authentication
    pub fn bearer(token: impl Into<String>) -> Self {
        Auth::Bearer(SecretString::new(token.into()))
    }

    /// Create an API key authentication
    pub fn api_key(key: impl Into<String>) -> Self {
        Auth::ApiKey(SecretString::new(key.into()))
    }

    /// Create a XJP key authentication (legacy)
    pub fn xjp_key(key: impl Into<String>) -> Self {
        Auth::XjpKey(SecretString::new(key.into()))
    }

    /// Create a dynamic token provider authentication
    pub fn token_provider(provider: impl TokenProvider + 'static) -> Self {
        Auth::TokenProvider(Box::new(provider))
    }

    /// Get the authorization header name and value
    pub(crate) async fn get_header(&self) -> Result<(&'static str, String), Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Auth::Bearer(token) => Ok(("Authorization", format!("Bearer {}", token.expose_secret()))),
            Auth::ApiKey(key) => Ok(("X-API-Key", key.expose_secret().clone())),
            Auth::XjpKey(key) => Ok(("XJP-KEY", key.expose_secret().clone())),
            Auth::TokenProvider(provider) => {
                let token = provider.get_token().await?;
                Ok(("Authorization", format!("Bearer {}", token.expose_secret())))
            }
        }
    }

    /// Check if this auth method supports token refresh
    pub(crate) fn supports_refresh(&self) -> bool {
        matches!(self, Auth::TokenProvider(_))
    }

    /// Refresh the token (only for TokenProvider)
    pub(crate) async fn refresh(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Auth::TokenProvider(provider) => provider.refresh_token().await,
            _ => Ok(()),
        }
    }
}

impl fmt::Debug for Auth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Auth::Bearer(_) => write!(f, "Auth::Bearer(****)"),
            Auth::ApiKey(_) => write!(f, "Auth::ApiKey(****)"),
            Auth::XjpKey(_) => write!(f, "Auth::XjpKey(****)"),
            Auth::TokenProvider(_) => write!(f, "Auth::TokenProvider(****)"),
        }
    }
}

/// Trait for providing dynamic tokens that can be refreshed
///
/// Implement this trait to support automatic token refresh on authentication failures.
/// The SDK will call `refresh_token` when it receives a 401 response and retry the request.
///
/// # Example
///
/// ```
/// # use secret_store_sdk::{TokenProvider, SecretString};
/// # use async_trait::async_trait;
/// # use std::sync::{Arc, Mutex};
/// #[derive(Clone)]
/// struct OAuthProvider {
///     client_id: String,
///     client_secret: SecretString,
///     current_token: Arc<Mutex<Option<String>>>,
/// }
///
/// #[async_trait]
/// impl TokenProvider for OAuthProvider {
///     async fn get_token(&self) -> Result<SecretString, Box<dyn std::error::Error + Send + Sync>> {
///         // Return cached token or fetch new one
///         let token = self.current_token.lock().unwrap()
///             .clone()
///             .ok_or("No token available")?;
///         Ok(SecretString::new(token))
///     }
///
///     async fn refresh_token(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
///         // Fetch new token from OAuth server
///         let new_token = "new-access-token"; // Actually fetch from OAuth endpoint
///         *self.current_token.lock().unwrap() = Some(new_token.to_string());
///         Ok(())
///     }
///
///     fn clone_box(&self) -> Box<dyn TokenProvider> {
///         Box::new(self.clone())
///     }
/// }
/// ```
#[async_trait]
pub trait TokenProvider: Send + Sync {
    /// Get the current token
    ///
    /// Called before each request to get the authentication token.
    /// Should return quickly, typically from a cached value.
    async fn get_token(&self) -> Result<SecretString, Box<dyn std::error::Error + Send + Sync>>;

    /// Refresh the token (called on 401 responses)
    ///
    /// Called when the server returns 401 Unauthorized.
    /// Should fetch a new token and update internal state.
    async fn refresh_token(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Clone the provider
    ///
    /// Required for the provider to be cloneable.
    /// Typically implemented as `Box::new(self.clone())`.
    fn clone_box(&self) -> Box<dyn TokenProvider>;
}

impl Clone for Box<dyn TokenProvider> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Static token provider (for testing or simple cases)
#[derive(Clone)]
#[allow(dead_code)]
pub struct StaticTokenProvider {
    token: SecretString,
}

impl StaticTokenProvider {
    /// Create a new static token provider
    #[allow(dead_code)]
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: SecretString::new(token.into()),
        }
    }
}

#[async_trait]
impl TokenProvider for StaticTokenProvider {
    async fn get_token(&self) -> Result<SecretString, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.token.clone())
    }

    async fn refresh_token(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Static tokens cannot be refreshed
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn TokenProvider> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_auth_headers() {
        let bearer = Auth::bearer("token123");
        let (header, value) = bearer.get_header().await.unwrap();
        assert_eq!(header, "Authorization");
        assert_eq!(value, "Bearer token123");

        let api_key = Auth::api_key("key456");
        let (header, value) = api_key.get_header().await.unwrap();
        assert_eq!(header, "X-API-Key");
        assert_eq!(value, "key456");

        let xjp_key = Auth::xjp_key("xjp789");
        let (header, value) = xjp_key.get_header().await.unwrap();
        assert_eq!(header, "XJP-KEY");
        assert_eq!(value, "xjp789");
    }

    #[test]
    fn test_auth_debug() {
        let auth = Auth::bearer("secret");
        let debug_str = format!("{:?}", auth);
        assert_eq!(debug_str, "Auth::Bearer(****)");
    }

    #[test]
    fn test_supports_refresh() {
        assert!(!Auth::bearer("token").supports_refresh());
        assert!(!Auth::api_key("key").supports_refresh());
        assert!(!Auth::xjp_key("key").supports_refresh());
        
        let provider = Auth::token_provider(StaticTokenProvider::new("token"));
        assert!(provider.supports_refresh());
    }
}