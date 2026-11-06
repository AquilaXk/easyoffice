//! Embeds the generated hwp-studio bundle into the executable when the `embed-hwp-studio` feature
//! is on (PD-083). Without the feature nothing is read and nothing is required, so ordinary
//! `cargo build` / `cargo test` work without the bundle. With the feature the bundle must exist
//! (`node scripts/build-hwp-studio.mjs`): the build fails with a clear message instead of silently
//! shipping a binary whose editor is missing.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn collect(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, base, out);
        } else {
            let rel = path
                .strip_prefix(base)
                .expect("entry is below the bundle root")
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var_os("CARGO_FEATURE_EMBED_HWP_STUDIO").is_none() {
        return;
    }
    println!("cargo:rerun-if-changed=ui/hwp-studio");
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let bundle = manifest.join("ui/hwp-studio");
    if !bundle.join("index.html").is_file() || !bundle.join("asset-manifest.json").is_file() {
        panic!(
            "feature `embed-hwp-studio` needs the generated editor bundle at {}: run `node scripts/checkout-rhwp.mjs` and `node scripts/build-hwp-studio.mjs` first",
            bundle.display()
        );
    }
    let mut files = Vec::new();
    collect(&bundle, &bundle, &mut files);
    let mut source = String::from("pub static FILES: &[(&str, &[u8])] = &[\n");
    for (rel, path) in &files {
        writeln!(
            source,
            "    ({rel:?}, include_bytes!({:?})),",
            path.to_string_lossy()
        )
        .expect("write to string");
    }
    source.push_str("];\n");
    let out =
        PathBuf::from(std::env::var("OUT_DIR").expect("out dir")).join("hwp_studio_embedded.rs");
    std::fs::write(out, source).expect("write embedded bundle table");
}
