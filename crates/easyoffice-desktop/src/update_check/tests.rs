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
    format!(r#"{{"id":1,"tag_name":"{tag}","html_url":"{url}","body":"notes","assets":[]{flags}}}"#)
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
fn only_https_or_the_exact_loopback_address_may_be_requested() {
    let allowed = |url: &str, https_only: bool| {
        tauri::Url::parse(url).is_ok_and(|parsed| url_allowed(&parsed, https_only))
    };
    assert!(allowed(RELEASES_API, true));
    assert!(allowed("http://127.0.0.1:8080/releases/latest", false));
    for bad in [
        "http://127.0.0.1:8080/x",
        "http://example.com/",
        "ftp://example.com/",
        "file:///etc/passwd",
    ] {
        assert!(!allowed(bad, true), "{bad}");
    }
    for bad in [
        RELEASES_API,
        "https://127.0.0.1:8080/x",
        "http://127.0.0.1/x",
        "http://127.0.0.1.evil.example:80/",
        "http://127.0.0.1:80@evil.example/",
        "http://user:secret@127.0.0.1:80/",
        "http://localhost:80/",
        "http://[::1]:80/",
        "http://10.0.0.1:80/",
        "http://example.com:80/",
        "ftp://127.0.0.1:80/",
    ] {
        assert!(!allowed(bad, false), "{bad}");
    }
}

#[test]
fn a_refused_address_fails_before_any_connection() {
    for (url, https_only) in [
        ("not a url", true),
        ("http://127.0.0.1:9/x", true),
        ("http://example.com/", false),
        ("https://api.github.com/", false),
        ("http://127.0.0.1:80@evil.example/", false),
    ] {
        let err = fetch_release(url, https_only, "1.2.3").unwrap_err();
        assert!(err.contains("허용되지 않는"), "{url}: {err}");
    }
}

#[test]
fn newer_means_a_larger_version_in_major_minor_patch_order() {
    assert!(is_newer([1, 0, 1], [1, 0, 0]));
    assert!(is_newer([1, 1, 0], [1, 0, 9]));
    assert!(is_newer([2, 0, 0], [1, 99, 99]));
    assert!(!is_newer([1, 0, 0], [1, 0, 0]));
    assert!(!is_newer([0, 9, 9], [1, 0, 0]));
}

#[test]
fn the_app_version_text_decides_which_release_is_news() {
    let built_as_0_1_0 = current_version("0.1.0");
    assert_eq!(built_as_0_1_0, Running::from([0, 1, 0]));
    let notice =
        |tag: &str| parse_release(&release_json(tag, RELEASE_URL, ""), built_as_0_1_0).unwrap();
    assert!(notice("v0.1.0").is_none());
    assert!(notice("v0.1.1").is_some());
    assert!(notice("v0.2.0").is_some());
    assert_eq!(current_version("not a version"), Running::from([0, 0, 0]));
}

/// A build made from a pre-release tag (`1.2.0-rc.1`, which the release workflow accepts) used
/// to count as `0.0.0`: every older release looked like news.
#[test]
fn a_pre_release_build_is_older_than_its_release_and_newer_than_the_one_before() {
    let news = |running: &str, tag: &str| {
        parse_release(
            &release_json(tag, RELEASE_URL, ""),
            current_version(running),
        )
        .unwrap()
        .is_some()
    };
    assert!(!news("1.2.0-rc.1", "v0.1.0"), "an older release is no news");
    assert!(!news("1.2.0-rc.1", "v1.1.9"));
    assert!(news("1.2.0-rc.1", "v1.2.0"), "its own release is news");
    assert!(news("1.2.0-rc.1", "v1.2.1"));
    assert!(
        !news("1.2.0", "v1.2.0"),
        "a release build is at its own version"
    );
    assert!(
        !news("1.2.0+build.5", "v1.2.0"),
        "build metadata is not a pre-release"
    );
    assert!(!news("1.2.3-rc.1+exp.sha", "v1.2.2"));
    assert!(news("1.2.3-rc.1+exp.sha", "v1.2.3"));
    // Text that is not a version at all still counts as 0.0.0.
    assert!(news("1.2.0-", "v0.0.1"));
    assert!(news("1.2.0-rc..1", "v0.0.1"));
    assert!(news("not a version", "v0.0.1"));
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
        Ok(Some(release_json("v1.2.0", RELEASE_URL, "")))
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
            supported: true,
            downloaded_bytes: None,
            total_bytes: None,
            message: None
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
fn no_release_yet_is_quiet_clears_the_notice_and_waits_a_day() {
    let dir = scratch("none");
    let service = UpdateService::new(Some(&dir), true, [1, 0, 0]);
    service
        .check(NOW, || Ok(Some(release_json("v1.2.0", RELEASE_URL, ""))))
        .unwrap();
    assert_eq!(service.view().status, "available");
    let calls = Cell::new(0);
    let none = || {
        calls.set(calls.get() + 1);
        Ok(None)
    };
    service.check(NOW + CHECK_INTERVAL_SECS, none).unwrap();
    assert_eq!(service.view().status, "idle");
    service.check(NOW + CHECK_INTERVAL_SECS + 60, none).unwrap();
    assert_eq!(calls.get(), 1, "not asked again the same day");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn switching_the_check_off_stops_requests_and_hides_the_notice() {
    let dir = scratch("off");
    let service = UpdateService::new(Some(&dir), true, [1, 0, 0]);
    service
        .check(NOW, || Ok(Some(release_json("v1.2.0", RELEASE_URL, ""))))
        .unwrap();
    assert!(service.release_url().is_some());
    service.set_enabled(false);
    assert_eq!(
        service.view(),
        UpdateNoticeView {
            status: "idle",
            version: None,
            enabled: false,
            supported: true,
            downloaded_bytes: None,
            total_bytes: None,
            message: None
        }
    );
    assert_eq!(service.release_url(), None);
    let calls = Cell::new(0);
    service
        .check(NOW + 2 * CHECK_INTERVAL_SECS, || {
            calls.set(1);
            Ok(None)
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
            Ok(None)
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

#[test]
fn the_install_state_reaches_the_page_and_the_install_needs_a_shown_notice() {
    let dir = scratch("install");
    let service = UpdateService::new(Some(&dir), true, [1, 0, 0]);
    assert_eq!(
        service.begin_install(false),
        Err(NOTHING_TO_INSTALL.to_string()),
        "no notice, nothing to install"
    );
    service
        .check(NOW, || Ok(Some(release_json("v1.2.0", RELEASE_URL, ""))))
        .unwrap();
    assert!(
        service.begin_install(true).is_err(),
        "unsaved windows block it"
    );
    assert_eq!(service.view().status, "available");
    assert_eq!(service.begin_install(false), Ok("1.2.0".to_string()));
    assert!(
        service.begin_install(false).is_err(),
        "one install at a time"
    );

    service.install.downloading("1.2.0", 5, Some(10));
    let view = service.view();
    assert_eq!(
        (view.status, view.downloaded_bytes, view.total_bytes),
        ("downloading", Some(5), Some(10))
    );
    service.install.installing("1.2.0");
    assert_eq!(service.view().status, "installing");
    service.install.fail("1.2.0", "서명".into());
    let view = service.view();
    assert_eq!(
        (
            view.status,
            view.version.as_deref(),
            view.message.as_deref()
        ),
        ("error", Some("1.2.0"), Some("서명"))
    );
    assert_eq!(
        service.begin_install(false),
        Ok("1.2.0".to_string()),
        "retry"
    );
    service.install.installed("1.2.0");
    assert_eq!(service.view().status, "installed");
    let json = serde_json::to_value(service.view()).unwrap();
    assert!(json.get("downloadedBytes").is_some() && json.get("totalBytes").is_some());

    service.set_enabled(false);
    let off = UpdateService::new(Some(&dir), true, [1, 0, 0]);
    assert_eq!(
        off.begin_install(false),
        Err(NOTHING_TO_INSTALL.to_string()),
        "a switched-off check shows no notice and installs nothing"
    );
    let smoke = UpdateService::new(Some(&dir), false, [1, 0, 0]);
    assert!(smoke.begin_install(false).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
