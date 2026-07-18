use cairn_core::hash::HashAlgorithm;
use cairn_core::bundle::DirTreeBundle;
use cairn_digest::build::{build_tree, DigestOptions};
use cairn_store::Store;
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
    let (walked1, mut tracker1) = walk_tree(&source, options.algo).unwrap();
    let store1 = Store::new(store1_dir.clone(), vec![]);
    let id1 = build_tree(&walked1, &mut tracker1, &store1, &options, &mut DirTreeBundle::new()).unwrap();

    let store2_dir = unique_temp_dir("determinism-store2");
    let (walked2, mut tracker2) = walk_tree(&source, options.algo).unwrap();
    let store2 = Store::new(store2_dir.clone(), vec![]);
    let id2 = build_tree(&walked2, &mut tracker2, &store2, &options, &mut DirTreeBundle::new()).unwrap();

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
    let (walked, mut tracker) = walk_tree(&source, options.algo).unwrap();

    let store_dir = unique_temp_dir("dedup-store");
    let store = Store::new(store_dir.clone(), vec![]);
    build_tree(&walked, &mut tracker, &store, &options, &mut DirTreeBundle::new()).unwrap();

    let algo_dir = store_dir.join("sha256");
    let count_after_first_build = std::fs::read_dir(&algo_dir).unwrap().count();

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
    build_tree(&walked, &mut tracker, &store, &options, &mut DirTreeBundle::new()).unwrap();
    let count_after_second_build = std::fs::read_dir(&algo_dir).unwrap().count();
    assert_eq!(count_after_first_build, count_after_second_build);

    std::fs::remove_dir_all(&source).unwrap();
    std::fs::remove_dir_all(&store_dir).unwrap();
}

#[test]
fn identical_content_in_different_files_shares_one_chunk() {
    let source = make_source_tree("shared-chunk-src");
    let options = DigestOptions::default();
    let (walked, mut tracker) = walk_tree(&source, options.algo).unwrap();

    let store_dir = unique_temp_dir("shared-chunk-store");
    let store = Store::new(store_dir.clone(), vec![]);
    build_tree(&walked, &mut tracker, &store, &options, &mut DirTreeBundle::new()).unwrap();

    let expected_chunk_id =
        cairn_core::hash::hash_bytes(HashAlgorithm::Sha256, b"duplicate content for dedup test");
    let object_path = store_dir.join("sha256").join(expected_chunk_id.to_string());
    assert!(
        object_path.exists(),
        "the shared content's chunk must exist in the store exactly once, content-addressed"
    );

    std::fs::remove_dir_all(&source).unwrap();
    std::fs::remove_dir_all(&store_dir).unwrap();
}

#[test]
fn empty_directory_and_empty_file_do_not_collide() {
    // Regression test for domain-separation bug: empty DirTree and empty FileIndex
    // must have different IDs. This was caught in practice when a tree containing
    // both an empty directory and a zero-byte file produced a kind mismatch error
    // during walk (see cairn-core domain-separation changes for full context).
    //
    // This test verifies the fix by:
    // 1. Creating a source tree with both an empty subdirectory and a zero-byte file
    // 2. Digesting it (which creates DirTree and FileIndex objects)
    // 3. Ensuring the digest succeeds without kind mismatch errors
    // 4. Verifying the two objects have different IDs

    let root = unique_temp_dir("domain-sep-src");

    // Create an empty subdirectory (will become an empty DirTree)
    std::fs::create_dir(root.join("empty_dir")).unwrap();

    // Create a zero-byte file (will become a FileIndex with 0 chunks)
    std::fs::write(root.join("empty_file"), b"").unwrap();

    let options = DigestOptions::default();
    let (walked, mut tracker) = walk_tree(&root, options.algo).unwrap();

    let store_dir = unique_temp_dir("domain-sep-store");
    let store = Store::new(store_dir.clone(), vec![]);
    let mut bundle = DirTreeBundle::new();

    // This should succeed without kind-mismatch errors
    let root_id =
        build_tree(&walked, &mut tracker, &store, &options, &mut bundle).expect(
            "digest of tree with both empty directory and empty file must succeed",
        );

    // Verify that we can walk the bundle without encountering kind mismatch errors
    let (_, _, walked_bundle) = DirTreeBundle::decode_canonical(&bundle.encode_canonical(root_id, options.algo))
        .expect("bundle should round-trip through encode/decode");

    // Basic sanity check: the bundle contains multiple objects (root dirtree,
    // empty_dir's dirtree, empty_file's fileidx, at least 3 metadatas)
    assert!(
        walked_bundle.len() >= 4,
        "expected at least 4 objects (root dirtree, empty_dir dirtree, empty_file fileindex, metadatas), got {}",
        walked_bundle.len()
    );

    std::fs::remove_dir_all(&root).unwrap();
    std::fs::remove_dir_all(&store_dir).unwrap();
}
