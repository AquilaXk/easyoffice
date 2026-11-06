use std::path::Path;

fn main() {
    // The WebUI embeds the generated hwp-studio editor bundle (`frontendDist` is the app's ui dir).
    // It is never tracked in Git, so building the shell without it would ship a window whose HWP
    // editor is missing: fail here with the exact fix instead (PD-083).
    let bundle = Path::new(env!("CARGO_MANIFEST_DIR")).join("../easyoffice-app/ui/hwp-studio");
    println!("cargo:rerun-if-changed=../easyoffice-app/ui/hwp-studio/asset-manifest.json");
    for required in ["index.html", "asset-manifest.json"] {
        assert!(
            bundle.join(required).is_file(),
            "the hwp-studio editor bundle is missing ({} not found). Build it before the desktop shell: \
             `node scripts/checkout-rhwp.mjs` then `node scripts/build-hwp-studio.mjs`, then run the build again.",
            bundle.join(required).display()
        );
    }

    // Declaring the app commands in the manifest makes them deny-by-default: the WebView may only
    // call what `capabilities/main-window.json` grants explicitly.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["ipc_dispatch", "save_document_file"]),
    ))
    .expect("failed to run tauri-build");
}
