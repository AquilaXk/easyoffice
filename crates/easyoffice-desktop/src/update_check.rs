//! Update notice (PD-103).
//!
//! After the app has started (never in the smoke run) it asks the public GitHub releases API for the
//! latest release at most once a day. When that release is a newer, final `x.y.z` version, the page
//! shows a small card (hop's "update notice") with a button that opens the release page in the
//! default browser. Nothing is downloaded or installed: there is no signing key, so no automatic
//! update.
//!
//! Privacy and safety:
//! * The request carries no information about the user or the document; the only identifying text is
//!   the user agent `EasyOffice/<version>`.
//! * The response is read with a hard size limit and only `tag_name`, `html_url` and the draft and
//!   pre-release flags are used. The page never supplies a URL: "open the release page" opens the
//!   URL Rust stored, and that URL must start with the project's releases address.
//! * The time of the last check and the on/off switch live in one small record in the app data
//!   folder, not in the page.
//!
//! The notice card follows hop `apps/studio-host/src/ui/update-notice.ts` (MIT, see
//! THIRD_PARTY_NOTICES.md); this module replaces hop's signed updater with a notice only.

use crate::save_file::write_document_bytes;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager, State};

pub const RELEASES_API: &str = "https://api.github.com/repos/AquilaXk/easyoffice/releases/latest";
pub const RELEASE_URL_PREFIX: &str = "https://github.com/AquilaXk/easyoffice/releases/";
/// Largest accepted API response (bytes).
pub const MAX_BODY_BYTES: u64 = 256 * 1024;
/// Largest accepted record file (bytes).
const MAX_RECORD_BYTES: u64 = 64 * 1024;
const CHECK_INTERVAL_SECS: u64 = 86_400;
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_URL_LEN: usize = 300;
/// A version part has at most nine digits, so it always fits a `u32` without overflow.
const MAX_PART_DIGITS: usize = 9;
const RECORD_FILE: &str = "update-check.json";

/// `[major, minor, patch]`.
pub type Version = [u32; 3];

/// One numeric part: 1 to nine digits, no leading zero (except `0` itself).
fn parse_part(digits: &[u8]) -> Option<u32> {
    if digits.is_empty()
        || digits.len() > MAX_PART_DIGITS
        || (digits.len() > 1 && digits[0] == b'0')
    {
        return None;
    }
    digits.iter().try_fold(0u32, |sum, &byte| {
        if !byte.is_ascii_digit() {
            return None;
        }
        sum.checked_mul(10)?.checked_add(u32::from(byte - b'0'))
    })
}

/// Parses `1.2.3` or `v1.2.3` (exactly three numeric parts). Anything else, including pre-release
/// and build suffixes, is not a final release and yields `None`. Works on bytes, no allocation.
pub fn parse_version(text: &[u8]) -> Option<Version> {
    let body = match text.split_first() {
        Some((b'v' | b'V', rest)) => rest,
        _ => text,
    };
    let mut parts = [0u32; 3];
    let mut count = 0usize;
    for part in body.split(|&byte| byte == b'.') {
        *parts.get_mut(count)? = parse_part(part)?;
        count += 1;
    }
    (count == parts.len()).then_some(parts)
}

/// True when `candidate` is a newer version than `current`.
pub fn is_newer(candidate: Version, current: Version) -> bool {
    candidate > current
}

/// The running version (`CARGO_PKG_VERSION`).
pub fn current_version() -> Version {
    parse_version(env!("CARGO_PKG_VERSION").as_bytes()).unwrap_or([0, 0, 0])
}

/// True for an address inside the project's releases page (no whitespace or control characters).
pub fn release_url_ok(url: &str) -> bool {
    url.len() <= MAX_URL_LEN
        && url.starts_with(RELEASE_URL_PREFIX)
        && url.bytes().all(|b| b.is_ascii_graphic())
}

/// A newer release: its version text and release page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Available {
    pub version: String,
    pub url: String,
}

#[derive(Deserialize)]
struct ReleaseJson {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Reads an API response: `Ok(Some(..))` for a newer final release, `Ok(None)` for anything else
/// that is well formed, `Err` for an oversized, malformed or unsafe response.
pub fn parse_release(body: &[u8], current: Version) -> Result<Option<Available>, String> {
    if body.len() as u64 > MAX_BODY_BYTES {
        return Err("업데이트 확인 응답이 너무 큽니다.".to_string());
    }
    let release: ReleaseJson = serde_json::from_slice(body)
        .map_err(|_| "업데이트 확인 응답을 해석하지 못했습니다.".to_string())?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let Some(version) = parse_version(release.tag_name.as_bytes()) else {
        return Ok(None);
    };
    if !is_newer(version, current) {
        return Ok(None);
    }
    if !release_url_ok(&release.html_url) {
        return Err("업데이트 확인 응답의 주소가 올바르지 않습니다.".to_string());
    }
    Ok(Some(Available {
        version: format!("{}.{}.{}", version[0], version[1], version[2]),
        url: release.html_url,
    }))
}

fn default_enabled() -> bool {
    true
}

/// What is remembered between runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateRecord {
    /// Unix seconds of the last check attempt.
    #[serde(default)]
    pub last_check: u64,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub available: Option<Available>,
}

impl Default for UpdateRecord {
    fn default() -> Self {
        Self {
            last_check: 0,
            enabled: true,
            available: None,
        }
    }
}

impl UpdateRecord {
    /// A check is due once a day has passed; a time in the future (clock change) counts as due.
    pub fn is_due(&self, now: u64) -> bool {
        self.last_check > now || now - self.last_check >= CHECK_INTERVAL_SECS
    }

    /// Drops a remembered release that is not newer than `current` or whose address is not ours (the
    /// file may have been edited).
    fn sanitized(mut self, current: Version) -> Self {
        let keep = self.available.as_ref().is_some_and(|a| {
            parse_version(a.version.as_bytes()).is_some_and(|v| is_newer(v, current))
                && release_url_ok(&a.url)
        });
        if !keep {
            self.available = None;
        }
        self
    }
}

fn read_record(path: &Path, current: Version) -> UpdateRecord {
    let bytes = std::fs::File::open(path).ok().and_then(|file| {
        let mut buf = Vec::new();
        file.take(MAX_RECORD_BYTES + 1)
            .read_to_end(&mut buf)
            .ok()
            .map(|_| buf)
    });
    bytes
        .filter(|b| b.len() as u64 <= MAX_RECORD_BYTES)
        .and_then(|b| serde_json::from_slice::<UpdateRecord>(&b).ok())
        .unwrap_or_default()
        .sanitized(current)
}

/// What the page sees.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct UpdateNoticeView {
    pub status: &'static str,
    pub version: Option<String>,
    pub enabled: bool,
    pub supported: bool,
}

/// The update state of the running app.
pub struct UpdateService {
    record: Mutex<UpdateRecord>,
    path: Option<PathBuf>,
    /// False in the smoke run and when there is no app data folder: nothing is checked or stored.
    active: bool,
    current: Version,
}

impl UpdateService {
    pub fn new(data_dir: Option<&Path>, active: bool, current: Version) -> Self {
        let path = data_dir.map(|dir| dir.join(RECORD_FILE));
        let record = match (&path, active) {
            (Some(path), true) => read_record(path, current),
            _ => UpdateRecord::default(),
        };
        Self {
            record: Mutex::new(record),
            path,
            active: active && data_dir.is_some(),
            current,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, UpdateRecord> {
        self.record
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn persist(&self, record: &UpdateRecord) {
        let Some(path) = self.path.as_ref().filter(|_| self.active) else {
            return;
        };
        if let Ok(bytes) = serde_json::to_vec(record) {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = write_document_bytes(path, &bytes);
        }
    }

    pub fn view(&self) -> UpdateNoticeView {
        let record = self.lock();
        let available = record
            .available
            .as_ref()
            .filter(|_| record.enabled && self.active);
        UpdateNoticeView {
            status: if available.is_some() {
                "available"
            } else {
                "idle"
            },
            version: available.map(|a| a.version.clone()),
            enabled: record.enabled,
            supported: self.active,
        }
    }

    /// The release page to open, only while a notice is shown.
    pub fn release_url(&self) -> Option<String> {
        let record = self.lock();
        record
            .available
            .as_ref()
            .filter(|a| record.enabled && self.active && release_url_ok(&a.url))
            .map(|a| a.url.clone())
    }

    pub fn set_enabled(&self, enabled: bool) {
        if !self.active {
            return;
        }
        let mut record = self.lock();
        record.enabled = enabled;
        self.persist(&record);
    }

    /// Runs one check if it is switched on and due. `fetch` returns the API response body. The time
    /// of the attempt is stored first, so a failing network is not retried more than once a day.
    pub fn check(
        &self,
        now: u64,
        fetch: impl FnOnce() -> Result<Vec<u8>, String>,
    ) -> Result<(), String> {
        {
            let mut record = self.lock();
            if !self.active || !record.enabled || !record.is_due(now) {
                return Ok(());
            }
            record.last_check = now;
            self.persist(&record);
        }
        let body = fetch()?;
        let found = parse_release(&body, self.current)?;
        let mut record = self.lock();
        record.available = found;
        self.persist(&record);
        Ok(())
    }
}

/// Downloads the latest-release answer: HTTPS only, OS TLS, short timeouts, no redirects, and at
/// most [`MAX_BODY_BYTES`] bytes.
fn fetch_latest_release() -> Result<Vec<u8>, String> {
    let tls =
        native_tls::TlsConnector::new().map_err(|_| "TLS를 준비하지 못했습니다.".to_string())?;
    let agent = ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls))
        .https_only(true)
        .redirects(0)
        .timeout(HTTP_TIMEOUT)
        .user_agent(&format!("EasyOffice/{}", env!("CARGO_PKG_VERSION")))
        .build();
    let response = agent
        .get(RELEASES_API)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|_| "업데이트 정보를 가져오지 못했습니다.".to_string())?;
    let mut body = Vec::with_capacity(16 * 1024);
    response
        .into_reader()
        .take(MAX_BODY_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|_| "업데이트 정보를 읽지 못했습니다.".to_string())?;
    Ok(body)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Creates the service (the page can always ask for its state) and, unless this is the smoke run,
/// starts the background check.
pub fn start(app: &AppHandle, smoke_mode: bool) {
    let dir = app.path().app_data_dir().ok();
    let service = Arc::new(UpdateService::new(
        dir.as_deref(),
        !smoke_mode,
        current_version(),
    ));
    app.manage(Arc::clone(&service));
    if smoke_mode {
        return;
    }
    std::thread::spawn(move || {
        if let Err(message) = service.check(unix_now(), fetch_latest_release) {
            eprintln!("update check: {message}");
        }
    });
}

#[tauri::command]
pub fn get_update_notice(service: State<'_, Arc<UpdateService>>) -> UpdateNoticeView {
    service.view()
}

#[tauri::command]
pub fn set_update_check(service: State<'_, Arc<UpdateService>>, enabled: bool) -> UpdateNoticeView {
    service.set_enabled(enabled);
    service.view()
}

/// Opens the stored release page in the default browser. The page supplies no address.
#[tauri::command]
pub async fn open_release_page(service: State<'_, Arc<UpdateService>>) -> Result<(), String> {
    let url = service
        .release_url()
        .ok_or_else(|| "열 수 있는 릴리스 페이지가 없습니다.".to_string())?;
    tauri::async_runtime::spawn_blocking(move || open::that(url))
        .await
        .map_err(|_| "브라우저를 열지 못했습니다.".to_string())?
        .map_err(|_| "브라우저를 열지 못했습니다.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const NOW: u64 = 1_800_000_000;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eo-update-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn release_json(tag: &str, url: &str, flags: &str) -> Vec<u8> {
        format!(
            r#"{{"id":1,"tag_name":"{tag}","html_url":"{url}","body":"notes","assets":[]{flags}}}"#
        )
        .into_bytes()
    }

    const RELEASE_URL: &str = "https://github.com/AquilaXk/easyoffice/releases/tag/v1.2.0";

    #[test]
    fn versions_have_exactly_three_numeric_parts_with_an_optional_v() {
        assert_eq!(parse_version(b"1.2.3"), Some([1, 2, 3]));
        assert_eq!(parse_version(b"v10.0.25"), Some([10, 0, 25]));
        assert_eq!(parse_version(b"V0.0.0"), Some([0, 0, 0]));
        assert_eq!(parse_version(b"999999999.1.1"), Some([999_999_999, 1, 1]));
        for bad in [
            "",
            "v",
            "1",
            "1.2",
            "1.2.3.4",
            "1.2.3-rc1",
            "1.2.3+b",
            "1..3",
            ".1.2",
            "1.2.",
            "a.b.c",
            "01.2.3",
            "1.2.3 ",
            "1000000000.0.0",
            "1.2.x",
            "v v1.2.3",
            "-1.2.3",
        ] {
            assert_eq!(parse_version(bad.as_bytes()), None, "{bad:?}");
        }
    }

    #[test]
    fn newer_means_a_larger_version_in_major_minor_patch_order() {
        assert!(is_newer([1, 0, 1], [1, 0, 0]));
        assert!(is_newer([1, 1, 0], [1, 0, 9]));
        assert!(is_newer([2, 0, 0], [1, 99, 99]));
        assert!(!is_newer([1, 0, 0], [1, 0, 0]));
        assert!(!is_newer([0, 9, 9], [1, 0, 0]));
        assert_eq!(
            parse_version(env!("CARGO_PKG_VERSION").as_bytes()),
            Some(current_version())
        );
    }

    #[test]
    fn only_the_projects_release_pages_are_openable() {
        assert!(release_url_ok(RELEASE_URL));
        assert!(release_url_ok(
            "https://github.com/AquilaXk/easyoffice/releases/"
        ));
        for bad in [
            "http://github.com/AquilaXk/easyoffice/releases/tag/v1",
            "https://github.com/AquilaXk/easyoffice/issues/1",
            "https://github.com.evil.example/AquilaXk/easyoffice/releases/",
            "https://github.com/AquilaXk/easyoffice-evil/releases/x",
            "https://github.com/AquilaXk/easyoffice/releases/ x",
            "https://github.com/AquilaXk/easyoffice/releases/\n",
            "file:///etc/passwd",
            "",
        ] {
            assert!(!release_url_ok(bad), "{bad:?}");
        }
        let long = format!("{RELEASE_URL}{}", "a".repeat(MAX_URL_LEN));
        assert!(!release_url_ok(&long));
    }

    #[test]
    fn a_newer_final_release_is_extracted_and_everything_else_is_ignored() {
        let current = [1, 0, 0];
        let found = parse_release(&release_json("v1.2.0", RELEASE_URL, ""), current).unwrap();
        assert_eq!(
            found,
            Some(Available {
                version: "1.2.0".to_string(),
                url: RELEASE_URL.to_string()
            })
        );
        assert_eq!(
            parse_release(&release_json("v1.0.0", RELEASE_URL, ""), current),
            Ok(None)
        );
        assert_eq!(
            parse_release(&release_json("v0.9.0", RELEASE_URL, ""), current),
            Ok(None)
        );
        assert_eq!(
            parse_release(&release_json("v2.0.0-rc1", RELEASE_URL, ""), current),
            Ok(None)
        );
        assert_eq!(
            parse_release(&release_json("nightly", RELEASE_URL, ""), current),
            Ok(None)
        );
        assert_eq!(
            parse_release(
                &release_json("v2.0.0", RELEASE_URL, r#","prerelease":true"#),
                current
            ),
            Ok(None)
        );
        assert_eq!(
            parse_release(
                &release_json("v2.0.0", RELEASE_URL, r#","draft":true"#),
                current
            ),
            Ok(None)
        );
    }

    #[test]
    fn malformed_oversized_or_unsafe_responses_are_errors() {
        let current = [1, 0, 0];
        assert!(parse_release(b"", current).is_err());
        assert!(parse_release(b"not json", current).is_err());
        assert!(
            parse_release(br#"{"tag_name":"v2.0.0"}"#, current).is_err(),
            "html_url is required"
        );
        assert!(parse_release(br#"{"tag_name":5,"html_url":"x"}"#, current).is_err());
        assert!(parse_release(
            &release_json("v2.0.0", "https://evil.example/x", ""),
            current
        )
        .is_err());
        let mut big = release_json("v2.0.0", RELEASE_URL, "");
        big.resize(usize::try_from(MAX_BODY_BYTES).unwrap() + 1, b' ');
        assert!(parse_release(&big, current).is_err());
        let mut exactly = release_json("v2.0.0", RELEASE_URL, "");
        exactly.resize(usize::try_from(MAX_BODY_BYTES).unwrap(), b' ');
        assert!(parse_release(&exactly, current).is_ok());
    }

    #[test]
    fn a_check_is_due_once_a_day_and_a_future_time_counts_as_due() {
        let record = UpdateRecord {
            last_check: NOW,
            ..UpdateRecord::default()
        };
        assert!(!record.is_due(NOW));
        assert!(!record.is_due(NOW + CHECK_INTERVAL_SECS - 1));
        assert!(record.is_due(NOW + CHECK_INTERVAL_SECS));
        assert!(record.is_due(NOW - 1), "a clock that went backwards");
        assert!(UpdateRecord::default().is_due(NOW), "never checked");
    }

    #[test]
    fn the_check_runs_once_a_day_and_the_notice_survives_a_restart() {
        let dir = scratch("daily");
        let calls = Cell::new(0);
        let fetch = || {
            calls.set(calls.get() + 1);
            Ok(release_json("v1.2.0", RELEASE_URL, ""))
        };
        let service = UpdateService::new(Some(&dir), true, [1, 0, 0]);
        assert_eq!(service.view().status, "idle");
        service.check(NOW, fetch).unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(
            service.view(),
            UpdateNoticeView {
                status: "available",
                version: Some("1.2.0".into()),
                enabled: true,
                supported: true
            }
        );
        service.check(NOW + 60, fetch).unwrap();
        assert_eq!(calls.get(), 1, "not asked again the same day");
        service.check(NOW + CHECK_INTERVAL_SECS, fetch).unwrap();
        assert_eq!(calls.get(), 2);
        let restarted = UpdateService::new(Some(&dir), true, [1, 0, 0]);
        assert_eq!(
            restarted.view().version.as_deref(),
            Some("1.2.0"),
            "the cached notice is shown at the next start"
        );
        restarted
            .check(NOW + CHECK_INTERVAL_SECS + 5, fetch)
            .unwrap();
        assert_eq!(calls.get(), 2, "the stored time of the last check is kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failing_check_is_not_retried_the_same_day_and_keeps_the_known_state() {
        let dir = scratch("fail");
        let service = UpdateService::new(Some(&dir), true, [1, 0, 0]);
        let calls = Cell::new(0);
        let failing = || {
            calls.set(calls.get() + 1);
            Err("offline".to_string())
        };
        assert!(service.check(NOW, failing).is_err());
        assert!(service.check(NOW + 10, failing).is_ok());
        assert_eq!(calls.get(), 1);
        assert_eq!(service.view().status, "idle");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn switching_the_check_off_stops_requests_and_hides_the_notice() {
        let dir = scratch("off");
        let service = UpdateService::new(Some(&dir), true, [1, 0, 0]);
        service
            .check(NOW, || Ok(release_json("v1.2.0", RELEASE_URL, "")))
            .unwrap();
        assert!(service.release_url().is_some());
        service.set_enabled(false);
        assert_eq!(
            service.view(),
            UpdateNoticeView {
                status: "idle",
                version: None,
                enabled: false,
                supported: true
            }
        );
        assert_eq!(service.release_url(), None);
        let calls = Cell::new(0);
        service
            .check(NOW + 2 * CHECK_INTERVAL_SECS, || {
                calls.set(1);
                Ok(Vec::new())
            })
            .unwrap();
        assert_eq!(calls.get(), 0, "no request while switched off");
        assert!(
            !UpdateService::new(Some(&dir), true, [1, 0, 0])
                .view()
                .enabled,
            "the switch is remembered"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_smoke_run_and_a_missing_data_folder_do_nothing() {
        let dir = scratch("smoke");
        let inactive = UpdateService::new(Some(&dir), false, [1, 0, 0]);
        let called = Cell::new(false);
        inactive
            .check(NOW, || {
                called.set(true);
                Ok(Vec::new())
            })
            .unwrap();
        assert!(!called.get());
        assert!(!inactive.view().supported);
        inactive.set_enabled(false);
        assert!(!dir.join(RECORD_FILE).exists(), "nothing is written");
        let nowhere = UpdateService::new(None, true, [1, 0, 0]);
        assert!(!nowhere.view().supported);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_edited_record_cannot_smuggle_in_an_address_or_an_old_version() {
        let dir = scratch("tamper");
        let path = dir.join(RECORD_FILE);
        for (version, url) in [
            ("1.2.0", "https://evil.example/x"),
            ("0.5.0", RELEASE_URL),
            ("x", RELEASE_URL),
        ] {
            let record = serde_json::json!({"last_check": NOW, "enabled": true, "available": {"version": version, "url": url}});
            std::fs::write(&path, record.to_string()).unwrap();
            assert_eq!(
                UpdateService::new(Some(&dir), true, [1, 0, 0])
                    .view()
                    .status,
                "idle",
                "{version} {url}"
            );
        }
        std::fs::write(&path, b"{ broken").unwrap();
        assert!(
            UpdateService::new(Some(&dir), true, [1, 0, 0])
                .view()
                .enabled
        );
        std::fs::write(
            &path,
            vec![b' '; usize::try_from(MAX_RECORD_BYTES).unwrap() + 1],
        )
        .unwrap();
        assert_eq!(
            UpdateService::new(Some(&dir), true, [1, 0, 0])
                .view()
                .status,
            "idle"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
