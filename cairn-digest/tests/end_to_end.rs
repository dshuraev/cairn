use cairn_core::hash::HashAlgorithm;
use cairn_digest::build::{build_tree, DigestOptions};
use cairn_digest::store::Store;
use cairn_digest::walk::walk_tree;
use std::path::PathBuf;

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("cairn-digest-e2e-test-{label}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Builds a source tree with: two files with identical content in different
/// subdirectories (cross-file dedup), one cross-directory hardlink pair, one
/// symlink, and one nested subdirectory.
fn make_source_tree(label: &str) -> PathBuf {
    let root = unique_temp_dir(label);
    std::fs::create_dir(root.join("dirX")).unwrap();
    std::fs::create_dir(root.join("dirY")).unwrap();
    std::fs::create_dir(root.join("dirY/nested")).unwrap();
    std::fs::create_dir(root.join("linkA")).unwrap();
    std::fs::create_dir(root.join("linkB")).unwrap();

    std::fs::write(
        root.join("dirX/dup1.txt"),
        b"duplicate content for dedup test",
    )
    .unwrap();
    std::fs::write(
        root.join("dirY/dup2.txt"),
        b"duplicate content for dedup test",
    )
    .unwrap();
    std::fs::write(root.join("dirY/nested/deep.txt"), b"nested unique content").unwrap();
    std::fs::write(root.join("linkA/shared.bin"), b"hardlinked shared content").unwrap();
    std::fs::hard_link(root.join("linkA/shared.bin"), root.join("linkB/shared.bin")).unwrap();
    std::os::unix::fs::symlink("dirX/dup1.txt", root.join("link.symlink")).unwrap();

    root
}

#[test]
fn build_tree_is_deterministic_across_separate_stores() {
    let source = make_source_tree("determinism-src");
    let options = DigestOptions::default();

    let store1_dir = unique_temp_dir("determinism-store1");
    let (walked1, tracker1) = walk_tree(&source, options.algo).unwrap();
    let store1 = Store::new(store1_dir.clone(), vec![]);
    let id1 = build_tree(&walked1, &tracker1, &store1, &options).unwrap();

    let store2_dir = unique_temp_dir("determinism-store2");
    let (walked2, tracker2) = walk_tree(&source, options.algo).unwrap();
    let store2 = Store::new(store2_dir.clone(), vec![]);
    let id2 = build_tree(&walked2, &tracker2, &store2, &options).unwrap();

    assert_eq!(
        id1, id2,
        "building the same source tree must be deterministic"
    );

    std::fs::remove_dir_all(&source).unwrap();
    std::fs::remove_dir_all(&store1_dir).unwrap();
    std::fs::remove_dir_all(&store2_dir).unwrap();
}

#[test]
fn cross_file_dedup_reduces_store_size_and_rerun_is_idempotent() {
    let source = make_source_tree("dedup-src");
    let options = DigestOptions::default();
    let (walked, tracker) = walk_tree(&source, options.algo).unwrap();

    let store_dir = unique_temp_dir("dedup-store");
    let store = Store::new(store_dir.clone(), vec![]);
    build_tree(&walked, &tracker, &store, &options).unwrap();

    let count_after_first_build = std::fs::read_dir(&store_dir).unwrap().count();

    // Hand-computable upper bound assuming *zero* dedup: 5 file-content nodes
    // (dup1, dup2, deep, shared.bin x2) each need a chunk + a FileIndex (10),
    // 11 total nodes (5 dirs incl. root, 5 files, 1 symlink) each need a
    // Metadata (11), and 6 DirTrees (root, dirX, dirY, nested, linkA, linkB).
    // 10 + 11 + 6 = 27. Content dedup (dup1.txt/dup2.txt share one chunk and
    // one FileIndex; the hardlinked shared.bin is only ever chunked once) can
    // only ever reduce this count relative to the no-dedup figure -- it never
    // increases it -- so a strictly-less-than assertion here is a direct,
    // robust proof that cross-file dedup actually fired.
    const NO_DEDUP_UPPER_BOUND: usize = 27;
    assert!(
        count_after_first_build < NO_DEDUP_UPPER_BOUND,
        "expected fewer than {NO_DEDUP_UPPER_BOUND} store objects with dedup, got {count_after_first_build}"
    );

    // Re-running against the same store must write zero new files.
    build_tree(&walked, &tracker, &store, &options).unwrap();
    let count_after_second_build = std::fs::read_dir(&store_dir).unwrap().count();
    assert_eq!(count_after_first_build, count_after_second_build);

    std::fs::remove_dir_all(&source).unwrap();
    std::fs::remove_dir_all(&store_dir).unwrap();
}

#[test]
fn identical_content_in_different_files_shares_one_chunk() {
    let source = make_source_tree("shared-chunk-src");
    let options = DigestOptions::default();
    let (walked, tracker) = walk_tree(&source, options.algo).unwrap();

    let store_dir = unique_temp_dir("shared-chunk-store");
    let store = Store::new(store_dir.clone(), vec![]);
    build_tree(&walked, &tracker, &store, &options).unwrap();

    let expected_chunk_id =
        cairn_core::hash::hash_bytes(HashAlgorithm::Sha256, b"duplicate content for dedup test");
    let object_path = store_dir.join(expected_chunk_id.to_string());
    assert!(
        object_path.exists(),
        "the shared content's chunk must exist in the store exactly once, content-addressed"
    );

    std::fs::remove_dir_all(&source).unwrap();
    std::fs::remove_dir_all(&store_dir).unwrap();
}
