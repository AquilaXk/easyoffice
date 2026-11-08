//! Response policy of the embedded editor bundle (`/hwp-studio/*`, PD-083).
//!
//! The editor runs in an `<iframe sandbox="allow-scripts allow-modals">`, so it has an opaque origin:
//! it cannot touch the shell window's DOM or its Tauri IPC objects, and its IPC requests carry
//! `Origin: null`, which Tauri rejects (`tauri/src/ipc/protocol.rs`, "Origin header is not a valid
//! URL"). Two things follow for the bundle responses, both applied here to the app protocol's answer:
//! - its module scripts, WASM and fonts are cross-origin reads, so the files carry
//!   `Access-Control-Allow-Origin: *` (the bundle is public static content; nothing else gets it);
//! - Tauri puts the application CSP on HTML assets, whose `connect-src` allows the IPC endpoints.
//!   The editor document gets the stricter policy of `--serve` instead (`default-src 'self'` covers
//!   `connect-src`), so the frame cannot even open a connection to `ipc:` / `ipc.localhost`;
//! - that policy names the app origin explicitly instead of `'self'`: WebKit (macOS WKWebView) does
//!   not match `'self'` for a frame with an opaque origin and blocks the frame's every subresource,
//!   scripts included, before a request is made (`hwp_bundle::studio_csp_for_origin`).

use std::borrow::Cow;

use easyoffice_app::hwp_bundle::{
    fold_request_path, is_bundle_request_path, studio_csp_for_origin, STUDIO_CSP,
};
use tauri::http::header::{
    HeaderValue, ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_SECURITY_POLICY, CONTENT_TYPE,
};
use tauri::http::{Request, Response};

/// Pages the shell loads into a frame sandboxed like the editor: the editor bundle and the smoke
/// probe (`ui/frame-probe.*`), which has to see exactly the editor's environment.
const PROBE_PAGES: [&str; 2] = ["frame-probe.html", "frame-probe.js"];

fn is_sandboxed_frame_page(raw_path: &str) -> bool {
    is_bundle_request_path(raw_path)
        || fold_request_path(raw_path).is_some_and(|folded| PROBE_PAGES.contains(&folded.as_str()))
}

fn is_html(response: &Response<Cow<'static, [u8]>>) -> bool {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/html"))
}

/// The editor document's policy for a request to the app protocol: [`STUDIO_CSP`] with the origin the
/// request came in on (`tauri://localhost`, `http://tauri.localhost`, ...). When that origin cannot
/// be written safely the plain policy is kept, so nothing is ever looser than [`STUDIO_CSP`].
fn studio_csp_value(request: &Request<Vec<u8>>) -> HeaderValue {
    let uri = request.uri();
    uri.scheme_str()
        .zip(uri.authority())
        .and_then(|(scheme, authority)| studio_csp_for_origin(&format!("{scheme}://{authority}")))
        .and_then(|csp| HeaderValue::from_str(&csp).ok())
        .unwrap_or_else(|| HeaderValue::from_static(STUDIO_CSP))
}

/// Applied to every response of the app protocol; only `/hwp-studio/*` is touched.
pub(crate) fn harden_studio_response(
    request: &Request<Vec<u8>>,
    response: &mut Response<Cow<'static, [u8]>>,
) {
    if !is_sandboxed_frame_page(request.uri().path()) {
        return;
    }
    response
        .headers_mut()
        .insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    if is_html(response) {
        response
            .headers_mut()
            .insert(CONTENT_SECURITY_POLICY, studio_csp_value(request));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP_CSP: &str = "default-src 'self'; connect-src 'self' ipc: http://ipc.localhost";

    fn request(uri: &str) -> Request<Vec<u8>> {
        Request::builder().uri(uri).body(Vec::new()).unwrap()
    }

    fn response(content_type: &str) -> Response<Cow<'static, [u8]>> {
        Response::builder()
            .header(CONTENT_TYPE, content_type)
            .header(ACCESS_CONTROL_ALLOW_ORIGIN, "tauri://localhost")
            .header(CONTENT_SECURITY_POLICY, APP_CSP)
            .body(Cow::Borrowed(&b""[..]))
            .unwrap()
    }

    #[test]
    fn editor_document_loses_the_ipc_endpoints_and_every_editor_file_is_readable_cross_origin() {
        for (uri, origin) in [
            (
                "tauri://localhost/hwp-studio/index.html",
                "tauri://localhost",
            ),
            (
                "http://tauri.localhost/hwp-studio/index.html?theme=dark",
                "http://tauri.localhost",
            ),
            (
                "tauri://localhost/hwp%2Dstudio/index.html",
                "tauri://localhost",
            ),
            ("tauri://localhost/frame-probe.html", "tauri://localhost"),
            (
                "http://tauri.localhost/%66rame-probe.html",
                "http://tauri.localhost",
            ),
            (
                "http://tauri.localhost/%68wp-studio/index.html",
                "http://tauri.localhost",
            ),
        ] {
            let mut html = response("text/html");
            harden_studio_response(&request(uri), &mut html);
            let csp = html.headers()[CONTENT_SECURITY_POLICY].to_str().unwrap();
            // The origin is spelled out: WebKit does not match 'self' for the opaque-origin frame
            // and blocks all of its subresources (macOS smoke, PD-083).
            assert_eq!(csp, STUDIO_CSP.replace("'self'", origin), "{uri}");
            assert!(!csp.contains("'self'"), "{uri}: {csp}");
            assert!(
                csp.starts_with(&format!("default-src {origin};")),
                "{uri}: {csp}"
            );
            assert!(!csp.contains("ipc"), "{uri}: {csp}");
            assert_eq!(html.headers()[ACCESS_CONTROL_ALLOW_ORIGIN], "*", "{uri}");
        }
        let mut wasm = response("application/wasm");
        harden_studio_response(
            &request("tauri://localhost/hwp-studio/assets/rhwp_bg.wasm"),
            &mut wasm,
        );
        assert_eq!(wasm.headers()[ACCESS_CONTROL_ALLOW_ORIGIN], "*");
        assert_eq!(
            wasm.headers()[CONTENT_SECURITY_POLICY],
            APP_CSP,
            "only documents get the editor CSP"
        );
    }

    #[test]
    fn shell_pages_and_lookalike_paths_keep_the_app_policy() {
        for uri in [
            "tauri://localhost/index.html?module=HWP",
            "tauri://localhost/app.js",
            "tauri://localhost/hwp-studio",
            "tauri://localhost/x/hwp-studio/index.html",
            "http://tauri.localhost/hwp-studio-evil/index.html",
            "tauri://localhost/frame-probe.html.evil",
            "tauri://localhost/x/frame-probe.html",
        ] {
            let mut page = response("text/html");
            harden_studio_response(&request(uri), &mut page);
            assert_eq!(page.headers()[CONTENT_SECURITY_POLICY], APP_CSP, "{uri}");
            assert_eq!(
                page.headers()[ACCESS_CONTROL_ALLOW_ORIGIN],
                "tauri://localhost",
                "{uri}"
            );
        }
    }
}
