# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2025-01-21

### Fixed
- **Breaking idempotency bug**: Fixed header inconsistency where `batch_operate` used `Idempotency-Key` while all other methods used `X-Idempotency-Key`. This prevented batch operations from being properly idempotent.
- **Potential panic**: Fixed unwrap on `try_clone()` in retry logic that could cause panics with non-cloneable request bodies.
- **Retry timeout**: Fixed hardcoded 60-second retry timeout that ignored configured timeout and retry values. Now calculates dynamically based on `(retries + 1) * timeout + 30s buffer`.
- **Error classification**: Token refresh failures are now correctly mapped to `Error::Network` instead of `Error::Config` to properly indicate transient network issues rather than configuration problems.
- **Documentation**: Documented that `ExportEnvOpts::use_cache` is currently unimplemented and reserved for future use.

### Changed
- Improved error handling robustness in retry mechanism
- Better timeout calculation for retry backoff strategy

## [0.1.0] - 2025-01-20

### Added

#### Core Features
- Full XJP Secret Store API v2 compatibility
- Async/await support with Tokio runtime
- Comprehensive error handling with detailed error types
- Request ID tracking for all operations
- Type-safe secret value handling with `secrecy` crate

#### Authentication
- Multiple authentication methods:
  - Bearer token (highest priority)
  - API key
  - XJP key (legacy)
  - Dynamic token provider with automatic refresh on 401 responses
- Automatic token refresh support for long-running applications

#### Secret Operations
- CRUD operations for secrets:
  - `get_secret` with ETag/304 conditional request support
  - `put_secret` with TTL and metadata support
  - `delete_secret` with soft delete
  - `list_secrets` with pagination and prefix filtering
- Batch operations:
  - `batch_get` for retrieving multiple secrets
  - `batch_operate` for bulk put/delete with transaction support
- Environment export in multiple formats:
  - JSON
  - Dotenv (.env)
  - Shell script
  - Docker Compose
  - Kubernetes ConfigMap

#### Caching
- Built-in high-performance caching using `moka`
- ETag-based cache invalidation
- Configurable TTL and max entries
- Cache statistics (hit rate, hits/misses/evictions)
- Manual cache management (clear, invalidate specific entries)

#### Version Management
- List all versions of a secret
- Retrieve specific versions
- Rollback to previous versions
- Version metadata tracking

#### Namespace Management
- List all namespaces
- Get namespace details and statistics
- Initialize namespaces from templates

#### Audit Trail
- Query audit logs with filtering:
  - By namespace
  - By time range
  - By actor
  - By action type
  - By success/failure
- Pagination support for large result sets
- Detailed audit entry information

#### Reliability
- Automatic retry with exponential backoff
- Configurable retry count and timeouts
- Idempotency key support for write operations
- Connection pooling for performance

#### Observability
- Optional OpenTelemetry metrics integration
- Metrics exposed:
  - Request count by method/path/status
  - Request duration histogram
  - Error count by type
  - Cache hit/miss rates
  - Active connections gauge
  - Retry attempts counter

#### Developer Experience
- Comprehensive examples for all major features
- Builder pattern for client configuration
- Sensible defaults for all options
- Zero-copy operations where possible
- Full API documentation with examples

#### Testing
- Unit tests for all modules
- Integration tests with wiremock
- Property-based tests for critical paths
- Performance benchmarks with Criterion

#### Platform Support
- Native TLS and rustls support
- WebAssembly (WASM) compatibility
- Configurable HTTP/2 support
- Optional insecure HTTP for development

### Security
- HTTPS enforced by default
- Secure secret handling with automatic zeroization
- No secrets in debug output or logs
- Support for custom CA certificates

### Performance
- Connection pooling with keep-alive
- HTTP/2 multiplexing when available
- Efficient batch operations
- Minimal allocations in hot paths
- Cache-friendly data structures

### Documentation
- Comprehensive README with examples
- API documentation for all public types
- Migration guide from other SDKs
- Best practices guide
- Performance tuning tips

[Unreleased]: https://github.com/rickyjim626/secret-store-sdk/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/rickyjim626/secret-store-sdk/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/rickyjim626/secret-store-sdk/releases/tag/v0.1.0