//! The preview of a `.pages` folder bundle: the cache follows the files inside the folder, and the
//! folder is read only by a request that holds a render permit.

use super::*;
use crate::pages_bundle_dir::bundle_stamp;
use std::fs;

fn bundle_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "easyoffice-preview-pages-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn jpeg(tail: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0];
    bytes.extend_from_slice(tail);
    bytes
}

fn make_bundle(root: &Path, preview: &[u8]) -> PathBuf {
    let bundle = root.join("doc.pages");
    fs::create_dir_all(bundle.join("Index")).unwrap();
    fs::write(bundle.join("preview.jpg"), preview).unwrap();
    fs::write(bundle.join("Index").join("Document.iwa"), b"stub").unwrap();
    bundle
}

#[test]
fn the_cache_follows_a_file_changed_inside_the_bundle() {
    let root = bundle_dir("cache");
    let (first, second) = (jpeg(b"A"), jpeg(b"second picture"));
    let bundle = make_bundle(&root, &first);
    let state = PreviewState::new();
    let found = load_preview_with(&state, "00000000000000f1", &bundle, |_| panic!("no draw"))
        .expect("thumbnail");
    assert_eq!(found.bytes, first);

    // The folder's own time does not move when a file inside is rewritten.
    fs::write(bundle.join("preview.jpg"), &second).unwrap();
    let again = load_preview_with(&state, "00000000000000f1", &bundle, |_| panic!("no draw"))
        .expect("new thumbnail");
    assert_eq!(again.bytes, second, "a stale picture was served");

    // An unchanged bundle is a cache hit: nothing is read or drawn.
    let hit = load_preview_with(&state, "00000000000000f1", &bundle, |_| panic!("no draw"))
        .expect("cache hit");
    assert!(Arc::ptr_eq(&again, &hit));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_file_added_or_removed_inside_the_bundle_changes_the_stamp() {
    let root = bundle_dir("stamp");
    let bundle = make_bundle(&root, &jpeg(b"A"));
    let before = bundle_stamp(&bundle).expect("stamp");
    fs::write(bundle.join("Index").join("Extra.iwa"), b"x").unwrap();
    let added = bundle_stamp(&bundle).expect("stamp");
    assert_ne!(before, added);
    fs::remove_file(bundle.join("Index").join("Extra.iwa")).unwrap();
    assert_ne!(added, bundle_stamp(&bundle).expect("stamp"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_request_without_a_permit_reads_nothing_and_caches_nothing() {
    let root = bundle_dir("busy");
    let bundle = make_bundle(&root, &jpeg(b"A"));
    let state = PreviewState::with_limits(
        Vec::new(),
        1,
        RenderLimits {
            deadline: Duration::from_secs(10),
            permit_wait: Duration::from_millis(20),
        },
    );
    let held = state.gate.acquire(Duration::ZERO).expect("the only permit");
    let found = load_preview_with(&state, "00000000000000f2", &bundle, |_| panic!("no draw"));
    assert!(found.is_none(), "the folder was read without a permit");
    assert_eq!(state.cache.lock().unwrap().len(), 0, "a busy answer is not remembered");

    drop(held);
    let found = load_preview_with(&state, "00000000000000f2", &bundle, |_| panic!("no draw"));
    assert!(found.is_some(), "with a free permit the thumbnail comes");
    let _ = fs::remove_dir_all(&root);
}
