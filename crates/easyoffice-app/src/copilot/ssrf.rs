//! 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd., see THIRD_PARTY_NOTICES.md)

use easyoffice_formats::EasyOfficeError;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};

use super::types::MAX_BASE_URL_BYTES;

const MAX_RESOLVED_ADDRS: usize = 32;

/// IPv4 CIDR blocklist `(network_be, mask_be)` matching private, loopback, link-local,
/// CGNAT, benchmarking, documentation, multicast, and reserved ranges.
const BLOCKED_IPV4_CIDRS: [(u32, u32); 14] = [
    (0x0000_0000, 0xFF00_0000), // 0.0.0.0/8
    (0x0A00_0000, 0xFF00_0000), // 10.0.0.0/8
    (0x6440_0000, 0xFFC0_0000), // 100.64.0.0/10
    (0x7F00_0000, 0xFF00_0000), // 127.0.0.0/8
    (0xA9FE_0000, 0xFFFF_0000), // 169.254.0.0/16
    (0xAC10_0000, 0xFFF0_0000), // 172.16.0.0/12
    (0xC000_0000, 0xFFFF_FF00), // 192.0.0.0/24
    (0xC000_0200, 0xFFFF_FF00), // 192.0.2.0/24
    (0xC058_6300, 0xFFFF_FF00), // 192.88.99.0/24
    (0xC0A8_0000, 0xFFFF_0000), // 192.168.0.0/16
    (0xC612_0000, 0xFFFE_0000), // 198.18.0.0/15
    (0xC633_6400, 0xFFFF_FF00), // 198.51.100.0/24
    (0xCB00_7100, 0xFFFF_FF00), // 203.0.113.0/24
    (0xE000_0000, 0xE000_0000), // 224.0.0.0/3
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedEndpoint {
    pub url: String,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub is_https: bool,
    pub resolved_addrs: Vec<SocketAddr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointTarget<'a> {
    OpenAiChat {
        default_prefix: &'a str,
    },
    AnthropicMessages,
    GoogleGenerate {
        model: &'a str,
    },
    /// 같은 모델의 `streamGenerateContent`(스트림 응답).
    GoogleStream {
        model: &'a str,
    },
    ModelsList {
        default_prefix: &'a str,
    },
}

struct ParsedUrl<'a> {
    is_https: bool,
    host: &'a str,
    is_ipv6_bracketed: bool,
    port: u16,
    has_explicit_port: bool,
    path: &'a str,
}

#[inline]
pub fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let bits = u32::from(ip);
    !BLOCKED_IPV4_CIDRS
        .iter()
        .any(|&(net, mask)| (bits & mask) == net)
}

#[inline]
pub fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    let bits = u128::from(ip);
    // Global unicast `2000::/3` (`^[23]` in canonical hex).
    if (bits & 0xE000_0000_0000_0000_0000_0000_0000_0000)
        != 0x2000_0000_0000_0000_0000_0000_0000_0000
    {
        return false;
    }
    // Documentation `2001:db8::/32`, Teredo `2001::/32`, 6to4 `2002::/16`.
    let top32 = (bits >> 96) as u32;
    let top16 = (bits >> 112) as u16;
    top32 != 0x2001_0DB8 && top32 != 0x2001_0000 && top16 != 0x2002
}

#[inline]
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_ipv4(v4),
        IpAddr::V6(v6) => is_public_ipv6(v6),
    }
}

fn invalid_endpoint(msg: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(msg.to_string())
}

fn security_endpoint(msg: &str) -> EasyOfficeError {
    EasyOfficeError::SecurityViolation(msg.to_string())
}

fn parse_port_strict(raw: &str) -> Option<u16> {
    if raw.is_empty()
        || raw.len() > 5
        || raw.starts_with('0')
        || !raw.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    raw.parse::<u16>().ok().filter(|&p| p > 0)
}

fn parse_url_strict(raw: &str) -> Result<ParsedUrl<'_>, EasyOfficeError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_BASE_URL_BYTES {
        return Err(invalid_endpoint("모델 엔드포인트 URL을 입력하세요."));
    }
    if trimmed
        .bytes()
        .any(|b| b <= 0x20 || b == 0x7F || b == b'\\')
    {
        return Err(invalid_endpoint(
            "모델 엔드포인트 URL에 허용되지 않는 문자 또는 공백이 있습니다.",
        ));
    }
    if trimmed.contains('?') || trimmed.contains('#') {
        return Err(invalid_endpoint(
            "모델 엔드포인트 URL에는 쿼리 문자열(?)이나 프래그먼트(#)를 포함할 수 없습니다.",
        ));
    }

    let (is_https, rest) = if trimmed.len() >= 8 && trimmed[..8].eq_ignore_ascii_case("https://") {
        (true, &trimmed[8..])
    } else if trimmed.len() >= 7 && trimmed[..7].eq_ignore_ascii_case("http://") {
        (false, &trimmed[7..])
    } else {
        return Err(invalid_endpoint(
            "모델 엔드포인트는 https:// 또는 허용된 로컬 http:// 주소여야 합니다.",
        ));
    };

    let (authority, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, ""),
    };
    if authority.is_empty() {
        return Err(invalid_endpoint("모델 엔드포인트 호스트가 비어 있습니다."));
    }
    if authority.contains('@') || authority.contains('%') {
        return Err(security_endpoint(
            "모델 엔드포인트 URL에 사용자 인증 정보(@)나 인코딩된 문자를 포함할 수 없습니다.",
        ));
    }

    let default_port = if is_https { 443 } else { 80 };
    if let Some(after_open) = authority.strip_prefix('[') {
        let Some(close_idx) = after_open.find(']') else {
            return Err(invalid_endpoint("IPv6 주소 대괄호가 올바르지 않습니다."));
        };
        let host = &after_open[..close_idx];
        let tail = &after_open[close_idx + 1..];
        let (port, has_explicit_port) = if tail.is_empty() {
            (default_port, false)
        } else if let Some(p_raw) = tail.strip_prefix(':') {
            let p = parse_port_strict(p_raw)
                .ok_or_else(|| invalid_endpoint("포트 번호가 올바르지 않습니다."))?;
            (p, true)
        } else {
            return Err(invalid_endpoint(
                "모델 엔드포인트 호스트 형식이 올바르지 않습니다.",
            ));
        };
        if host.parse::<Ipv6Addr>().is_err() {
            return Err(invalid_endpoint("IPv6 주소가 올바르지 않습니다."));
        }
        Ok(ParsedUrl {
            is_https,
            host,
            is_ipv6_bracketed: true,
            port,
            has_explicit_port,
            path,
        })
    } else {
        let (host, port, has_explicit_port) = match authority.split_once(':') {
            Some((h, p_raw)) => {
                if p_raw.contains(':') {
                    return Err(invalid_endpoint("포트 구문이 올바르지 않습니다."));
                }
                let p = parse_port_strict(p_raw)
                    .ok_or_else(|| invalid_endpoint("포트 번호가 올바르지 않습니다."))?;
                (h, p, true)
            }
            None => (authority, default_port, false),
        };
        if host.is_empty() {
            return Err(invalid_endpoint("모델 엔드포인트 호스트가 비어 있습니다."));
        }
        Ok(ParsedUrl {
            is_https,
            host,
            is_ipv6_bracketed: false,
            port,
            has_explicit_port,
            path,
        })
    }
}

pub fn clean_google_model_segment(raw_model: &str) -> Result<&str, EasyOfficeError> {
    let trimmed = raw_model.trim();
    let seg = trimmed.strip_prefix("models/").unwrap_or(trimmed);
    if seg.is_empty()
        || seg.len() > 128
        || seg.contains("..")
        || !seg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        return Err(security_endpoint(
            "Google 모델 ID에는 영숫자·하이픈(-)·밑줄(_)·마침표(.)만 사용할 수 있습니다.",
        ));
    }
    Ok(seg)
}

/// Gemini 호출 경로. 이미 모델 메서드(`:generateContent`·`:streamGenerateContent`)까지 적은 주소는 메서드만 바꾼다.
fn google_method_path(trimmed: &str, seg: &str, method: &str) -> String {
    let named = trimmed
        .strip_suffix(":streamGenerateContent")
        .or_else(|| trimmed.strip_suffix(":generateContent"));
    if let Some(base) = named {
        format!("{base}:{method}")
    } else if trimmed.is_empty() {
        format!("/v1beta/models/{seg}:{method}")
    } else if trimmed.ends_with("/models") {
        format!("{trimmed}/{seg}:{method}")
    } else {
        format!("{trimmed}/models/{seg}:{method}")
    }
}

fn resolve_target_path(
    raw_path: &str,
    target: EndpointTarget<'_>,
) -> Result<String, EasyOfficeError> {
    if raw_path.contains("..") || raw_path.contains('%') {
        return Err(security_endpoint(
            "모델 엔드포인트 경로에 상위 경로(..)나 인코딩된 문자를 포함할 수 없습니다.",
        ));
    }
    let trimmed = raw_path.trim_end_matches('/');
    let out = match target {
        EndpointTarget::OpenAiChat { default_prefix } => {
            if trimmed.ends_with("/chat/completions") {
                trimmed.to_string()
            } else if trimmed.is_empty() {
                format!("{default_prefix}/chat/completions")
            } else {
                format!("{trimmed}/chat/completions")
            }
        }
        EndpointTarget::AnthropicMessages => {
            if trimmed.ends_with("/messages") {
                trimmed.to_string()
            } else if trimmed.is_empty() {
                "/v1/messages".to_string()
            } else {
                format!("{trimmed}/messages")
            }
        }
        EndpointTarget::GoogleGenerate { model } => google_method_path(
            trimmed,
            clean_google_model_segment(model)?,
            "generateContent",
        ),
        EndpointTarget::GoogleStream { model } => google_method_path(
            trimmed,
            clean_google_model_segment(model)?,
            "streamGenerateContent",
        ),
        EndpointTarget::ModelsList { default_prefix } => {
            if trimmed.ends_with("/models") || trimmed.ends_with("/tags") {
                trimmed.to_string()
            } else if let Some(base) = trimmed.strip_suffix("/chat/completions") {
                format!("{base}/models")
            } else if let Some(base) = trimmed.strip_suffix("/messages") {
                format!("{base}/models")
            } else if trimmed.is_empty() {
                format!("{default_prefix}/models")
            } else {
                format!("{trimmed}/models")
            }
        }
    };
    Ok(out)
}

fn is_disallowed_internal_hostname(host: &str) -> bool {
    let lower = host.trim_end_matches('.').to_ascii_lowercase();
    if lower.is_empty() {
        return true;
    }
    if lower == "localhost"
        || lower.ends_with(".localhost")
        || lower.ends_with(".local")
        || lower.ends_with(".internal")
        || lower.ends_with(".arpa")
    {
        return true;
    }
    // Reject single-label numeric/hex/octal obfuscated IPv4 literals (e.g. `2130706433`, `0x7f000001`).
    if !lower.contains('.')
        && (lower.bytes().all(|b| b.is_ascii_digit()) || lower.starts_with("0x"))
    {
        return true;
    }
    // Reject dotted numeric forms with leading-zero octal or hex octets when strict IPv4 failed.
    if lower.split('.').all(|seg| {
        !seg.is_empty() && (seg.bytes().all(|b| b.is_ascii_digit()) || seg.starts_with("0x"))
    }) {
        return true;
    }
    false
}

pub fn validate_remote_https_endpoint(raw_url: &str) -> Result<ValidatedEndpoint, EasyOfficeError> {
    validate_remote_https_target(
        raw_url,
        EndpointTarget::OpenAiChat {
            default_prefix: "/v1",
        },
    )
}

pub fn validate_remote_https_target(
    raw_url: &str,
    target: EndpointTarget<'_>,
) -> Result<ValidatedEndpoint, EasyOfficeError> {
    let parsed = parse_url_strict(raw_url)?;
    if !parsed.is_https {
        return Err(security_endpoint(
            "원격 모델 엔드포인트는 https:// 프로토콜만 허용됩니다.",
        ));
    }

    let resolved_addrs: Vec<SocketAddr> = if parsed.is_ipv6_bracketed {
        let ip: Ipv6Addr = parsed
            .host
            .parse()
            .map_err(|_| invalid_endpoint("IPv6 주소가 올바르지 않습니다."))?;
        if !is_public_ipv6(ip) {
            return Err(security_endpoint(
                "사설·루프백 또는 내부 IPv6 대역의 모델 엔드포인트는 허용되지 않습니다.",
            ));
        }
        vec![SocketAddr::new(IpAddr::V6(ip), parsed.port)]
    } else if let Ok(ip) = parsed.host.parse::<Ipv4Addr>() {
        if !is_public_ipv4(ip) {
            return Err(security_endpoint(
                "사설·루프백 또는 내부 IPv4 대역의 모델 엔드포인트는 허용되지 않습니다.",
            ));
        }
        vec![SocketAddr::new(IpAddr::V4(ip), parsed.port)]
    } else {
        if is_disallowed_internal_hostname(parsed.host) {
            return Err(security_endpoint(
                "내부 호스트 이름 또는 비표준 IP 표기는 허용되지 않습니다.",
            ));
        }
        let clean_host = parsed.host.trim_end_matches('.');
        let iter = (clean_host, parsed.port)
            .to_socket_addrs()
            .map_err(|_| invalid_endpoint("모델 엔드포인트 호스트 이름을 확인할 수 없습니다."))?;
        let addrs: Vec<SocketAddr> = iter.take(MAX_RESOLVED_ADDRS).collect();
        if addrs.is_empty() || addrs.iter().any(|a| !is_public_ip(a.ip())) {
            return Err(security_endpoint(
                "사설·루프백 또는 내부 네트워크로 해석되는 모델 엔드포인트는 차단되었습니다.",
            ));
        }
        addrs
    };

    let path = resolve_target_path(parsed.path, target)?;
    let host_part = if parsed.is_ipv6_bracketed {
        format!("[{}]", parsed.host)
    } else {
        parsed.host.trim_end_matches('.').to_string()
    };
    let port_part = if parsed.has_explicit_port {
        format!(":{}", parsed.port)
    } else {
        String::new()
    };
    let url = format!("https://{host_part}{port_part}{path}");

    Ok(ValidatedEndpoint {
        url,
        host: parsed.host.trim_end_matches('.').to_string(),
        port: parsed.port,
        path,
        is_https: true,
        resolved_addrs,
    })
}

pub fn validate_local_http_endpoint(raw_url: &str) -> Result<ValidatedEndpoint, EasyOfficeError> {
    validate_local_http_target(
        raw_url,
        EndpointTarget::OpenAiChat {
            default_prefix: "/v1",
        },
    )
}

pub fn validate_local_http_target(
    raw_url: &str,
    target: EndpointTarget<'_>,
) -> Result<ValidatedEndpoint, EasyOfficeError> {
    let parsed = parse_url_strict(raw_url)?;
    if parsed.is_https {
        return Err(security_endpoint(
            "로컬 모델 엔드포인트(local_http)는 http:// 프로토콜만 허용됩니다.",
        ));
    }
    if !parsed.has_explicit_port {
        return Err(security_endpoint(
            "로컬 HTTP 엔드포인트에는 명시적 포트 번호(예: http://127.0.0.1:11434)가 필요합니다.",
        ));
    }

    let addr = if parsed.is_ipv6_bracketed {
        let ip: Ipv6Addr = parsed
            .host
            .parse()
            .map_err(|_| invalid_endpoint("IPv6 주소가 올바르지 않습니다."))?;
        if ip != Ipv6Addr::LOCALHOST {
            return Err(security_endpoint(
                "로컬 HTTP 엔드포인트는 http://127.0.0.1:<포트> 또는 http://[::1]:<포트>만 허용됩니다.",
            ));
        }
        SocketAddr::new(IpAddr::V6(ip), parsed.port)
    } else {
        let Ok(ip) = parsed.host.parse::<Ipv4Addr>() else {
            return Err(security_endpoint(
                "로컬 HTTP 엔드포인트는 http://127.0.0.1:<포트> 또는 http://[::1]:<포트>만 허용됩니다.",
            ));
        };
        if ip != Ipv4Addr::LOCALHOST {
            return Err(security_endpoint(
                "로컬 HTTP 엔드포인트는 http://127.0.0.1:<포트> 또는 http://[::1]:<포트>만 허용됩니다.",
            ));
        }
        SocketAddr::new(IpAddr::V4(ip), parsed.port)
    };

    let path = resolve_target_path(parsed.path, target)?;
    let host_part = if parsed.is_ipv6_bracketed {
        "[::1]"
    } else {
        "127.0.0.1"
    };
    let url = format!("http://{host_part}:{}{path}", parsed.port);

    Ok(ValidatedEndpoint {
        url,
        host: parsed.host.to_string(),
        port: parsed.port,
        path,
        is_https: false,
        resolved_addrs: vec![addr],
    })
}

pub fn validate_provider_target(
    is_local_loopback: bool,
    raw_url: &str,
    target: EndpointTarget<'_>,
) -> Result<ValidatedEndpoint, EasyOfficeError> {
    if is_local_loopback {
        validate_local_http_target(raw_url, target)
    } else {
        validate_remote_https_target(raw_url, target)
    }
}
