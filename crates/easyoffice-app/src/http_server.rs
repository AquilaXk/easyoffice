use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use std::borrow::Cow;
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
const UPDATE_NOTICE_JS: &str = include_str!("../ui/update-notice.js");
const RECENT_DIALOG_JS: &str = include_str!("../ui/recent-documents-dialog.js");
const ICONS_JS: &str = include_str!("../ui/icons.js");
const WINDOW_BRIDGE_JS: &str = include_str!("../ui/window-bridge.js");
const IPC_TRANSPORT_JS: &str = include_str!("../ui/ipc-transport.js");
const MENU_KEYBOARD_JS: &str = include_str!("../ui/menu-keyboard.js");
const MENU_KINDS_JS: &str = include_str!("../ui/menu-kinds.js");
const SHEET_UTILS_JS: &str = include_str!("../ui/sheet-utils.js");
const PII_DOM_SCRUBBER_JS: &str = include_str!("../ui/pii-dom-scrubber.js");
const SHEET_STORE_JS: &str = include_str!("../ui/sheet-store.js");
const RIBBON_CATALOG_JS: &str = include_str!("../ui/ribbon-catalog.js");
const SAVE_PAYLOAD_JS: &str = include_str!("../ui/save-payload.js");
const DESKTOP_BRIDGE_JS: &str = include_str!("../ui/desktop-bridge.js");
const CUSTOM_SELECT_JS: &str = include_str!("../ui/custom-select.js");
const COLOR_PICKER_JS: &str = include_str!("../ui/color-picker.js");
const WORKSPACE_SCRUB_JS: &str = include_str!("../ui/workspace-scrub.js");
const WORKSPACE_TOOLS_JS: &str = include_str!("../ui/workspace-tools.js");
const WORKSPACE_PANELS_JS: &str = include_str!("../ui/workspace-panels.js");
const SHEET_GRID_JS: &str = include_str!("../ui/sheet-grid.js");
const SHEET_CLIPBOARD_JS: &str = include_str!("../ui/sheet-clipboard.js");
const SHEET_INPUT_JS: &str = include_str!("../ui/sheet-input.js");
const SHEET_SORT_JS: &str = include_str!("../ui/sheet-sort.js");
const SHEET_SORT_UI_JS: &str = include_str!("../ui/sheet-sort-ui.js");
const SHEET_OPS_JS: &str = include_str!("../ui/sheet-ops.js");
const SHEET_LAYOUT_JS: &str = include_str!("../ui/sheet-layout.js");
const SHEET_FILTER_JS: &str = include_str!("../ui/sheet-filter.js");
const SHEET_FILTER_UI_JS: &str = include_str!("../ui/sheet-filter-ui.js");
const SHEET_STRUCTURE_JS: &str = include_str!("../ui/sheet-structure.js");
/// The scripts of the sheet's view (PD-208) by request path.
const SHEET_VIEW_SCRIPTS: [(&str, &str); 5] = [
    ("/sheet-outline.js", include_str!("../ui/sheet-outline.js")),
    ("/sheet-freeze.js", include_str!("../ui/sheet-freeze.js")),
    ("/sheet-view-ui.js", include_str!("../ui/sheet-view-ui.js")),
    (
        "/sheet-outline-ui.js",
        include_str!("../ui/sheet-outline-ui.js"),
    ),
    (
        "/sheet-tab-color.js",
        include_str!("../ui/sheet-tab-color.js"),
    ),
];
const SHEET_FORMAT_JS: &str = include_str!("../ui/sheet-format.js");
const SHEET_CONTROLLER_JS: &str = include_str!("../ui/sheet-controller.js");
const RIBBON_HISTORY_JS: &str = include_str!("../ui/ribbon-history.js");
const WORKSPACE_SESSION_JS: &str = include_str!("../ui/workspace-session.js");
const FILE_OPEN_JS: &str = include_str!("../ui/file-open.js");
const PDF_EDIT_JS: &str = include_str!("../ui/pdf-edit.js");
const SLIDE_STUDIO_JS: &str = include_str!("../ui/slide-studio.js");
const DOC_RUNS_JS: &str = include_str!("../ui/doc-runs.js");
const DOC_RUNS_UI_JS: &str = include_str!("../ui/doc-runs-ui.js");
const HWP_HOST_JS: &str = include_str!("../ui/hwp-host.js");
const HWP_PRINT_JS: &str = include_str!("../ui/hwp-print.js");
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

#[inline]
fn is_loopback_name(name: &str) -> bool {
    name == "127.0.0.1" || name == "[::1]" || name.eq_ignore_ascii_case("localhost")
}

pub fn is_allowed_host(host: &str, port: u16) -> bool {
    let h = host.trim();
    if is_loopback_name(h) {
        return true;
    }
    if let Some((name, port_str)) = h.rsplit_once(':') {
        is_loopback_name(name) && port_str.parse::<u16>() == Ok(port)
    } else {
        false
    }
}

pub fn is_allowed_origin(origin: &str, port: u16) -> bool {
    let o = origin.trim().trim_end_matches('/');
    if o.len() <= 7 || !o[..7].eq_ignore_ascii_case("http://") {
        return false;
    }
    let authority = &o[7..];
    if let Some((name, port_str)) = authority.rsplit_once(':') {
        is_loopback_name(name) && port_str.parse::<u16>() == Ok(port)
    } else {
        false
    }
}

/// CSP of the main page and every API/error response. A same-origin <iframe> is already allowed by
/// the `default-src` fallback (`frame-src` -> `child-src` -> `default-src`), so the editor frame
/// needs no relaxation here; only `/hwp-studio/*` gets its own policy (`hwp_bundle::STUDIO_CSP`).
const MAIN_CSP: &str = "default-src 'self'";

fn write_http_response(stream: &mut TcpStream, status: &str, content_type: &str, body: &str) {
    write_http_bytes(stream, status, content_type, MAIN_CSP, "", body.as_bytes());
}

fn write_http_bytes(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    csp: &str,
    extra_headers: &str,
    body: &[u8],
) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nContent-Security-Policy: {csp}\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store\r\n{extra_headers}Connection: close\r\n\r\n",
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

fn write_http_400_framing(stream: &mut TcpStream) {
    write_http_response(
        stream,
        "400 Bad Request",
        "application/json; charset=utf-8",
        "{\"status\":\"bad_request\",\"message\":\"Content-Length 헤더가 올바르지 않습니다.\"}",
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

/// CSP of the editor document. It names the origin the page was opened from instead of `'self'`:
/// the editor frame has an opaque origin and WebKit does not match `'self'` there, so it would
/// block the frame's scripts before any request (see `hwp_bundle::studio_csp_for_origin`). The
/// origin comes from the `Host` header, which `forbidden_header_message` already limited to
/// loopback names; without a usable one the page is taken to be at `127.0.0.1`.
fn studio_csp_for_host(host: Option<&str>, port: u16) -> String {
    let origin = host
        .map(str::trim)
        .filter(|host| is_allowed_host(host, port) && host.ends_with(&format!(":{port}")))
        .map_or_else(
            || format!("http://127.0.0.1:{port}"),
            |host| format!("http://{host}"),
        );
    hwp_bundle::studio_csp_for_origin(&origin).unwrap_or_else(|| hwp_bundle::STUDIO_CSP.to_string())
}

/// Serves one file of the hwp-studio bundle (binary body, exact MIME type, iframe CSP). Anything
/// that is not a plain file path inside the bundle, or a bundle that is not present, is a 404.
fn write_bundle_file(stream: &mut TcpStream, request_path: &str, csp: &str) {
    let path_only = request_path.split(['?', '#']).next().unwrap_or("");
    let file =
        hwp_bundle::sanitize_relative_path(path_only).and_then(|rel| hwp_bundle::lookup(&rel));
    match file {
        Some(file) => write_http_bytes(
            stream,
            "200 OK",
            file.content_type,
            csp,
            hwp_bundle::STUDIO_CORS_HEADER,
            &file.bytes,
        ),
        None => write_http_bytes(
            stream,
            "404 Not Found",
            "application/json; charset=utf-8",
            csp,
            hwp_bundle::STUDIO_CORS_HEADER,
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
    /// The page's IPC client sent [`IPC_REQUEST_HEADER`].
    ipc_marker: bool,
}

/// Why the header block was refused.
enum HeaderError {
    /// `Content-Length` above the body limit: 413.
    TooLarge,
    /// A `Content-Length` that is not plain digits or appears more than once: the message framing
    /// is unreliable (RFC 9112 6.3), 400.
    BadFraming,
}

/// Header of a `POST /api/ipc` request that only the page's own IPC client sends. A script on
/// another origin cannot add a custom header without a CORS preflight, which this server never
/// answers, so a request without it (an Origin-less one from a plain HTTP client included) is
/// refused.
const IPC_REQUEST_HEADER: &str = "x-easyoffice-ipc";

/// Parses the header lines. A repeated `Content-Length` is refused even when the values agree: the
/// one to trust would be a guess.
fn parse_request_headers<'a>(
    lines: impl Iterator<Item = &'a str>,
    body_limit: usize,
) -> Result<RequestHeaders, HeaderError> {
    let mut headers = RequestHeaders::default();
    let mut length_seen = false;
    for line in lines {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let key = k.trim();
        let value = v.trim();
        if key.eq_ignore_ascii_case("content-length") {
            if length_seen || value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(HeaderError::BadFraming);
            }
            length_seen = true;
            headers.content_length = value
                .parse::<usize>()
                .ok()
                .filter(|&len| len <= body_limit)
                .ok_or(HeaderError::TooLarge)?;
        } else if key.eq_ignore_ascii_case("host") {
            headers.host = Some(value.to_string());
        } else if key.eq_ignore_ascii_case("origin") {
            headers.origin = Some(value.to_string());
        } else if key.eq_ignore_ascii_case(IPC_REQUEST_HEADER) {
            headers.ipc_marker = value == "1";
        }
    }
    Ok(headers)
}

/// Rejection message for a disallowed `Host` (any method) or `Origin` (POST only) header, or an IPC
/// request that lacks the page's marker header.
fn forbidden_header_message(
    headers: &RequestHeaders,
    method: &str,
    path: &str,
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
    if bad_origin {
        return Some("{\"status\":\"forbidden\",\"message\":\"허용되지 않은 Origin 헤더입니다.\"}");
    }
    (method == "POST" && is_ipc_path(path) && !headers.ipc_marker)
        .then_some("{\"status\":\"forbidden\",\"message\":\"IPC 요청 헤더가 없습니다.\"}")
}

/// Whether `path` addresses the IPC endpoint (launch parameters after `?` do not matter).
fn is_ipc_path(path: &str) -> bool {
    path.split('?').next() == Some("/api/ipc")
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
    let body_limit = if method == "POST" && is_ipc_path(path) {
        MAX_IPC_HTTP_BODY_BYTES
    } else {
        MAX_HTTP_BODY_BYTES
    };

    let headers = match parse_request_headers(lines, body_limit) {
        Ok(headers) => headers,
        Err(HeaderError::TooLarge) => return write_http_413(stream),
        Err(HeaderError::BadFraming) => return write_http_400_framing(stream),
    };
    if let Some(message) = forbidden_header_message(&headers, method, path, port) {
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
        write_bundle_file(
            stream,
            path,
            &studio_csp_for_host(headers.host.as_deref(), port),
        );
        return;
    }

    // Document windows are addressed with launch parameters (`/?module=DOC&win=...`); routing ignores them.
    let route = path.split('?').next().unwrap_or("/");
    let view_script = SHEET_VIEW_SCRIPTS.iter().find(|(path, _)| *path == route);
    let (status, content_type, response_body) = match (method, route) {
        ("GET", "/" | "/index.html") => (
            "200 OK",
            "text/html; charset=utf-8",
            Cow::Borrowed(INDEX_HTML),
        ),
        ("GET", "/favicon.ico" | "/favicon.svg") => (
            "200 OK",
            "image/svg+xml; charset=utf-8",
            Cow::Borrowed(FAVICON_SVG),
        ),
        ("GET", "/styles.css") => (
            "200 OK",
            "text/css; charset=utf-8",
            Cow::Borrowed(STYLES_CSS),
        ),
        ("GET", "/shell.css") => (
            "200 OK",
            "text/css; charset=utf-8",
            Cow::Borrowed(SHELL_CSS),
        ),
        ("GET", "/tokens.css") => (
            "200 OK",
            "text/css; charset=utf-8",
            Cow::Borrowed(TOKENS_CSS),
        ),
        ("GET", "/content.css") => (
            "200 OK",
            "text/css; charset=utf-8",
            Cow::Borrowed(CONTENT_CSS),
        ),
        ("GET", "/menu-bar.css") => (
            "200 OK",
            "text/css; charset=utf-8",
            Cow::Borrowed(MENU_BAR_CSS),
        ),
        ("GET", "/toolbar.css") => (
            "200 OK",
            "text/css; charset=utf-8",
            Cow::Borrowed(TOOLBAR_CSS),
        ),
        ("GET", "/home.css") => ("200 OK", "text/css; charset=utf-8", Cow::Borrowed(HOME_CSS)),
        ("GET", "/home-screen.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(HOME_SCREEN_JS),
        ),
        ("GET", "/recent-documents-dialog.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(RECENT_DIALOG_JS),
        ),
        ("GET", "/update-notice.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(UPDATE_NOTICE_JS),
        ),
        ("GET", "/theme.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(THEME_JS),
        ),
        ("GET", "/icons.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(ICONS_JS),
        ),
        ("GET", "/window-bridge.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(WINDOW_BRIDGE_JS),
        ),
        ("GET", "/ipc-transport.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(IPC_TRANSPORT_JS),
        ),
        ("GET", "/menu-keyboard.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(MENU_KEYBOARD_JS),
        ),
        ("GET", "/menu-kinds.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(MENU_KINDS_JS),
        ),
        ("GET", "/sheet-utils.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(SHEET_UTILS_JS),
        ),
        ("GET", "/pii-dom-scrubber.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(PII_DOM_SCRUBBER_JS),
        ),
        ("GET", "/sheet-store.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(SHEET_STORE_JS),
        ),
        ("GET", "/ribbon-catalog.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(RIBBON_CATALOG_JS),
        ),
        ("GET", "/save-payload.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(SAVE_PAYLOAD_JS),
        ),
        ("GET", "/desktop-bridge.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(DESKTOP_BRIDGE_JS),
        ),
        ("GET", "/custom-select.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(CUSTOM_SELECT_JS),
        ),
        ("GET", "/color-picker.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(COLOR_PICKER_JS),
        ),
        ("GET", "/workspace-scrub.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(WORKSPACE_SCRUB_JS),
        ),
        ("GET", "/workspace-tools.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(WORKSPACE_TOOLS_JS),
        ),
        ("GET", "/workspace-panels.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(WORKSPACE_PANELS_JS),
        ),
        ("GET", "/sheet-grid.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(SHEET_GRID_JS),
        ),
        ("GET", "/sheet-clipboard.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(SHEET_CLIPBOARD_JS),
        ),
        ("GET", "/sheet-input.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(SHEET_INPUT_JS),
        ),
        ("GET", "/sheet-sort.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(SHEET_SORT_JS),
        ),
        ("GET", "/sheet-sort-ui.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(SHEET_SORT_UI_JS),
        ),
        ("GET", "/sheet-ops.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(SHEET_OPS_JS),
        ),
        ("GET", "/sheet-layout.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(SHEET_LAYOUT_JS),
        ),
        ("GET", "/sheet-filter.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(SHEET_FILTER_JS),
        ),
        ("GET", "/sheet-filter-ui.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(SHEET_FILTER_UI_JS),
        ),
        ("GET", "/sheet-structure.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(SHEET_STRUCTURE_JS),
        ),
        ("GET", _) if view_script.is_some() => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(view_script.map_or("", |(_, js)| js)),
        ),
        ("GET", "/sheet-format.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(SHEET_FORMAT_JS),
        ),
        ("GET", "/sheet-controller.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(SHEET_CONTROLLER_JS),
        ),
        ("GET", "/ribbon-history.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(RIBBON_HISTORY_JS),
        ),
        ("GET", "/workspace-session.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(WORKSPACE_SESSION_JS),
        ),
        ("GET", "/file-open.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(FILE_OPEN_JS),
        ),
        ("GET", "/pdf-edit.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(PDF_EDIT_JS),
        ),
        ("GET", "/slide-studio.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(SLIDE_STUDIO_JS),
        ),
        ("GET", "/doc-runs.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(DOC_RUNS_JS),
        ),
        ("GET", "/doc-runs-ui.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(DOC_RUNS_UI_JS),
        ),
        ("GET", "/hwp-host.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(HWP_HOST_JS),
        ),
        ("GET", "/hwp-print.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(HWP_PRINT_JS),
        ),
        ("GET", "/hwp-session.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(HWP_SESSION_JS),
        ),
        ("GET", "/shell-chrome.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(SHELL_CHROME_JS),
        ),
        ("GET", "/app.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            Cow::Borrowed(APP_JS),
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
                Ok(json) => (
                    "200 OK",
                    "application/json; charset=utf-8",
                    Cow::Owned(json),
                ),
                Err(err) => {
                    let escaped = EasyOfficeIpcRouter::escape_json_str(&err.to_string());
                    (
                        "400 Bad Request",
                        "application/json; charset=utf-8",
                        Cow::Owned(format!(
                            "{{\"status\":\"error\",\"error\":\"{escaped}\",\"message\":\"{escaped}\"}}"
                        )),
                    )
                }
            }
        }
        _ => (
            "404 Not Found",
            "application/json; charset=utf-8",
            Cow::Borrowed("{\"status\":\"not_found\"}"),
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
