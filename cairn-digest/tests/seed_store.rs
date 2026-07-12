use cairn_core::hash::{hash_bytes, HashAlgorithm};
use cairn_core::bundle::DirTreeBundle;
use cairn_digest::build::{build_tree, DigestOptions};
use cairn_digest::store::Store;
use cairn_digest::walk::walk_tree;
use std::path::PathBuf;

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("cairn-digest-seed-test-{label}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const SHARED_CONTENT: &[u8] = b"content shared across two independently-built sources";

#[test]
fn seed_store_dedup_avoids_writing_content_already_present_in_the_seed() {
    let options = DigestOptions::default();

    // Build source A into store A, containing a chunk C.
    let source_a = unique_temp_dir("source-a");
    std::fs::write(source_a.join("f.txt"), SHARED_CONTENT).unwrap();
    let store_a_dir = unique_temp_dir("store-a");
    let store_a = Store::new(store_a_dir.clone(), vec![]);
    let (walked_a, tracker_a) = walk_tree(&source_a, options.algo).unwrap();
    build_tree(&walked_a, &tracker_a, &store_a, &options, &mut DirTreeBundle::new()).unwrap();

    let chunk_id = hash_bytes(HashAlgorithm::Sha256, SHARED_CONTENT);
    assert!(
        store_a_dir.join(chunk_id.to_string()).exists(),
        "store A must contain the shared chunk after building source A"
    );

    // Build a different source B, which happens to share content C, into a
    // fresh empty store B seeded with store A.
    let source_b = unique_temp_dir("source-b");
    std::fs::write(source_b.join("different_name.txt"), SHARED_CONTENT).unwrap();
    let store_b_dir = unique_temp_dir("store-b");
    let store_b = Store::new(store_b_dir.clone(), vec![store_a_dir.clone()]);
    let (walked_b, tracker_b) = walk_tree(&source_b, options.algo).unwrap();
    build_tree(&walked_b, &tracker_b, &store_b, &options, &mut DirTreeBundle::new()).unwrap();

    // The seed lookup must have found chunk C in store A and skipped writing
    // it into store B (§6.1) -- it should exist only in A, never in B.
    assert!(store_a_dir.join(chunk_id.to_string()).exists());
    assert!(
        !store_b_dir.join(chunk_id.to_string()).exists(),
        "chunk already present in the seed store must not be duplicated into the primary store"
    );

    std::fs::remove_dir_all(&source_a).unwrap();
    std::fs::remove_dir_all(&source_b).unwrap();
    std::fs::remove_dir_all(&store_a_dir).unwrap();
    std::fs::remove_dir_all(&store_b_dir).unwrap();
}
