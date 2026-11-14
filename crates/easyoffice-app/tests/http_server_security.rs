use easyoffice_app::{
    serve_http_on_listener, serve_http_on_listener_with_handler, EasyOfficeIpcRouter,
    MAX_CONCURRENT_HTTP_THREADS, MAX_CONNECTION_DURATION,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

fn send_raw_http_request(port: u16, raw_request: &[u8], timeout: Duration) -> String {
    let mut stream =
        TcpStream::connect(("127.0.0.1", port)).expect("connect to local test listener");
    stream
        .set_read_timeout(Some(timeout))
        .expect("set read timeout");
    stream
        .set_write_timeout(Some(timeout))
        .expect("set write timeout");
    stream.write_all(raw_request).expect("write HTTP request");
    stream.flush().expect("flush HTTP request");
    let mut out = Vec::with_capacity(4096);
    let _ = stream.read_to_end(&mut out);
    String::from_utf8_lossy(&out).into_owned()
}

#[test]
fn test_http_ipc_rejects_unpaired_surrogate_with_400_and_csp() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let server_handle = thread::spawn(move || {
        serve_http_on_listener(listener, server_stop);
    });

    let body = br#"{"command":"scrub_pii","payload":"\ud83d"}"#;
    let req = format!(
        "POST /api/ipc HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: http://127.0.0.1:{port}\r\nX-EasyOffice-Ipc: 1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let mut raw = req.into_bytes();
    raw.extend_from_slice(body);

    let resp = send_raw_http_request(port, &raw, Duration::from_secs(2));
    assert!(
        resp.starts_with("HTTP/1.1 400 Bad Request\r\n"),
        "expected 400 Bad Request, got: {resp}"
    );
    assert!(
        resp.contains(
            "Content-Security-Policy: default-src 'self'; base-uri 'none'; object-src 'none'\r\n"
        ),
        "missing CSP header on 400 response: {resp}"
    );

    stop.store(true, Ordering::Release);
    server_handle.join().expect("join server thread");
}

#[test]
fn test_http_panic_recovery_releases_all_32_slots_and_serves_subsequent_get() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);

    let handler = Arc::new(|cmd: &str, payload: &str| {
        if cmd == "simulate_panic" {
            panic!("simulated handler panic for slot guard test");
        }
        EasyOfficeIpcRouter::dispatch(cmd, payload)
    });

    let server_handle = thread::spawn(move || {
        serve_http_on_listener_with_handler(listener, server_stop, handler);
    });

    let panic_body = br#"{"command":"simulate_panic","payload":""}"#;
    for i in 0..MAX_CONCURRENT_HTTP_THREADS {
        let req = format!(
            "POST /api/ipc HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: http://127.0.0.1:{port}\r\nX-EasyOffice-Ipc: 1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            panic_body.len()
        );
        let mut raw = req.into_bytes();
        raw.extend_from_slice(panic_body);
        let resp = send_raw_http_request(port, &raw, Duration::from_secs(2));
        assert!(
            resp.starts_with("HTTP/1.1 500 Internal Server Error\r\n"),
            "iteration {i}: expected 500 Internal Server Error, got: {resp}"
        );
        assert!(
            resp.contains("Content-Security-Policy: default-src 'self'; base-uri 'none'; object-src 'none'\r\n"),
            "iteration {i}: missing CSP header on 500 response: {resp}"
        );
    }

    let get_req = format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    let ok_resp = send_raw_http_request(port, get_req.as_bytes(), Duration::from_secs(2));
    assert!(
        ok_resp.starts_with("HTTP/1.1 200 OK\r\n"),
        "expected 200 OK after 32 panics, got: {ok_resp}"
    );
    assert!(
        ok_resp.contains(
            "Content-Security-Policy: default-src 'self'; base-uri 'none'; object-src 'none'\r\n"
        ),
        "missing CSP header on 200 response: {ok_resp}"
    );

    stop.store(true, Ordering::Release);
    server_handle.join().expect("join server thread");
}

#[test]
fn test_http_slowloris_overflow_and_deadline_eviction_preserve_fast_get() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let server_handle = thread::spawn(move || {
        serve_http_on_listener(listener, server_stop);
    });

    let slow_stop = Arc::new(AtomicBool::new(false));
    let slow_count = MAX_CONCURRENT_HTTP_THREADS + 2;
    let mut slow_handles = Vec::with_capacity(slow_count);
    let wave_started = Instant::now();

    for _ in 0..slow_count {
        let client_stop = Arc::clone(&slow_stop);
        slow_handles.push(thread::spawn(move || {
            let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else {
                return;
            };
            let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));
            let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
            let header_bytes = b"GET / HTTP/1.1\r\nX-Slow: aaaaaaaaaaaaaaaa";
            for &b in header_bytes {
                if client_stop.load(Ordering::Acquire) {
                    break;
                }
                if stream.write_all(&[b]).is_err() || stream.flush().is_err() {
                    break;
                }
                for _ in 0..8 {
                    if client_stop.load(Ordering::Acquire) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }));
    }

    thread::sleep(Duration::from_millis(120));
    let get_req = format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    // The accept loop must answer without waiting for slow clients to be
    // evicted (3 s). A loaded machine may delay one sample, so a second sample
    // is allowed; the budget stays well below the eviction deadline.
    let timed_get = |budget: Duration| {
        let start = Instant::now();
        let resp = send_raw_http_request(port, get_req.as_bytes(), budget);
        (resp, start.elapsed())
    };
    let overflow_budget = Duration::from_millis(1500);
    let (mut overflow_resp, mut overflow_elapsed) = timed_get(overflow_budget);
    if overflow_elapsed >= overflow_budget {
        (overflow_resp, overflow_elapsed) = timed_get(overflow_budget);
    }
    assert!(
        overflow_elapsed < overflow_budget,
        "accept loop must never block on slowloris overflow ({overflow_elapsed:?})"
    );
    assert!(
        overflow_resp.starts_with("HTTP/1.1 503 Service Unavailable\r\n")
            || overflow_resp.starts_with("HTTP/1.1 200 OK\r\n"),
        "expected immediate 503 or 200 during slowloris wave, got: {overflow_resp}"
    );
    assert!(
        overflow_resp.contains(
            "Content-Security-Policy: default-src 'self'; base-uri 'none'; object-src 'none'\r\n"
        ),
        "missing CSP header during slowloris wave"
    );

    let wait_budget = (MAX_CONNECTION_DURATION + Duration::from_millis(200))
        .saturating_sub(wave_started.elapsed());
    if !wait_budget.is_zero() {
        thread::sleep(wait_budget);
    }

    let normal_budget = Duration::from_millis(2500);
    let (mut ok_resp, mut normal_elapsed) = timed_get(normal_budget);
    if !ok_resp.starts_with("HTTP/1.1 200 OK\r\n") || normal_elapsed >= normal_budget {
        (ok_resp, normal_elapsed) = timed_get(normal_budget);
    }
    assert!(
        ok_resp.starts_with("HTTP/1.1 200 OK\r\n"),
        "expected 200 OK after slowloris deadline eviction, got: {ok_resp}"
    );
    assert!(
        ok_resp.contains(
            "Content-Security-Policy: default-src 'self'; base-uri 'none'; object-src 'none'\r\n"
        ),
        "missing CSP header on GET /: {ok_resp}"
    );
    assert!(
        normal_elapsed < normal_budget,
        "normal GET / took {normal_elapsed:?} (>= {normal_budget:?})"
    );

    slow_stop.store(true, Ordering::Release);
    for h in slow_handles {
        let _ = h.join();
    }
    stop.store(true, Ordering::Release);
    server_handle.join().expect("join server thread");
}

fn start_server() -> (u16, Arc<AtomicBool>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let handle = thread::spawn(move || serve_http_on_listener(listener, server_stop));
    (port, stop, handle)
}

fn get(port: u16, path: &str) -> Vec<u8> {
    let raw = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(raw.as_bytes()).unwrap();
    let mut out = Vec::new();
    let _ = stream.read_to_end(&mut out);
    out
}

fn split_response(raw: &[u8]) -> (String, Vec<u8>) {
    let end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("header end")
        + 4;
    (
        String::from_utf8_lossy(&raw[..end]).into_owned(),
        raw[end..].to_vec(),
    )
}

#[test]
fn test_bundle_routes_never_leave_the_bundle_directory() {
    let (port, stop, handle) = start_server();
    for path in [
        "/hwp-studio/../Cargo.toml",
        "/hwp-studio/%2e%2e/Cargo.toml",
        "/hwp-studio/%2e%2e%2f%2e%2e%2fCargo.toml",
        "/hwp-studio/..%5cCargo.toml",
        "/hwp-studio/assets/../../ui/index.html",
        "/hwp-studio/C:/Windows/win.ini",
        "/hwp-studio/nope.js",
    ] {
        let (head, body) = split_response(&get(port, path));
        assert!(
            head.starts_with("HTTP/1.1 404 Not Found\r\n"),
            "{path}: {head}"
        );
        assert!(
            !String::from_utf8_lossy(&body).contains("[package]"),
            "{path} leaked a file outside the bundle"
        );
    }
    stop.store(true, Ordering::Release);
    handle.join().unwrap();
}

#[test]
fn test_cross_origin_read_is_granted_only_to_editor_bundle_responses() {
    // The editor frame has an opaque origin (sandbox without allow-same-origin), so its module
    // scripts, WASM and fonts are cross-origin reads. The grant must stay on `/hwp-studio/*`: the
    // page, the API and error pages never carry it.
    let (port, stop, handle) = start_server();
    for path in [
        "/",
        "/index.html",
        "/nope",
        "/hwp-studio/nope.js",
        "/hwp-studio/../Cargo.toml",
    ] {
        let (head, _) = split_response(&get(port, path));
        let granted = head.contains("Access-Control-Allow-Origin");
        assert_eq!(
            granted,
            path.starts_with("/hwp-studio/"),
            "{path} -> {head}"
        );
    }
    stop.store(true, Ordering::Release);
    handle.join().unwrap();
}

#[test]
fn test_bundle_files_are_binary_exact_with_mime_and_iframe_csp() {
    let bundle = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/hwp-studio");
    if !bundle.join("index.html").is_file() {
        eprintln!("hwp-studio bundle not built; skipping the bundle-serving assertions");
        return;
    }
    let (port, stop, handle) = start_server();
    let (head, body) = split_response(&get(port, "/hwp-studio/index.html?x=1"));
    assert!(head.starts_with("HTTP/1.1 200 OK\r\n"), "{head}");
    assert!(head.contains("Content-Type: text/html; charset=utf-8\r\n"));
    assert!(head.contains("Access-Control-Allow-Origin: *\r\n"));
    assert!(head.contains(&format!(
        "Content-Security-Policy: {}\r\n",
        editor_csp(port)
    )));
    assert_eq!(body, std::fs::read(bundle.join("index.html")).unwrap());

    let wasm = std::fs::read_dir(bundle.join("assets"))
        .unwrap()
        .filter_map(Result::ok)
        .find(|e| e.file_name().to_string_lossy().ends_with(".wasm"))
        .expect("bundle has a wasm module");
    let (head, body) = split_response(&get(
        port,
        &format!("/hwp-studio/assets/{}", wasm.file_name().to_string_lossy()),
    ));
    assert!(
        head.contains("Content-Type: application/wasm\r\n"),
        "{head}"
    );
    assert_eq!(&body[..4], b"\0asm");
    assert_eq!(
        body,
        std::fs::read(wasm.path()).unwrap(),
        "binary body must be byte exact"
    );

    let font = std::fs::read_dir(bundle.join("fonts"))
        .unwrap()
        .filter_map(Result::ok)
        .find(|e| e.file_name().to_string_lossy().ends_with(".woff2"))
        .expect("bundle has a font");
    let (head, _) = split_response(&get(
        port,
        &format!("/hwp-studio/fonts/{}", font.file_name().to_string_lossy()),
    ));
    assert!(head.contains("Content-Type: font/woff2\r\n"), "{head}");

    let (head, _) = split_response(&get(port, "/"));
    assert!(
        head.contains(
            "Content-Security-Policy: default-src 'self'; base-uri 'none'; object-src 'none'\r\n"
        ),
        "main page keeps its CSP: {head}"
    );
    stop.store(true, Ordering::Release);
    handle.join().unwrap();
}

#[test]
fn test_main_page_loads_the_hwp_host_script() {
    let (port, stop, handle) = start_server();
    let (head, body) = split_response(&get(port, "/hwp-host.js"));
    assert!(head.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(String::from_utf8_lossy(&body).contains("__EASYOFFICE_CREATE_HWP_HOST__"));
    stop.store(true, Ordering::Release);
    handle.join().unwrap();
}

/// The editor document's policy as the page at `http://127.0.0.1:{port}` must receive it.
fn editor_csp(port: u16) -> String {
    easyoffice_app::hwp_bundle::studio_csp_for_origin(&format!("http://127.0.0.1:{port}")).unwrap()
}

fn get_with_host(port: u16, path: &str, host: &str) -> String {
    let raw = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    send_raw_http_request(port, raw.as_bytes(), Duration::from_secs(5))
}

#[test]
fn test_editor_csp_names_the_page_origin_not_self() {
    // WebKit does not match 'self' in the opaque-origin editor frame; the policy of every
    // /hwp-studio response (found or not) spells out the origin the page was opened from.
    let (port, stop, handle) = start_server();
    let own = get_with_host(port, "/hwp-studio/nope.js", &format!("127.0.0.1:{port}"));
    assert!(
        own.contains(&format!(
            "Content-Security-Policy: {}\r\n",
            editor_csp(port)
        )),
        "{own}"
    );
    assert!(!own.contains("'self'"), "{own}");
    let by_name = get_with_host(port, "/hwp-studio/nope.js", &format!("localhost:{port}"));
    assert!(
        by_name.contains(&format!(
            "default-src http://localhost:{port}; script-src http://localhost:{port} 'wasm-unsafe-eval'"
        )),
        "{by_name}"
    );
    let ipv6 = get_with_host(port, "/hwp-studio/nope.js", &format!("[::1]:{port}"));
    assert!(
        ipv6.contains(&format!("default-src http://[::1]:{port};")),
        "{ipv6}"
    );
    // Without a port the origin is not the server's: fall back to the one that is.
    let bare = get_with_host(port, "/hwp-studio/nope.js", "localhost");
    assert!(
        bare.contains(&format!(
            "Content-Security-Policy: {}\r\n",
            editor_csp(port)
        )),
        "{bare}"
    );
    // The main page keeps the plain policy.
    let main = get_with_host(port, "/", &format!("127.0.0.1:{port}"));
    assert!(
        main.contains(
            "Content-Security-Policy: default-src 'self'; base-uri 'none'; object-src 'none'\r\n"
        ),
        "{main}"
    );
    stop.store(true, Ordering::Release);
    handle.join().unwrap();
}

fn post_ipc(port: u16, extra_headers: &str, body: &str) -> String {
    let raw = format!(
        "POST /api/ipc HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{extra_headers}Connection: close\r\n\r\n{body}"
    );
    send_raw_http_request(port, raw.as_bytes(), Duration::from_secs(5))
}

const BOOTSTRAP: &str = r#"{"command":"get_workspace_bootstrap","payload":""}"#;

fn assert_status(resp: &str, status: &str) {
    assert!(
        resp.starts_with(&format!("HTTP/1.1 {status}\r\n")),
        "wanted {status}: {}",
        &resp[..resp.len().min(300)]
    );
    assert!(
        resp.contains(
            "Content-Security-Policy: default-src 'self'; base-uri 'none'; object-src 'none'\r\n"
        ),
        "{resp}"
    );
}

#[test]
fn test_ipc_needs_the_page_marker_header_with_or_without_an_origin() {
    let (port, stop, handle) = start_server();
    let len = format!("Content-Length: {}\r\n", BOOTSTRAP.len());
    let origin = format!("Origin: http://127.0.0.1:{port}\r\n");
    // Plain HTTP clients send no Origin; a cross-site form post sends a foreign one.
    assert_status(&post_ipc(port, &len, BOOTSTRAP), "403 Forbidden");
    assert_status(
        &post_ipc(port, &format!("{origin}{len}"), BOOTSTRAP),
        "403 Forbidden",
    );
    assert_status(
        &post_ipc(
            port,
            &format!("Origin: http://evil.example\r\nX-EasyOffice-Ipc: 1\r\n{len}"),
            BOOTSTRAP,
        ),
        "403 Forbidden",
    );
    // A marker with another value is no marker.
    for value in ["0", "", "true", "11"] {
        let headers = format!("X-EasyOffice-Ipc: {value}\r\n{len}");
        assert_status(&post_ipc(port, &headers, BOOTSTRAP), "403 Forbidden");
    }
    for headers in [
        format!("X-EasyOffice-Ipc: 1\r\n{len}"),
        format!("{origin}x-easyoffice-ipc: 1\r\n{len}"),
        format!("{origin}X-EASYOFFICE-IPC:   1  \r\n{len}"),
    ] {
        assert_status(&post_ipc(port, &headers, BOOTSTRAP), "200 OK");
    }
    // The launch parameters after `?` do not turn the endpoint into another route.
    let raw = format!(
        "POST /api/ipc?x=1 HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{len}Connection: close\r\n\r\n{BOOTSTRAP}"
    );
    assert_status(
        &send_raw_http_request(port, raw.as_bytes(), Duration::from_secs(5)),
        "403 Forbidden",
    );
    stop.store(true, Ordering::Release);
    handle.join().unwrap();
}

#[test]
fn test_content_length_must_be_one_plain_number() {
    let (port, stop, handle) = start_server();
    let n = BOOTSTRAP.len();
    let marker = "X-EasyOffice-Ipc: 1\r\n";
    let send = |lengths: &str| post_ipc(port, &format!("{marker}{lengths}"), BOOTSTRAP);
    assert_status(&send(&format!("Content-Length: {n}\r\n")), "200 OK");
    // A repeated header is refused whether the values differ (the smaller one would cut the body
    // and leave the rest to be read as the next message) or agree.
    assert_status(
        &send(&format!("Content-Length: 2\r\nContent-Length: {n}\r\n")),
        "400 Bad Request",
    );
    assert_status(
        &send(&format!("Content-Length: {n}\r\nContent-Length: 2\r\n")),
        "400 Bad Request",
    );
    assert_status(
        &send(&format!("Content-Length: {n}\r\ncontent-length: {n}\r\n")),
        "400 Bad Request",
    );
    // Anything but digits is a framing error, not "too large".
    for bad in ["+5", "-1", "5, 5", "0x10", "1e3", "", " ", "5 6", "٥"] {
        assert_status(
            &send(&format!("Content-Length: {bad}\r\n")),
            "400 Bad Request",
        );
    }
    // Only a well-formed length above the limit is a 413 (a digit string past `usize` as well).
    assert_status(
        &send("Content-Length: 99999999999\r\n"),
        "413 Payload Too Large",
    );
    assert_status(
        &send(&format!("Content-Length: {}\r\n", "9".repeat(40))),
        "413 Payload Too Large",
    );
    stop.store(true, Ordering::Release);
    handle.join().unwrap();
}

#[test]
fn test_one_host_and_one_origin_and_no_transfer_encoding() {
    let (port, stop, handle) = start_server();
    let local = format!("127.0.0.1:{port}");
    let get = |headers: &str| {
        let raw = format!("GET / HTTP/1.1\r\n{headers}Connection: close\r\n\r\n");
        send_raw_http_request(port, raw.as_bytes(), Duration::from_secs(5))
    };
    // Two `Host` lines are refused whichever one is the loopback name.
    assert_status(&get(&format!("Host: {local}\r\n")), "200 OK");
    assert_status(
        &get(&format!("Host: evil.example\r\nHost: {local}\r\n")),
        "400 Bad Request",
    );
    assert_status(
        &get(&format!("Host: {local}\r\nhost: evil.example\r\n")),
        "400 Bad Request",
    );
    assert_status(
        &get(&format!("Host: {local}\r\nHost: {local}\r\n")),
        "400 Bad Request",
    );
    let len = format!(
        "Content-Length: {}\r\nX-EasyOffice-Ipc: 1\r\n",
        BOOTSTRAP.len()
    );
    let origin = format!("Origin: http://{local}\r\n");
    assert_status(
        &post_ipc(port, &format!("{origin}{len}"), BOOTSTRAP),
        "200 OK",
    );
    assert_status(
        &post_ipc(
            port,
            &format!("Origin: http://evil.example\r\n{origin}{len}"),
            BOOTSTRAP,
        ),
        "400 Bad Request",
    );
    assert_status(
        &post_ipc(port, &format!("{origin}{origin}{len}"), BOOTSTRAP),
        "400 Bad Request",
    );
    // A body framed another way than by `Content-Length` is not read at all.
    for coding in ["chunked", "gzip, chunked", "identity"] {
        let headers = format!("{origin}{len}Transfer-Encoding: {coding}\r\n");
        assert_status(&post_ipc(port, &headers, BOOTSTRAP), "400 Bad Request");
    }
    stop.store(true, Ordering::Release);
    handle.join().unwrap();
}

#[test]
fn test_a_request_arriving_in_small_pieces_is_read_whole_at_every_split() {
    let (port, stop, handle) = start_server();
    let raw = format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    let end = raw.len() - 4;
    // The header terminator split at each of its three inner positions, and one byte at a time.
    for cut in [end + 1, end + 2, end + 3] {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream.write_all(&raw.as_bytes()[..cut]).unwrap();
        stream.flush().unwrap();
        thread::sleep(Duration::from_millis(60));
        stream.write_all(&raw.as_bytes()[cut..]).unwrap();
        let mut out = String::new();
        let _ = stream.read_to_string(&mut out);
        assert_status(&out, "200 OK");
    }
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    for byte in raw.as_bytes() {
        stream.write_all(&[*byte]).unwrap();
        stream.flush().unwrap();
    }
    let mut out = String::new();
    let _ = stream.read_to_string(&mut out);
    assert_status(&out, "200 OK");
    stop.store(true, Ordering::Release);
    handle.join().unwrap();
}
