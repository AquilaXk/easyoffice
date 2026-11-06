//! Validation and atomic writing for the desktop "save document" command.

use easyoffice_app::EasyOfficeIpcRouter;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::Path;

/// Upper bound for one saved document (decoded bytes).
pub const MAX_SAVE_BYTES: usize = 256 * 1_048_576;

/// A save request from the WebView: a proposed file name (never a path) plus the document bytes.
#[derive(Debug)]
pub struct SaveRequest {
    file_name: String,
    extension: String,
    bytes: Vec<u8>,
}

impl SaveRequest {
    pub fn parse(file_name: &str, base64: &str) -> Result<Self, String> {
        // Keep only the final component so a proposed name can never point at another directory.
        let base = file_name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        let Some((stem, ext)) = base.rsplit_once('.') else {
            return Err("저장할 파일 이름에 확장자가 없습니다.".to_string());
        };
        let ext_ok =
            !ext.is_empty() && ext.len() <= 8 && ext.chars().all(|c| c.is_ascii_alphanumeric());
        if stem.trim().is_empty() || !ext_ok {
            return Err(format!("저장할 수 없는 파일 이름입니다: {base}"));
        }
        if base64.len() / 4 * 3 > MAX_SAVE_BYTES + 3 {
            return Err("저장할 문서가 허용 크기(256MiB)를 초과합니다.".to_string());
        }
        let bytes = EasyOfficeIpcRouter::decode_base64(base64.trim())
            .ok_or_else(|| "저장할 문서 데이터가 올바른 Base64가 아닙니다.".to_string())?;
        if bytes.is_empty() {
            return Err("저장할 문서 데이터가 비어 있습니다.".to_string());
        }
        if bytes.len() > MAX_SAVE_BYTES {
            return Err("저장할 문서가 허용 크기(256MiB)를 초과합니다.".to_string());
        }
        Ok(Self {
            extension: ext.to_ascii_lowercase(),
            file_name: base,
            bytes,
        })
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub fn extension(&self) -> &str {
        &self.extension
    }

    pub fn filter_label(&self) -> String {
        format!("{} 문서", self.extension.to_ascii_uppercase())
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SaveOutcome {
    status: &'static str,
    path: Option<String>,
    bytes: usize,
}

impl SaveOutcome {
    pub fn saved(path: &Path, bytes: usize) -> Self {
        Self {
            status: "saved",
            path: Some(path.display().to_string()),
            bytes,
        }
    }

    pub fn cancelled() -> Self {
        Self {
            status: "cancelled",
            path: None,
            bytes: 0,
        }
    }
}

/// Writes through a sibling temporary file and renames it into place, so a failed write never
/// leaves a truncated document where an existing one used to be.
pub fn write_document_bytes(path: &Path, bytes: &[u8]) -> Result<usize, String> {
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("저장 위치가 파일이 아닙니다: {}", path.display()))?;
    let mut tmp_name = file_name.to_os_string();
    tmp_name.push(".easyoffice-saving");
    let tmp_path = path.with_file_name(tmp_name);
    match write_then_rename(&tmp_path, path, bytes) {
        Ok(()) => Ok(bytes.len()),
        Err(err) => {
            let _ = fs::remove_file(&tmp_path);
            Err(format!(
                "파일을 저장하지 못했습니다 ({}): {err}",
                path.display()
            ))
        }
    }
}

fn write_then_rename(tmp_path: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = fs::File::create(tmp_path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(tmp_path, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "easyoffice-desktop-save-{tag}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_keeps_only_the_file_name_and_decodes_bytes() {
        let req = SaveRequest::parse("..\\..\\Windows/보고서.DOCX", "UEsDBA==").unwrap();
        assert_eq!(req.file_name(), "보고서.DOCX");
        assert_eq!(req.extension(), "docx");
        assert_eq!(req.filter_label(), "DOCX 문서");
        assert_eq!(req.bytes(), b"PK\x03\x04");
    }

    #[test]
    fn parse_rejects_bad_names_and_payloads() {
        assert!(SaveRequest::parse("noext", "UEsDBA==").is_err());
        assert!(SaveRequest::parse(".docx", "UEsDBA==").is_err());
        assert!(SaveRequest::parse("a.do cx", "UEsDBA==").is_err());
        assert!(SaveRequest::parse("a.docx", "***").is_err());
        assert!(SaveRequest::parse("a.docx", "").is_err());
    }

    #[test]
    fn write_replaces_existing_file_atomically() {
        let dir = scratch_dir("write");
        let target = dir.join("문서.hwpx");
        fs::write(&target, b"old").unwrap();
        assert_eq!(write_document_bytes(&target, b"new-bytes").unwrap(), 9);
        assert_eq!(fs::read(&target).unwrap(), b"new-bytes");
        assert!(!dir.join("문서.hwpx.easyoffice-saving").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_failure_is_reported_and_leaves_no_temp_file() {
        let dir = scratch_dir("fail");
        let target = dir.join("missing-dir").join("a.pdf");
        let err = write_document_bytes(&target, b"x").unwrap_err();
        assert!(err.contains("파일을 저장하지 못했습니다"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn outcomes_serialize_for_the_webview() {
        let saved = serde_json::to_string(&SaveOutcome::saved(Path::new("C:/a.pdf"), 3)).unwrap();
        assert_eq!(saved, r#"{"status":"saved","path":"C:/a.pdf","bytes":3}"#);
        let cancelled = serde_json::to_string(&SaveOutcome::cancelled()).unwrap();
        assert_eq!(cancelled, r#"{"status":"cancelled","path":null,"bytes":0}"#);
    }
}
