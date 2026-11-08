//! The steps of `EasyOffice --smoke` (PD-088 item 12, PD-103 loopback hook).
//!
//! Each step has a fixed name; the smoke run passes only when every step of [`required_steps`] has
//! reported success (`smoke.rs` keeps the book and prints one `EASYOFFICE_SMOKE_STEP_OK <name>` or
//! `EASYOFFICE_SMOKE_STEP_FAIL <name>` line per step). Two kinds exist:
//!
//! * Page steps run in the first window (`hwp`, `pdf`, `print`, `frame_ipc`) or the second one (`window_loaded`)
//!   and are reported by the page over the ordinary `scrub_pii` call; a page can report only the
//!   names of [`PAGE_STEPS`].
//! * Shell steps run here and are reported directly: `window_state` (the remembered-geometry file),
//!   `preview` (the thumbnail of a sample Hangul file), `update` (the update request against a
//!   one-shot server on `127.0.0.1`), and `menu` (every custom menu id is installed).
//!
//! The sample file is the rhwp `shift-return.hwp` fixture, found through the compile-time manifest
//! directory. That is valid here because the smoke run exists only for CI, which builds and starts the
//! binary in the same job and checkout (`ci.yml`, after `scripts/checkout-rhwp.mjs`); an installed
//! copy never runs `--smoke`.

use crate::pdf_export;
use crate::preview::{self, PreviewState};
use crate::save_file::SaveOutcome;
use crate::update_check::{self, ACCEPT_HEADER};
use crate::window_state::{MonitorRect, WindowGeometry, RECORD_LEN};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const IPC: &str = "ipc";
pub const HWP: &str = "hwp";
pub const PDF: &str = "pdf";
pub const PRINT: &str = "print";
/// The sandboxed editor frame must not be able to run an app command; see `frame-probe.js`.
pub const FRAME_IPC: &str = "frame_ipc";
pub const WINDOW: &str = "window";
pub const WINDOW_STATE: &str = "window_state";
pub const PREVIEW: &str = "preview";
pub const UPDATE: &str = "update";
pub const MENU: &str = "menu";
/// Reported by the second window once it loaded; it becomes [`WINDOW`] when that window is destroyed.
pub const WINDOW_LOADED: &str = "window_loaded";

/// The names a page may report.
pub const PAGE_STEPS: [&str; 5] = [HWP, PDF, PRINT, FRAME_IPC, WINDOW_LOADED];

const REQUIRED: &[&str] = &[
    IPC,
    HWP,
    PDF,
    PRINT,
    FRAME_IPC,
    WINDOW,
    WINDOW_STATE,
    PREVIEW,
    UPDATE,
    MENU,
];

/// Every step that must succeed.
pub fn required_steps() -> &'static [&'static str] {
    REQUIRED
}

/// A shell step: its result is the detail text of the marker line, or the reason it failed.
pub type StepFn = fn() -> Result<String, String>;

/// The shell steps that need nothing from the app, run on a worker thread (`update` also needs the
/// app version and is called by `smoke::run_rust_steps`).
pub const WORKER_STEPS: [(&str, StepFn); 2] =
    [(WINDOW_STATE, window_state_step), (PREVIEW, preview_step)];

/// Name of the PDF the smoke export reports; nothing is written (see [`verify_pdf`]).
const SMOKE_PDF_NAME: &str = "smoke.pdf";

fn scratch_dir(tag: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("easyoffice-smoke-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|err| format!("임시 폴더를 만들지 못했습니다: {err}"))?;
    Ok(dir)
}

// ---- pdf ---------------------------------------------------------------------------------------

/// The smoke branch of `export_searchable_pdf`: checks the in-memory PDF and answers as if it had
/// been saved, without a dialog and without writing a file.
pub fn verify_pdf(pages: &[String], pdf: &[u8]) -> Result<SaveOutcome, String> {
    let has_text = pages.iter().any(|svg| svg.contains("<text"));
    let check = pdf_export::verify_exported_pdf(pdf, pages.len(), has_text)?;
    if let Some(warning) = check.warning {
        eprintln!("smoke: warning: {warning}");
    }
    Ok(SaveOutcome::saved(Path::new(SMOKE_PDF_NAME), pdf.len()))
}

// ---- window_state ------------------------------------------------------------------------------

const SAMPLE_WINDOW: (f64, f64) = (1280.0, 860.0);
const SAMPLE_ORIGIN: (f64, f64) = (40.0, 40.0);
const SAMPLE_MONITOR: MonitorRect = MonitorRect {
    x: 0,
    y: 0,
    width: 1920,
    height: 1080,
};

/// The remembered-geometry file survives a write and read in this machine's temporary folder, and a
/// stored position is reused only while it lies on a monitor.
pub fn window_state_step() -> Result<String, String> {
    let dir = scratch_dir("window-state")?;
    let result = window_state_round_trip(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn window_state_round_trip(dir: &Path) -> Result<String, String> {
    for maximized in [false, true] {
        let geometry = WindowGeometry::from_logical(SAMPLE_WINDOW, SAMPLE_ORIGIN, maximized)
            .ok_or("창 크기를 기록할 수 없습니다.")?;
        geometry
            .save(dir)
            .map_err(|err| format!("창 상태를 쓰지 못했습니다: {err}"))?;
        if WindowGeometry::load(dir) != Some(geometry) {
            return Err(format!(
                "창 상태를 다시 읽으면 다릅니다 (maximized={maximized})."
            ));
        }
        if geometry.visible_origin(&[SAMPLE_MONITOR]) != Some(SAMPLE_ORIGIN) {
            return Err("모니터 안의 위치가 재사용되지 않았습니다.".to_string());
        }
        if geometry.visible_origin(&[]).is_some() {
            return Err("모니터가 없는데 위치가 재사용되었습니다.".to_string());
        }
    }
    Ok(format!("record={RECORD_LEN} bytes"))
}

// ---- preview -----------------------------------------------------------------------------------

const PREVIEW_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../third_party/rhwp/rhwp-studio/public/samples/shift-return.hwp"
);
const PREVIEW_ID: &str = "0123456789abcdef";

/// The home screen's preview reader finds the picture embedded in the sample Hangul file.
pub fn preview_step() -> Result<String, String> {
    preview_step_for(Path::new(PREVIEW_FIXTURE))
}

fn preview_step_for(path: &Path) -> Result<String, String> {
    if !path.is_file() {
        return Err(format!(
            "미리보기 샘플 파일이 없습니다 (node scripts/checkout-rhwp.mjs로 받습니다): {}",
            path.display()
        ));
    }
    let found = preview::load_preview(&PreviewState::new(), PREVIEW_ID, path)
        .ok_or("샘플 파일에서 미리보기 그림을 찾지 못했습니다.")?;
    Ok(format!("{} {} bytes", found.mime, found.bytes.len()))
}

// ---- update ------------------------------------------------------------------------------------

const FAKE_PATH: &str = "/repos/AquilaXk/easyoffice/releases/latest";
/// Far above any real version, so the fake release always counts as newer.
const FAKE_VERSION: &str = "999.0.0";
const SERVER_DEADLINE: Duration = Duration::from_secs(10);
const SERVER_IO_TIMEOUT: Duration = Duration::from_secs(5);
const ACCEPT_POLL: Duration = Duration::from_millis(10);
/// Largest request head the fake server reads.
const MAX_REQUEST_HEAD_BYTES: usize = 8 * 1024;
const HEAD_END: &[u8] = b"\r\n\r\n";

fn fake_release_body() -> Vec<u8> {
    format!(
        r#"{{"tag_name":"v{FAKE_VERSION}","html_url":"{}tag/v{FAKE_VERSION}","draft":false,"prerelease":false}}"#,
        update_check::RELEASE_URL_PREFIX
    )
    .into_bytes()
}

/// Accepts one connection, answers it with `status` (for example `200 OK`) and `body`, and returns
/// the request head it received.
fn serve_once(listener: &TcpListener, status: &str, body: &[u8]) -> Result<String, String> {
    let mut stream = accept_before(listener, Instant::now() + SERVER_DEADLINE)?;
    let head = read_request_head(&mut stream)?;
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .and_then(|()| stream.write_all(body))
        .and_then(|()| stream.flush())
        .map_err(|err| format!("가짜 서버가 응답하지 못했습니다: {err}"))?;
    Ok(head)
}

fn accept_before(listener: &TcpListener, deadline: Instant) -> Result<TcpStream, String> {
    listener
        .set_nonblocking(true)
        .map_err(|err| format!("가짜 서버를 설정하지 못했습니다: {err}"))?;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                // Some platforms hand the accepted socket the listener's non-blocking mode.
                stream
                    .set_nonblocking(false)
                    .and_then(|()| stream.set_read_timeout(Some(SERVER_IO_TIMEOUT)))
                    .and_then(|()| stream.set_write_timeout(Some(SERVER_IO_TIMEOUT)))
                    .map_err(|err| format!("가짜 서버 연결을 설정하지 못했습니다: {err}"))?;
                return Ok(stream);
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err("가짜 서버에 요청이 오지 않았습니다.".to_string());
                }
                std::thread::sleep(ACCEPT_POLL);
            }
            Err(err) => return Err(format!("가짜 서버가 연결을 받지 못했습니다: {err}")),
        }
    }
}

fn head_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(HEAD_END.len())
        .position(|w| w == HEAD_END)
        .map(|at| at + HEAD_END.len())
}

/// Reads up to the blank line that ends the request head and returns the head only.
fn read_request_head(stream: &mut impl Read) -> Result<String, String> {
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 1024];
    let end = loop {
        if let Some(end) = head_end(&buf) {
            break end;
        }
        if buf.len() > MAX_REQUEST_HEAD_BYTES {
            return Err("요청 머리글이 너무 깁니다.".to_string());
        }
        let read = stream
            .read(&mut chunk)
            .map_err(|err| format!("요청을 읽지 못했습니다: {err}"))?;
        if read == 0 {
            return Err("요청이 끝나기 전에 연결이 닫혔습니다.".to_string());
        }
        buf.extend_from_slice(&chunk[..read]);
    };
    String::from_utf8(buf[..end].to_vec())
        .map_err(|_| "요청 머리글이 UTF-8이 아닙니다.".to_string())
}

fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

/// The update request has the request line, user agent and `Accept` header the real check sends.
fn check_request(head: &str, app_version: &str) -> Result<(), String> {
    let line = head.lines().next().unwrap_or_default();
    if line != format!("GET {FAKE_PATH} HTTP/1.1") {
        return Err(format!("요청 줄이 다릅니다: {line}"));
    }
    if header(head, "user-agent") != Some(update_check::user_agent(app_version).as_str()) {
        return Err("User-Agent 머리글이 다릅니다.".to_string());
    }
    if header(head, "accept") != Some(ACCEPT_HEADER) {
        return Err("Accept 머리글이 다릅니다.".to_string());
    }
    Ok(())
}

struct FakeServer {
    url: String,
    thread: JoinHandle<Result<String, String>>,
}

fn start_fake_server(status: &'static str, body: Vec<u8>) -> Result<FakeServer, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|err| format!("가짜 서버를 열지 못했습니다: {err}"))?;
    let port = listener
        .local_addr()
        .map_err(|err| format!("가짜 서버 주소를 알 수 없습니다: {err}"))?
        .port();
    Ok(FakeServer {
        url: format!("http://127.0.0.1:{port}{FAKE_PATH}"),
        thread: std::thread::spawn(move || serve_once(&listener, status, &body)),
    })
}

/// Addresses the loopback hook must refuse before opening any connection (the port is never used).
const REFUSED_ADDRESSES: [(&str, bool); 4] = [
    ("http://example.com/", false),
    ("http://127.0.0.1.invalid:9/x", false),
    ("http://127.0.0.1:9@evil.invalid/x", false),
    ("http://127.0.0.1:9/x", true),
];

/// One real update request against a one-shot server that answers `status` and `body`: the request
/// must be the one the real check sends. Returns what `fetch_release` returned.
fn ask_fake_server(
    app_version: &str,
    status: &'static str,
    body: Vec<u8>,
) -> Result<Option<Vec<u8>>, String> {
    let server = start_fake_server(status, body)?;
    let fetched = update_check::fetch_release(&server.url, false, app_version);
    let head = server
        .thread
        .join()
        .map_err(|_| "가짜 서버가 중단되었습니다.".to_string())?;
    match (fetched, head) {
        (Ok(answer), Ok(head)) => check_request(&head, app_version).map(|()| answer),
        (Err(fetch), Err(serve)) => Err(format!("{fetch} ({serve})")),
        (Err(fetch), Ok(_)) => Err(fetch),
        (Ok(_), Err(serve)) => Err(serve),
    }
}

/// The update check against 127.0.0.1: the hook refuses every other address; a release answer is
/// read like the real one and counts as news; a 404 answer means "no release" (`None`).
/// `app_version` is the running app's version, as in the real check.
pub fn update_check_step(app_version: &str) -> Result<String, String> {
    for (url, https_only) in REFUSED_ADDRESSES {
        if update_check::fetch_release(url, https_only, app_version).is_ok() {
            return Err(format!("허용되지 않는 주소가 거부되지 않았습니다: {url}"));
        }
    }
    let body = ask_fake_server(app_version, "200 OK", fake_release_body())?
        .ok_or("가짜 서버의 릴리스를 찾지 못했습니다 (404).")?;
    let current = update_check::current_version(app_version);
    let found = update_check::parse_release(&body, current)?
        .ok_or("가짜 릴리스가 새 버전으로 인식되지 않았습니다.")?;
    if found.version != FAKE_VERSION || !update_check::release_url_ok(&found.url) {
        return Err(format!("릴리스를 잘못 읽었습니다: {found:?}"));
    }
    if ask_fake_server(app_version, "404 Not Found", Vec::new())?.is_some() {
        return Err("404 응답이 릴리스 없음(None)으로 읽히지 않았습니다.".to_string());
    }
    Ok(format!("version={} not-found=none", found.version))
}

// ---- menu --------------------------------------------------------------------------------------

/// Checks that every custom item this OS's menu bar holds is among the installed menu ids and has a
/// page action script.
fn check_menu_ids<S: AsRef<str>>(installed: &[S]) -> Result<(), String> {
    let missing: Vec<&str> = crate::native_menu::installed_ids()
        .filter(|id| {
            !installed.iter().any(|have| have.as_ref() == *id)
                || crate::native_menu::dispatch_script(id).is_none()
        })
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "메뉴 항목이 설치되지 않았습니다: {}",
            missing.join(", ")
        ))
    }
}

/// Deepest submenu level read; the menu bar has three (menu, submenu, item).
const MAX_MENU_DEPTH: usize = 4;

fn collect_menu_ids<R: tauri::Runtime>(
    items: &[tauri::menu::MenuItemKind<R>],
    depth: usize,
    out: &mut Vec<String>,
) -> Result<(), String> {
    for item in items {
        out.push(item.id().as_ref().to_string());
        let Some(submenu) = item.as_submenu().filter(|_| depth < MAX_MENU_DEPTH) else {
            continue;
        };
        let children = submenu
            .items()
            .map_err(|err| format!("메뉴 항목을 읽지 못했습니다: {err}"))?;
        collect_menu_ids(&children, depth + 1, out)?;
    }
    Ok(())
}

/// The installed menu bar holds every custom item of the table.
pub fn menu_step(app: &tauri::AppHandle) -> Result<String, String> {
    let menu = app.menu().ok_or("앱 메뉴가 설치되지 않았습니다.")?;
    let top = menu
        .items()
        .map_err(|err| format!("메뉴 항목을 읽지 못했습니다: {err}"))?;
    let mut ids = Vec::new();
    collect_menu_ids(&top, 0, &mut ids)?;
    check_menu_ids(&ids)?;
    Ok(format!(
        "{} custom items",
        crate::native_menu::installed_ids().count()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_steps_are_unique_and_cover_every_name() {
        let required = required_steps();
        let unique: std::collections::HashSet<_> = required.iter().collect();
        assert_eq!(unique.len(), required.len());
        for name in [
            IPC,
            HWP,
            PDF,
            PRINT,
            FRAME_IPC,
            WINDOW,
            WINDOW_STATE,
            PREVIEW,
            UPDATE,
        ] {
            assert!(required.contains(&name), "{name}");
        }
        assert!(required.contains(&MENU));
        assert!(!required.contains(&WINDOW_LOADED));
    }

    #[test]
    fn pages_may_report_only_page_steps_and_shell_steps_are_not_among_them() {
        for name in PAGE_STEPS {
            assert!(name == WINDOW_LOADED || required_steps().contains(&name));
        }
        for shell in [IPC, WINDOW, WINDOW_STATE, PREVIEW, UPDATE, MENU] {
            assert!(!PAGE_STEPS.contains(&shell), "{shell}");
        }
        let workers: Vec<&str> = WORKER_STEPS.iter().map(|(name, _)| *name).collect();
        assert_eq!(workers, [WINDOW_STATE, PREVIEW]);
    }

    #[test]
    fn page_step_names_match_the_page_scripts() {
        let page = |file: &str| {
            std::fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../easyoffice-app/ui")
                    .join(file),
            )
            .unwrap()
        };
        let app = page("app.js");
        for name in [HWP, PDF, PRINT, FRAME_IPC] {
            assert!(app.contains(&format!("{name}: ")), "app.js reports {name}");
        }
        assert!(page("window-bridge.js").contains(&format!("\"{WINDOW_LOADED}\"")));
    }

    #[test]
    fn the_window_state_file_round_trips_on_this_machine() {
        let detail = window_state_step().expect("window state step");
        assert_eq!(detail, "record=18 bytes");
    }

    #[test]
    fn the_preview_step_reads_a_picture_from_a_hangul_file_and_names_a_missing_sample() {
        let missing = preview_step_for(Path::new("/nonexistent/sample.hwp")).unwrap_err();
        assert!(missing.contains("checkout-rhwp"), "{missing}");
        let dir = scratch_dir("preview-test").unwrap();
        let plain = dir.join("plain.hwp");
        std::fs::write(&plain, b"not a hangul file").unwrap();
        assert!(preview_step_for(&plain)
            .unwrap_err()
            .contains("찾지 못했습니다"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_checked_out_sample_has_a_preview_picture() {
        let fixture = Path::new(PREVIEW_FIXTURE);
        if !fixture.is_file() {
            return; // the sparse rhwp checkout is optional for local test runs; CI has it
        }
        let detail = preview_step().expect("preview step");
        assert!(detail.starts_with("image/"), "{detail}");
    }

    #[test]
    fn the_update_step_runs_the_real_request_against_the_fake_server() {
        let detail = update_check_step("0.1.0").expect("update step");
        assert_eq!(detail, format!("version={FAKE_VERSION} not-found=none"));
    }

    #[test]
    fn the_fake_release_is_newer_than_this_build_and_points_at_the_project() {
        let found = update_check::parse_release(
            &fake_release_body(),
            update_check::current_version("0.1.0"),
        )
        .unwrap()
        .expect("newer");
        assert_eq!(found.version, FAKE_VERSION);
        assert!(update_check::release_url_ok(&found.url));
        assert_eq!(
            update_check::parse_release(&fake_release_body(), [999, 0, 0]),
            Ok(None),
            "an equal version is not news"
        );
    }

    #[test]
    fn the_request_check_demands_the_real_request_line_agent_and_accept() {
        let good = format!(
            "GET {FAKE_PATH} HTTP/1.1\r\nHost: 127.0.0.1:1\r\nuser-agent: {}\r\nACCEPT: {ACCEPT_HEADER}\r\n\r\n",
            update_check::user_agent("0.1.0")
        );
        assert_eq!(check_request(&good, "0.1.0"), Ok(()));
        for bad in [
            good.replace("GET ", "POST "),
            good.replace(FAKE_PATH, "/other"),
            good.replace(&update_check::user_agent("0.1.0"), "curl/8"),
            good.replace(ACCEPT_HEADER, "*/*"),
            good.replace("user-agent", "x-agent"),
        ] {
            assert!(check_request(&bad, "0.1.0").is_err(), "{bad}");
        }
        assert!(
            check_request(&good, "0.2.0").is_err(),
            "the agent carries the app version"
        );
    }

    #[test]
    fn the_request_head_is_read_to_its_end_and_bounded() {
        let mut complete: &[u8] = b"GET / HTTP/1.1\r\nA: b\r\n\r\nbody";
        assert_eq!(
            read_request_head(&mut complete).unwrap(),
            "GET / HTTP/1.1\r\nA: b\r\n\r\n"
        );
        let mut cut: &[u8] = b"GET / HTTP/1.1\r\n";
        assert!(read_request_head(&mut cut)
            .unwrap_err()
            .contains("닫혔습니다"));
        let endless = vec![b'a'; MAX_REQUEST_HEAD_BYTES * 2];
        assert!(read_request_head(&mut endless.as_slice())
            .unwrap_err()
            .contains("너무 깁니다"));
    }

    #[test]
    fn a_server_nobody_calls_gives_up_at_its_deadline() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let err = accept_before(&listener, Instant::now() + Duration::from_millis(50)).unwrap_err();
        assert!(err.contains("요청이 오지 않았습니다"), "{err}");
    }

    #[test]
    fn the_pdf_branch_answers_like_a_save_and_names_what_is_wrong() {
        let pages = vec!["<svg/>".to_string()];
        assert!(verify_pdf(&pages, b"not a pdf").is_err());
        let outcome = verify_pdf(&pages, &minimal_pdf(1)).expect("valid");
        assert_eq!(
            serde_json::to_string(&outcome).unwrap(),
            format!(
                r#"{{"status":"saved","name":"smoke.pdf","bytes":{}}}"#,
                minimal_pdf(1).len()
            )
        );
    }

    /// A hand-made PDF skeleton with the keys the check reads and no font.
    fn minimal_pdf(pages: usize) -> Vec<u8> {
        let mut pdf = b"%PDF-1.7\n<< /Type /Pages /Count ".to_vec();
        pdf.extend_from_slice(pages.to_string().as_bytes());
        for _ in 0..pages {
            pdf.extend_from_slice(b" /MediaBox");
        }
        pdf
    }

    #[test]
    fn the_menu_check_names_every_missing_item() {
        let all: Vec<&str> = crate::native_menu::installed_ids().collect();
        assert_eq!(check_menu_ids(&all), Ok(()));
        assert_eq!(
            check_menu_ids(&[all.as_slice(), &["extra:item"]].concat()),
            Ok(())
        );
        let err = check_menu_ids(&all[2..]).unwrap_err();
        assert!(
            err.contains("app:about") && err.contains("file:new"),
            "{err}"
        );
        assert!(check_menu_ids::<&str>(&[]).is_err());
    }
}
