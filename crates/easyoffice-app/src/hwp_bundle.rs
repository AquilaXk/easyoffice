//! Static files of the generated hwp-studio editor bundle (PD-081, PD-083) for `--serve`.
//!
//! The bundle (`ui/hwp-studio/`, produced by `scripts/build-hwp-studio.mjs`, never tracked) is
//! served from two possible sources, in this order:
//! 1. the executable itself, when it was built with the `embed-hwp-studio` cargo feature
//!    (`build.rs` embeds every file; this keeps the release CLI binary self-contained), and
//! 2. a directory next to the executable (`ui/hwp-studio`, `hwp-studio`) or the source tree's
//!    `ui/hwp-studio` (development builds without the feature).
//!
//! Request paths are validated before any lookup: only plain `[A-Za-z0-9._-]` segments, no `.`/`..`,
//! no backslash, drive letter or stream syntax, so a request can never leave the bundle directory.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

pub const BUNDLE_URL_PREFIX: &str = "/hwp-studio/";
const MAX_REL_LEN: usize = 160;

/// Content-Security-Policy of every `/hwp-studio/*` response. `wasm-unsafe-eval` is the only
/// relaxation (WebAssembly compilation); there is no inline script or style, no remote origin and
/// no `unsafe-eval`. Only the app's own origin may embed the document.
pub const STUDIO_CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; img-src 'self' data: blob:; object-src 'none'; base-uri 'none'; frame-ancestors 'self'";

/// [`STUDIO_CSP`] with every `'self'` written as the explicit `origin` (`scheme://authority`).
///
/// The editor frame has an opaque origin (sandbox without `allow-same-origin`). Chromium still
/// matches `'self'` against the document's URL there, but WebKit (macOS WKWebView) does not: it
/// blocks every subresource of the frame before sending a request (measured in the desktop smoke,
/// where not even `storage-shim.js` reached the app protocol). A host-source names the same origin
/// without that dependency. Returns `None` for an origin that is not a plain `scheme://host[:port]`,
/// so a value can never smuggle another directive into the header.
pub fn studio_csp_for_origin(origin: &str) -> Option<String> {
    let (scheme, authority) = origin.split_once("://")?;
    let scheme_ok = !scheme.is_empty()
        && scheme.starts_with(|c: char| c.is_ascii_lowercase())
        && scheme
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '+' | '-' | '.'));
    let authority_ok = !authority.is_empty()
        && authority
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'));
    (scheme_ok && authority_ok).then(|| STUDIO_CSP.replace("'self'", origin))
}

/// Response header that lets the editor frame read its own files. The frame is sandboxed without
/// `allow-same-origin`, so it has an opaque origin and its module scripts, WASM and fonts are
/// cross-origin reads. The bundle is public static content; no other response carries this header.
pub const STUDIO_CORS_HEADER: &str = "Access-Control-Allow-Origin: *\r\n";

#[cfg(feature = "embed-hwp-studio")]
mod embedded {
    include!(concat!(env!("OUT_DIR"), "/hwp_studio_embedded.rs"));
}

pub struct BundleFile {
    pub bytes: Cow<'static, [u8]>,
    pub content_type: &'static str,
}

pub fn content_type_for(rel: &str) -> &'static str {
    let Some((_, ext)) = rel.rsplit_once('.') else {
        return "application/octet-stream";
    };
    if ext.eq_ignore_ascii_case("html") {
        "text/html; charset=utf-8"
    } else if ext.eq_ignore_ascii_case("js") || ext.eq_ignore_ascii_case("mjs") {
        "text/javascript; charset=utf-8"
    } else if ext.eq_ignore_ascii_case("css") {
        "text/css; charset=utf-8"
    } else if ext.eq_ignore_ascii_case("wasm") {
        "application/wasm"
    } else if ext.eq_ignore_ascii_case("woff2") {
        "font/woff2"
    } else if ext.eq_ignore_ascii_case("json") {
        "application/json; charset=utf-8"
    } else if ext.eq_ignore_ascii_case("txt") {
        "text/plain; charset=utf-8"
    } else if ext.eq_ignore_ascii_case("svg") {
        "image/svg+xml; charset=utf-8"
    } else if ext.eq_ignore_ascii_case("png") {
        "image/png"
    } else if ext.eq_ignore_ascii_case("ico") {
        "image/x-icon"
    } else {
        "application/octet-stream"
    }
}

#[inline]
fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    if !bytes.contains(&b'%') {
        return Some(raw.to_string());
    }
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = hex_nibble(bytes[i + 1])?;
            let lo = hex_nibble(bytes[i + 2])?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Folds a request path (query removed) for comparison: percent-decoded, backslashes as slashes,
/// lower case, no leading separator. `None` when it does not decode. This is the form to compare
/// against, so `/hwp%2Dstudio/...` and `/%68wp-studio/...` cannot slip past a check that the file
/// lookup would still resolve.
pub fn fold_request_path(raw_path: &str) -> Option<String> {
    let bytes = raw_path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = hex_nibble(bytes[i + 1])?;
            let lo = hex_nibble(bytes[i + 2])?;
            i += 3;
            (hi << 4) | lo
        } else {
            let cur = bytes[i];
            i += 1;
            cur
        };
        let norm = if b == b'\\' {
            b'/'
        } else {
            b.to_ascii_lowercase()
        };
        if out.is_empty() && norm == b'/' {
            continue;
        }
        out.push(norm);
    }
    String::from_utf8(out).ok()
}

/// True when a request path (query removed, not yet percent-decoded) addresses the editor bundle.
pub fn is_bundle_request_path(raw_path: &str) -> bool {
    fold_request_path(raw_path)
        .is_some_and(|folded| folded.starts_with(BUNDLE_URL_PREFIX.trim_start_matches('/')))
}

/// Maps a request path (query already removed) below `/hwp-studio/` to a safe relative path, or
/// `None` when it is not a plain file path inside the bundle. A trailing slash serves `index.html`.
pub fn sanitize_relative_path(request_path: &str) -> Option<String> {
    let rel_raw = request_path.strip_prefix(BUNDLE_URL_PREFIX)?;
    let decoded = percent_decode(rel_raw)?;
    let rel = if decoded.is_empty() {
        "index.html".to_string()
    } else {
        decoded
    };
    if rel.len() > MAX_REL_LEN || rel.ends_with('/') {
        return None;
    }
    let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
    for segment in rel.split('/') {
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || !segment.chars().all(plain)
            || segment.ends_with('.')
        {
            return None;
        }
    }
    Some(rel)
}

fn disk_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.join("ui").join("hwp-studio"));
            roots.push(dir.join("hwp-studio"));
        }
    }
    roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/hwp-studio"));
    roots
}

/// Reads `rel` below `root`; the resolved file must stay inside the canonical root (symlink guard).
pub fn read_from_dir(root: &Path, rel: &str) -> Option<Vec<u8>> {
    let canonical_root = root.canonicalize().ok()?;
    let file = canonical_root.join(rel).canonicalize().ok()?;
    if !file.starts_with(&canonical_root) || !file.is_file() {
        return None;
    }
    std::fs::read(file).ok()
}

pub fn lookup(rel: &str) -> Option<BundleFile> {
    let content_type = content_type_for(rel);
    #[cfg(feature = "embed-hwp-studio")]
    if let Some((_, bytes)) = embedded::FILES.iter().find(|(name, _)| *name == rel) {
        return Some(BundleFile {
            bytes: Cow::Borrowed(bytes),
            content_type,
        });
    }
    disk_roots().iter().find_map(|root| {
        read_from_dir(root, rel).map(|bytes| BundleFile {
            bytes: Cow::Owned(bytes),
            content_type,
        })
    })
}

/// True when an embedded bundle is part of this executable (feature `embed-hwp-studio`).
pub fn is_embedded() -> bool {
    cfg!(feature = "embed-hwp-studio")
}

/// Verifies the embedded bundle has the entry point and the WebAssembly module; used by `--self-test`.
#[cfg(feature = "embed-hwp-studio")]
pub fn embedded_integrity_error() -> Option<String> {
    let has = |suffix: &str| {
        embedded::FILES
            .iter()
            .any(|(n, b)| n.ends_with(suffix) && !b.is_empty())
    };
    if !has("index.html") || !has(".wasm") || !has("asset-manifest.json") {
        return Some(
            "내장 한글 편집기 번들에 index.html/.wasm/asset-manifest.json이 없습니다".to_string(),
        );
    }
    None
}

#[cfg(not(feature = "embed-hwp-studio"))]
pub fn embedded_integrity_error() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn studio_csp_names_the_origin_instead_of_self_and_only_for_plain_origins() {
        let csp = studio_csp_for_origin("tauri://localhost").unwrap();
        assert_eq!(
            csp,
            "default-src tauri://localhost; script-src tauri://localhost 'wasm-unsafe-eval'; \
             img-src tauri://localhost data: blob:; object-src 'none'; base-uri 'none'; \
             frame-ancestors tauri://localhost"
        );
        assert_eq!(
            studio_csp_for_origin("http://tauri.localhost").unwrap(),
            STUDIO_CSP.replace("'self'", "http://tauri.localhost")
        );
        assert!(studio_csp_for_origin("http://127.0.0.1:8080").is_some());
        assert!(studio_csp_for_origin("http://[::1]:8080").is_some());
        for bad in [
            "",
            "localhost",
            "://localhost",
            "tauri://",
            "tauri://localhost;script-src *",
            "tauri://local host",
            "tauri://localhost/x",
            "Tauri://localhost",
            "1tauri://localhost",
            "tauri://localhost,*",
        ] {
            assert!(studio_csp_for_origin(bad).is_none(), "{bad:?}");
        }
        assert!(!csp.contains("connect-src") && !csp.contains("ipc") && !csp.contains("'unsafe-"));
    }

    #[test]
    fn bundle_request_paths_are_recognised_after_decoding() {
        for yes in [
            "/hwp-studio/index.html",
            "/hwp%2Dstudio/index.html",
            "/%68wp-studio/assets/a.js",
            "//hwp-studio/index.html",
            "/HWP-Studio/index.html",
            "/hwp-studio/",
            "\\hwp-studio\\index.html",
        ] {
            assert!(is_bundle_request_path(yes), "{yes}");
        }
        for no in [
            "/",
            "/index.html",
            "/app.js",
            "/hwp-studio",
            "/hwp-studio-evil/index.html",
            "/x/hwp-studio/index.html",
            "/hwp%2",
            "/hwp%zz-studio/index.html",
        ] {
            assert!(!is_bundle_request_path(no), "{no}");
        }
    }

    #[test]
    fn sanitize_accepts_plain_bundle_paths() {
        assert_eq!(
            sanitize_relative_path("/hwp-studio/").as_deref(),
            Some("index.html")
        );
        assert_eq!(
            sanitize_relative_path("/hwp-studio/assets/index-Dxr1V8VE.js").as_deref(),
            Some("assets/index-Dxr1V8VE.js")
        );
        assert_eq!(
            sanitize_relative_path("/hwp-studio/fonts/Pretendard-Bold.woff2").as_deref(),
            Some("fonts/Pretendard-Bold.woff2")
        );
    }

    #[test]
    fn sanitize_rejects_traversal_and_odd_syntax() {
        for bad in [
            "/hwp-studio/../Cargo.toml",
            "/hwp-studio/%2e%2e/Cargo.toml",
            "/hwp-studio/%2E%2E%2FCargo.toml",
            "/hwp-studio/assets/../../x",
            "/hwp-studio/a\\b",
            "/hwp-studio/%5c..%5cx",
            "/hwp-studio/C:/Windows/win.ini",
            "/hwp-studio/index.html:stream",
            "/hwp-studio//etc/passwd",
            "/hwp-studio/a%00b",
            "/hwp-studio/%zz",
            "/hwp-studio/dir/",
            "/hwp-studio/trailing.",
            "/other/index.html",
        ] {
            assert_eq!(sanitize_relative_path(bad), None, "{bad}");
        }
    }

    #[test]
    fn mime_types_cover_the_bundle() {
        assert_eq!(
            content_type_for("assets/rhwp_bg-C2vGNh5n.wasm"),
            "application/wasm"
        );
        assert_eq!(
            content_type_for("fonts/NanumGothic-Bold.woff2"),
            "font/woff2"
        );
        assert!(content_type_for("assets/index.js").starts_with("text/javascript"));
        assert!(content_type_for("index.html").starts_with("text/html"));
        assert_eq!(content_type_for("noext"), "application/octet-stream");
    }

    #[test]
    fn disk_reads_stay_inside_the_root() {
        let base = std::env::temp_dir().join(format!("eo-bundle-test-{}", std::process::id()));
        let root = base.join("root");
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("index.html"), b"<html>ok</html>").unwrap();
        std::fs::write(base.join("secret.txt"), b"secret").unwrap();
        assert_eq!(
            read_from_dir(&root, "index.html").unwrap(),
            b"<html>ok</html>"
        );
        assert!(read_from_dir(&root, "../secret.txt").is_none());
        assert!(read_from_dir(&root, "assets").is_none());
        assert!(read_from_dir(&root, "missing.js").is_none());
        let _ = std::fs::remove_dir_all(base);
    }
}
