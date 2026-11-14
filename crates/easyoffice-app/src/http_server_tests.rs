use super::*;
use std::io::{Read, Write};
use std::net::TcpStream;

#[test]
fn test_parse_ipc_http_body_unicode_escapes_and_host_origin_guards() {
    let json =
        r#"{"command":"evaluate_sheet","payload":"\uAC00\uB098\uB2E4\uD83D\uDE00\/A1=10\nB1=20"}"#;
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

#[test]
fn requests_without_host_header_are_forbidden() {
    let headers = RequestHeaders {
        host: None,
        origin: None,
        content_length: 0,
        ipc_marker: false,
    };
    let get_msg = forbidden_header_message(&headers, "GET", "/", 8787);
    assert!(
        get_msg.is_some_and(|m| m.contains("허용되지 않은 Host 헤더")),
        "missing Host header must be rejected for GET: {get_msg:?}"
    );

    let post_headers = RequestHeaders {
        host: None,
        origin: Some("http://127.0.0.1:8787".to_string()),
        content_length: 0,
        ipc_marker: true,
    };
    let post_msg = forbidden_header_message(&post_headers, "POST", "/api/ipc", 8787);
    assert!(
        post_msg.is_some_and(|m| m.contains("허용되지 않은 Host 헤더")),
        "missing Host header must be rejected for POST /api/ipc: {post_msg:?}"
    );

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let handle = std::thread::spawn(move || serve_http_on_listener(listener, server_stop));

    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .write_all(b"GET / HTTP/1.1\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();

    stop.store(true, Ordering::Release);
    handle.join().unwrap();

    assert!(resp.starts_with("HTTP/1.1 403 Forbidden\r\n"), "{resp}");
    assert!(resp.contains("허용되지 않은 Host 헤더"), "{resp}");
}
