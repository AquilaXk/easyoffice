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

/// Origin of the shell and of the editor document on Windows (WebView2 serves the custom protocol
/// as `http://tauri.localhost`; `useHttpsScheme` is not set, `tauri` 2.12.5 `protocol::origin`).
const WINDOWS_APP_ORIGIN: &str = "http://tauri.localhost";
/// The same origin on macOS and Linux (WKWebView, WebKitGTK).
const CUSTOM_SCHEME_APP_ORIGIN: &str = "tauri://localhost";

/// The origin the editor document is loaded from, which its CSP has to name.
///
/// It cannot be read from the request: on Windows wry turns WebView2's `http://tauri.localhost/...`
/// back into `tauri://localhost/...` before the response hook sees it (`wry` 0.57.1
/// `custom_protocol_workaround::revert_uri_work_around`, called from `webview2::prepare_request`),
/// so the request says `tauri://localhost` while the document's origin is `http://tauri.localhost`.
const fn app_origin(windows: bool) -> &'static str {
    if windows {
        WINDOWS_APP_ORIGIN
    } else {
        CUSTOM_SCHEME_APP_ORIGIN
    }
}

/// The editor document's policy: [`STUDIO_CSP`] with the app origin spelled out (see
/// `hwp_bundle::studio_csp_for_origin`). Falls back to the plain policy, never to a looser one.
fn studio_csp_value(origin: &str) -> HeaderValue {
    studio_csp_for_origin(origin)
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
        response.headers_mut().insert(
            CONTENT_SECURITY_POLICY,
            studio_csp_value(app_origin(cfg!(windows))),
        );
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
        // The request URI the hook gets is `tauri://localhost/...` on every OS (wry reverts the
        // Windows form); the authority of the request plays no part in the policy.
        let origin = app_origin(cfg!(windows));
        for uri in [
            "tauri://localhost/hwp-studio/index.html",
            "tauri://localhost/hwp-studio/index.html?theme=dark",
            "tauri://localhost/hwp%2Dstudio/index.html",
            "tauri://localhost/frame-probe.html",
            "http://tauri.localhost/%66rame-probe.html",
            "http://tauri.localhost/%68wp-studio/index.html",
        ] {
            let mut html = response("text/html");
            harden_studio_response(&request(uri), &mut html);
            let csp = html.headers()[CONTENT_SECURITY_POLICY].to_str().unwrap();
            // The origin is spelled out: WebKit does not match 'self' for the opaque-origin frame
            // and blocks all of its subresources (macOS smoke, PD-083).
            assert_eq!(csp, STUDIO_CSP.replace("'self'", origin), "{uri}");
            assert!(!csp.contains("'self'"), "{uri}: {csp}");
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
    fn the_policy_names_the_origin_the_document_really_has_on_each_platform() {
        assert_eq!(app_origin(true), "http://tauri.localhost");
        assert_eq!(app_origin(false), "tauri://localhost");
        let windows = studio_csp_value(app_origin(true));
        assert_eq!(
            windows.to_str().unwrap(),
            "default-src http://tauri.localhost; script-src http://tauri.localhost 'wasm-unsafe-eval'; \
             img-src http://tauri.localhost data: blob:; object-src 'none'; base-uri 'none'; \
             frame-ancestors http://tauri.localhost"
        );
        let mac = studio_csp_value(app_origin(false));
        assert_eq!(
            mac.to_str().unwrap(),
            "default-src tauri://localhost; script-src tauri://localhost 'wasm-unsafe-eval'; \
             img-src tauri://localhost data: blob:; object-src 'none'; base-uri 'none'; \
             frame-ancestors tauri://localhost"
        );
        // Both are origins the shell accepts for navigation, i.e. the ones its pages are served from.
        for windows in [true, false] {
            let url = tauri::Url::parse(&format!("{}/index.html", app_origin(windows))).unwrap();
            assert!(crate::has_app_origin(&url, windows), "{windows}");
        }
        assert_eq!(studio_csp_value("not an origin"), STUDIO_CSP);
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
