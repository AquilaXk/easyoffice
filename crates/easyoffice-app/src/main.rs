use easyoffice_app::{run_self_test, EasyOfficeIpcRouter};
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
const APP_JS: &str = include_str!("../ui/app.js");
const MAX_HTTP_BODY_BYTES: usize = 262_144;
const MAX_CONCURRENT_HTTP_THREADS: usize = 32;

fn extract_json_string_field(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next()? {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'b' => out.push('\u{0008}'),
                'f' => out.push('\u{000C}'),
                '"' => out.push('"'),
                '/' => out.push('/'),
                '\\' => out.push('\\'),
                'u' => {
                    let mut hex = String::with_capacity(4);
                    for _ in 0..4 {
                        hex.push(chars.next()?);
                    }
                    let code = u32::from_str_radix(&hex, 16).ok()?;
                    out.push(char::from_u32(code)?);
                }
                other => out.push(other),
            }
        } else if ch == '"' {
            return Some(out);
        } else {
            out.push(ch);
        }
    }
    None
}

fn is_allowed_host(host: &str, port: u16) -> bool {
    let h = host.trim();
    h.eq_ignore_ascii_case(&format!("127.0.0.1:{port}"))
        || h.eq_ignore_ascii_case(&format!("localhost:{port}"))
        || h.eq_ignore_ascii_case("127.0.0.1")
        || h.eq_ignore_ascii_case("localhost")
}

fn is_allowed_origin(origin: &str, port: u16) -> bool {
    let o = origin.trim().trim_end_matches('/');
    o.eq_ignore_ascii_case(&format!("http://127.0.0.1:{port}"))
        || o.eq_ignore_ascii_case(&format!("http://localhost:{port}"))
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
                let raw_len = v.trim().parse::<usize>().unwrap_or(0);
                if raw_len > MAX_HTTP_BODY_BYTES {
                    let body = "{\"status\":\"payload_too_large\",\"message\":\"HTTP 본문 크기가 허용 한도(256KB)를 초과했습니다.\"}";
                    let resp = format!(
                        "HTTP/1.1 413 Payload Too Large\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(resp.as_bytes());
                    let _ = stream.flush();
                    return;
                }
                content_length = raw_len;
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
        ("GET", "/app.js") => (
            "200 OK",
            "application/javascript; charset=utf-8",
            APP_JS.to_string(),
        ),
        ("POST", "/api/ipc") => {
            let trimmed = body_str.trim();
            let (cmd, payload) = if trimmed.starts_with('{') {
                let c = extract_json_string_field(trimmed, "command").unwrap_or_default();
                let p = extract_json_string_field(trimmed, "payload").unwrap_or_default();
                (c, p)
            } else if let Some((c, p)) = trimmed.split_once('|') {
                (c.trim().to_string(), p.to_string())
            } else {
                (trimmed.to_string(), String::new())
            };
            match EasyOfficeIpcRouter::dispatch(&cmd, &payload) {
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

fn serve_webui(port: u16) -> ExitCode {
    let addr = format!("127.0.0.1:{port}");
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(err) => {
            eprintln!("Failed to bind {addr}: {err}");
            return ExitCode::FAILURE;
        }
    };
    println!("EasyOffice WebUI Server listening on http://{addr}");
    let active_threads = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming().flatten() {
        let current = active_threads.load(Ordering::Relaxed);
        if current < MAX_CONCURRENT_HTTP_THREADS {
            active_threads.fetch_add(1, Ordering::Relaxed);
            let counter = Arc::clone(&active_threads);
            thread::spawn(move || {
                handle_http_connection(stream, port);
                counter.fetch_sub(1, Ordering::Relaxed);
            });
        } else {
            handle_http_connection(stream, port);
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

    if let Some(idx) = args.iter().position(|a| a == "--serve") {
        let port = args
            .get(idx + 1)
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(8787);
        return serve_webui(port);
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
    fn test_extract_json_string_field_unicode_escapes_and_host_origin_guards() {
        let json = r#"{"command":"evaluate_sheet","payload":"\uAC00\uB098\uB2E4\/A1=10\nB1=20"}"#;
        assert_eq!(
            extract_json_string_field(json, "command").as_deref(),
            Some("evaluate_sheet")
        );
        assert_eq!(
            extract_json_string_field(json, "payload").as_deref(),
            Some("가나다/A1=10\nB1=20")
        );
        assert!(is_allowed_host("127.0.0.1:8787", 8787));
        assert!(!is_allowed_host("evil.example.com", 8787));
        assert!(is_allowed_origin("http://localhost:8787/", 8787));
        assert!(!is_allowed_origin("http://evil.example.com", 8787));
    }
}
