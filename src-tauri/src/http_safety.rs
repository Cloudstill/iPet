//! URL safety helpers for outbound HTTP from custom tools.
//!
//! Two layers:
//! - [`validate_url_syntax`]: cheap, synchronous — call at save time and before
//!   every request to reject malformed URLs, non-http(s) schemes and IP
//!   literals pointing at loopback/private ranges.
//! - [`validate_url_runtime`]: async — also resolves the host through DNS and
//!   refuses any address in a disallowed range. Use right before sending.

use crate::app_error::{AppError, AppResult};
use reqwest::Url;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Hard timeout for a single HTTP tool request (connect + read).
pub const HTTP_TIMEOUT_SECS: u64 = 30;
/// Maximum number of HTTP redirects to follow.
pub const HTTP_MAX_REDIRECTS: usize = 5;
/// Maximum response body size that will be buffered into memory (2 MiB).
pub const HTTP_MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// Parse a URL string and reject schemes / hosts we never want to contact.
///
/// This is the fast, side-effect-free check suitable for the save-time path
/// (e.g. when persisting a custom tool definition).
pub fn validate_url_syntax(raw: &str) -> AppResult<Url> {
    let url = Url::parse(raw.trim())
        .map_err(|err| AppError::InvalidInput(format!("URL 解析失败: {err}")))?;
    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(AppError::InvalidInput(
            "URL 协议必须是 http 或 https".to_string(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| AppError::InvalidInput("URL 缺少主机名".to_string()))?;
    if let Some(addr) = parse_host_ip(host) {
        if is_disallowed_ip(addr) {
            return Err(AppError::InvalidInput(format!(
                "URL 指向受限地址 {addr}（loopback / 私有 / 链路本地等）"
            )));
        }
    }
    Ok(url)
}

/// Parse the URL and resolve its host through DNS, rejecting any answer that
/// falls in a disallowed range.
///
/// Hostnames that already parse as an IP literal skip the DNS step — they were
/// vetted by [`validate_url_syntax`].
pub async fn validate_url_runtime(raw: &str) -> AppResult<Url> {
    let url = validate_url_syntax(raw)?;
    let host = url
        .host_str()
        .expect("validate_url_syntax guarantees a host");
    if parse_host_ip(host).is_some() {
        return Ok(url);
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| AppError::InvalidInput("无法确定 URL 端口".to_string()))?;
    let target = format!("{host}:{port}");
    let addrs = tokio::net::lookup_host(target.as_str())
        .await
        .map_err(|err| AppError::InvalidInput(format!("DNS 解析失败: {err}")))?;
    let mut any = false;
    for addr in addrs {
        any = true;
        if is_disallowed_ip(addr.ip()) {
            return Err(AppError::InvalidInput(format!(
                "DNS 解析到受限地址 {}（loopback / 私有 / 链路本地等）",
                addr.ip()
            )));
        }
    }
    if !any {
        return Err(AppError::InvalidInput(format!("主机名无法解析: {host}")));
    }
    Ok(url)
}

fn parse_host_ip(host: &str) -> Option<IpAddr> {
    // url::Url returns bracketed IPv6 literals as plain text already, but be
    // defensive in case a caller hands us the bracketed form.
    let trimmed = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(host);
    trimmed.parse::<IpAddr>().ok()
}

fn is_disallowed_ip(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => is_disallowed_ipv4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_disallowed_ipv4(v4);
            }
            is_disallowed_ipv6(v6)
        }
    }
}

fn is_disallowed_ipv4(addr: Ipv4Addr) -> bool {
    if addr.is_loopback()
        || addr.is_private()
        || addr.is_link_local()
        || addr.is_unspecified()
        || addr.is_broadcast()
        || addr.is_documentation()
    {
        return true;
    }
    // CGNAT shared address space 100.64.0.0/10
    let octets = addr.octets();
    octets[0] == 100 && (octets[1] & 0xc0) == 0x40
}

fn is_disallowed_ipv6(addr: Ipv6Addr) -> bool {
    if addr.is_loopback() || addr.is_unspecified() {
        return true;
    }
    let segments = addr.segments();
    // Unique local fc00::/7
    if (segments[0] & 0xfe00) == 0xfc00 {
        return true;
    }
    // Link-local fe80::/10
    if (segments[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_http_schemes() {
        for raw in [
            "file:///etc/passwd",
            "ftp://example.com/x",
            "javascript:alert(1)",
        ] {
            assert!(validate_url_syntax(raw).is_err(), "should reject {raw}");
        }
    }

    #[test]
    fn rejects_ip_literals_in_disallowed_ranges() {
        for raw in [
            "http://127.0.0.1/",
            "http://10.0.0.1/",
            "http://192.168.1.1/",
            "http://169.254.169.254/latest/meta-data",
            "http://0.0.0.0/",
            "http://[::1]/",
            "http://[fc00::1]/",
            "http://[fe80::1]/",
            "http://[::ffff:127.0.0.1]/",
            "http://100.64.0.1/",
        ] {
            let err = validate_url_syntax(raw).expect_err(&format!("should reject {raw}"));
            assert!(
                matches!(err, AppError::InvalidInput(_)),
                "unexpected error kind for {raw}: {err:?}"
            );
        }
    }

    #[test]
    fn accepts_public_addresses() {
        for raw in [
            "https://example.com/path",
            "http://93.184.216.34/",      // example.com
            "https://[2606:2800:220:1::1]/",
            "http://101.64.0.1/",         // outside 100.64/10
        ] {
            validate_url_syntax(raw).unwrap_or_else(|e| panic!("should accept {raw}: {e:?}"));
        }
    }

    #[test]
    fn rejects_malformed_input() {
        assert!(validate_url_syntax("").is_err());
        assert!(validate_url_syntax("not-a-url").is_err());
        assert!(validate_url_syntax("http://").is_err());
    }
}
