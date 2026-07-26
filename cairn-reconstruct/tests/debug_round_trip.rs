use cairn_core::bundle::DirTreeBundle;
use cairn_core::hash::HashAlgorithm;
use cairn_digest::{digest, DigestOptions};
use cairn_reconstruct::{materialize, MaterializeOptions};
use cairn_store::Store;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use tempfile::TempDir;

#[test]
fn debug_round_trip() {
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
    let symlink_result =
        std::os::unix::fs::symlink("regular_file.txt", src_dir.join("link_to_file"));
    eprintln!("Symlink creation: {:?}", symlink_result);

    // Hardlinked pair
    fs::write(src_dir.join("hardlink1"), b"shared content").expect("write hardlink1");
    fs::hard_link(src_dir.join("hardlink1"), src_dir.join("hardlink2")).expect("create hardlink2");

    if symlink_result.is_ok() {
        let meta = fs::symlink_metadata(src_dir.join("link_to_file")).unwrap();
        eprintln!("Symlink mode: 0o{:o}", meta.mode());
        eprintln!("Symlink uid: {}", meta.uid());
        eprintln!("Symlink gid: {}", meta.gid());
    }

    // First digest
    let store_dir1 = tmpdir_path.join("store1");
    let bundle_file1 = tmpdir_path.join("bundle1.dirtree");
    let store1 = Store::new(store_dir1.clone(), vec![]);

    let options = DigestOptions {
        algo: HashAlgorithm::Sha256,
        ..Default::default()
    };

    let root_id1 = digest(&src_dir, &store1, &bundle_file1, &options).expect("first digest failed");
    eprintln!("\n=== FIRST DIGEST ===");
    eprintln!("Root ID: {:?}", root_id1);

    // Read and inspect the first bundle
    let bundle_bytes1 = fs::read(&bundle_file1).expect("read bundle1");
    let (_version1, _root_from_bundle1, _algo1, bundle1) =
        DirTreeBundle::decode_canonical(&bundle_bytes1).expect("decode bundle1");

    eprintln!("Bundle1 size: {} bytes", bundle_bytes1.len());
    eprintln!("Bundle1 objects: {}", bundle1.len());

    // Reconstruct
    let reconstruct_dir = tmpdir_path.join("reconstruct");
    let options_recon = MaterializeOptions { no_root: false };

    let report = materialize(
        &bundle1,
        root_id1,
        HashAlgorithm::Sha256,
        &store1,
        &reconstruct_dir,
        &options_recon,
    )
    .expect("reconstruction failed");

    eprintln!("\n=== RECONSTRUCTION ===");
    if symlink_result.is_ok() {
        let meta = fs::symlink_metadata(reconstruct_dir.join("link_to_file")).unwrap();
        eprintln!("Reconstructed symlink mode: 0o{:o}", meta.mode());
        eprintln!("Reconstructed symlink uid: {}", meta.uid());
        eprintln!("Reconstructed symlink gid: {}", meta.gid());
    }

    if !report.skips.is_empty() {
        eprintln!("Skips: {:?}", report.skips);
    }

    // Second digest
    let store_dir2 = tmpdir_path.join("store2");
    let bundle_file2 = tmpdir_path.join("bundle2.dirtree");
    let store2 = Store::new(store_dir2.clone(), vec![]);

    let root_id2 =
        digest(&reconstruct_dir, &store2, &bundle_file2, &options).expect("second digest failed");

    eprintln!("\n=== SECOND DIGEST ===");
    eprintln!("Root ID: {:?}", root_id2);

    // Read and inspect the second bundle
    let bundle_bytes2 = fs::read(&bundle_file2).expect("read bundle2");
    let (_version2, _root_from_bundle2, _algo2, bundle2) =
        DirTreeBundle::decode_canonical(&bundle_bytes2).expect("decode bundle2");

    eprintln!("Bundle2 size: {} bytes", bundle_bytes2.len());
    eprintln!("Bundle2 objects: {}", bundle2.len());

    eprintln!("\n=== COMPARISON ===");
    eprintln!("Root IDs match: {}", root_id1 == root_id2);
    eprintln!(
        "Bundle sizes match: {} == {} = {}",
        bundle_bytes1.len(),
        bundle_bytes2.len(),
        bundle_bytes1.len() == bundle_bytes2.len()
    );

    // Simple function to list directory contents
    fn list_dir(path: &std::path::Path) -> Vec<(String, u32)> {
        let mut entries = Vec::new();
        if let Ok(dir_entries) = fs::read_dir(path) {
            for entry in dir_entries {
                if let Ok(entry) = entry {
                    let name = entry.file_name();
                    if let Ok(meta) = fs::symlink_metadata(entry.path()) {
                        let mode = meta.mode();
                        entries.push((name.to_string_lossy().into_owned(), mode));
                    }
                }
            }
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }

    eprintln!("\n=== DIRECTORY STRUCTURES ===");
    eprintln!("Source dir files:");
    for (name, mode) in list_dir(&src_dir) {
        eprintln!("  {} mode=0o{:o}", name, mode);
    }

    eprintln!("Reconstructed dir files:");
    for (name, mode) in list_dir(&reconstruct_dir) {
        eprintln!("  {} mode=0o{:o}", name, mode);
    }

    // Check inodes of hardlinks
    eprintln!("\n=== HARDLINK CHECK ===");
    let mut inode_map: HashMap<u64, Vec<String>> = HashMap::new();

    fn collect_inodes(
        path: &std::path::Path,
        prefix: &str,
        inode_map: &mut HashMap<u64, Vec<String>>,
    ) {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let entry_path = entry.path();
                    if let Ok(meta) = fs::symlink_metadata(&entry_path) {
                        let full_name = if prefix.is_empty() {
                            name.clone()
                        } else {
                            format!("{}/{}", prefix, name)
                        };
                        inode_map
                            .entry(meta.ino())
                            .or_insert_with(Vec::new)
                            .push(full_name.clone());
                        if meta.is_dir() {
                            collect_inodes(&entry_path, &full_name, inode_map);
                        }
                    }
                }
            }
        }
    }

    eprintln!("Source directory inode groups:");
    let mut src_inodes = HashMap::new();
    collect_inodes(&src_dir, "", &mut src_inodes);
    for (ino, names) in src_inodes.iter() {
        if names.len() > 1 {
            eprintln!("  Inode {}: {:?}", ino, names);
        }
    }

    eprintln!("Reconstructed directory inode groups:");
    let mut recon_inodes = HashMap::new();
    collect_inodes(&reconstruct_dir, "", &mut recon_inodes);
    for (ino, names) in recon_inodes.iter() {
        if names.len() > 1 {
            eprintln!("  Inode {}: {:?}", ino, names);
        }
    }

    // Check if hardlinks are properly linked
    let hardlink1_src = fs::symlink_metadata(src_dir.join("hardlink1")).ok();
    let hardlink2_src = fs::symlink_metadata(src_dir.join("hardlink2")).ok();
    let hardlink1_recon = fs::symlink_metadata(reconstruct_dir.join("hardlink1")).ok();
    let hardlink2_recon = fs::symlink_metadata(reconstruct_dir.join("hardlink2")).ok();

    if let (Some(h1s), Some(h2s)) = (hardlink1_src, hardlink2_src) {
        eprintln!(
            "Source hardlink1 inode: {}, hardlink2 inode: {} (share: {})",
            h1s.ino(),
            h2s.ino(),
            h1s.ino() == h2s.ino()
        );
    }

    if let (Some(h1r), Some(h2r)) = (hardlink1_recon, hardlink2_recon) {
        eprintln!(
            "Reconstructed hardlink1 inode: {}, hardlink2 inode: {} (share: {})",
            h1r.ino(),
            h2r.ino(),
            h1r.ino() == h2r.ino()
        );
    }

    // Compare detailed metadata by listing files recursively and checking mode/uid/gid
    eprintln!("\n=== DETAILED FILE COMPARISON ===");

    fn get_file_metadata(path: &std::path::Path) -> Vec<(String, u32, u32, u32)> {
        let mut files = Vec::new();
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let path = entry.path();
                    if let Ok(meta) = fs::symlink_metadata(&path) {
                        files.push((name.clone(), meta.mode(), meta.uid(), meta.gid()));

                        // Recurse into directories
                        if meta.is_dir() {
                            if let Ok(subentries) = fs::read_dir(&path) {
                                for subentry in subentries {
                                    if let Ok(subentry) = subentry {
                                        let subname =
                                            subentry.file_name().to_string_lossy().into_owned();
                                        let subpath = subentry.path();
                                        if let Ok(submeta) = fs::symlink_metadata(&subpath) {
                                            let full_name = format!("{}/{}", name, subname);
                                            files.push((
                                                full_name,
                                                submeta.mode(),
                                                submeta.uid(),
                                                submeta.gid(),
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        files
    }

    let src_files = get_file_metadata(&src_dir);
    let recon_files = get_file_metadata(&reconstruct_dir);

    eprintln!("Source files:");
    for (name, mode, uid, gid) in &src_files {
        eprintln!("  {} mode=0o{:o} uid={} gid={}", name, mode, uid, gid);
    }

    eprintln!("Reconstructed files:");
    for (name, mode, uid, gid) in &recon_files {
        eprintln!("  {} mode=0o{:o} uid={} gid={}", name, mode, uid, gid);
    }

    eprintln!("Comparison:");
    let mut any_diff = false;
    for i in 0..src_files.len().max(recon_files.len()) {
        let src = src_files.get(i);
        let recon = recon_files.get(i);

        if src != recon {
            any_diff = true;
            eprintln!("  DIFF at index {}:", i);
            if let Some((name, mode, uid, gid)) = src {
                eprintln!(
                    "    SRC:  {} mode=0o{:o} uid={} gid={}",
                    name, mode, uid, gid
                );
            } else {
                eprintln!("    SRC:  (none)");
            }
            if let Some((name, mode, uid, gid)) = recon {
                eprintln!(
                    "    RECON: {} mode=0o{:o} uid={} gid={}",
                    name, mode, uid, gid
                );
            } else {
                eprintln!("    RECON: (none)");
            }
        }
    }

    if !any_diff {
        eprintln!("  No differences!");
    }
}
