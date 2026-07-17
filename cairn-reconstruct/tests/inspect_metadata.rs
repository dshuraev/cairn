use cairn_core::bundle::DirTreeBundle;
use cairn_core::hash::HashAlgorithm;
use cairn_core::model::Metadata;
use cairn_digest::{digest, DigestOptions};
use cairn_reconstruct::{materialize, MaterializeOptions};
use cairn_store::Store;
use std::fs;
use std::os::unix::fs::MetadataExt;
use tempfile::TempDir;

#[test]
fn inspect_metadata_objects() {
    let tmpdir = TempDir::new().expect("failed to create temp dir");
    let tmpdir_path = tmpdir.path();

    // Create a source directory with various file types
    let src_dir = tmpdir_path.join("src");
    fs::create_dir(&src_dir).expect("failed to create src dir");

    // Regular file
    fs::write(src_dir.join("regular_file.txt"), b"hello world").expect("write file1");

    // Large file (should span multiple FastCDC chunks)
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

    // First digest
    let store_dir1 = tmpdir_path.join("store1");
    let bundle_file1 = tmpdir_path.join("bundle1.dirtree");
    let store1 = Store::new(store_dir1.clone(), vec![]);

    let options = DigestOptions {
        algo: HashAlgorithm::Sha256,
        ..Default::default()
    };

    let root_id1 = digest(&src_dir, &store1, &bundle_file1, &options)
        .expect("first digest failed");
    eprintln!("First digest root ID: {:?}", root_id1);

    // Read and inspect the first bundle
    let bundle_bytes1 = fs::read(&bundle_file1).expect("read bundle1");
    let (_root_from_bundle1, _algo1, bundle1) =
        DirTreeBundle::decode_canonical(&bundle_bytes1).expect("decode bundle1");

    // Collect all metadata from bundle1
    let mut bundle1_metadata = Vec::new();
    for (_hash, (_kind, bytes)) in bundle1_bytes_iter(&bundle_bytes1) {
        if let Ok(meta) = Metadata::decode_canonical(bytes) {
            bundle1_metadata.push((meta.clone(), hex_digest(&bytes)));
        }
    }

    eprintln!("\nBundle1 metadata objects ({}):", bundle1_metadata.len());
    for (meta, hash_hex) in &bundle1_metadata {
        eprintln!("  [{}] mode=0o{:o} uid={} gid={} xattrs={}",
            hash_hex, meta.mode(), meta.uid(), meta.gid(), meta.xattrs().len());
    }

    // Reconstruct
    let reconstruct_dir = tmpdir_path.join("reconstruct");
    let options_recon = MaterializeOptions { no_root: false };

    let _report = materialize(
        &bundle1,
        root_id1,
        HashAlgorithm::Sha256,
        &store1,
        &reconstruct_dir,
        &options_recon,
    )
    .expect("reconstruction failed");

    // Second digest
    let store_dir2 = tmpdir_path.join("store2");
    let bundle_file2 = tmpdir_path.join("bundle2.dirtree");
    let store2 = Store::new(store_dir2.clone(), vec![]);

    let root_id2 = digest(&reconstruct_dir, &store2, &bundle_file2, &options)
        .expect("second digest failed");

    eprintln!("\nSecond digest root ID: {:?}", root_id2);

    // Read and inspect the second bundle
    let bundle_bytes2 = fs::read(&bundle_file2).expect("read bundle2");
    let (_root_from_bundle2, _algo2, bundle2) =
        DirTreeBundle::decode_canonical(&bundle_bytes2).expect("decode bundle2");

    // Collect all metadata from bundle2
    let mut bundle2_metadata = Vec::new();
    for (_hash, (_kind, bytes)) in bundle2_bytes_iter(&bundle_bytes2) {
        if let Ok(meta) = Metadata::decode_canonical(bytes) {
            bundle2_metadata.push((meta.clone(), hex_digest(&bytes)));
        }
    }

    eprintln!("\nBundle2 metadata objects ({}):", bundle2_metadata.len());
    for (meta, hash_hex) in &bundle2_metadata {
        eprintln!("  [{}] mode=0o{:o} uid={} gid={} xattrs={}",
            hash_hex, meta.mode(), meta.uid(), meta.gid(), meta.xattrs().len());
    }

    eprintln!("\nComparison:");
    if bundle1_metadata.len() != bundle2_metadata.len() {
        eprintln!("  Different number of metadata objects: {} vs {}", bundle1_metadata.len(), bundle2_metadata.len());
    } else {
        for i in 0..bundle1_metadata.len() {
            let (meta1, hash1) = &bundle1_metadata[i];
            let (meta2, hash2) = &bundle2_metadata[i];
            if meta1 != meta2 {
                eprintln!("  Metadata #{} differs:", i);
                eprintln!("    Bundle1: mode=0o{:o} uid={} gid={}", meta1.mode(), meta1.uid(), meta1.gid());
                eprintln!("    Bundle2: mode=0o{:o} uid={} gid={}", meta2.mode(), meta2.uid(), meta2.gid());
            } else if hash1 != hash2 {
                eprintln!("  Metadata #{} has different hash (encoding differs):", i);
            }
        }
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    for &b in bytes.iter().take(16) {
        let _ = write!(s, "{:02x}", b);
    }
    s
}

// Helper to iterate over bundle bytes
fn bundle1_bytes_iter(bytes: &[u8]) -> Vec<([u8; 32], (u8, &[u8]))> {
    vec![]
}

fn bundle2_bytes_iter(bytes: &[u8]) -> Vec<([u8; 32], (u8, &[u8]))> {
    vec![]
}
