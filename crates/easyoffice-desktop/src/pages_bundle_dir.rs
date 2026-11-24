//! Bounded, fail-closed reader for directory-bundle `.pages/` packages.
//!
//! Collects regular files up to explicit depth, entry count and cumulative byte ceilings without
//! ever following symbolic links or blocking on named pipes/devices, and packs them into a
//! deterministic stored ZIP buffer via [`easyoffice_formats::ZipArchiveCodec`].

use crate::tracked_files::open_regular;
use easyoffice_formats::{EasyOfficeError, ZipArchiveCodec};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

/// Maximum relative directory depth inside a `.pages/` bundle (e.g. `a/b/c/file` is depth 4).
pub const MAX_BUNDLE_DEPTH: usize = 4;
/// Maximum regular files packed from one `.pages/` bundle.
pub const MAX_BUNDLE_FILES: usize = 2_048;
/// Maximum subdirectories traversed in one `.pages/` bundle (prevents empty-dir floods).
const MAX_BUNDLE_DIRS: usize = 2_048;

#[cfg(windows)]
fn is_reparse_point(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_meta: &fs::Metadata) -> bool {
    false
}

/// Strips trailing path separators while preserving prefixes/roots so POSIX `lstat`
/// cannot traverse a symlink root via a trailing `/`.
fn strip_trailing_separators(path: &Path) -> PathBuf {
    path.components().collect()
}

/// Returns `true` only when `path` has a `.pages` extension (case-insensitive) and its
/// `symlink_metadata` is a real directory (never a symbolic link or reparse point).
pub fn is_pages_bundle_dir(path: &Path) -> bool {
    let normalized = strip_trailing_separators(path);
    let has_pages_ext = normalized
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pages"));
    has_pages_ext
        && fs::symlink_metadata(&normalized).is_ok_and(|meta| {
            !meta.file_type().is_symlink() && meta.file_type().is_dir() && !is_reparse_point(&meta)
        })
}

/// Returns `true` when `path` is a regular file (including a link to one) or a `.pages` folder bundle.
pub fn is_openable_target(path: &Path) -> bool {
    path.is_file() || is_pages_bundle_dir(path)
}

pub(crate) fn validate_entry_name(name: &str) -> io::Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('\0')
        || name.contains('/')
        || name.contains('\\')
        || name.contains(':')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "허용되지 않는 번들 항목 이름입니다.",
        ));
    }
    let mut comps = Path::new(name).components();
    match (comps.next(), comps.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "허용되지 않는 번들 경로 성분입니다.",
        )),
    }
}

struct WalkState {
    max_bytes: usize,
    total_read: usize,
    dir_count: usize,
    entries: Vec<(String, Vec<u8>)>,
}

fn walk_dir(dir: &Path, prefix: &str, depth: usize, state: &mut WalkState) -> io::Result<()> {
    let mut children: Vec<fs::DirEntry> = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|a| a.file_name());

    for child in children {
        let path = child.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() || is_reparse_point(&meta) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "심볼릭 링크가 포함된 번들은 열 수 없습니다.",
            ));
        }
        if !meta.file_type().is_dir() && !meta.file_type().is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "비정규 파일이 포함된 번들은 열 수 없습니다.",
            ));
        }

        let name_os = child.file_name();
        let Some(name) = name_os.to_str() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "UTF-8이 아닌 번들 파일 이름이 있습니다.",
            ));
        };
        validate_entry_name(name)?;

        let next_depth = depth + 1;
        if next_depth > MAX_BUNDLE_DEPTH {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Pages 번들 디렉터리 깊이가 허용 상한(4)을 초과했습니다.",
            ));
        }

        let rel_path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };

        if meta.file_type().is_dir() {
            if state.dir_count >= MAX_BUNDLE_DIRS {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Pages 번들 디렉터리 수가 허용 상한을 초과했습니다.",
                ));
            }
            state.dir_count += 1;
            walk_dir(&path, &rel_path, next_depth, state)?;
            continue;
        }

        if state.entries.len() >= MAX_BUNDLE_FILES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Pages 번들 파일 수가 허용 상한(2,048개)을 초과했습니다.",
            ));
        }

        let remaining = state.max_bytes.saturating_sub(state.total_read);
        let (file, opened_meta) = open_regular(&path)?;
        if opened_meta.len() > remaining as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Pages 번들 누적 크기가 허용 상한을 초과했습니다.",
            ));
        }
        let cap = usize::try_from(opened_meta.len())
            .unwrap_or(0)
            .min(remaining);
        let mut bytes = Vec::with_capacity(cap);
        file.take((remaining as u64).saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() > remaining {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Pages 번들 누적 크기가 허용 상한을 초과했습니다.",
            ));
        }
        state.total_read += bytes.len();
        state.entries.push((rel_path, bytes));
    }
    Ok(())
}

fn zip_pack_error(err: EasyOfficeError) -> io::Error {
    match err {
        EasyOfficeError::SecurityViolation(msg) => io::Error::new(io::ErrorKind::InvalidInput, msg),
        other => io::Error::new(io::ErrorKind::InvalidData, other.to_string()),
    }
}

/// Reads all regular files from the `.pages/` directory bundle at `dir` under strict bounds
/// (`MAX_BUNDLE_DEPTH = 4`, `MAX_BUNDLE_FILES = 2,048`, total read and output ZIP `<= max_bytes`),
/// refusing symbolic links, special files, path anomalies and non-UTF-8 names fail-closed.
pub fn read_pages_bundle_as_zip(
    dir: &Path,
    max_bytes: usize,
) -> io::Result<(Vec<u8>, fs::Metadata)> {
    let normalized = strip_trailing_separators(dir);
    let root_meta = fs::symlink_metadata(&normalized)?;
    if root_meta.file_type().is_symlink() || is_reparse_point(&root_meta) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "심볼릭 링크 번들은 허용되지 않습니다.",
        ));
    }
    if !root_meta.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Pages 번들 경로가 디렉터리가 아닙니다.",
        ));
    }

    let mut state = WalkState {
        max_bytes,
        total_read: 0,
        dir_count: 0,
        entries: Vec::new(),
    };
    walk_dir(&normalized, "", 0, &mut state)?;
    if state.entries.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Pages 번들에 파일이 없습니다.",
        ));
    }
    state.entries.sort_by(|a, b| a.0.cmp(&b.0));

    let zip_bytes = ZipArchiveCodec::pack(&state.entries).map_err(zip_pack_error)?;
    if zip_bytes.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Pages 번들 ZIP 크기가 허용 상한을 초과했습니다.",
        ));
    }
    Ok((zip_bytes, root_meta))
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyoffice_formats::{
        DecodedContent, DocumentFormat, RichBlock, RichDocument, RichParagraph, RichRun,
        UniversalFormatConverter, ZipArchiveCodec,
    };

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "easyoffice-pages-bundle-{tag}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_pages_zip_bytes() -> Vec<u8> {
        let doc = RichDocument {
            title: "번들 문서".to_string(),
            blocks: vec![RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("폴더 번들 본문 텍스트")],
                ..Default::default()
            })],
            ..Default::default()
        };
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Pages, None).unwrap()
    }

    fn write_bundle_from_zip(bundle_dir: &Path, zip_bytes: &[u8]) {
        let entries =
            ZipArchiveCodec::unpack(zip_bytes, &easyoffice_formats::ZipSecurityPolicy::default())
                .unwrap();
        for entry in entries {
            if entry.path.ends_with('/') {
                continue;
            }
            let out = bundle_dir.join(&entry.path);
            fs::create_dir_all(out.parent().unwrap()).unwrap();
            fs::write(out, entry.data).unwrap();
        }
    }

    #[test]
    fn reads_valid_pages_directory_bundle_and_nested_index_zip_deterministically() {
        let root = scratch("valid");
        let bundle = root.join("보고서.PaGeS");
        let zip_bytes = sample_pages_zip_bytes();
        write_bundle_from_zip(&bundle, &zip_bytes);

        assert!(is_pages_bundle_dir(&bundle));
        assert!(!is_pages_bundle_dir(&root.join("plain")));

        let (packed_a, meta_a) = read_pages_bundle_as_zip(&bundle, 10 * 1_048_576).unwrap();
        let (packed_b, _) = read_pages_bundle_as_zip(&bundle, 10 * 1_048_576).unwrap();
        assert!(meta_a.is_dir());
        assert_eq!(packed_a, packed_b, "ZIP output must be deterministic");

        let decoded = UniversalFormatConverter::decode_rich(&packed_a, Some("pages")).unwrap();
        let DecodedContent::Document(doc) = decoded.content else {
            panic!("expected document");
        };
        assert!(doc.blocks.iter().any(|b| matches!(
            b,
            RichBlock::Paragraph(p) if p.plain_text() == "폴더 번들 본문 텍스트"
        )));

        // Nested Index.zip inside a folder bundle works identically.
        let nested_bundle = root.join("중첩.pages");
        let entries = ZipArchiveCodec::unpack(
            &zip_bytes,
            &easyoffice_formats::ZipSecurityPolicy::default(),
        )
        .unwrap();
        let iwa_only: Vec<(String, Vec<u8>)> = entries
            .iter()
            .filter(|e| e.path.starts_with("Index/"))
            .map(|e| {
                (
                    e.path.strip_prefix("Index/").unwrap().to_string(),
                    e.data.clone(),
                )
            })
            .collect();
        let index_zip = ZipArchiveCodec::pack(&iwa_only).unwrap();
        fs::create_dir_all(&nested_bundle).unwrap();
        fs::write(nested_bundle.join("Index.zip"), index_zip).unwrap();
        for e in entries
            .into_iter()
            .filter(|e| !e.path.starts_with("Index/"))
        {
            let p = nested_bundle.join(&e.path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, e.data).unwrap();
        }
        let (nested_packed, _) = read_pages_bundle_as_zip(&nested_bundle, 10 * 1_048_576).unwrap();
        let nested_decoded =
            UniversalFormatConverter::decode_rich(&nested_packed, Some("pages")).unwrap();
        let DecodedContent::Document(nested_doc) = nested_decoded.content else {
            panic!("expected document");
        };
        assert!(nested_doc.blocks.iter().any(|b| matches!(
            b,
            RichBlock::Paragraph(p) if p.plain_text() == "폴더 번들 본문 텍스트"
        )));

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinks_inside_or_as_bundle_root_without_following() {
        let root = scratch("symlink");
        let outside = root.join("secret.txt");
        fs::write(&outside, b"top-secret").unwrap();

        let bundle = root.join("evil.pages");
        fs::create_dir_all(bundle.join("Index")).unwrap();
        fs::write(bundle.join("Index/Document.iwa"), b"ok").unwrap();
        std::os::unix::fs::symlink(&outside, bundle.join("Index/leak.iwa")).unwrap();

        assert!(is_pages_bundle_dir(&bundle));
        let err = read_pages_bundle_as_zip(&bundle, 1024 * 1024).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);

        let link_root = root.join("alias.pages");
        let real_dir = root.join("real.pages");
        fs::create_dir_all(&real_dir).unwrap();
        fs::write(real_dir.join("a.txt"), b"hello").unwrap();
        std::os::unix::fs::symlink(&real_dir, &link_root).unwrap();
        assert!(!is_pages_bundle_dir(&link_root));
        let root_err = read_pages_bundle_as_zip(&link_root, 1024 * 1024).unwrap_err();
        assert_eq!(root_err.kind(), io::ErrorKind::PermissionDenied);

        let trailing = PathBuf::from(format!("{}/", link_root.display()));
        assert!(!is_pages_bundle_dir(&trailing));
        let trailing_err = read_pages_bundle_as_zip(&trailing, 1024 * 1024).unwrap_err();
        assert_eq!(trailing_err.kind(), io::ErrorKind::PermissionDenied);

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_unix_fifo_inside_bundle_without_blocking() {
        let root = scratch("fifo");
        let bundle = root.join("pipe.pages");
        fs::create_dir_all(bundle.join("Index")).unwrap();
        fs::write(bundle.join("Index/Document.iwa"), b"ok").unwrap();
        let fifo = bundle.join("Index/pipe.iwa");
        crate::named_pipe_support::make_pipe(&fifo);

        let bundle_clone = bundle.clone();
        let result = crate::named_pipe_support::within_time(&fifo, move || {
            read_pages_bundle_as_zip(&bundle_clone, 1024 * 1024)
                .unwrap_err()
                .kind()
        });
        assert_eq!(result, Some(io::ErrorKind::InvalidInput));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn enforces_depth_file_count_byte_limits_and_path_component_guards() {
        let root = scratch("limits");

        // 1) Depth > 4 refused (depth 4 allowed, depth 5 refused).
        let ok_depth = root.join("d4.pages");
        let deep4 = ok_depth.join("a/b/c");
        fs::create_dir_all(&deep4).unwrap();
        fs::write(deep4.join("file.txt"), b"ok").unwrap();
        assert!(read_pages_bundle_as_zip(&ok_depth, 1024 * 1024).is_ok());

        let bad_depth = root.join("d5.pages");
        let deep5 = bad_depth.join("a/b/c/d");
        fs::create_dir_all(&deep5).unwrap();
        fs::write(deep5.join("file.txt"), b"too-deep").unwrap();
        assert_eq!(
            read_pages_bundle_as_zip(&bad_depth, 1024 * 1024)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );

        // 2) File count > 2,048 refused.
        let too_many = root.join("many.pages");
        fs::create_dir_all(&too_many).unwrap();
        for i in 0..=MAX_BUNDLE_FILES {
            fs::write(too_many.join(format!("f{i:04}.bin")), b"1").unwrap();
        }
        assert_eq!(
            read_pages_bundle_as_zip(&too_many, 10 * 1_048_576)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );

        // 3) Cumulative bytes > max_bytes refused (both raw read and final ZIP size).
        let sized = root.join("sized.pages");
        fs::create_dir_all(&sized).unwrap();
        fs::write(sized.join("a.bin"), vec![7u8; 512]).unwrap();
        fs::write(sized.join("b.bin"), vec![8u8; 512]).unwrap();
        assert_eq!(
            read_pages_bundle_as_zip(&sized, 1023).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        // Exactly 1024 raw bytes still exceeds 1024 once ZIP headers are added.
        assert_eq!(
            read_pages_bundle_as_zip(&sized, 1024).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );

        // 4) Path component guards reject `..`, root, prefix, empty, colon.
        for bad in ["..", ".", "../x", "/etc/passwd", r"C:\x", "a/b", "a:b", ""] {
            assert_eq!(
                validate_entry_name(bad).unwrap_err().kind(),
                io::ErrorKind::InvalidInput,
                "{bad}"
            );
        }

        let _ = fs::remove_dir_all(&root);
    }
}
