//! XJP Secret Store SDK for Rust
//!
//! A comprehensive SDK for interacting with the XJP Secret Store service,
//! providing secure storage and retrieval of secrets, configuration values,
//! and sensitive data.
//!
//! # Features
//!
//! - Async/await support with tokio runtime
//! - Automatic retries with exponential backoff
//! - Built-in caching with ETag/304 support
//! - Multiple authentication methods (Bearer, API Key, XJP Key)
//! - Batch operations with transactional support
//! - Environment export in multiple formats
//! - Version management and rollback
//! - Comprehensive error handling
//! - Secure value handling with zeroization
//!
//! # Example
//!
//! ```no_run
//! use secret_store_sdk::{Client, ClientBuilder, Auth};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = ClientBuilder::new("https://secret.example.com")
//!         .auth(Auth::bearer("your-api-key"))
//!         .build()?;
//!
//!     let secret = client.get_secret("production", "database-url", Default::default()).await?;
//!     println!("Secret version: {}", secret.version);
//!
//!     Ok(())
//! }
//! ```

#![deny(
    missing_docs,
    missing_debug_implementations,
    unsafe_code,
    unused_results,
    warnings
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod auth;
mod cache;
mod client;
mod config;
mod endpoints;
mod errors;
mod models;
/// Telemetry and observability support
#[cfg(feature = "metrics")]
pub mod telemetry;

#[cfg(not(feature = "metrics"))]
mod telemetry;
mod util;

pub use auth::{Auth, TokenProvider};
pub use cache::{CacheConfig, CacheStats};
pub use client::Client;
pub use config::{ClientBuilder, ClientConfig};
pub use errors::{Error, ErrorKind, Result};
pub use models::*;

// Re-export commonly used types
pub use secrecy::SecretString;

/// SDK version, matches Cargo.toml version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default timeout in milliseconds
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Default number of retries
pub const DEFAULT_RETRIES: u32 = 3;

/// Maximum cache entries
pub const DEFAULT_CACHE_MAX_ENTRIES: u64 = 10_000;

/// Default cache TTL in seconds
pub const DEFAULT_CACHE_TTL_SECS: u64 = 300;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
}