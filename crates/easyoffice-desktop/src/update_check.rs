//! Update notice and signed install (PD-103).
//!
//! After the app has started (never in the smoke run) it asks the public GitHub releases API for the
//! latest release at most once a day. When that release is a newer, final `x.y.z` version, the page
//! shows a small card (the reference app's "update notice") with a button that installs it and
//! restarts the app (`update_install.rs`: signed package, refused while a window has unsaved
//! changes) and a button that opens the release page in the default browser, the way to update when
//! the signed install is unavailable or fails (a debug build, a platform entry missing from the
//! release, a failed download or signature check). Every updatable bundle (AppImage, deb, rpm, NSIS,
//! macOS app archive) is signed and listed in `latest.json` by `scripts/release-assets.mjs`.
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
//! The notice card follows the reference app `apps/studio-host/src/ui/update-notice.ts` (MIT, see
//! THIRD_PARTY_NOTICES.md); the install is `update_install.rs`.

use crate::save_file::write_document_bytes;
use crate::shell_caller::ShellCaller;
use crate::update_install::{InstallProgress, Phase, NOTHING_TO_INSTALL};
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
/// `Accept` header of the update request (the smoke run's fake server checks it).
pub const ACCEPT_HEADER: &str = "application/vnd.github+json";
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

/// The running build: its version numbers and whether it is a pre-release of them. Releases are
/// compared as semantic versions: `1.2.0-rc.1` is older than `1.2.0` and newer than `1.1.9`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Running {
    version: Version,
    prerelease: bool,
}

impl From<Version> for Running {
    fn from(version: Version) -> Self {
        Self {
            version,
            prerelease: false,
        }
    }
}

/// True when `candidate` (always a final release) is newer than the running build.
pub fn is_newer(candidate: Version, current: impl Into<Running>) -> bool {
    let current = current.into();
    candidate > current.version || (current.prerelease && candidate == current.version)
}

/// Whether `text` is a semantic pre-release part: dot-separated, non-empty identifiers of ASCII
/// letters, digits and hyphens.
fn is_prerelease_part(text: &[u8]) -> bool {
    text.split(|&byte| byte == b'.')
        .all(|id| !id.is_empty() && id.iter().all(|&b| b.is_ascii_alphanumeric() || b == b'-'))
}

/// The running build from the app's version text (the Tauri config version, which the release
/// build overrides with the tag): `1.2.3`, `1.2.3-rc.1` or either with `+build` metadata, which
/// does not take part in the order. Unparseable text counts as the final release `0.0.0`.
pub fn current_version(text: &str) -> Running {
    let without_build = text.split_once('+').map_or(text, |(version, _)| version);
    let (core, pre) = match without_build.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (without_build, None),
    };
    let version = parse_version(core.as_bytes());
    match (version, pre) {
        (Some(version), None) => version.into(),
        (Some(version), Some(pre)) if is_prerelease_part(pre.as_bytes()) => Running {
            version,
            prerelease: true,
        },
        _ => Running::from([0, 0, 0]),
    }
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
pub fn parse_release(
    body: &[u8],
    current: impl Into<Running>,
) -> Result<Option<Available>, String> {
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
    if !is_newer(version, current.into()) {
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
    fn sanitized(mut self, current: Running) -> Self {
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

fn read_record(path: &Path, current: Running) -> UpdateRecord {
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

/// What the page sees. `status` is `idle`, `available`, `downloading`, `installing`, `installed`
/// (restart held for unsaved changes) or `error` (`message` says why); the byte counts belong to
/// `downloading`.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNoticeView {
    pub status: &'static str,
    pub version: Option<String>,
    pub enabled: bool,
    pub supported: bool,
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub message: Option<String>,
}

/// The update state of the running app.
pub struct UpdateService {
    record: Mutex<UpdateRecord>,
    path: Option<PathBuf>,
    /// False in the smoke run and when there is no app data folder: nothing is checked or stored.
    active: bool,
    current: Running,
    /// The signed install in progress, if any (not stored: a restart starts clean).
    pub install: InstallProgress,
}

impl UpdateService {
    pub fn new(data_dir: Option<&Path>, active: bool, current: impl Into<Running>) -> Self {
        let current = current.into();
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
            install: InstallProgress::default(),
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
        let mut view = UpdateNoticeView {
            status: if available.is_some() {
                "available"
            } else {
                "idle"
            },
            version: available.map(|a| a.version.clone()),
            enabled: record.enabled,
            supported: self.active,
            downloaded_bytes: None,
            total_bytes: None,
            message: None,
        };
        match self.install.phase() {
            Phase::Idle => {}
            Phase::Downloading {
                version,
                downloaded,
                total,
            } => {
                view.status = "downloading";
                view.version = Some(version);
                view.downloaded_bytes = Some(downloaded);
                view.total_bytes = total;
            }
            Phase::Installing { version } => {
                view.status = "installing";
                view.version = Some(version);
            }
            Phase::Installed { version } => {
                view.status = "installed";
                view.version = Some(version);
            }
            Phase::Failed { version, message } => {
                view.status = "error";
                view.version = Some(version);
                view.message = Some(message);
            }
        }
        view
    }

    /// Starts the signed install of the release the notice shows; returns its version. Refused when
    /// no notice is shown, one install is running, or `unsaved` windows exist.
    pub fn begin_install(&self, unsaved: bool) -> Result<String, String> {
        let version = {
            let record = self.lock();
            record
                .available
                .as_ref()
                .filter(|_| record.enabled && self.active)
                .map(|available| available.version.clone())
        }
        .ok_or_else(|| NOTHING_TO_INSTALL.to_string())?;
        self.install.begin(&version, unsaved)?;
        Ok(version)
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

    /// Runs one check if it is switched on and due. `fetch` returns the API response body, or `None`
    /// when the project has no release yet (the API answers 404), which clears any known notice. The
    /// time of the attempt is stored first, so a failing network is not retried more than once a day.
    pub fn check(
        &self,
        now: u64,
        fetch: impl FnOnce() -> Result<Option<Vec<u8>>, String>,
    ) -> Result<(), String> {
        {
            let mut record = self.lock();
            if !self.active || !record.enabled || !record.is_due(now) {
                return Ok(());
            }
            record.last_check = now;
            self.persist(&record);
        }
        let found = match fetch()? {
            Some(body) => parse_release(&body, self.current)?,
            None => None,
        };
        let mut record = self.lock();
        record.available = found;
        self.persist(&record);
        Ok(())
    }
}

/// Message for every failure to get the release answer (the reason is never shown to the page).
const FETCH_FAILED: &str = "업데이트 정보를 가져오지 못했습니다.";

/// The user agent of the update request: the app's name and version, nothing about the user.
pub fn user_agent(app_version: &str) -> String {
    format!("EasyOffice/{app_version}")
}

/// True when `url` may be requested. `https_only` (the real check) allows `https` addresses only.
/// Without it (the smoke run's fake server, PD-103) only `http://127.0.0.1:<port>/...` is allowed:
/// the host is compared after URL parsing, so `127.0.0.1@evil.example` or `127.0.0.1.evil.example`
/// do not pass, and no address with a user name or password does.
fn url_allowed(url: &tauri::Url, https_only: bool) -> bool {
    if https_only {
        return url.scheme() == "https";
    }
    url.scheme() == "http"
        && url.host_str() == Some("127.0.0.1")
        && url.port().is_some()
        && url.username().is_empty()
        && url.password().is_none()
}

/// Downloads a release answer: OS TLS, short timeouts, no redirects, and at most
/// [`MAX_BODY_BYTES`] + 1 bytes (a longer body is rejected by [`parse_release`]). `None` means the
/// server has no such release (HTTP 404). `https_only = false` exists for the loopback hook of the
/// smoke run and refuses every address except `http://127.0.0.1:<port>`.
pub fn fetch_release(
    url: &str,
    https_only: bool,
    app_version: &str,
) -> Result<Option<Vec<u8>>, String> {
    let url = tauri::Url::parse(url)
        .ok()
        .filter(|parsed| url_allowed(parsed, https_only))
        .ok_or_else(|| "허용되지 않는 업데이트 주소입니다.".to_string())?;
    let tls =
        native_tls::TlsConnector::new().map_err(|_| "TLS를 준비하지 못했습니다.".to_string())?;
    let agent = ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls))
        .https_only(https_only)
        .redirects(0)
        .timeout(HTTP_TIMEOUT)
        .user_agent(&user_agent(app_version))
        .build();
    let response = match agent.get(url.as_str()).set("Accept", ACCEPT_HEADER).call() {
        Ok(response) => response,
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(_) => return Err(FETCH_FAILED.to_string()),
    };
    let mut body = Vec::with_capacity(16 * 1024);
    response
        .into_reader()
        .take(MAX_BODY_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|_| "업데이트 정보를 읽지 못했습니다.".to_string())?;
    Ok(Some(body))
}

/// The real check: HTTPS only, against the project's releases API. `None` means there is no
/// release yet (HTTP 404).
fn fetch_latest_release(app_version: &str) -> Result<Option<Vec<u8>>, String> {
    fetch_release(RELEASES_API, true, app_version)
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
    let app_version = app.package_info().version.to_string();
    let service = Arc::new(UpdateService::new(
        dir.as_deref(),
        !smoke_mode,
        current_version(&app_version),
    ));
    app.manage(Arc::clone(&service));
    if smoke_mode {
        return;
    }
    std::thread::spawn(move || {
        if let Err(message) = service.check(unix_now(), || fetch_latest_release(&app_version)) {
            eprintln!("update check: {message}");
        }
    });
}

#[tauri::command]
pub fn get_update_notice(
    _caller: ShellCaller,
    service: State<'_, Arc<UpdateService>>,
) -> UpdateNoticeView {
    service.view()
}

#[tauri::command]
pub fn set_update_check(
    _caller: ShellCaller,
    app: AppHandle,
    service: State<'_, Arc<UpdateService>>,
    enabled: bool,
) -> UpdateNoticeView {
    service.set_enabled(enabled);
    // The menu's check item follows the switch.
    crate::menu_state::refresh_logged(&app);
    service.view()
}

/// Opens the stored release page in the default browser. The page supplies no address.
#[tauri::command]
pub async fn open_release_page(
    _caller: ShellCaller,
    service: State<'_, Arc<UpdateService>>,
) -> Result<(), String> {
    let url = service
        .release_url()
        .ok_or_else(|| "열 수 있는 릴리스 페이지가 없습니다.".to_string())?;
    tauri::async_runtime::spawn_blocking(move || open::that(url))
        .await
        .map_err(|_| "브라우저를 열지 못했습니다.".to_string())?
        .map_err(|_| "브라우저를 열지 못했습니다.".to_string())
}

#[cfg(test)]
mod tests;
