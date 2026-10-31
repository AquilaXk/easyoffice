use easyoffice_app::{run_self_test, EasyOfficeIpcRouter};
use easyoffice_formats::StrictJsonValue;
use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const INDEX_HTML: &str = include_str!("../ui/index.html");
const STYLES_CSS: &str = include_str!("../ui/styles.css");
const OFFLINE_FORMULA_AST_JS: &str = include_str!("../ui/offline-formula-ast.js");
const OFFLINE_FORMULA_EVAL_JS: &str = include_str!("../ui/offline-formula-eval.js");
const OFFLINE_RPC_JS: &str = include_str!("../ui/offline-rpc.js");
const PII_DOM_SCRUBBER_JS: &str = include_str!("../ui/pii-dom-scrubber.js");
const SHEET_STORE_JS: &str = include_str!("../ui/sheet-store.js");
const RIBBON_CATALOG_JS: &str = include_str!("../ui/ribbon-catalog.js");
const WORKSPACE_TOOLS_JS: &str = include_str!("../ui/workspace-tools.js");
const WORKSPACE_PANELS_JS: &str = include_str!("../ui/workspace-panels.js");
const SHEET_CONTROLLER_JS: &str = include_str!("../ui/sheet-controller.js");
const RIBBON_HISTORY_JS: &str = include_str!("../ui/ribbon-history.js");
const APP_JS: &str = include_str!("../ui/app.js");
const MAX_HTTP_BODY_BYTES: usize = 1_048_576;
const MAX_CONCURRENT_HTTP_THREADS: usize = 32;

fn parse_ipc_http_body(
    body: &str,
) -> Result<(String, String), easyoffice_formats::EasyOfficeError> {
    let trimmed = body.strip_prefix('\u{FEFF}').unwrap_or(body).trim();
    if trimmed.starts_with('{') {
        let root = StrictJsonValue::parse(trimmed)?;
        let entries = root.as_object().ok_or_else(|| {
            easyoffice_formats::EasyOfficeError::InvalidArgument(
                "IPC HTTP 요청 본문은 JSON 객체여야 합니다.".to_string(),
            )
        })?;
        let cmd = StrictJsonValue::get_str(entries, "command")
            .ok_or_else(|| {
                easyoffice_formats::EasyOfficeError::InvalidArgument(
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

fn is_allowed_host(host: &str, port: u16) -> bool {
    let h = host.trim();
    h.eq_ignore_ascii_case(&format!("127.0.0.1:{port}"))
        || h.eq_ignore_ascii_case(&format!("localhost:{port}"))
        || h.eq_ignore_ascii_case(&format!("[::1]:{port}"))
        || h.eq_ignore_ascii_case("127.0.0.1")
        || h.eq_ignore_ascii_case("localhost")
        || h.eq_ignore_ascii_case("[::1]")
}

fn is_allowed_origin(origin: &str, port: u16) -> bool {
    let o = origin.trim().trim_end_matches('/');
    o.eq_ignore_ascii_case(&format!("http://127.0.0.1:{port}"))
        || o.eq_ignore_ascii_case(&format!("http://localhost:{port}"))
        || o.eq_ignore_ascii_case(&format!("http://[::1]:{port}"))
}

fn write_http_413(stream: &mut TcpStream) {
    let body = "{\"status\":\"payload_too_large\",\"message\":\"HTTP 본문 크기가 허용 한도(1MB)를 초과했습니다.\"}";
    let resp = format!(
        "HTTP/1.1 413 Payload Too Large\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
}

fn handle_http_connection(mut stream: TcpStream, port: u16) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

    let mut buf = Vec::with_capacity(8192);
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    break pos + 4;
                }
                if buf.len() > MAX_HTTP_BODY_BYTES {
                    write_http_413(&mut stream);
                    return;
                }
            }
        }
    };

    let header_str = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = header_str.lines();
    let Some(req_line) = lines.next() else {
        return;
    };
    let mut parts = req_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let path = parts.next().unwrap_or("/");

    let mut content_length = 0usize;
    let mut host_header: Option<String> = None;
    let mut origin_header: Option<String> = None;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim();
            if key.eq_ignore_ascii_case("content-length") {
                match v.trim().parse::<usize>() {
                    Ok(raw_len) if raw_len <= MAX_HTTP_BODY_BYTES => {
                        content_length = raw_len;
                    }
                    _ => {
                        write_http_413(&mut stream);
                        return;
                    }
                }
            } else if key.eq_ignore_ascii_case("host") {
                host_header = Some(v.trim().to_string());
            } else if key.eq_ignore_ascii_case("origin") {
                origin_header = Some(v.trim().to_string());
            }
        }
    }

    if let Some(ref host) = host_header {
        if !is_allowed_host(host, port) {
            let body = "{\"status\":\"forbidden\",\"message\":\"허용되지 않은 Host 헤더입니다.\"}";
            let resp = format!(
                "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
            return;
        }
    }

    if method == "POST" {
        if let Some(ref origin) = origin_header {
            if !is_allowed_origin(origin, port) {
                let body =
                    "{\"status\":\"forbidden\",\"message\":\"허용되지 않은 Origin 헤더입니다.\"}";
                let resp = format!(
                    "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
                return;
            }
        }
    }

    while buf.len() < header_end + content_length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > header_end + MAX_HTTP_BODY_BYTES {
                    break;
                }
            }
        }
    }

    let body_slice = if buf.len() >= header_end {
        &buf[header_end..buf.len().min(header_end + content_length)]
    } else {
        &[][..]
    };
    let body_str = String::from_utf8_lossy(body_slice);

    let (status, content_type, response_body) = match (method, path) {
        ("GET", "/" | "/index.html") => {
            ("200 OK", "text/html; charset=utf-8", INDEX_HTML.to_string())
        }
        ("GET", "/styles.css") => ("200 OK", "text/css; charset=utf-8", STYLES_CSS.to_string()),
        ("GET", "/offline-formula-ast.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            OFFLINE_FORMULA_AST_JS.to_string(),
        ),
        ("GET", "/offline-formula-eval.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            OFFLINE_FORMULA_EVAL_JS.to_string(),
        ),
        ("GET", "/offline-rpc.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            OFFLINE_RPC_JS.to_string(),
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
        ("GET", "/app.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            APP_JS.to_string(),
        ),
        ("POST", "/api/ipc") => {
            let dispatched = parse_ipc_http_body(&body_str)
                .and_then(|(cmd, payload)| EasyOfficeIpcRouter::dispatch(&cmd, &payload));
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

    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
}

fn bind_with_fallback(preferred_port: u16) -> std::io::Result<TcpListener> {
    if let Ok(listener) = TcpListener::bind(format!("127.0.0.1:{preferred_port}")) {
        return Ok(listener);
    }
    if preferred_port == 8787 {
        for candidate in 8788..=8800 {
            if let Ok(listener) = TcpListener::bind(format!("127.0.0.1:{candidate}")) {
                return Ok(listener);
            }
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

fn serve_webui(preferred_port: u16, open_browser: bool) -> ExitCode {
    let listener = match bind_with_fallback(preferred_port) {
        Ok(l) => l,
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
    if open_browser {
        try_open_desktop_browser(&url);
    }
    let active_threads = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming().flatten() {
        let current = active_threads.load(Ordering::Relaxed);
        if current < MAX_CONCURRENT_HTTP_THREADS {
            active_threads.fetch_add(1, Ordering::Relaxed);
            let counter = Arc::clone(&active_threads);
            thread::spawn(move || {
                handle_http_connection(stream, bound_port);
                counter.fetch_sub(1, Ordering::Relaxed);
            });
        } else {
            handle_http_connection(stream, bound_port);
        }
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.iter().any(|a| a == "--self-test" || a == "--doctor") {
        match run_self_test() {
            Ok(report) => {
                println!("{report}");
                return ExitCode::SUCCESS;
            }
            Err(err) => {
                eprintln!("Self-test failed: {err}");
                return ExitCode::FAILURE;
            }
        }
    }

    let open_flag = args.iter().any(|a| a == "--open");
    if let Some(idx) = args.iter().position(|a| a == "--serve") {
        let port = args
            .get(idx + 1)
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(8787);
        return serve_webui(port, open_flag);
    }
    if open_flag {
        return serve_webui(8787, true);
    }

    let headless = args.iter().any(|a| a == "--bootstrap" || a == "--headless")
        || env::var("EASYOFFICE_HEADLESS").is_ok()
        || env::var("CI").is_ok();
    if cfg!(target_os = "windows") && args.is_empty() && !headless {
        return serve_webui(8787, true);
    }

    match EasyOfficeIpcRouter::dispatch("get_workspace_bootstrap", "") {
        Ok(bootstrap) => {
            println!("EasyOffice Desktop Host Ready: {bootstrap}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Bootstrap error: {err}");
            ExitCode::FAILURE
        }
    }
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
