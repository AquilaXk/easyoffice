//! Guard against symlinks pointing to sensitive credential and autostart files (PD-177).

use std::path::Path;

const SENSITIVE_DIRS: &[&[u8]] = &[
    b".ssh",
    b".gnupg",
    b".aws",
    b".kube",
    b".git",
    b".docker",
    b"gcloud",
    b"launchagents",
    b"launchdaemons",
    b"autostart",
    b"systemd",
];

const SENSITIVE_FILES: &[&[u8]] = &[b".pypirc", b".npmrc", b".netrc", b"credentials.toml"];

pub fn in_sensitive_credential_dir(path: &Path) -> bool {
    path.components().any(|comp| match comp {
        std::path::Component::Normal(part) => {
            let bytes = part.as_encoded_bytes();
            SENSITIVE_DIRS
                .iter()
                .any(|dir| bytes.eq_ignore_ascii_case(dir))
        }
        _ => false,
    })
}

pub fn sensitive_symlink_error() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "숨김 파일이나 자격증명 디렉터리를 가리키는 심볼릭 링크는 허용되지 않습니다.",
    )
}

/// Rejects symlink targets pointing at hidden dotfiles (`.zshrc`, `.env`, …) or inside sensitive
/// credential/autostart directories (`.ssh`, `.gnupg`, `.aws`, `.kube`, `LaunchAgents`, `LaunchDaemons`,
/// `.config/autostart`, `.config/systemd`, `.cargo/credentials.toml`, `.pypirc`, `.npmrc`, `.netrc`)
/// while allowing normal non-hidden targets regardless of extension (PD-177).
pub fn ensure_safe_symlink_target(canonical: &Path) -> std::io::Result<()> {
    let is_dotfile = canonical
        .file_name()
        .is_some_and(|name| name.as_encoded_bytes().starts_with(b"."));
    let is_sensitive_file = canonical.file_name().is_some_and(|name| {
        let bytes = name.as_encoded_bytes();
        SENSITIVE_FILES
            .iter()
            .any(|f| bytes.eq_ignore_ascii_case(f))
    });
    if is_dotfile || is_sensitive_file || in_sensitive_credential_dir(canonical) {
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
        ];
        for path in allowed {
            assert!(
                ensure_safe_symlink_target(&PathBuf::from(path)).is_ok(),
                "Expected {path} to be allowed"
            );
        }
    }
}
