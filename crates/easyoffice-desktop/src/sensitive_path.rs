//! Guard against symlinks and NTFS junctions pointing to sensitive credential and autostart files (PD-177, PD-261).

use std::path::Path;

const SENSITIVE_DIRS: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".kube",
    ".git",
    ".docker",
    "gcloud",
    "launchagents",
    "launchdaemons",
    "autostart",
    "systemd",
];

const SENSITIVE_FILES: &[&str] = &[".pypirc", ".npmrc", ".netrc", "credentials.toml"];

fn path_segments(path: &Path) -> Vec<String> {
    let raw = path.to_string_lossy();
    let trimmed = raw
        .strip_prefix(r"\\?\UNC\")
        .or_else(|| raw.strip_prefix(r"\\?\unc\"))
        .or_else(|| raw.strip_prefix(r"\\?\"))
        .unwrap_or(&raw);
    trimmed
        .split(['/', '\\'])
        .filter(|seg| !seg.is_empty() && !(seg.len() == 2 && seg.as_bytes()[1] == b':'))
        .map(str::to_owned)
        .collect()
}

fn has_windows_startup_sequence(segments: &[String]) -> bool {
    segments.windows(3).any(|w| {
        w[0].eq_ignore_ascii_case("start menu")
            && w[1].eq_ignore_ascii_case("programs")
            && w[2].eq_ignore_ascii_case("startup")
    })
}

fn segments_in_sensitive_dir(segments: &[String]) -> bool {
    if segments.iter().any(|part| {
        let base = part.split(':').next().unwrap_or(part);
        SENSITIVE_DIRS
            .iter()
            .any(|dir| base.eq_ignore_ascii_case(dir))
    }) {
        return true;
    }
    has_windows_startup_sequence(segments)
}

pub fn in_sensitive_credential_dir(path: &Path) -> bool {
    let segments = path_segments(path);
    segments_in_sensitive_dir(&segments)
}

pub fn sensitive_symlink_error() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "숨김 파일이나 자격증명 디렉터리를 가리키는 심볼릭 링크는 허용되지 않습니다.",
    )
}

/// Rejects targets pointing at hidden dotfiles (`.zshrc`, `.env`, …) or inside sensitive
/// credential/autostart directories (`.ssh`, `.gnupg`, `.aws`, `.kube`, `LaunchAgents`, `LaunchDaemons`,
/// `.config/autostart`, `.config/systemd`, Windows `Start Menu\Programs\Startup`,
/// `.cargo/credentials.toml`, `.pypirc`, `.npmrc`, `.netrc`) while allowing normal non-hidden
/// targets regardless of extension (PD-177, PD-261).
pub fn ensure_safe_symlink_target(canonical: &Path) -> std::io::Result<()> {
    let segments = path_segments(canonical);
    let raw_leaf = segments.last().map_or("", String::as_str);
    let leaf = raw_leaf.split(':').next().unwrap_or(raw_leaf);
    let is_dotfile = leaf.starts_with('.');
    let is_sensitive_file = SENSITIVE_FILES.iter().any(|f| leaf.eq_ignore_ascii_case(f));
    if is_dotfile || is_sensitive_file || segments_in_sensitive_dir(&segments) {
        return Err(sensitive_symlink_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn sensitive_credential_and_autostart_paths_are_refused() {
        let blocked = [
            "/home/user/.ssh/id_rsa",
            "/home/user/.gnupg/secring.gpg",
            "/home/user/.aws/credentials",
            "/home/user/.kube/config",
            "/repo/.git/config",
            "/repo/.git/hooks/pre-commit",
            "/home/user/.docker/config.json",
            "/home/user/.config/gcloud/credentials.db",
            "/Users/user/Library/LaunchAgents/com.evil.plist",
            "/Library/LaunchDaemons/com.evil.plist",
            "/home/user/.config/autostart/evil.desktop",
            "/home/user/.config/systemd/user/evil.service",
            "/home/user/.cargo/credentials.toml",
            "/home/user/.pypirc",
            "/home/user/.npmrc",
            "/home/user/.netrc",
            "/home/user/.zshrc",
            "/home/user/.env",
            r"C:\Users\user\AppData\Roaming\Windows\Start Menu\Programs\Startup\autorun.bat",
            r"\\?\C:\ProgramData\Windows\Start Menu\Programs\Startup\autorun.exe",
            r"\\?\C:\Users\user\.ssh\id_rsa",
            r"C:\Users\user\credentials.toml::$DATA",
            r"C:\Users\user\.env:stream",
        ];
        for path in blocked {
            assert!(
                ensure_safe_symlink_target(&PathBuf::from(path)).is_err(),
                "Expected {path} to be refused"
            );
        }

        let allowed = [
            "/home/user/Documents/report.docx",
            "/Users/user/Desktop/test.hwp",
            "/tmp/safe.hwpx",
            r"C:\Users\user\Documents\Startup\pitch.pptx",
            r"\\?\C:\Users\user\Documents\분기 보고서.docx",
        ];
        for path in allowed {
            assert!(
                ensure_safe_symlink_target(&PathBuf::from(path)).is_ok(),
                "Expected {path} to be allowed"
            );
        }
    }
}
