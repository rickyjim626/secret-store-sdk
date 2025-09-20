//! Utility functions

use std::time::Duration;

/// Parse max-age from Cache-Control header
#[allow(dead_code)]
pub fn parse_cache_control_max_age(headers: &http::HeaderMap) -> Option<Duration> {
    headers
        .get(http::header::CACHE_CONTROL)?
        .to_str()
        .ok()?
        .split(',')
        .map(|s| s.trim())
        .find(|s| s.starts_with("max-age="))?
        .strip_prefix("max-age=")?
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// Extract header value as string
pub fn header_str(headers: &http::HeaderMap, name: &str) -> Option<String> {
    headers.get(name)?.to_str().ok().map(|s| s.to_string())
}

/// Generate a new request ID
pub fn generate_request_id() -> String {
    format!("sdk-{}", uuid::Uuid::new_v4())
}

/// URL encode a path segment
pub fn encode_path(s: &str) -> String {
    use percent_encoding::{AsciiSet, CONTROLS};

    // Define which characters to encode - RFC 3986 unreserved characters plus common safe chars
    const FRAGMENT: &AsciiSet = &CONTROLS
        .add(b' ')
        .add(b'"')
        .add(b'<')
        .add(b'>')
        .add(b'`')
        .add(b'#')
        .add(b'?')
        .add(b'{')
        .add(b'}')
        .add(b'/')
        .add(b'%');

    percent_encoding::utf8_percent_encode(s, FRAGMENT).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cache_control() {
        let mut headers = http::HeaderMap::new();
        let _ = headers.insert(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("private, max-age=300"),
        );

        let duration = parse_cache_control_max_age(&headers).unwrap();
        assert_eq!(duration.as_secs(), 300);
    }

    #[test]
    fn test_encode_path() {
        assert_eq!(encode_path("hello world"), "hello%20world");
        assert_eq!(encode_path("test/path"), "test%2Fpath");
        assert_eq!(encode_path("test-namespace"), "test-namespace");
        assert_eq!(encode_path("my-key"), "my-key");
        assert_eq!(encode_path("my_key"), "my_key");
        assert_eq!(encode_path("my.key"), "my.key");
    }
}
