//! Safe, fail-closed file reader and CLI runner for `easyoffice-app --inspect <FILE>` (PD-248).

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use crate::cli_args::InspectArgs;
use crate::readback::{inspect_source_bytes, ReadbackQuery};

/// Upper bound on files inspected via CLI (`64 MiB`, matching the largest desktop open cap).
pub const MAX_INSPECT_FILE_BYTES: usize = 64 * 1_048_576;

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

fn in_sensitive_credential_dir(path: &Path) -> bool {
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

pub fn ensure_safe_inspect_target(canonical: &Path) -> Result<(), String> {
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
        return Err(
            "숨김 파일이나 자격증명 디렉터리를 가리키는 경로는 점검할 수 없습니다.".to_string(),
        );
    }
    Ok(())
}

fn read_bounded_regular_file(path: &Path, max_bytes: usize) -> Result<Vec<u8>, String> {
    let sym_meta = fs::symlink_metadata(path)
        .map_err(|err| format!("파일을 확인할 수 없습니다 ('{}'): {err}", path.display()))?;
    let canonical = fs::canonicalize(path).map_err(|err| {
        format!(
            "파일 경로를 해석할 수 없습니다 ('{}'): {err}",
            path.display()
        )
    })?;
    if sym_meta.file_type().is_symlink() {
        ensure_safe_inspect_target(&canonical)?;
    } else {
        // Also refuse direct credential/dotfile access inside `.ssh`, `.gnupg`, `.aws`, `.kube`, `.env`, etc.
        ensure_safe_inspect_target(&canonical)?;
    }
    let meta = fs::metadata(&canonical)
        .map_err(|err| format!("파일 정보를 읽을 수 없습니다 ('{}'): {err}", path.display()))?;
    if !meta.is_file() {
        return Err(format!(
            "일반 파일만 점검할 수 있습니다: '{}'",
            path.display()
        ));
    }
    if meta.len() > max_bytes as u64 {
        return Err(format!(
            "파일 크기가 허용 한도({} MiB)를 넘었습니다: {} 바이트",
            max_bytes / 1_048_576,
            meta.len()
        ));
    }
    let file = File::open(&canonical)
        .map_err(|err| format!("파일을 열 수 없습니다 ('{}'): {err}", path.display()))?;
    let mut buf = Vec::with_capacity((meta.len() as usize).min(max_bytes));
    file.take((max_bytes + 1) as u64)
        .read_to_end(&mut buf)
        .map_err(|err| {
            format!(
                "파일을 읽는 중 오류가 발생했습니다 ('{}'): {err}",
                path.display()
            )
        })?;
    if buf.len() > max_bytes {
        return Err(format!(
            "파일 크기가 허용 한도({} MiB)를 넘었습니다.",
            max_bytes / 1_048_576
        ));
    }
    Ok(buf)
}

pub fn run_inspect_cli(args: &InspectArgs) -> Result<String, String> {
    run_inspect_cli_with_limit(args, MAX_INSPECT_FILE_BYTES)
}

pub fn run_inspect_cli_with_limit(args: &InspectArgs, max_bytes: usize) -> Result<String, String> {
    let query =
        ReadbackQuery::from_cli(args.sheet.as_deref(), args.range.as_deref(), args.max_items)
            .map_err(|err| err.to_string())?;
    let path = Path::new(&args.path);
    let bytes = read_bounded_regular_file(path, max_bytes)?;
    let file_name = path.file_name().and_then(|n| n.to_str());
    inspect_source_bytes(&bytes, file_name, None, &query).map_err(|err| err.to_string())
}
