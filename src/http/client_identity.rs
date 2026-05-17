//! Client identity helpers for public abuse controls.
//!
//! Rate limiting prefers trusted network identity, but can fall back to request
//! metadata in route tests where no socket address exists.

use axum::http::HeaderMap;

/// Builds a stable request key for public rate-limit buckets.
pub fn request_rate_limit_key(headers: &HeaderMap, trust_proxy_headers: bool) -> String {
    if trust_proxy_headers && let Some(forwarded_ip) = forwarded_for_ip(headers) {
        return format!("ip:{forwarded_ip}");
    }
    if trust_proxy_headers && let Some(real_ip) = header_value(headers, "x-real-ip") {
        return format!("ip:{real_ip}");
    }

    headers
        .get("x-install-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("install:{value}"))
        .unwrap_or_else(|| "anonymous".to_string())
}

fn forwarded_for_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn header_value(headers: &HeaderMap, name: &'static str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::request_rate_limit_key;
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn uses_install_id_when_no_socket_is_available() {
        let mut headers = HeaderMap::new();
        headers.insert("x-install-id", HeaderValue::from_static("install-1"));

        assert_eq!(request_rate_limit_key(&headers, false), "install:install-1");
    }

    #[test]
    fn uses_forwarded_for_when_proxy_headers_are_trusted() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.1, 10.0.0.1"),
        );

        assert_eq!(request_rate_limit_key(&headers, true), "ip:203.0.113.1");
    }
}
