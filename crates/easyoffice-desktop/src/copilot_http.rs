//! 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd.) 및 continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., see THIRD_PARTY_NOTICES.md)
//!
//! Desktop HTTP(S) model transport using `ureq` + OS `native-tls`, DNS pinning via `ureq::Resolver`,
//! zero-redirect enforcement, `Accept-Encoding: identity`, and a 512 KiB request/response ceiling.
//!
//! Cancellation: HTTPS hands the connected socket to the request's `CancelHandle` from inside the TLS
//! connector hook, so `copilot_cancel` can shut it down and unblock the handshake, the write and the
//! read. Loopback HTTP uses the engine's own socket client, which registers its socket the same way.

use easyoffice_app::copilot::cancel::{CancelHandle, CANCELLED_MESSAGE};
use easyoffice_app::copilot::llm_client::{send_local_http_streaming, validate_wire_headers};
use easyoffice_app::copilot::ssrf::is_public_ip;
use easyoffice_app::copilot::types::{
    ChunkSink, HttpMethod, MAX_LLM_REQUEST_BYTES, MAX_LLM_RESPONSE_BYTES,
};
use easyoffice_app::copilot::wire::{
    STREAM_DEADLINE_MESSAGE, STREAM_IDLE_TIMEOUT, STREAM_STOPPED_MESSAGE, STREAM_TOTAL_LIMIT,
};
use easyoffice_app::{CopilotLlmRequest, CopilotStreamEvent};
use std::io::{Error as IoError, ErrorKind, Read};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What the page receives for one piece of the answer (`requestId` ties it to the question, `seq` counts from 0).
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamPayload {
    request_id: String,
    seq: u32,
    text: String,
}

/// The page's stream channel: a Tauri IPC channel the page names in the request (`onStream`). Pieces reach that
/// page only and in order, and the page needs no event permission to receive them.
pub type StreamChannel = tauri::ipc::Channel<StreamPayload>;

/// Sends one piece of the streamed answer to the page that asked.
pub fn send_stream_piece(channel: &StreamChannel, event: &CopilotStreamEvent<'_>) {
    let _ = channel.send(StreamPayload {
        request_id: event.request_id.to_string(),
        seq: event.seq,
        text: event.text.to_string(),
    });
}

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

/// 오류 문구가 말하는 대로 `127.0.0.1`과 `::1`만 받는다(`127.0.0.0/8` 나머지는 받지 않는다).
fn is_exact_loopback(ip: IpAddr) -> bool {
    ip == IpAddr::V4(Ipv4Addr::LOCALHOST) || ip == IpAddr::V6(Ipv6Addr::LOCALHOST)
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
        } else if !is_exact_loopback(addr.ip()) {
            return Err("로컬 HTTP 엔드포인트는 127.0.0.1 또는 [::1]만 허용됩니다.".to_string());
        }
    }

    Ok(req.resolved_addrs.clone())
}

/// `native-tls` connector that registers the connected TCP socket with the request's cancel handle
/// before the handshake, so a cancel can close the connection at any later point.
struct CancellableTls {
    inner: native_tls::TlsConnector,
    cancel: CancelHandle,
}

impl ureq::TlsConnector for CancellableTls {
    fn connect(
        &self,
        dns_name: &str,
        io: Box<dyn ureq::ReadWrite>,
    ) -> Result<Box<dyn ureq::ReadWrite>, ureq::Error> {
        if let Some(socket) = io.socket() {
            self.cancel.attach(socket);
        }
        if self.cancel.is_cancelled() {
            return Err(IoError::new(ErrorKind::Interrupted, CANCELLED_MESSAGE).into());
        }
        ureq::TlsConnector::connect(&self.inner, dns_name, io)
    }
}

/// Executes a validated chat-completion request: loopback HTTP over the engine's socket client,
/// HTTPS over `ureq` + OS `native-tls`.
pub fn complete(req: &CopilotLlmRequest) -> Result<String, String> {
    run(req, None)
}

/// Same as [`complete`] for a streamed answer: the body goes to `sink` as it arrives (only for a 2xx
/// response), the whole body is still returned at the end, and the 512 KiB ceiling, the pinned address,
/// the zero redirects and the cancel handle apply as before. A sink that returns `false` stops the read.
pub fn complete_stream(
    req: &CopilotLlmRequest,
    sink: &mut ChunkSink<'_>,
) -> Result<String, String> {
    run(req, Some(sink))
}

fn run(req: &CopilotLlmRequest, sink: Option<&mut ChunkSink<'_>>) -> Result<String, String> {
    let pinned_addrs = validate_request_guards(req)?;
    req.cancel.check()?;
    if !req.is_https {
        return send_local_http_streaming(req, sink).map_err(|err| err.to_string());
    }
    complete_https(req, pinned_addrs, sink)
}

/// Reads the body in pieces so a streamed answer reaches the page while the model is still writing.
fn read_body(
    reader: impl Read,
    mut sink: Option<&mut ChunkSink<'_>>,
    or_cancelled: &dyn Fn(String) -> String,
    deadline: Option<Instant>,
) -> Result<Vec<u8>, String> {
    let mut reader = reader.take(MAX_LLM_RESPONSE_BYTES as u64 + 1);
    let mut body = Vec::with_capacity(8192);
    let mut buf = [0u8; 8192];
    loop {
        if deadline.is_some_and(|limit| Instant::now() >= limit) {
            return Err(STREAM_DEADLINE_MESSAGE.to_string());
        }
        let n = reader
            .read(&mut buf)
            .map_err(|_| or_cancelled("모델 응답 본문을 읽지 못했습니다.".to_string()))?;
        if n == 0 {
            return Ok(body);
        }
        body.extend_from_slice(&buf[..n]);
        if body.len() > MAX_LLM_RESPONSE_BYTES {
            return Ok(body);
        }
        if let Some(sink) = sink.as_deref_mut() {
            if !sink(&buf[..n]) {
                return Err(STREAM_STOPPED_MESSAGE.to_string());
            }
        }
    }
}

fn complete_https(
    req: &CopilotLlmRequest,
    pinned_addrs: Vec<SocketAddr>,
    sink: Option<&mut ChunkSink<'_>>,
) -> Result<String, String> {
    // A connection closed by a cancel surfaces as an I/O error of any kind: report the cancel instead.
    let or_cancelled = |message: String| -> String {
        if req.cancel.is_cancelled() {
            CANCELLED_MESSAGE.to_string()
        } else {
            message
        }
    };
    let expected_host = req.host.clone();
    let expected_port = req.port;

    let mut builder = ureq::AgentBuilder::new()
        .https_only(true)
        .redirects(0)
        .timeout_connect(CONNECT_TIMEOUT)
        // 스트림은 읽기 사이의 유휴 시간만 재고(`STREAM_IDLE_TIMEOUT`) 전체는 `STREAM_TOTAL_LIMIT`로 따로 막는다.
        // 한꺼번에 받는 요청은 전체 90초다.
        .timeout_read(if req.stream {
            STREAM_IDLE_TIMEOUT
        } else {
            IO_TIMEOUT
        })
        .timeout_write(Duration::from_secs(15))
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

    let tls = native_tls::TlsConnector::new()
        .map_err(|_| "OS TLS 커넥터를 초기화하지 못했습니다.".to_string())?;
    builder = builder.tls_connector(Arc::new(CancellableTls {
        inner: tls,
        cancel: req.cancel.clone(),
    }));

    if !req.stream {
        builder = builder.timeout(IO_TIMEOUT);
    }
    let agent = builder.build();
    let mut http_req = match req.method {
        HttpMethod::Get => agent.get(&req.url),
        HttpMethod::Post => agent.post(&req.url).set("Content-Type", "application/json"),
    }
    .set("Accept", req.accept_header())
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
            return Err(or_cancelled(format!(
                "모델 엔드포인트 요청에 실패했습니다 ({detail})."
            )));
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

    let deadline = req.stream.then(|| Instant::now() + STREAM_TOTAL_LIMIT);
    let body = read_body(resp.into_reader(), sink, &or_cancelled, deadline)?;
    req.cancel.check()?;

    if body.len() > MAX_LLM_RESPONSE_BYTES {
        return Err("모델 응답 크기가 허용 상한(512 KiB)을 초과했습니다.".to_string());
    }

    String::from_utf8(body).map_err(|_| "모델 응답 본문이 유효한 UTF-8이 아닙니다.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;
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
            cancel: CancelHandle::default(),
            stream: false,
        }
    }

    #[test]
    fn local_http_accepts_only_127_0_0_1_and_ipv6_loopback() {
        for (ip, accepted) in [
            (IpAddr::V4(Ipv4Addr::LOCALHOST), true),
            (IpAddr::V6(Ipv6Addr::LOCALHOST), true),
            (IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), false),
            (IpAddr::V4(Ipv4Addr::new(127, 255, 255, 254)), false),
            (IpAddr::V6(Ipv4Addr::LOCALHOST.to_ipv6_mapped()), false),
        ] {
            let mut req = base_local_req(11434);
            req.resolved_addrs = vec![SocketAddr::new(ip, 11434)];
            let verdict = validate_request_guards(&req);
            assert_eq!(verdict.is_ok(), accepted, "{ip}: {verdict:?}");
            if !accepted {
                assert!(verdict.unwrap_err().contains("127.0.0.1 또는 [::1]만"));
            }
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
            cancel: CancelHandle::default(),
            stream: false,
        };
        let get_out = complete(&get_req).unwrap();
        let get_cap = m_handle.join().unwrap();
        assert_eq!(get_out, models_body);
        assert!(get_cap.starts_with("GET /v1/models HTTP/1.1"));
        assert!(get_cap.contains("x-api-key: ant-key"));
        assert!(get_cap.contains("anthropic-version: 2023-06-01"));
    }

    /// A server that accepts the connection and never answers; `accepted` fires once it is connected.
    fn spawn_silent_server() -> (u16, std::sync::mpsc::Receiver<()>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = tx.send(());
            let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
            let mut buf = [0u8; 4096];
            while matches!(stream.read(&mut buf), Ok(n) if n > 0) {}
        });
        (port, rx, handle)
    }

    #[test]
    fn cancel_unblocks_a_loopback_request_that_never_gets_an_answer() {
        let (port, accepted, server) = spawn_silent_server();
        let req = base_local_req(port);
        let cancel = req.cancel.clone();
        let worker = thread::spawn(move || complete(&req));
        accepted.recv_timeout(Duration::from_secs(5)).unwrap();
        let started = std::time::Instant::now();
        cancel.cancel();
        let err = worker.join().unwrap().unwrap_err();
        assert!(err.contains("취소"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(5));
        server.join().unwrap();

        let done = base_local_req(1);
        done.cancel.cancel();
        assert!(
            complete(&done).unwrap_err().contains("취소"),
            "a cancelled request never connects"
        );
    }

    #[test]
    fn cancel_unblocks_a_tls_handshake_through_the_connector_hook() {
        use ureq::TlsConnector as _;
        let (port, accepted, server) = spawn_silent_server();
        let client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let cancel = CancelHandle::default();
        let connector = CancellableTls {
            inner: native_tls::TlsConnector::new().unwrap(),
            cancel: cancel.clone(),
        };
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            let outcome = connector.connect("example.com", Box::new(client));
            let _ = done_tx.send(outcome.is_err());
        });
        accepted.recv_timeout(Duration::from_secs(5)).unwrap();
        // The handshake waits for a server hello that never comes.
        assert!(done_rx.recv_timeout(Duration::from_millis(300)).is_err());
        cancel.cancel();
        assert_eq!(done_rx.recv_timeout(Duration::from_secs(5)), Ok(true));
        worker.join().unwrap();
        server.join().unwrap();
    }

    #[test]
    fn a_streamed_answer_reaches_the_sink_in_pieces_and_is_still_returned_whole() {
        let body = "data: {\"a\":1}\n\ndata: [DONE]\n\n";
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (port, handle) = spawn_once(resp);
        let mut req = base_local_req(port);
        req.stream = true;
        let mut pieces = Vec::new();
        let mut sink = |chunk: &[u8]| {
            pieces.push(chunk.to_vec());
            true
        };
        let out = complete_stream(&req, &mut sink).unwrap();
        let captured = handle.join().unwrap();
        assert_eq!(out, body);
        assert_eq!(pieces.concat(), body.as_bytes());
        assert!(captured
            .to_ascii_lowercase()
            .contains("accept: text/event-stream"));
    }

    #[test]
    fn a_stream_the_sink_stops_ends_with_the_stop_reason() {
        let body = "data: x\n\n".repeat(50);
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (port, handle) = spawn_once(resp);
        let mut req = base_local_req(port);
        req.stream = true;
        let err = complete_stream(&req, &mut |_| false).unwrap_err();
        let _ = handle.join();
        assert!(err.contains("멈췄습니다"), "{err}");
    }

    #[test]
    fn the_body_reader_stops_at_the_ceiling_and_hands_pieces_to_the_sink() {
        let big = vec![b'x'; MAX_LLM_RESPONSE_BYTES + 10_000];
        let mut seen = 0usize;
        let mut sink = |chunk: &[u8]| {
            seen += chunk.len();
            true
        };
        let body = read_body(&big[..], Some(&mut sink), &|m| m, None).unwrap();
        assert!(
            body.len() > MAX_LLM_RESPONSE_BYTES,
            "the caller rejects a body over the ceiling"
        );
        assert!(
            body.len() <= MAX_LLM_RESPONSE_BYTES + 8192 + 1,
            "reading stops right after the ceiling: {}",
            body.len()
        );
        assert!(
            seen <= MAX_LLM_RESPONSE_BYTES,
            "no piece over the ceiling reaches the page"
        );
        let stopped =
            read_body(&b"abc"[..], Some(&mut |_: &[u8]| false), &|m| m, None).unwrap_err();
        assert_eq!(stopped, STREAM_STOPPED_MESSAGE);
    }

    #[test]
    fn a_stream_past_its_total_limit_ends_even_while_data_keeps_coming() {
        // 조각이 끊기지 않고 계속 와도(유휴 타임아웃에 걸리지 않아도) 전체 상한을 넘으면 끝낸다.
        let endless = std::io::repeat(b'x');
        let past = std::time::Instant::now() - Duration::from_secs(1);
        let err = read_body(endless, Some(&mut |_: &[u8]| true), &|m| m, Some(past)).unwrap_err();
        assert!(err.contains("허용 시간"), "{err}");
        let far = std::time::Instant::now() + Duration::from_secs(60);
        let body = read_body(&b"abc"[..], None, &|m| m, Some(far)).unwrap();
        assert_eq!(body, b"abc");
    }
}
