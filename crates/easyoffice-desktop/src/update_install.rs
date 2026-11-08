//! Signed in-app update (PD-103): downloads the new version, checks its signature, installs it and
//! restarts the app.
//!
//! The download, the minisign check and the install belong to `tauri-plugin-updater`; this module
//! decides when they may run and never installs anything the plugin did not verify:
//! * The page only asks to start (`install_update`, no arguments, per-window command token) and polls
//!   the state (`get_update_notice`). The plugin's own JS commands are registered but never granted
//!   to a window (`capabilities/main-window.json`), so the page cannot reach the updater directly.
//! * The install starts only for the release the daily check announced; `latest.json` must name the
//!   same version and a package under the project's release downloads. The signature, which the
//!   plugin checks against the public key in `tauri.conf.json`, must carry that same version
//!   (`requireSignedVersion`), so an older signed package cannot be paired with a newer number.
//! * Fail-closed: a download or signature failure ends in an error state and `install` is never
//!   called. A package over [`MAX_PACKAGE_BYTES`] is dropped while it downloads, and the check and the
//!   download have time limits.
//! * Nothing is installed while a window holds unsaved changes, and this is checked again after the
//!   download. After the install the app restarts; a change made in the meantime holds the restart
//!   and tells the user to save and restart.
//!
//! The flow follows the reference app `apps/desktop/src-tauri/src/updates.rs` (MIT, Copyright (c)
//! 2025-2026 Edward Kim; see THIRD_PARTY_NOTICES.md): check, download with progress, refuse while a
//! document is dirty (`has_dirty_documents`), install, restart; an error keeps the update so it can be
//! tried again. Differences: the reference app's state lives in its app state and reaches the page
//! as an event, here it lives in [`UpdateService`] and the page polls; the pipeline is generic over
//! [`Package`] so the fail-closed rules are tested without a network; the announced version, the
//! download address and the package size are checked before the plugin installs; the install runs on
//! a blocking worker.

use crate::shell_caller::ShellCaller;
use crate::update_check::{parse_version, UpdateNoticeView, UpdateService};
use crate::window_commands::AppWindows;
use futures_util::future::{AbortHandle, Abortable};
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_updater::UpdaterExt;

/// Every update package must come from the project's release downloads.
pub const DOWNLOAD_URL_PREFIX: &str = "https://github.com/AquilaXk/easyoffice/releases/download/";
/// Largest accepted package (bytes); a longer download is dropped while it runs. The plugin grows one
/// `Vec` while it downloads, so the memory near the limit is about twice this (the old buffer and the
/// grown one).
pub const MAX_PACKAGE_BYTES: u64 = 256 * 1024 * 1024;
/// Asking `latest.json` which package fits this installation.
const CHECK_TIMEOUT: Duration = Duration::from_secs(15);
/// The whole package download, including a slow connection.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(20 * 60);

pub const NOTHING_TO_INSTALL: &str = "설치할 새 버전이 없습니다.";
const ALREADY_RUNNING: &str = "업데이트를 이미 진행하고 있습니다.";
const ALREADY_INSTALLED: &str = "새 버전이 이미 설치되었습니다. 저장한 뒤 앱을 다시 시작하세요.";
const UNSAVED_BEFORE: &str =
    "저장하지 않은 변경 사항이 있는 창이 있어 지금은 설치할 수 없습니다. 저장한 뒤 다시 시도하세요.";
const UNSAVED_AFTER_DOWNLOAD: &str =
    "업데이트를 내려받는 동안 저장하지 않은 변경 사항이 생겨 설치하지 않았습니다. 저장한 뒤 다시 시도하세요.";
const DEBUG_BUILD: &str = "업데이트 설치는 릴리스 빌드에서만 사용할 수 있습니다.";
const NO_UPDATE: &str = "받을 수 있는 새 버전 정보가 아직 없습니다. 잠시 뒤 다시 시도하세요.";
const MISMATCH: &str =
    "업데이트 정보가 릴리스와 달라 설치하지 않았습니다. 릴리스 페이지에서 받아 설치하세요.";
const TOO_LARGE: &str = "업데이트 파일이 너무 커서 설치하지 않았습니다.";
const SIGNATURE_FAILED: &str = "업데이트 파일의 서명을 확인하지 못해 설치하지 않았습니다.";
const UNSUPPORTED: &str =
    "이 설치 형태는 자동 설치를 지원하지 않습니다. 릴리스 페이지에서 받아 설치하세요.";
const NETWORK_FAILED: &str =
    "업데이트 정보를 가져오지 못했습니다. 네트워크를 확인하고 다시 시도하세요.";
const INSTALL_FAILED: &str =
    "업데이트를 설치하지 못했습니다. 다시 시도하거나 릴리스 페이지에서 받아 설치하세요.";

/// Where the install is. `Failed` keeps the announced version so "try again" and the card can name it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    Downloading {
        version: String,
        downloaded: u64,
        total: Option<u64>,
    },
    Installing {
        version: String,
    },
    /// Installed, but the restart is held because a window has unsaved changes.
    Installed {
        version: String,
    },
    Failed {
        version: String,
        message: String,
    },
}

/// The install state shared by every window (one install at a time).
#[derive(Default)]
pub struct InstallProgress(Mutex<Phase>);

impl InstallProgress {
    fn lock(&self) -> MutexGuard<'_, Phase> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn phase(&self) -> Phase {
        self.lock().clone()
    }

    /// Starts an install. Refused while one runs or has finished, and while any window has unsaved
    /// changes; a failed earlier try may be started again.
    pub fn begin(&self, version: &str, unsaved: bool) -> Result<(), String> {
        let mut phase = self.lock();
        match &*phase {
            Phase::Downloading { .. } | Phase::Installing { .. } => {
                return Err(ALREADY_RUNNING.to_string());
            }
            Phase::Installed { .. } => return Err(ALREADY_INSTALLED.to_string()),
            Phase::Idle | Phase::Failed { .. } => {}
        }
        if unsaved {
            return Err(UNSAVED_BEFORE.to_string());
        }
        *phase = Phase::Downloading {
            version: version.to_string(),
            downloaded: 0,
            total: None,
        };
        Ok(())
    }

    pub fn downloading(&self, version: &str, downloaded: u64, total: Option<u64>) {
        *self.lock() = Phase::Downloading {
            version: version.to_string(),
            downloaded,
            total,
        };
    }

    pub fn installing(&self, version: &str) {
        *self.lock() = Phase::Installing {
            version: version.to_string(),
        };
    }

    pub fn installed(&self, version: &str) {
        *self.lock() = Phase::Installed {
            version: version.to_string(),
        };
    }

    pub fn fail(&self, version: &str, message: String) {
        *self.lock() = Phase::Failed {
            version: version.to_string(),
            message,
        };
    }
}

/// True while a download of `downloaded` bytes (and the announced size, when known) fits the limit.
pub fn within_limit(downloaded: u64, announced: Option<u64>) -> bool {
    downloaded <= MAX_PACKAGE_BYTES && announced.is_none_or(|total| total <= MAX_PACKAGE_BYTES)
}

/// What `latest.json` announced must be the release the check found: the same `x.y.z` version and a
/// package under the project's release downloads.
pub fn verify_candidate(announced: &str, url: &tauri::Url, expected: &str) -> Result<(), String> {
    let same_version = match (
        parse_version(announced.as_bytes()),
        parse_version(expected.as_bytes()),
    ) {
        (Some(announced), Some(expected)) => announced == expected,
        _ => false,
    };
    if same_version && url.scheme() == "https" && url.as_str().starts_with(DOWNLOAD_URL_PREFIX) {
        Ok(())
    } else {
        Err(MISMATCH.to_string())
    }
}

/// The message for a plugin failure; the details go to stderr, not to the page.
pub fn failure_message(error: &tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error;
    match error {
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion => SIGNATURE_FAILED,
        Error::TargetNotFound(_) | Error::TargetsNotFound(_) => UNSUPPORTED,
        Error::Network(_) | Error::Reqwest(_) | Error::ReleaseNotFound | Error::Http(_) => {
            NETWORK_FAILED
        }
        _ => INSTALL_FAILED,
    }
    .to_string()
}

/// A verified-on-download update package: what the pipeline needs from the plugin's `Update`.
pub trait Package {
    /// Downloads and verifies the package. `on_chunk` gets the bytes so far and the announced size.
    fn download(
        &self,
        on_chunk: impl FnMut(u64, Option<u64>) + Send,
    ) -> impl Future<Output = Result<Vec<u8>, String>> + Send;

    /// Installs bytes the download returned. On Windows this ends the process.
    fn install(&self, bytes: Vec<u8>) -> impl Future<Output = Result<(), String>> + Send;
}

/// Download, check for unsaved changes again, install. `install` runs only after a download that
/// succeeded (the signature is part of it) and only while no window has unsaved changes.
pub async fn run_install<P: Package>(
    package: &P,
    progress: &InstallProgress,
    version: &str,
    has_unsaved: impl Fn() -> bool,
) -> Result<(), String> {
    progress.downloading(version, 0, None);
    let bytes = package
        .download(|downloaded, total| progress.downloading(version, downloaded, total))
        .await?;
    if has_unsaved() {
        return Err(UNSAVED_AFTER_DOWNLOAD.to_string());
    }
    progress.installing(version);
    package.install(bytes).await
}

struct UpdatePackage(tauri_plugin_updater::Update);

impl Package for UpdatePackage {
    async fn download(
        &self,
        mut on_chunk: impl FnMut(u64, Option<u64>) + Send,
    ) -> Result<Vec<u8>, String> {
        let (stop, registration) = AbortHandle::new_pair();
        let mut downloaded = 0u64;
        let download = self.0.download(
            |chunk, announced| {
                downloaded = downloaded.saturating_add(chunk as u64);
                if !within_limit(downloaded, announced) {
                    stop.abort();
                }
                on_chunk(downloaded, announced);
            },
            || {},
        );
        match Abortable::new(download, registration).await {
            // A download that ends in the same poll that crossed the limit is not cut off in time.
            Ok(Ok(bytes)) if within_limit(bytes.len() as u64, None) => Ok(bytes),
            Ok(Ok(_)) => Err(TOO_LARGE.to_string()),
            Ok(Err(error)) => {
                eprintln!("update download: {error}");
                Err(failure_message(&error))
            }
            Err(_) => Err(TOO_LARGE.to_string()),
        }
    }

    async fn install(&self, bytes: Vec<u8>) -> Result<(), String> {
        let update = self.0.clone();
        tauri::async_runtime::spawn_blocking(move || update.install(bytes))
            .await
            .map_err(|_| INSTALL_FAILED.to_string())?
            .map_err(|error| {
                eprintln!("update install: {error}");
                failure_message(&error)
            })
    }
}

/// Asks `latest.json` for the package of this installation and checks it is the announced release.
async fn find_update(app: &AppHandle, expected: &str) -> Result<UpdatePackage, String> {
    let report = |error: tauri_plugin_updater::Error| {
        eprintln!("update check: {error}");
        failure_message(&error)
    };
    let updater = app
        .updater_builder()
        .timeout(CHECK_TIMEOUT)
        .build()
        .map_err(report)?;
    let mut update = updater
        .check()
        .await
        .map_err(report)?
        .ok_or_else(|| NO_UPDATE.to_string())?;
    verify_candidate(&update.version, &update.download_url, expected)?;
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    Ok(UpdatePackage(update))
}

fn any_unsaved(app: &AppHandle) -> bool {
    app.state::<AppWindows>().has_unsaved_documents()
}

async fn drive(app: AppHandle, service: Arc<UpdateService>, version: String) {
    let outcome = match find_update(&app, &version).await {
        Ok(package) => {
            let probe = app.clone();
            run_install(&package, &service.install, &version, move || {
                any_unsaved(&probe)
            })
            .await
        }
        Err(message) => Err(message),
    };
    match outcome {
        Err(message) => service.install.fail(&version, message),
        // A change made while the package was installing holds the restart (Windows has already
        // exited by now: its installer restarts the app).
        Ok(()) if any_unsaved(&app) => service.install.installed(&version),
        Ok(()) => app.restart(),
    }
}

/// Starts the install of the release the notice shows and returns at once; the page polls
/// `get_update_notice` for the progress. The page supplies nothing.
#[tauri::command]
pub fn install_update(
    _caller: ShellCaller,
    app: AppHandle,
    service: State<'_, Arc<UpdateService>>,
) -> Result<UpdateNoticeView, String> {
    if cfg!(debug_assertions) {
        return Err(DEBUG_BUILD.to_string());
    }
    let version = service.begin_install(any_unsaved(&app))?;
    let worker = Arc::clone(&service);
    tauri::async_runtime::spawn(drive(app, worker, version));
    Ok(service.view())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update_check::RELEASE_URL_PREFIX;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tauri_plugin_updater::Error;

    fn url(text: &str) -> tauri::Url {
        tauri::Url::parse(text).unwrap()
    }

    const PACKAGE_URL: &str =
        "https://github.com/AquilaXk/easyoffice/releases/download/v1.2.0/EasyOffice-linux-x64.AppImage";

    /// A package whose download and install are scripted; it records what was called.
    struct Fake {
        download: Result<Vec<u8>, String>,
        install: Result<(), String>,
        chunks: Vec<(u64, Option<u64>)>,
        installed: AtomicBool,
        installed_bytes: Mutex<Vec<u8>>,
    }

    impl Fake {
        fn new(download: Result<Vec<u8>, String>) -> Self {
            Self {
                download,
                install: Ok(()),
                chunks: vec![(4, Some(8)), (8, Some(8))],
                installed: AtomicBool::new(false),
                installed_bytes: Mutex::new(Vec::new()),
            }
        }
    }

    impl Package for Fake {
        async fn download(
            &self,
            mut on_chunk: impl FnMut(u64, Option<u64>) + Send,
        ) -> Result<Vec<u8>, String> {
            for (downloaded, total) in &self.chunks {
                on_chunk(*downloaded, *total);
            }
            self.download.clone()
        }

        async fn install(&self, bytes: Vec<u8>) -> Result<(), String> {
            self.installed.store(true, Ordering::SeqCst);
            *self.installed_bytes.lock().unwrap() = bytes;
            self.install.clone()
        }
    }

    fn run(
        fake: &Fake,
        progress: &InstallProgress,
        unsaved: impl Fn() -> bool,
    ) -> Result<(), String> {
        tauri::async_runtime::block_on(run_install(fake, progress, "1.2.0", unsaved))
    }

    #[test]
    fn a_failed_download_or_signature_never_installs() {
        for message in [SIGNATURE_FAILED, NETWORK_FAILED, TOO_LARGE] {
            let fake = Fake::new(Err(message.to_string()));
            let progress = InstallProgress::default();
            assert_eq!(run(&fake, &progress, || false), Err(message.to_string()));
            assert!(!fake.installed.load(Ordering::SeqCst), "{message}");
        }
    }

    #[test]
    fn unsaved_changes_after_the_download_stop_the_install() {
        let fake = Fake::new(Ok(vec![1, 2, 3]));
        let progress = InstallProgress::default();
        assert_eq!(
            run(&fake, &progress, || true),
            Err(UNSAVED_AFTER_DOWNLOAD.to_string())
        );
        assert!(!fake.installed.load(Ordering::SeqCst));
    }

    #[test]
    fn a_verified_download_is_installed_with_exactly_its_bytes() {
        let fake = Fake::new(Ok(vec![1, 2, 3]));
        let progress = InstallProgress::default();
        assert_eq!(run(&fake, &progress, || false), Ok(()));
        assert!(fake.installed.load(Ordering::SeqCst));
        assert_eq!(*fake.installed_bytes.lock().unwrap(), vec![1, 2, 3]);
        assert_eq!(
            progress.phase(),
            Phase::Installing {
                version: "1.2.0".into()
            }
        );
    }

    #[test]
    fn a_failing_install_is_reported() {
        let mut fake = Fake::new(Ok(vec![9]));
        fake.install = Err(INSTALL_FAILED.to_string());
        let progress = InstallProgress::default();
        assert_eq!(
            run(&fake, &progress, || false),
            Err(INSTALL_FAILED.to_string())
        );
        assert!(fake.installed.load(Ordering::SeqCst));
    }

    #[test]
    fn download_progress_is_recorded_and_unsaved_changes_are_not_asked_after_a_failed_download() {
        let asked = AtomicUsize::new(0);
        let fake = Fake::new(Err(NETWORK_FAILED.to_string()));
        let progress = InstallProgress::default();
        let _ = run(&fake, &progress, || {
            asked.fetch_add(1, Ordering::SeqCst);
            false
        });
        assert_eq!(asked.load(Ordering::SeqCst), 0);
        assert_eq!(
            progress.phase(),
            Phase::Downloading {
                version: "1.2.0".into(),
                downloaded: 8,
                total: Some(8)
            }
        );
    }

    #[test]
    fn begin_refuses_unsaved_windows_and_a_second_install_but_allows_a_retry() {
        let progress = InstallProgress::default();
        assert_eq!(
            progress.begin("1.2.0", true),
            Err(UNSAVED_BEFORE.to_string())
        );
        assert_eq!(
            progress.phase(),
            Phase::Idle,
            "a refused start changes nothing"
        );
        assert_eq!(progress.begin("1.2.0", false), Ok(()));
        assert_eq!(
            progress.begin("1.2.0", false),
            Err(ALREADY_RUNNING.to_string())
        );
        progress.installing("1.2.0");
        assert_eq!(
            progress.begin("1.2.0", false),
            Err(ALREADY_RUNNING.to_string())
        );
        progress.fail("1.2.0", "x".into());
        assert_eq!(
            progress.begin("1.2.0", true),
            Err(UNSAVED_BEFORE.to_string())
        );
        assert_eq!(
            progress.begin("1.2.0", false),
            Ok(()),
            "a failed try may be repeated"
        );
        progress.installed("1.2.0");
        assert_eq!(
            progress.begin("1.2.0", false),
            Err(ALREADY_INSTALLED.to_string())
        );
    }

    #[test]
    fn the_package_size_limit_applies_to_the_running_total_and_the_announced_size() {
        assert_eq!(MAX_PACKAGE_BYTES, 256 * 1024 * 1024);
        assert!(within_limit(0, None));
        assert!(within_limit(MAX_PACKAGE_BYTES, Some(MAX_PACKAGE_BYTES)));
        assert!(!within_limit(MAX_PACKAGE_BYTES + 1, None));
        assert!(!within_limit(1, Some(MAX_PACKAGE_BYTES + 1)));
    }

    #[test]
    fn only_the_announced_release_from_our_downloads_may_be_installed() {
        assert_eq!(
            verify_candidate("1.2.0", &url(PACKAGE_URL), "1.2.0"),
            Ok(())
        );
        assert_eq!(
            verify_candidate("v1.2.0", &url(PACKAGE_URL), "1.2.0"),
            Ok(())
        );
        for (announced, address, expected) in [
            ("1.3.0", PACKAGE_URL, "1.2.0"),
            ("1.2.0-rc.1", PACKAGE_URL, "1.2.0"),
            ("1.2.0", PACKAGE_URL, "not a version"),
            (
                "1.2.0",
                "http://github.com/AquilaXk/easyoffice/releases/download/v1/x",
                "1.2.0",
            ),
            (
                "1.2.0",
                "https://evil.example/AquilaXk/easyoffice/releases/download/v1/x",
                "1.2.0",
            ),
            (
                "1.2.0",
                "https://github.com@evil.example/AquilaXk/easyoffice/releases/download/x",
                "1.2.0",
            ),
            (
                "1.2.0",
                "https://github.com/AquilaXk/easyoffice/releases/download/../../x",
                "1.2.0",
            ),
            (
                "1.2.0",
                "https://github.com/AquilaXk/easyoffice-evil/releases/download/v1/x",
                "1.2.0",
            ),
            (
                "1.2.0",
                "https://github.com/AquilaXk/easyoffice/releases/latest/x",
                "1.2.0",
            ),
        ] {
            assert_eq!(
                verify_candidate(announced, &url(address), expected),
                Err(MISMATCH.to_string()),
                "{announced} {address} {expected}"
            );
        }
        assert!(DOWNLOAD_URL_PREFIX.starts_with(RELEASE_URL_PREFIX));
    }

    #[test]
    fn plugin_failures_become_short_messages_without_details() {
        let signed_for_other = Error::SignedVersionMismatch {
            signed: "1.0.0".into(),
            announced: "9.9.9".into(),
        };
        for error in [signed_for_other, Error::MissingSignedVersion] {
            assert_eq!(failure_message(&error), SIGNATURE_FAILED);
        }
        assert_eq!(
            failure_message(&Error::TargetsNotFound(vec!["linux-x86_64-deb".into()])),
            UNSUPPORTED
        );
        assert_eq!(
            failure_message(&Error::Network("503".into())),
            NETWORK_FAILED
        );
        assert_eq!(failure_message(&Error::ReleaseNotFound), NETWORK_FAILED);
        assert_eq!(failure_message(&Error::DebInstallFailed), INSTALL_FAILED);
    }

    /// The shipped configuration: the one public key, the one endpoint, signed versions required.
    #[test]
    fn the_updater_configuration_is_pinned() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let updater = &conf["plugins"]["updater"];
        // base64 of "untrusted comment: minisign public key: ".
        assert!(updater["pubkey"]
            .as_str()
            .unwrap()
            .starts_with("dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6"));
        assert_eq!(
            updater["endpoints"],
            serde_json::json!([
                "https://github.com/AquilaXk/easyoffice/releases/latest/download/latest.json"
            ])
        );
        assert_eq!(updater["requireSignedVersion"], serde_json::json!(true));
        assert_eq!(updater["windows"]["installMode"], "passive");
        for danger in [
            "dangerousInsecureTransportProtocol",
            "dangerousAcceptInvalidCerts",
            "dangerousAcceptInvalidHostnames",
            "allowDowngrades",
        ] {
            assert!(updater.get(danger).is_none(), "{danger}");
        }
        assert!(
            conf["bundle"].get("createUpdaterArtifacts").is_none(),
            "the release workflow turns the signed artifacts on; local and CI builds have no key"
        );
        let capability = include_str!("../capabilities/main-window.json");
        assert!(
            !capability.contains("updater:"),
            "the plugin's JS commands stay ungranted"
        );
        let manifest = include_str!("../Cargo.toml");
        let dependency = manifest
            .lines()
            .find(|line| line.starts_with("tauri-plugin-updater"))
            .unwrap();
        assert!(
            dependency.contains("default-features = false") && dependency.contains("native-tls"),
            "the operating system's TLS stack, like the release check: {dependency}"
        );
    }
}
