use cairn_core::bundle::DirTreeBundle;
use cairn_core::hash::HashAlgorithm;
use cairn_digest::{digest, DigestOptions};
use cairn_store::Store;
use std::path::PathBuf;

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("cairn-digest-entrypoint-test-{label}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn digest_writes_a_standalone_bundle_decodable_without_the_store() {
    let src = unique_temp_dir("digest-success-src");
    std::fs::write(src.join("file.txt"), b"some content").unwrap();

    let store_dir = unique_temp_dir("digest-success-store");
    let store = Store::new(store_dir.clone(), vec![]);
    let out_dir = unique_temp_dir("digest-success-out");
    let out_path = out_dir.join("root.dirtree");
    let options = DigestOptions::default();

    let root_id = digest(&src, &store, &out_path, &options).unwrap();

    let written = std::fs::read(&out_path).unwrap();
    let (_version, decoded_root, algo, bundle) = DirTreeBundle::decode_canonical(&written).unwrap();
    assert_eq!(decoded_root, root_id);
    assert_eq!(algo, HashAlgorithm::Sha256);
    // Root DirTree + "file.txt"'s Metadata + its FileIndex (no raw chunk bytes).
    assert_eq!(bundle.len(), 3);

    // No leftover temp file.
    assert!(!out_dir.join("root.dirtree.tmp").exists());

    std::fs::remove_dir_all(&src).unwrap();
    std::fs::remove_dir_all(&store_dir).unwrap();
    std::fs::remove_dir_all(&out_dir).unwrap();
}

#[test]
fn digest_leaves_no_partial_out_file_on_failure() {
    let missing_src = std::env::temp_dir().join("cairn-digest-entrypoint-test-nonexistent-src");
    // Deliberately do not create `missing_src`.

    let store_dir = unique_temp_dir("digest-failure-store");
    let store = Store::new(store_dir.clone(), vec![]);
    let out_dir = unique_temp_dir("digest-failure-out");
    let out_path = out_dir.join("root.dirtree");
    let options = DigestOptions::default();

    let result = digest(&missing_src, &store, &out_path, &options);
    assert!(result.is_err());
    assert!(!out_path.exists());

    std::fs::remove_dir_all(&store_dir).unwrap();
    std::fs::remove_dir_all(&out_dir).unwrap();
}
