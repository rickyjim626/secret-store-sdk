//! Error types and handling for the XJP Secret Store SDK
//!
//! This module defines the error types that can be returned by SDK operations.
//! Errors are designed to provide detailed information for debugging while
//! maintaining security by not exposing sensitive data.
//!
//! # Error Categories
//!
//! The SDK uses a structured error system with the following main categories:
//!
//! - **HTTP Errors**: API errors with status code, category, and message
//! - **Network Errors**: Connection and DNS failures
//! - **Timeout**: Request deadline exceeded
//! - **Configuration**: Invalid client configuration
//! - **Deserialization**: Failed to parse API responses
//!
//! # Example
//!
//! ```no_run
//! # use secret_store_sdk::{Client, Error};
//! # async fn example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
//! match client.get_secret("prod", "key", Default::default()).await {
//!     Ok(secret) => println!("Got secret v{}", secret.version),
//!     Err(Error::Http { status: 404, .. }) => println!("Secret not found"),
//!     Err(Error::Http { status: 403, .. }) => println!("Access denied"),
//!     Err(Error::Timeout) => println!("Request timed out"),
//!     Err(e) => return Err(e.into()),
//! }
//! # Ok(())
//! # }
//! ```

use thiserror::Error;

/// Result type alias for the SDK
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for the SDK
#[derive(Error, Debug)]
pub enum Error {
    /// HTTP error from the API
    #[error("http {status}: {category} - {message} (req={request_id:?})")]
    Http {
        /// HTTP status code
        status: u16,
        /// Error category from server (auth, validation, rate_limit, etc.)
        category: String,
        /// Error message from server
        message: String,
        /// Request ID from x-request-id header
        request_id: Option<String>,
    },

    /// Deserialization error
    #[error("deserialize: {0}")]
    Deserialize(String),

    /// Network error
    #[error("network: {0}")]
    Network(String),

    /// Request timeout
    #[error("timeout")]
    Timeout,

    /// Configuration error
    #[error("config: {0}")]
    Config(String),

    /// Other errors
    #[error("other: {0}")]
    Other(String),
}

/// Error categories returned by the server
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    /// Authentication/authorization errors (401/403)
    Auth,
    /// Validation errors (400)
    Validation,
    /// Resource not found (404)
    NotFound,
    /// Rate limit exceeded (429)
    RateLimit,
    /// Request timeout (408)
    Timeout,
    /// Internal server error (500)
    Internal,
    /// Service unavailable (503)
    ServiceUnavailable,
    /// Cryptographic operation error
    Crypto,
    /// Configuration error
    Config,
    /// Other/unknown error
    Other,
}

impl ErrorKind {
    /// Parse error kind from server error category string
    pub fn from_category(category: &str) -> Self {
        match category {
            "auth" => ErrorKind::Auth,
            "validation" => ErrorKind::Validation,
            "not_found" => ErrorKind::NotFound,
            "rate_limit" => ErrorKind::RateLimit,
            "timeout" => ErrorKind::Timeout,
            "internal" => ErrorKind::Internal,
            "service" => ErrorKind::ServiceUnavailable,
            "crypto" => ErrorKind::Crypto,
            "config" => ErrorKind::Config,
            _ => ErrorKind::Other,
        }
    }
}

impl Error {
    /// Get the error kind for categorization
    pub fn kind(&self) -> ErrorKind {
        match self {
            Error::Http { category, .. } => ErrorKind::from_category(category),
            Error::Timeout => ErrorKind::Timeout,
            Error::Config(_) => ErrorKind::Config,
            _ => ErrorKind::Other,
        }
    }

    /// Check if the error is retryable
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Http { status, .. } => matches!(status, 429 | 500 | 502 | 503 | 504),
            Error::Network(_) => true,
            Error::Timeout => true,
            _ => false,
        }
    }

    /// Get the HTTP status code if this is an HTTP error
    pub fn status_code(&self) -> Option<u16> {
        match self {
            Error::Http { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// Get the request ID if available
    pub fn request_id(&self) -> Option<&str> {
        match self {
            Error::Http { request_id, .. } => request_id.as_deref(),
            _ => None,
        }
    }

    /// Create an HTTP error from server response
    pub(crate) fn from_response(
        status: u16,
        error: &str,
        message: &str,
        request_id: Option<String>,
    ) -> Self {
        Error::Http {
            status,
            category: error.to_string(),
            message: message.to_string(),
            request_id,
        }
    }
}

/// Server error response structure
#[derive(Debug, serde::Deserialize)]
pub(crate) struct ErrorResponse {
    pub error: String,
    pub message: String,
    #[allow(dead_code)]
    pub timestamp: String,
    pub status: u16,
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            Error::Timeout
        } else if err.is_connect() || err.is_request() {
            Error::Network(err.to_string())
        } else if err.is_decode() {
            Error::Deserialize(err.to_string())
        } else {
            Error::Other(err.to_string())
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Deserialize(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_kind_from_category() {
        assert_eq!(ErrorKind::from_category("auth"), ErrorKind::Auth);
        assert_eq!(ErrorKind::from_category("validation"), ErrorKind::Validation);
        assert_eq!(ErrorKind::from_category("not_found"), ErrorKind::NotFound);
        assert_eq!(ErrorKind::from_category("unknown"), ErrorKind::Other);
    }

    #[test]
    fn test_error_is_retryable() {
        let err = Error::Http {
            status: 429,
            category: "rate_limit".to_string(),
            message: "Too many requests".to_string(),
            request_id: Some("req-123".to_string()),
        };
        assert!(err.is_retryable());

        let err = Error::Http {
            status: 404,
            category: "not_found".to_string(),
            message: "Secret not found".to_string(),
            request_id: None,
        };
        assert!(!err.is_retryable());

        let err = Error::Network("Connection failed".to_string());
        assert!(err.is_retryable());

        let err = Error::Config("Invalid URL".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_error_status_code() {
        let err = Error::Http {
            status: 401,
            category: "auth".to_string(),
            message: "Unauthorized".to_string(),
            request_id: None,
        };
        assert_eq!(err.status_code(), Some(401));

        let err = Error::Timeout;
        assert_eq!(err.status_code(), None);
    }

    #[test]
    fn test_error_request_id() {
        let err = Error::Http {
            status: 500,
            category: "internal".to_string(),
            message: "Server error".to_string(),
            request_id: Some("req-456".to_string()),
        };
        assert_eq!(err.request_id(), Some("req-456"));

        let err = Error::Network("Failed".to_string());
        assert_eq!(err.request_id(), None);
    }
}