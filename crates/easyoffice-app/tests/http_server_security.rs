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
        "POST /api/ipc HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: http://127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
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
        resp.contains("Content-Security-Policy: default-src 'self'\r\n"),
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
            "POST /api/ipc HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: http://127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
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
            resp.contains("Content-Security-Policy: default-src 'self'\r\n"),
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
        ok_resp.contains("Content-Security-Policy: default-src 'self'\r\n"),
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
    let overflow_start = Instant::now();
    let get_req = format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    let overflow_resp = send_raw_http_request(port, get_req.as_bytes(), Duration::from_millis(900));
    assert!(
        overflow_start.elapsed() < Duration::from_millis(1000),
        "accept loop must never block on slowloris overflow"
    );
    assert!(
        overflow_resp.starts_with("HTTP/1.1 503 Service Unavailable\r\n")
            || overflow_resp.starts_with("HTTP/1.1 200 OK\r\n"),
        "expected immediate 503 or 200 during slowloris wave, got: {overflow_resp}"
    );
    assert!(
        overflow_resp.contains("Content-Security-Policy: default-src 'self'\r\n"),
        "missing CSP header during slowloris wave"
    );

    let wait_budget = (MAX_CONNECTION_DURATION + Duration::from_millis(200))
        .saturating_sub(wave_started.elapsed());
    if !wait_budget.is_zero() {
        thread::sleep(wait_budget);
    }

    let normal_start = Instant::now();
    let ok_resp = send_raw_http_request(port, get_req.as_bytes(), Duration::from_millis(950));
    let normal_elapsed = normal_start.elapsed();
    assert!(
        ok_resp.starts_with("HTTP/1.1 200 OK\r\n"),
        "expected 200 OK after slowloris deadline eviction, got: {ok_resp}"
    );
    assert!(
        ok_resp.contains("Content-Security-Policy: default-src 'self'\r\n"),
        "missing CSP header on GET /: {ok_resp}"
    );
    assert!(
        normal_elapsed < Duration::from_millis(1000),
        "normal GET / took {normal_elapsed:?} (>= 1000ms)"
    );

    slow_stop.store(true, Ordering::Release);
    for h in slow_handles {
        let _ = h.join();
    }
    stop.store(true, Ordering::Release);
    server_handle.join().expect("join server thread");
}
