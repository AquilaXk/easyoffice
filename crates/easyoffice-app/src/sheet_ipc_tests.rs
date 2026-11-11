use crate::http_server::{MAX_HTTP_BODY_BYTES, MAX_IPC_HTTP_BODY_BYTES};
use crate::{serve_http_on_listener, EasyOfficeIpcRouter};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const NUMBER_ROWS: u32 = 20_000;
const FORMULA_ROWS: u32 = 4_000;

fn build_large_recalc_payload() -> String {
    let mut cells = Vec::with_capacity((NUMBER_ROWS + FORMULA_ROWS) as usize);
    for r in 1..=NUMBER_ROWS {
        cells.push(format!(
            "{{\"sheet\":\"Sheet1\",\"cell\":\"A{r}\",\"value\":\"{r}\"}}"
        ));
    }
    let mut targets = Vec::with_capacity(FORMULA_ROWS as usize);
    for r in 1..=FORMULA_ROWS {
        cells.push(format!(
            "{{\"sheet\":\"Sheet1\",\"cell\":\"B{r}\",\"value\":\"=SUM($A$1:A{r})\"}}"
        ));
        targets.push(format!("{{\"sheet\":\"Sheet1\",\"cell\":\"B{r}\"}}"));
    }
    format!(
        "{{\"sheet\":\"Sheet1\",\"sheets\":[\"Sheet1\"],\"targets\":[{}],\"cells\":[{}]}}",
        targets.join(","),
        cells.join(",")
    )
}

fn send_raw_http_request(port: u16, raw_request: &[u8], timeout: Duration) -> String {
    let mut stream =
        TcpStream::connect(("127.0.0.1", port)).expect("connect to local test listener");
    stream.set_read_timeout(Some(timeout)).unwrap();
    stream.set_write_timeout(Some(timeout)).unwrap();
    let _ = stream.write_all(raw_request);
    let _ = stream.flush();
    let mut out = Vec::with_capacity(1 << 16);
    let _ = stream.read_to_end(&mut out);
    String::from_utf8_lossy(&out).into_owned()
}

fn ipc_post(port: u16, body: &[u8]) -> Vec<u8> {
    let mut raw = format!(
        "POST /api/ipc HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: http://127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    raw.extend_from_slice(body);
    raw
}

#[test]
fn test_large_workbook_recalc_dispatch_over_one_mebibyte() {
    let payload = build_large_recalc_payload();
    assert!(
        payload.len() > 1_048_576,
        "payload must exceed the legacy 1 MiB limit ({} bytes)",
        payload.len()
    );
    let started = Instant::now();
    let res = EasyOfficeIpcRouter::dispatch("evaluate_sheet", &payload).unwrap();
    let elapsed = started.elapsed();
    eprintln!(
        "evaluate_sheet 20,000 numbers + 4,000 formulas ({} bytes) took {elapsed:?}",
        payload.len()
    );
    assert!(res.contains("\"targetValues\":[\"1\",\"3\",\"6\","));
    assert!(res.contains("\"Sheet1!B4000\":\"8002000\""));
    assert!(res.contains("\"circular\":false"));
}

#[test]
fn test_large_workbook_recalc_over_http_and_bounded_413() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let server_handle = thread::spawn(move || serve_http_on_listener(listener, server_stop));

    let payload = build_large_recalc_payload();
    let body = format!(
        "{{\"command\":\"evaluate_sheet\",\"payload\":\"{}\"}}",
        EasyOfficeIpcRouter::escape_json_str(&payload)
    );
    assert!(body.len() > MAX_HTTP_BODY_BYTES);
    assert!(body.len() < MAX_IPC_HTTP_BODY_BYTES);
    let resp = send_raw_http_request(
        port,
        &ipc_post(port, body.as_bytes()),
        Duration::from_secs(60),
    );
    assert!(
        resp.starts_with("HTTP/1.1 200 OK\r\n"),
        "large recalc must succeed over HTTP: {}",
        &resp[..resp.len().min(300)]
    );
    assert!(resp.contains("\"Sheet1!B4000\":\"8002000\""));

    let oversized_ipc = format!(
        "POST /api/ipc HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: http://127.0.0.1:{port}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        MAX_IPC_HTTP_BODY_BYTES + 1
    );
    let resp_413 = send_raw_http_request(port, oversized_ipc.as_bytes(), Duration::from_secs(5));
    assert!(
        resp_413.starts_with("HTTP/1.1 413 Payload Too Large\r\n"),
        "got: {resp_413}"
    );
    assert!(resp_413.contains("Content-Security-Policy: default-src 'self'\r\n"));

    let oversized_static = format!(
        "POST /index.html HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        MAX_HTTP_BODY_BYTES + 1
    );
    let resp_static =
        send_raw_http_request(port, oversized_static.as_bytes(), Duration::from_secs(5));
    assert!(
        resp_static.starts_with("HTTP/1.1 413 Payload Too Large\r\n"),
        "non-IPC routes keep the 1 MiB body ceiling: {resp_static}"
    );

    stop.store(true, Ordering::Release);
    server_handle.join().expect("join server thread");
}

#[test]
fn test_missing_sheet_reference_is_ref_without_demo_seed() {
    for payload in [
        r#"{"sheet":"Data","expr":"=Sheet2!A1","cells":[{"sheet":"Data","cell":"A1","value":5}]}"#,
        r#"{"sheet":"Data","sheets":["Data"],"expr":"=Sheet2!A1","cells":[]}"#,
        r#"{"sheet":"Data","expr":"=Sheet1!A1+Sheet1!B1"}"#,
    ] {
        let res = EasyOfficeIpcRouter::dispatch("evaluate_workbook_formula", payload).unwrap();
        assert!(
            res.contains("\"value\":\"#REF!\""),
            "payload={payload} res={res}"
        );
    }
    let sheet_res = EasyOfficeIpcRouter::dispatch(
        "evaluate_sheet",
        r#"{"sheet":"Data","expr":"=Sheet2!A1+50","cells":[{"sheet":"Data","cell":"A1","value":500}]}"#,
    )
    .unwrap();
    assert!(sheet_res.contains("\"value\":\"#REF!\""), "{sheet_res}");
}

#[test]
fn test_circular_reference_reports_standard_error_and_flag() {
    let res = EasyOfficeIpcRouter::dispatch(
        "evaluate_sheet",
        r#"{"sheet":"Sheet1","targets":[{"cell":"A1"},{"cell":"C1"}],"cells":[{"cell":"A1","value":"=B1+1"},{"cell":"B1","value":"=A1+1"},{"cell":"C1","value":"=5"}]}"#,
    )
    .unwrap();
    assert!(res.contains("\"targetValues\":[\"#NUM!\",\"5\"]"), "{res}");
    assert!(res.contains("\"circular\":true"), "{res}");
    assert!(!res.contains("#CYCLE!"));

    let wb_res = EasyOfficeIpcRouter::dispatch(
        "evaluate_workbook_formula",
        r#"{"sheet":"Sheet1","expr":"A1","cells":[{"cell":"A1","value":"=A1"}]}"#,
    )
    .unwrap();
    assert!(wb_res.contains("\"value\":\"#NUM!\""), "{wb_res}");
    assert!(wb_res.contains("\"circular\":true"), "{wb_res}");
}

/// The desktop shell calls `dispatch` directly, without the HTTP server's body ceiling: the router
/// holds the same ceiling, so no command Base64-decodes or parses a payload larger than the server
/// would have accepted.
#[test]
fn a_payload_over_the_ipc_ceiling_is_refused_by_every_command() {
    use easyoffice_formats::EasyOfficeError;
    let over = " ".repeat(MAX_IPC_HTTP_BODY_BYTES + 1);
    for command in [
        "detect_format",
        "audit_security",
        "convert_document_format",
        "scrub_pii",
        "evaluate_sheet",
        "get_workspace_bootstrap",
    ] {
        let err = EasyOfficeIpcRouter::dispatch(command, &over).unwrap_err();
        assert_eq!(
            err,
            EasyOfficeError::PayloadTooLarge {
                actual: over.len(),
                max: MAX_IPC_HTTP_BODY_BYTES
            },
            "{command}"
        );
    }
    let at = " ".repeat(MAX_IPC_HTTP_BODY_BYTES);
    let reached = EasyOfficeIpcRouter::dispatch("scrub_pii", &at);
    assert!(
        !matches!(reached, Err(EasyOfficeError::PayloadTooLarge { .. })),
        "a payload at the ceiling still reaches the command"
    );
}
