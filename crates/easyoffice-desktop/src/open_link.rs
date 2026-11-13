//! Opening the web or mail address of a sheet link in the system's own program (PD-218).
//!
//! This is the one command that takes an address from the page, so it does not trust it: the address
//! has to pass the policy of the cell links (`easyoffice_formats::link_target`: a plain `http`, `https`
//! or `mailto` address, no control character or space, at most 2,048 characters), parse as a URL with a
//! host for the web schemes and carry no user name or password. `file:`, `javascript:`, `data:`, an
//! address inside the workbook (`#...`) and everything else are refused here as well as in the page.
//! The command opens nothing without a click on the cell (the page sends it from the click only) and
//! asks no question, as the document editor's links ask none.

use crate::shell_caller::ShellCaller;

/// The address to hand to the system, or `None` when it is not a plain web or mail address.
pub(crate) fn open_target(raw: &str) -> Option<String> {
    let kept = easyoffice_formats::link_target(raw)?;
    let url = tauri::Url::parse(&kept).ok()?;
    let allowed = match url.scheme() {
        "http" | "https" => url.host_str().is_some_and(|host| !host.is_empty()),
        "mailto" => !url.path().is_empty(),
        _ => false,
    };
    (allowed && url.username().is_empty() && url.password().is_none()).then(|| url.to_string())
}

/// Opens `url` in the default browser or mail program.
#[tauri::command]
pub async fn open_link(_caller: ShellCaller, url: String) -> Result<(), String> {
    let target = open_target(&url).ok_or_else(|| "열 수 없는 주소입니다.".to_string())?;
    tauri::async_runtime::spawn_blocking(move || open::that(target))
        .await
        .map_err(|_| "주소를 열지 못했습니다.".to_string())?
        .map_err(|_| "주소를 열지 못했습니다.".to_string())
}

#[cfg(test)]
mod tests {
    use super::open_target;

    #[test]
    fn web_and_mail_addresses_pass_as_the_system_reads_them() {
        assert_eq!(
            open_target(" https://example.com/a?b=1&c=2 ").as_deref(),
            Some("https://example.com/a?b=1&c=2")
        );
        assert_eq!(
            open_target("http://Example.COM").as_deref(),
            Some("http://example.com/")
        );
        assert_eq!(
            open_target("mailto:me@example.com?subject=hi").as_deref(),
            Some("mailto:me@example.com?subject=hi")
        );
    }

    #[test]
    fn every_other_address_is_refused() {
        for bad in [
            "",
            "javascript:alert(1)",
            " JaVaScRiPt:alert(1)",
            "file:///etc/passwd",
            "file://localhost/etc/passwd",
            "data:text/html,<script>alert(1)</script>",
            "vbscript:x",
            "ftp://example.com/",
            "tel:+8210",
            "ssh://example.com",
            "about:blank",
            "#Sheet1!A1",
            "https://",
            "https:///etc/passwd",
            "https://user:secret@example.com/",
            "https://user@example.com/",
            "https://exa mple.com/",
            "https://example.com/\u{7}",
            "mailto:",
            "example.com",
            "//example.com",
            "\\\\server\\share\\a.exe",
            "C:\\Windows\\System32\\calc.exe",
            "/Applications/Calculator.app",
            "-a Calculator",
        ] {
            assert_eq!(open_target(bad), None, "{bad:?}");
        }
        assert_eq!(
            open_target(&format!("https://example.com/{}", "x".repeat(2_048))),
            None
        );
    }
}
