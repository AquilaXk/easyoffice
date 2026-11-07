use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::hwp_bundle;
use crate::EasyOfficeIpcRouter;

const SERVE_DEV_ONLY_NOTICE: &str = "NOTICE: --serve is a developer/testing mode only, not for end users; install the EasyOffice desktop app instead.\n안내: --serve는 개발·테스트 전용 모드이며 일반 사용자용이 아닙니다. 설치형 EasyOffice 앱을 사용하세요.";
const INDEX_HTML: &str = include_str!("../ui/index.html");
const STYLES_CSS: &str = include_str!("../ui/styles.css");
const SHELL_CSS: &str = include_str!("../ui/shell.css");
const TOKENS_CSS: &str = include_str!("../ui/tokens.css");
const CONTENT_CSS: &str = include_str!("../ui/content.css");
const THEME_JS: &str = include_str!("../ui/theme.js");
const MENU_BAR_CSS: &str = include_str!("../ui/menu-bar.css");
const TOOLBAR_CSS: &str = include_str!("../ui/toolbar.css");
const HOME_CSS: &str = include_str!("../ui/home.css");
const HOME_SCREEN_JS: &str = include_str!("../ui/home-screen.js");
const RECENT_DIALOG_JS: &str = include_str!("../ui/recent-documents-dialog.js");
const ICONS_JS: &str = include_str!("../ui/icons.js");
const WINDOW_BRIDGE_JS: &str = include_str!("../ui/window-bridge.js");
const SHEET_UTILS_JS: &str = include_str!("../ui/sheet-utils.js");
const PII_DOM_SCRUBBER_JS: &str = include_str!("../ui/pii-dom-scrubber.js");
const SHEET_STORE_JS: &str = include_str!("../ui/sheet-store.js");
const RIBBON_CATALOG_JS: &str = include_str!("../ui/ribbon-catalog.js");
const SAVE_PAYLOAD_JS: &str = include_str!("../ui/save-payload.js");
const DESKTOP_BRIDGE_JS: &str = include_str!("../ui/desktop-bridge.js");
const WORKSPACE_SCRUB_JS: &str = include_str!("../ui/workspace-scrub.js");
const WORKSPACE_TOOLS_JS: &str = include_str!("../ui/workspace-tools.js");
const WORKSPACE_PANELS_JS: &str = include_str!("../ui/workspace-panels.js");
const SHEET_GRID_JS: &str = include_str!("../ui/sheet-grid.js");
const SHEET_CLIPBOARD_JS: &str = include_str!("../ui/sheet-clipboard.js");
const SHEET_INPUT_JS: &str = include_str!("../ui/sheet-input.js");
const SHEET_OPS_JS: &str = include_str!("../ui/sheet-ops.js");
const SHEET_FORMAT_JS: &str = include_str!("../ui/sheet-format.js");
const SHEET_CONTROLLER_JS: &str = include_str!("../ui/sheet-controller.js");
const RIBBON_HISTORY_JS: &str = include_str!("../ui/ribbon-history.js");
const WORKSPACE_SESSION_JS: &str = include_str!("../ui/workspace-session.js");
const FILE_OPEN_JS: &str = include_str!("../ui/file-open.js");
const SLIDE_STUDIO_JS: &str = include_str!("../ui/slide-studio.js");
const DOC_RUNS_JS: &str = include_str!("../ui/doc-runs.js");
const DOC_RUNS_UI_JS: &str = include_str!("../ui/doc-runs-ui.js");
const HWP_HOST_JS: &str = include_str!("../ui/hwp-host.js");
const HWP_SESSION_JS: &str = include_str!("../ui/hwp-session.js");
const SHELL_CHROME_JS: &str = include_str!("../ui/shell-chrome.js");
const APP_JS: &str = include_str!("../ui/app.js");
const FAVICON_SVG: &str = include_str!("../ui/favicon.svg");

/// Body ceiling for static routes and the request-header block.
pub const MAX_HTTP_BODY_BYTES: usize = 1_048_576;
/// Body ceiling for `POST /api/ipc`: the WebUI sends whole workbooks on recalculation
/// (20,000 numbers + 4,000 formulas is about 1.4 MB), so IPC gets a bounded 16 MiB budget.
pub const MAX_IPC_HTTP_BODY_BYTES: usize = 16 * 1_048_576;
pub const MAX_CONCURRENT_HTTP_THREADS: usize = 32;
pub const MAX_CONNECTION_DURATION: Duration = Duration::from_secs(3);

pub type IpcDispatchFn =
    dyn Fn(&str, &str) -> Result<String, EasyOfficeError> + Send + Sync + 'static;

pub struct ConnectionSlotGuard {
    active: Arc<AtomicUsize>,
}

impl ConnectionSlotGuard {
    pub fn try_acquire(active: &Arc<AtomicUsize>, max: usize) -> Option<Self> {
        let mut current = active.load(Ordering::Acquire);
        loop {
            if current >= max {
                return None;
            }
            match active.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Some(Self {
                        active: Arc::clone(active),
                    });
                }
                Err(observed) => current = observed,
            }
        }
    }
}

impl Drop for ConnectionSlotGuard {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::Release);
    }
}

pub fn parse_ipc_http_body(body: &str) -> Result<(String, String), EasyOfficeError> {
    let trimmed = body.strip_prefix('\u{FEFF}').unwrap_or(body).trim();
    if trimmed.starts_with('{') {
        let root = StrictJsonValue::parse(trimmed)?;
        let entries = root.as_object().ok_or_else(|| {
            EasyOfficeError::InvalidArgument(
                "IPC HTTP 요청 본문은 JSON 객체여야 합니다.".to_string(),
            )
        })?;
        let cmd = StrictJsonValue::get_str(entries, "command")
            .ok_or_else(|| {
                EasyOfficeError::InvalidArgument(
                    "IPC JSON 요청에 문자열 'command' 필드가 필요합니다.".to_string(),
                )
            })?
            .to_string();
        let payload = match StrictJsonValue::get_field(entries, "payload") {
            Some(v) => v.to_scalar_string().unwrap_or_else(|| v.to_json_string()),
            None => String::new(),
        };
        Ok((cmd, payload))
    } else if let Some((c, p)) = trimmed.split_once('|') {
        Ok((c.trim().to_string(), p.to_string()))
    } else {
        Ok((trimmed.to_string(), String::new()))
    }
}

pub fn is_allowed_host(host: &str, port: u16) -> bool {
    let h = host.trim();
    h.eq_ignore_ascii_case(&format!("127.0.0.1:{port}"))
        || h.eq_ignore_ascii_case(&format!("localhost:{port}"))
        || h.eq_ignore_ascii_case(&format!("[::1]:{port}"))
        || h.eq_ignore_ascii_case("127.0.0.1")
        || h.eq_ignore_ascii_case("localhost")
        || h.eq_ignore_ascii_case("[::1]")
}

pub fn is_allowed_origin(origin: &str, port: u16) -> bool {
    let o = origin.trim().trim_end_matches('/');
    o.eq_ignore_ascii_case(&format!("http://127.0.0.1:{port}"))
        || o.eq_ignore_ascii_case(&format!("http://localhost:{port}"))
        || o.eq_ignore_ascii_case(&format!("http://[::1]:{port}"))
}

/// CSP of the main page and every API/error response. A same-origin <iframe> is already allowed by
/// the `default-src` fallback (`frame-src` -> `child-src` -> `default-src`), so the editor frame
/// needs no relaxation here; only `/hwp-studio/*` gets its own policy (`hwp_bundle::STUDIO_CSP`).
const MAIN_CSP: &str = "default-src 'self'";

fn write_http_response(stream: &mut TcpStream, status: &str, content_type: &str, body: &str) {
    write_http_bytes(stream, status, content_type, MAIN_CSP, body.as_bytes());
}

fn write_http_bytes(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    csp: &str,
    body: &[u8],
) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nContent-Security-Policy: {csp}\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let _ = stream.set_read_timeout(Some(Duration::from_millis(20)));
    let mut drain = [0u8; 1024];
    for _ in 0..4 {
        match stream.read(&mut drain) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
    }
}

fn write_http_413(stream: &mut TcpStream) {
    write_http_response(
        stream,
        "413 Payload Too Large",
        "application/json; charset=utf-8",
        "{\"status\":\"payload_too_large\",\"message\":\"HTTP 본문 크기가 허용 한도(IPC 16MiB, 기타 1MiB)를 초과했습니다.\"}",
    );
}

fn apply_remaining_read_timeout(stream: &TcpStream, deadline: Instant) -> bool {
    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
        return false;
    };
    if remaining.is_zero() {
        return false;
    }
    let step_timeout = remaining.min(Duration::from_millis(1500));
    stream.set_read_timeout(Some(step_timeout)).is_ok()
}

/// Serves one file of the hwp-studio bundle (binary body, exact MIME type, iframe CSP). Anything
/// that is not a plain file path inside the bundle, or a bundle that is not present, is a 404.
fn write_bundle_file(stream: &mut TcpStream, request_path: &str) {
    let path_only = request_path.split(['?', '#']).next().unwrap_or("");
    let file =
        hwp_bundle::sanitize_relative_path(path_only).and_then(|rel| hwp_bundle::lookup(&rel));
    match file {
        Some(file) => write_http_bytes(
            stream,
            "200 OK",
            file.content_type,
            hwp_bundle::STUDIO_CSP,
            &file.bytes,
        ),
        None => write_http_bytes(
            stream,
            "404 Not Found",
            "application/json; charset=utf-8",
            hwp_bundle::STUDIO_CSP,
            b"{\"status\":\"not_found\"}",
        ),
    }
}

/// Why reading a request stopped early.
enum ReadStop {
    /// Connection closed, timed out or failed: drop silently.
    Drop,
    /// Request exceeded the size budget: answer 413.
    TooLarge,
}

const READ_CHUNK_BYTES: usize = 64 * 1024;
const HEADER_TERMINATOR: &[u8; 4] = b"\r\n\r\n";

/// Reads from `stream` until the header terminator arrives; returns the header length.
fn read_until_headers(
    stream: &mut TcpStream,
    deadline: Instant,
    buf: &mut Vec<u8>,
    chunk: &mut [u8],
) -> Result<usize, ReadStop> {
    loop {
        if !apply_remaining_read_timeout(stream, deadline) {
            return Err(ReadStop::Drop);
        }
        let n = match stream.read(chunk) {
            Ok(0) | Err(_) => return Err(ReadStop::Drop),
            Ok(n) => n,
        };
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == HEADER_TERMINATOR) {
            return Ok(pos + 4);
        }
        if buf.len() > MAX_HTTP_BODY_BYTES {
            return Err(ReadStop::TooLarge);
        }
    }
}

/// Reads the body until `header_end + content_length` bytes are buffered.
fn read_request_body(
    stream: &mut TcpStream,
    deadline: Instant,
    buf: &mut Vec<u8>,
    chunk: &mut [u8],
    (header_end, content_length, body_limit): (usize, usize, usize),
) -> Result<(), ReadStop> {
    // Grow toward the declared length as bytes arrive; never pre-commit the full IPC budget
    // for a client that has only sent headers.
    buf.reserve(
        (header_end + content_length)
            .saturating_sub(buf.len())
            .min(MAX_HTTP_BODY_BYTES),
    );
    while buf.len() < header_end + content_length {
        if !apply_remaining_read_timeout(stream, deadline) {
            return Err(ReadStop::Drop);
        }
        let n = match stream.read(chunk) {
            Ok(0) | Err(_) => return Err(ReadStop::Drop),
            Ok(n) => n,
        };
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > header_end + body_limit {
            return Err(ReadStop::TooLarge);
        }
    }
    Ok(())
}

fn finish_read_stop(stream: &mut TcpStream, stop: ReadStop) {
    if matches!(stop, ReadStop::TooLarge) {
        write_http_413(stream);
    }
}

/// Request headers that influence routing and limits.
#[derive(Default)]
struct RequestHeaders {
    content_length: usize,
    host: Option<String>,
    origin: Option<String>,
}

/// Parses the header lines; `None` when `Content-Length` is invalid or above `body_limit`.
fn parse_request_headers<'a>(
    lines: impl Iterator<Item = &'a str>,
    body_limit: usize,
) -> Option<RequestHeaders> {
    let mut headers = RequestHeaders::default();
    for line in lines {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let key = k.trim();
        let value = v.trim();
        if key.eq_ignore_ascii_case("content-length") {
            headers.content_length = value
                .parse::<usize>()
                .ok()
                .filter(|&len| len <= body_limit)?;
        } else if key.eq_ignore_ascii_case("host") {
            headers.host = Some(value.to_string());
        } else if key.eq_ignore_ascii_case("origin") {
            headers.origin = Some(value.to_string());
        }
    }
    Some(headers)
}

/// Rejection message for a disallowed `Host` (any method) or `Origin` (POST only) header.
fn forbidden_header_message(
    headers: &RequestHeaders,
    method: &str,
    port: u16,
) -> Option<&'static str> {
    if headers
        .host
        .as_deref()
        .is_some_and(|host| !is_allowed_host(host, port))
    {
        return Some("{\"status\":\"forbidden\",\"message\":\"허용되지 않은 Host 헤더입니다.\"}");
    }
    let bad_origin = method == "POST"
        && headers
            .origin
            .as_deref()
            .is_some_and(|origin| !is_allowed_origin(origin, port));
    bad_origin
        .then_some("{\"status\":\"forbidden\",\"message\":\"허용되지 않은 Origin 헤더입니다.\"}")
}

fn handle_http_connection(stream: &mut TcpStream, port: u16, ipc_handler: &IpcDispatchFn) {
    let deadline = Instant::now() + MAX_CONNECTION_DURATION;
    let _ = stream.set_write_timeout(Some(Duration::from_millis(1500)));

    let mut buf = Vec::with_capacity(8192);
    let mut chunk = vec![0u8; READ_CHUNK_BYTES];
    let header_end = match read_until_headers(stream, deadline, &mut buf, &mut chunk) {
        Ok(end) => end,
        Err(stop) => return finish_read_stop(stream, stop),
    };

    let header_str = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = header_str.lines();
    let Some(req_line) = lines.next() else {
        return;
    };
    let mut parts = req_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let path = parts.next().unwrap_or("/");
    let body_limit = if method == "POST" && path == "/api/ipc" {
        MAX_IPC_HTTP_BODY_BYTES
    } else {
        MAX_HTTP_BODY_BYTES
    };

    let Some(headers) = parse_request_headers(lines, body_limit) else {
        return write_http_413(stream);
    };
    if let Some(message) = forbidden_header_message(&headers, method, port) {
        return write_http_response(
            stream,
            "403 Forbidden",
            "application/json; charset=utf-8",
            message,
        );
    }
    let content_length = headers.content_length;
    if let Err(stop) = read_request_body(
        stream,
        deadline,
        &mut buf,
        &mut chunk,
        (header_end, content_length, body_limit),
    ) {
        return finish_read_stop(stream, stop);
    }

    let body_slice = &buf[header_end..header_end + content_length];

    if method == "GET" && path.starts_with(hwp_bundle::BUNDLE_URL_PREFIX) {
        write_bundle_file(stream, path);
        return;
    }

    // Document windows are addressed with launch parameters (`/?module=DOC&win=...`); routing ignores them.
    let route = path.split('?').next().unwrap_or("/");
    let (status, content_type, response_body) = match (method, route) {
        ("GET", "/" | "/index.html") => {
            ("200 OK", "text/html; charset=utf-8", INDEX_HTML.to_string())
        }
        ("GET", "/favicon.ico" | "/favicon.svg") => (
            "200 OK",
            "image/svg+xml; charset=utf-8",
            FAVICON_SVG.to_string(),
        ),
        ("GET", "/styles.css") => ("200 OK", "text/css; charset=utf-8", STYLES_CSS.to_string()),
        ("GET", "/shell.css") => ("200 OK", "text/css; charset=utf-8", SHELL_CSS.to_string()),
        ("GET", "/tokens.css") => ("200 OK", "text/css; charset=utf-8", TOKENS_CSS.to_string()),
        ("GET", "/content.css") => ("200 OK", "text/css; charset=utf-8", CONTENT_CSS.to_string()),
        ("GET", "/menu-bar.css") => (
            "200 OK",
            "text/css; charset=utf-8",
            MENU_BAR_CSS.to_string(),
        ),
        ("GET", "/toolbar.css") => ("200 OK", "text/css; charset=utf-8", TOOLBAR_CSS.to_string()),
        ("GET", "/home.css") => ("200 OK", "text/css; charset=utf-8", HOME_CSS.to_string()),
        ("GET", "/home-screen.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            HOME_SCREEN_JS.to_string(),
        ),
        ("GET", "/recent-documents-dialog.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            RECENT_DIALOG_JS.to_string(),
        ),
        ("GET", "/theme.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            THEME_JS.to_string(),
        ),
        ("GET", "/icons.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            ICONS_JS.to_string(),
        ),
        ("GET", "/window-bridge.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            WINDOW_BRIDGE_JS.to_string(),
        ),
        ("GET", "/sheet-utils.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            SHEET_UTILS_JS.to_string(),
        ),
        ("GET", "/pii-dom-scrubber.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            PII_DOM_SCRUBBER_JS.to_string(),
        ),
        ("GET", "/sheet-store.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            SHEET_STORE_JS.to_string(),
        ),
        ("GET", "/ribbon-catalog.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            RIBBON_CATALOG_JS.to_string(),
        ),
        ("GET", "/save-payload.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            SAVE_PAYLOAD_JS.to_string(),
        ),
        ("GET", "/desktop-bridge.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            DESKTOP_BRIDGE_JS.to_string(),
        ),
        ("GET", "/workspace-scrub.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            WORKSPACE_SCRUB_JS.to_string(),
        ),
        ("GET", "/workspace-tools.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            WORKSPACE_TOOLS_JS.to_string(),
        ),
        ("GET", "/workspace-panels.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            WORKSPACE_PANELS_JS.to_string(),
        ),
        ("GET", "/sheet-grid.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            SHEET_GRID_JS.to_string(),
        ),
        ("GET", "/sheet-clipboard.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            SHEET_CLIPBOARD_JS.to_string(),
        ),
        ("GET", "/sheet-input.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            SHEET_INPUT_JS.to_string(),
        ),
        ("GET", "/sheet-ops.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            SHEET_OPS_JS.to_string(),
        ),
        ("GET", "/sheet-format.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            SHEET_FORMAT_JS.to_string(),
        ),
        ("GET", "/sheet-controller.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            SHEET_CONTROLLER_JS.to_string(),
        ),
        ("GET", "/ribbon-history.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            RIBBON_HISTORY_JS.to_string(),
        ),
        ("GET", "/workspace-session.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            WORKSPACE_SESSION_JS.to_string(),
        ),
        ("GET", "/file-open.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            FILE_OPEN_JS.to_string(),
        ),
        ("GET", "/slide-studio.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            SLIDE_STUDIO_JS.to_string(),
        ),
        ("GET", "/doc-runs.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            DOC_RUNS_JS.to_string(),
        ),
        ("GET", "/doc-runs-ui.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            DOC_RUNS_UI_JS.to_string(),
        ),
        ("GET", "/hwp-host.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            HWP_HOST_JS.to_string(),
        ),
        ("GET", "/hwp-session.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            HWP_SESSION_JS.to_string(),
        ),
        ("GET", "/shell-chrome.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            SHELL_CHROME_JS.to_string(),
        ),
        ("GET", "/app.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            APP_JS.to_string(),
        ),
        ("POST", "/api/ipc") => {
            let dispatched = std::str::from_utf8(body_slice)
                .map_err(|_| {
                    EasyOfficeError::InvalidArgument(
                        "HTTP 본문이 유효한 UTF-8 문자열이 아닙니다.".to_string(),
                    )
                })
                .and_then(parse_ipc_http_body)
                .and_then(|(cmd, payload)| ipc_handler(&cmd, &payload));
            match dispatched {
                Ok(json) => ("200 OK", "application/json; charset=utf-8", json),
                Err(err) => {
                    let escaped = EasyOfficeIpcRouter::escape_json_str(&err.to_string());
                    (
                        "400 Bad Request",
                        "application/json; charset=utf-8",
                        format!(
                            "{{\"status\":\"error\",\"error\":\"{escaped}\",\"message\":\"{escaped}\"}}"
                        ),
                    )
                }
            }
        }
        _ => (
            "404 Not Found",
            "application/json; charset=utf-8",
            "{\"status\":\"not_found\"}".to_string(),
        ),
    };

    write_http_response(stream, status, content_type, &response_body);
}

pub fn serve_http_on_listener(listener: TcpListener, stop: Arc<AtomicBool>) {
    serve_http_on_listener_with_handler(listener, stop, Arc::new(EasyOfficeIpcRouter::dispatch));
}

pub fn serve_http_on_listener_with_handler(
    listener: TcpListener,
    stop: Arc<AtomicBool>,
    ipc_handler: Arc<IpcDispatchFn>,
) {
    let bound_port = listener.local_addr().map(|a| a.port()).unwrap_or(8787);
    let _ = listener.set_nonblocking(true);
    let active_threads = Arc::new(AtomicUsize::new(0));
    let mut workers: Vec<thread::JoinHandle<()>> = Vec::new();

    while !stop.load(Ordering::Acquire) {
        let mut idx = 0usize;
        while idx < workers.len() {
            if workers[idx].is_finished() {
                let _ = workers.swap_remove(idx).join();
            } else {
                idx += 1;
            }
        }

        match listener.accept() {
            Ok((mut stream, _)) => {
                // BSD/macOS sockets inherit O_NONBLOCK from the listener; restore blocking mode so timeouts apply.
                let _ = stream.set_nonblocking(false);
                if let Some(slot_guard) =
                    ConnectionSlotGuard::try_acquire(&active_threads, MAX_CONCURRENT_HTTP_THREADS)
                {
                    let handler = Arc::clone(&ipc_handler);
                    workers.push(thread::spawn(move || {
                        let _guard = slot_guard;
                        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            handle_http_connection(&mut stream, bound_port, &*handler);
                        }));
                        if outcome.is_err() {
                            let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
                            write_http_response(
                                &mut stream,
                                "500 Internal Server Error",
                                "application/json; charset=utf-8",
                                "{\"status\":\"internal_error\",\"message\":\"내부 요청 처리 중 오류가 발생했습니다.\"}",
                            );
                        }
                    }));
                } else {
                    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
                    write_http_response(
                        &mut stream,
                        "503 Service Unavailable",
                        "application/json; charset=utf-8",
                        "{\"status\":\"service_unavailable\",\"message\":\"동시 요청 한도를 초과했습니다.\"}",
                    );
                }
            }
            Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(15));
            }
            Err(_) => {
                thread::sleep(Duration::from_millis(15));
            }
        }
    }

    for worker in workers {
        let _ = worker.join();
    }
}

/// Binds the loopback listener. A user-specified port (`allow_fallback == false`)
/// is bound exactly or the error is returned; only the default port may move to
/// 8788..=8800 and finally an ephemeral port.
pub(crate) fn bind_with_fallback(
    preferred_port: u16,
    allow_fallback: bool,
) -> std::io::Result<TcpListener> {
    let first = TcpListener::bind(format!("127.0.0.1:{preferred_port}"));
    if first.is_ok() || !allow_fallback {
        return first;
    }
    for candidate in 8788..=8800 {
        if let Ok(listener) = TcpListener::bind(format!("127.0.0.1:{candidate}")) {
            return Ok(listener);
        }
    }
    TcpListener::bind("127.0.0.1:0")
}

fn try_open_desktop_browser(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

pub fn serve_webui(port: crate::cli_args::ServePort, open_browser: bool) -> ExitCode {
    let preferred_port = port.port();
    let listener = match bind_with_fallback(preferred_port, port.allows_fallback()) {
        Ok(l) => l,
        Err(err) if !port.allows_fallback() => {
            eprintln!(
                "error: port {preferred_port} on 127.0.0.1 is unavailable ({err}). \
                 Choose another port with --serve <PORT>, or use --serve without a port \
                 to try the default {} with automatic fallback.",
                crate::cli_args::DEFAULT_SERVE_PORT
            );
            return ExitCode::FAILURE;
        }
        Err(err) => {
            eprintln!("Failed to bind 127.0.0.1:{preferred_port}: {err}");
            return ExitCode::FAILURE;
        }
    };
    let bound_port = listener
        .local_addr()
        .map(|a| a.port())
        .unwrap_or(preferred_port);
    let url = format!("http://127.0.0.1:{bound_port}");
    println!("EasyOffice WebUI Server listening on {url}");
    println!("{SERVE_DEV_ONLY_NOTICE}");
    if open_browser {
        try_open_desktop_browser(&url);
    }
    serve_http_on_listener(listener, Arc::new(AtomicBool::new(false)));
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ipc_http_body_unicode_escapes_and_host_origin_guards() {
        let json = r#"{"command":"evaluate_sheet","payload":"\uAC00\uB098\uB2E4\uD83D\uDE00\/A1=10\nB1=20"}"#;
        let (cmd, payload) = parse_ipc_http_body(json).unwrap();
        assert_eq!(cmd, "evaluate_sheet");
        assert_eq!(payload, "가나다😀/A1=10\nB1=20");

        let nested = r#"{"command":"evaluate_sheet","payload":{"sheet":"Sheet1","cells":[{"cell":"A1","value":42}],"expr":"A1"}}"#;
        let (nested_cmd, nested_payload) = parse_ipc_http_body(nested).unwrap();
        assert_eq!(nested_cmd, "evaluate_sheet");
        let dispatched = EasyOfficeIpcRouter::dispatch(&nested_cmd, &nested_payload).unwrap();
        assert!(dispatched.contains("\"value\":\"42\""));

        assert!(parse_ipc_http_body(r#"{"command":"scrub_pii","payload":"\ud83d"}"#).is_err());
        assert!(parse_ipc_http_body(r#"{"command":"evaluate_sheet","command":"dup"}"#).is_err());
        assert!(is_allowed_host("127.0.0.1:8787", 8787));
        assert!(is_allowed_host("[::1]:8787", 8787));
        assert!(is_allowed_host("[::1]", 8787));
        assert!(!is_allowed_host("evil.example.com", 8787));
        assert!(is_allowed_origin("http://localhost:8787/", 8787));
        assert!(is_allowed_origin("http://[::1]:8787/", 8787));
        assert!(!is_allowed_origin("http://evil.example.com", 8787));
    }
}
