//! 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd.) 및 continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., see THIRD_PARTY_NOTICES.md)
//!
//! Desktop HTTP(S) model transport using `ureq` + OS `native-tls`, DNS pinning via `ureq::Resolver`,
//! zero-redirect enforcement, `Accept-Encoding: identity`, and a 512 KiB request/response ceiling.

use easyoffice_app::copilot::llm_client::validate_wire_headers;
use easyoffice_app::copilot::ssrf::is_public_ip;
use easyoffice_app::copilot::types::{HttpMethod, MAX_LLM_REQUEST_BYTES, MAX_LLM_RESPONSE_BYTES};
use easyoffice_app::CopilotLlmRequest;
use std::io::{Error as IoError, ErrorKind, Read};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(90);

fn normalize_host_for_netloc(host: &str) -> String {
    let trimmed = host.trim();
    if trimmed.contains(':') && !trimmed.starts_with('[') {
        format!("[{trimmed}]")
    } else {
        trimmed.to_string()
    }
}

fn netloc_matches(netloc: &str, expected_host: &str, expected_port: u16) -> bool {
    let bare = format!("{expected_host}:{expected_port}");
    let norm = format!(
        "{}:{expected_port}",
        normalize_host_for_netloc(expected_host)
    );
    netloc.eq_ignore_ascii_case(&bare) || netloc.eq_ignore_ascii_case(&norm)
}

fn validate_request_guards(req: &CopilotLlmRequest) -> Result<Vec<SocketAddr>, String> {
    if req.body_json.len() > MAX_LLM_REQUEST_BYTES {
        return Err("모델 요청 본문이 허용 상한(512 KiB)을 초과했습니다.".to_string());
    }
    validate_wire_headers(&req.headers).map_err(|_| {
        "인증 헤더에는 공백·제어 문자·비ASCII 문자를 포함할 수 없습니다.".to_string()
    })?;

    let parsed = tauri::Url::parse(&req.url)
        .map_err(|_| "모델 엔드포인트 URL을 해석할 수 없습니다.".to_string())?;
    let expected_scheme = if req.is_https { "https" } else { "http" };
    if parsed.scheme() != expected_scheme
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("보안 정책상 허용되지 않는 모델 엔드포인트 URL입니다.".to_string());
    }

    if req.resolved_addrs.is_empty() {
        return Err("확인된 모델 엔드포인트 IP 주소가 없습니다.".to_string());
    }

    for addr in &req.resolved_addrs {
        if req.is_https {
            if !is_public_ip(addr.ip()) {
                return Err(
                    "사설·루프백 또는 내부 네트워크 모델 엔드포인트는 차단되었습니다.".to_string(),
                );
            }
        } else if !addr.ip().is_loopback() {
            return Err("로컬 HTTP 엔드포인트는 127.0.0.1 또는 [::1]만 허용됩니다.".to_string());
        }
    }

    Ok(req.resolved_addrs.clone())
}

/// Executes a validated chat-completion request over `ureq` + OS `native-tls`.
pub fn complete(req: &CopilotLlmRequest) -> Result<String, String> {
    let pinned_addrs = validate_request_guards(req)?;
    let expected_host = req.host.clone();
    let expected_port = req.port;

    let mut builder = ureq::AgentBuilder::new()
        .https_only(req.is_https)
        .redirects(0)
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(IO_TIMEOUT)
        .timeout_write(Duration::from_secs(15))
        .timeout(IO_TIMEOUT)
        .user_agent(&format!("EasyOffice/{}", env!("CARGO_PKG_VERSION")))
        .resolver(move |netloc: &str| {
            if netloc_matches(netloc, &expected_host, expected_port) {
                Ok(pinned_addrs.clone())
            } else {
                Err(IoError::new(
                    ErrorKind::PermissionDenied,
                    "핀 고정된 모델 엔드포인트 호스트와 일치하지 않습니다.",
                ))
            }
        });

    if req.is_https {
        let tls = native_tls::TlsConnector::new()
            .map_err(|_| "OS TLS 커넥터를 초기화하지 못했습니다.".to_string())?;
        builder = builder.tls_connector(Arc::new(tls));
    }

    let agent = builder.build();
    let mut http_req = match req.method {
        HttpMethod::Get => agent.get(&req.url),
        HttpMethod::Post => agent.post(&req.url).set("Content-Type", "application/json"),
    }
    .set("Accept", "application/json")
    .set("Accept-Encoding", "identity");

    for (name, val) in &req.headers {
        http_req = http_req.set(name, val);
    }

    let send_res = match req.method {
        HttpMethod::Get => http_req.call(),
        HttpMethod::Post => http_req.send_string(&req.body_json),
    };
    let resp = match send_res {
        Ok(resp) => resp,
        Err(ureq::Error::Status(code, _)) => {
            return Err(format!("모델 서버가 HTTP {code} 오류를 반환했습니다."));
        }
        Err(ureq::Error::Transport(err)) => {
            let detail = match err.kind() {
                ureq::ErrorKind::Dns => "호스트 이름을 확인할 수 없습니다",
                ureq::ErrorKind::ConnectionFailed => "서버에 연결할 수 없습니다",
                ureq::ErrorKind::TooManyRedirects => {
                    return Err("모델 API 리다이렉트는 보안 정책상 허용되지 않습니다.".to_string());
                }
                ureq::ErrorKind::Io => "네트워크 입출력 오류가 발생했습니다",
                _ => "전송 오류가 발생했습니다",
            };
            return Err(format!("모델 엔드포인트 요청에 실패했습니다 ({detail})."));
        }
    };

    if (300..400).contains(&resp.status()) {
        return Err("모델 API 리다이렉트는 보안 정책상 허용되지 않습니다.".to_string());
    }
    if let Some(encoding) = resp.header("content-encoding") {
        if !encoding.trim().eq_ignore_ascii_case("identity") {
            return Err("지원하지 않는 콘텐츠 인코딩 응답입니다.".to_string());
        }
    }

    let mut body = Vec::with_capacity(8192);
    resp.into_reader()
        .take(MAX_LLM_RESPONSE_BYTES as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|_| "모델 응답 본문을 읽지 못했습니다.".to_string())?;

    if body.len() > MAX_LLM_RESPONSE_BYTES {
        return Err("모델 응답 크기가 허용 상한(512 KiB)을 초과했습니다.".to_string());
    }

    String::from_utf8(body).map_err(|_| "모델 응답 본문이 유효한 UTF-8이 아닙니다.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::{IpAddr, Ipv4Addr, TcpListener};
    use std::thread;

    fn spawn_once(raw_response: String) -> (u16, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&chunk[..n]);
                        if let Some(hdr_end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            let hdr_str = String::from_utf8_lossy(&buf[..hdr_end]);
                            let content_len = hdr_str
                                .lines()
                                .find_map(|l| {
                                    let (k, v) = l.split_once(':')?;
                                    if k.trim().eq_ignore_ascii_case("content-length") {
                                        v.trim().parse::<usize>().ok()
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or(0);
                            if buf.len() >= hdr_end + 4 + content_len {
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = stream.write_all(raw_response.as_bytes());
            let _ = stream.flush();
            String::from_utf8_lossy(&buf).into_owned()
        });
        (port, handle)
    }

    fn base_local_req(port: u16) -> CopilotLlmRequest {
        CopilotLlmRequest {
            method: HttpMethod::Post,
            url: format!("http://127.0.0.1:{port}/v1/chat/completions"),
            host: "127.0.0.1".to_string(),
            port,
            path: "/v1/chat/completions".to_string(),
            is_https: false,
            resolved_addrs: vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)],
            headers: vec![("Authorization".to_string(), "Bearer secret-key".to_string())],
            body_json: r#"{"model":"m1","messages":[]}"#.to_string(),
        }
    }

    #[test]
    fn desktop_http_complete_roundtrip_and_security_rejections() {
        let ok_body = r#"{"choices":[{"message":{"content":"ok"}}]}"#;
        let ok_resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            ok_body.len(),
            ok_body
        );
        let (port, handle) = spawn_once(ok_resp);
        let out = complete(&base_local_req(port)).unwrap();
        let captured = handle.join().unwrap();
        assert_eq!(out, ok_body);
        assert!(captured
            .to_ascii_lowercase()
            .contains("accept-encoding: identity"));
        assert!(captured.contains("Authorization: Bearer secret-key"));

        // Redirect 302 must be rejected.
        let (r_port, r_handle) = spawn_once(
            "HTTP/1.1 302 Found\r\nLocation: https://example.com\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_string(),
        );
        let r_err = complete(&base_local_req(r_port)).unwrap_err();
        let _ = r_handle.join();
        assert!(r_err.contains("리다이렉트"), "{r_err}");

        // Non-identity encoding must be rejected.
        let (g_port, g_handle) = spawn_once(
            "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}"
                .to_string(),
        );
        let g_err = complete(&base_local_req(g_port)).unwrap_err();
        let _ = g_handle.join();
        assert!(g_err.contains("인코딩"), "{g_err}");

        // Body > 512 KiB must be rejected.
        let big = "a".repeat(MAX_LLM_RESPONSE_BYTES + 16);
        let big_resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            big.len(),
            big
        );
        let (b_port, b_handle) = spawn_once(big_resp);
        let b_err = complete(&base_local_req(b_port)).unwrap_err();
        let _ = b_handle.join();
        assert!(b_err.contains("512 KiB"), "{b_err}");

        // Private IP pinned onto HTTPS request must be rejected.
        let mut https_private = base_local_req(443);
        https_private.url = "https://api.example.com/v1/chat/completions".to_string();
        https_private.host = "api.example.com".to_string();
        https_private.is_https = true;
        https_private.resolved_addrs =
            vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 443)];
        assert!(complete(&https_private).is_err());

        // Non-ASCII / control-char header must be rejected without leaking the secret in the error.
        let mut bad_key = base_local_req(443);
        bad_key.headers = vec![(
            "x-api-key".to_string(),
            "sk-secret-\u{200B}leak".to_string(),
        )];
        let k_err = complete(&bad_key).unwrap_err();
        assert!(!k_err.contains("sk-secret"), "{k_err}");

        // GET /v1/models roundtrip with Anthropic headers
        let models_body =
            r#"{"data":[{"id":"claude-sonnet-4-6","display_name":"Claude Sonnet 4.6"}]}"#;
        let models_resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            models_body.len(),
            models_body
        );
        let (m_port, m_handle) = spawn_once(models_resp);
        let get_req = CopilotLlmRequest {
            method: HttpMethod::Get,
            url: format!("http://127.0.0.1:{m_port}/v1/models"),
            host: "127.0.0.1".to_string(),
            port: m_port,
            path: "/v1/models".to_string(),
            is_https: false,
            resolved_addrs: vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), m_port)],
            headers: vec![
                ("x-api-key".to_string(), "ant-key".to_string()),
                ("anthropic-version".to_string(), "2023-06-01".to_string()),
            ],
            body_json: String::new(),
        };
        let get_out = complete(&get_req).unwrap();
        let get_cap = m_handle.join().unwrap();
        assert_eq!(get_out, models_body);
        assert!(get_cap.starts_with("GET /v1/models HTTP/1.1"));
        assert!(get_cap.contains("x-api-key: ant-key"));
        assert!(get_cap.contains("anthropic-version: 2023-06-01"));
    }
}
