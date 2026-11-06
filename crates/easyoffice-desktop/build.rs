fn main() {
    // Declaring the app commands in the manifest makes them deny-by-default: the WebView may only
    // call what `capabilities/main-window.json` grants explicitly.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["ipc_dispatch", "save_document_file"]),
    ))
    .expect("failed to run tauri-build");
}
