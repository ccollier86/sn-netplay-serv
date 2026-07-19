//! Client identity helpers for public abuse controls.
//!
//! Rate limiting prefers trusted network identity, but can fall back to request
//! metadata in route tests where no socket address exists.

use axum::http::HeaderMap;
use std::net::IpAddr;

/// Builds a stable request key for public rate-limit buckets.
pub fn request_rate_limit_key(headers: &HeaderMap, trust_proxy_headers: bool) -> String {
    if trust_proxy_headers && let Some(proxy_ip) = trusted_proxy_ip(headers) {
        return format!("ip:{proxy_ip}");
    }

    headers
        .get("x-install-id")
        .or_else(|| headers.get("x-installation-id"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("install:{value}"))
        .unwrap_or_else(|| "anonymous".to_string())
}

/// Builds a non-spoofable rate-limit key for capability-authenticated sockets.
///
/// Unverified installation headers are deliberately ignored. Production uses
/// the trusted proxy address when configured and otherwise the actual TCP peer.
pub fn capability_request_rate_limit_key(
    headers: &HeaderMap,
    trust_proxy_headers: bool,
    peer_ip: Option<IpAddr>,
) -> String {
    if trust_proxy_headers && let Some(proxy_ip) = trusted_proxy_ip(headers) {
        return format!("ip:{proxy_ip}");
    }

    peer_ip
        .map(|peer_ip| format!("ip:{peer_ip}"))
        .unwrap_or_else(|| "anonymous".to_string())
}

fn trusted_proxy_ip(headers: &HeaderMap) -> Option<IpAddr> {
    parsed_header_ip(headers, "cf-connecting-ip")
        .or_else(|| parsed_header_ip(headers, "x-real-ip"))
        .or_else(|| forwarded_for_last_valid_ip(headers))
}

fn parsed_header_ip(headers: &HeaderMap, name: &'static str) -> Option<IpAddr> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .and_then(|value| value.parse::<IpAddr>().ok())
}

fn forwarded_for_last_valid_ip(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .split(',')
                .rev()
                .map(str::trim)
                .find_map(|candidate| candidate.parse::<IpAddr>().ok())
        })
}

#[cfg(test)]
mod tests {
    use super::{capability_request_rate_limit_key, request_rate_limit_key};
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
            HeaderValue::from_static("198.51.100.9, 203.0.113.1"),
        );

        assert_eq!(request_rate_limit_key(&headers, true), "ip:203.0.113.1");
    }

    #[test]
    fn proxy_headers_prefer_cloudflare_connecting_ip() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "cf-connecting-ip",
            HeaderValue::from_static("198.51.100.44"),
        );
        headers.insert("x-real-ip", HeaderValue::from_static("198.51.100.45"));
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("198.51.100.46, 203.0.113.1"),
        );

        assert_eq!(request_rate_limit_key(&headers, true), "ip:198.51.100.44");
    }

    #[test]
    fn invalid_proxy_headers_fall_back_to_install_id() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("bad, also-bad"));
        headers.insert("x-install-id", HeaderValue::from_static("install-1"));

        assert_eq!(request_rate_limit_key(&headers, true), "install:install-1");
    }

    #[test]
    fn capability_key_ignores_spoofable_installation_headers() {
        let mut first = HeaderMap::new();
        first.insert("x-install-id", HeaderValue::from_static("spoof-one"));
        let mut second = HeaderMap::new();
        second.insert("x-install-id", HeaderValue::from_static("spoof-two"));
        let peer_ip = "198.51.100.20".parse().expect("peer ip");

        assert_eq!(
            capability_request_rate_limit_key(&first, false, Some(peer_ip)),
            capability_request_rate_limit_key(&second, false, Some(peer_ip))
        );
        assert_eq!(
            capability_request_rate_limit_key(&first, false, Some(peer_ip)),
            "ip:198.51.100.20"
        );
    }

    #[test]
    fn capability_key_prefers_trusted_proxy_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("cf-connecting-ip", HeaderValue::from_static("203.0.113.8"));
        let peer_ip = "10.0.0.4".parse().expect("peer ip");

        assert_eq!(
            capability_request_rate_limit_key(&headers, true, Some(peer_ip)),
            "ip:203.0.113.8"
        );
    }
}
