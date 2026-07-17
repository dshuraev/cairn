use cairn_core::hash::HashAlgorithm;
use cairn_digest::{digest, DigestOptions};
use cairn_store::Store;
use std::fs;
use tempfile::TempDir;

#[test]
fn double_digest_same_source() {
    let tmpdir = TempDir::new().expect("failed to create temp dir");
    let tmpdir_path = tmpdir.path();

    // Create a source directory with all the elements from the failing test
    let src_dir = tmpdir_path.join("src");
    fs::create_dir(&src_dir).expect("failed to create src dir");

    // Regular file
    fs::write(src_dir.join("regular_file.txt"), b"hello world").expect("write file1");

    // Large file
    let large_content = vec![42u8; 100_000];
    fs::write(src_dir.join("large_file.bin"), &large_content).expect("write large file");

    // Subdirectory
    fs::create_dir(src_dir.join("subdir")).expect("create subdir");
    fs::write(src_dir.join("subdir/nested.txt"), b"nested content").expect("write nested");

    // Symlink
    let _ = std::os::unix::fs::symlink("regular_file.txt", src_dir.join("link_to_file"));

    // Hardlinked pair
    fs::write(src_dir.join("hardlink1"), b"shared content").expect("write hardlink1");
    fs::hard_link(src_dir.join("hardlink1"), src_dir.join("hardlink2"))
        .expect("create hardlink2");

    // First digest of the source
    let store_dir1 = tmpdir_path.join("store1");
    let bundle_file1 = tmpdir_path.join("bundle1.dirtree");
    let store1 = Store::new(store_dir1.clone(), vec![]);

    let options = DigestOptions {
        algo: HashAlgorithm::Sha256,
        ..Default::default()
    };

    let root_id1 = digest(&src_dir, &store1, &bundle_file1, &options)
        .expect("first digest failed");
    eprintln!("First digest of source: {:?}", root_id1);

    // Second digest of the SAME source (no reconstruction)
    let store_dir2 = tmpdir_path.join("store2");
    let bundle_file2 = tmpdir_path.join("bundle2.dirtree");
    let store2 = Store::new(store_dir2.clone(), vec![]);

    let root_id2 = digest(&src_dir, &store2, &bundle_file2, &options)
        .expect("second digest failed");

    eprintln!("Second digest of source: {:?}", root_id2);
    eprintln!("Do they match? {}", root_id1 == root_id2);

    assert_eq!(root_id1, root_id2, "Two digests of the same source should match");
}
