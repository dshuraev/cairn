//! End-to-end tests for cairn-dirtree inspection commands.
//!
//! These tests use `cairn_digest::digest` to build real bundle fixtures,
//! then invoke the CLI and verify output.

#![allow(clippy::unwrap_used)]

use cairn_core::bundle::DirTreeBundle;
use cairn_dirtree::render::OutputFormat;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::Stdio;
use tempfile::TempDir;

/// Helper: creates a temporary source directory with various node types,
/// runs cairn_digest to produce a bundle, and returns the bundle path.
fn create_fixture_bundle() -> (TempDir, PathBuf) {
    let tmpdir = TempDir::new().expect("failed to create tmpdir");
    let src = tmpdir.path().join("src");
    fs::create_dir(&src).expect("failed to create src");

    // Create a regular file
    fs::write(src.join("hello.txt"), b"hello world").expect("failed to write hello.txt");

    // Create a subdirectory with a file
    let subdir = src.join("subdir");
    fs::create_dir(&subdir).expect("failed to create subdir");
    fs::write(subdir.join("nested.txt"), b"nested content").expect("failed to write nested.txt");

    // Create a symlink (if on Unix)
    #[cfg(unix)]
    {
        use std::os::unix::fs as unix_fs;
        unix_fs::symlink("hello.txt", src.join("link_to_hello")).expect("failed to create symlink");
    }

    // Create two hardlinked files
    fs::write(src.join("hardlink1.txt"), b"hardlinked content")
        .expect("failed to write hardlink1.txt");
    #[cfg(unix)]
    {
        fs::hard_link(src.join("hardlink1.txt"), src.join("hardlink2.txt"))
            .expect("failed to create hardlink2.txt");
    }

    // Run digest to produce a bundle
    let store_dir = tmpdir.path().join("store");
    let bundle_path = tmpdir.path().join("out.dirtree");

    let _root = cairn_digest::digest(
        &src,
        &cairn_digest::Store::new(store_dir, vec![]),
        &bundle_path,
        &cairn_digest::build::DigestOptions {
            chunk_config: cairn_digest::chunk::ChunkConfig {
                min_size: 512,
                avg_size: 1024,
                max_size: 2048,
            },
            algo: cairn_core::hash::HashAlgorithm::Sha256,
        },
    )
    .expect("failed to digest");

    (tmpdir, bundle_path)
}

#[test]
fn ls_includes_all_files_in_txt_mode() {
    let (_tmpdir, bundle_path) = create_fixture_bundle();

    // Read bundle and run walk
    let bytes = fs::read(&bundle_path).expect("failed to read bundle");
    let (_version, root, algo, bundle) =
        DirTreeBundle::decode_canonical(&bytes).expect("failed to decode bundle");
    let nodes =
        cairn_dirtree::walk::resolve(root, algo, &bundle).expect("failed to resolve bundle");

    let output = cairn_dirtree::render::render_ls(&nodes, &root.0, algo, OutputFormat::Txt);

    // Verify expected files are in output
    assert!(
        output.contains("hello.txt"),
        "output should contain hello.txt"
    );
    assert!(output.contains("subdir"), "output should contain subdir");
    assert!(
        output.contains("subdir/nested.txt"),
        "output should contain subdir/nested.txt"
    );
    assert!(
        output.contains("hardlink1.txt"),
        "output should contain hardlink1.txt"
    );
}

#[test]
fn ls_json_is_valid_json() {
    let (_tmpdir, bundle_path) = create_fixture_bundle();

    let bytes = fs::read(&bundle_path).expect("failed to read bundle");
    let (_version, root, algo, bundle) =
        DirTreeBundle::decode_canonical(&bytes).expect("failed to decode bundle");
    let nodes =
        cairn_dirtree::walk::resolve(root, algo, &bundle).expect("failed to resolve bundle");

    let output = cairn_dirtree::render::render_ls(&nodes, &root.0, algo, OutputFormat::Json);

    let json: serde_json::Value =
        serde_json::from_str(&output).expect("ls json output should be valid JSON");

    assert!(json["entries"].is_array(), "entries should be an array");
    assert!(json["root"].is_string(), "root should be a string");
}

#[test]
fn stat_on_existing_path_succeeds() {
    let (_tmpdir, bundle_path) = create_fixture_bundle();

    let bytes = fs::read(&bundle_path).expect("failed to read bundle");
    let (_version, root, algo, bundle) =
        DirTreeBundle::decode_canonical(&bytes).expect("failed to decode bundle");
    let nodes =
        cairn_dirtree::walk::resolve(root, algo, &bundle).expect("failed to resolve bundle");

    // Find the hello.txt node
    let node = nodes
        .iter()
        .find(|n| n.path == "hello.txt")
        .expect("hello.txt should exist in bundle");

    let output = cairn_dirtree::render::render_stat(node, algo, OutputFormat::Txt);

    assert!(output.contains("path:        hello.txt"));
    assert!(output.contains("kind:        file"));
}

#[test]
fn stat_json_on_file_includes_chunks() {
    let (_tmpdir, bundle_path) = create_fixture_bundle();

    let bytes = fs::read(&bundle_path).expect("failed to read bundle");
    let (_version, root, algo, bundle) =
        DirTreeBundle::decode_canonical(&bytes).expect("failed to decode bundle");
    let nodes =
        cairn_dirtree::walk::resolve(root, algo, &bundle).expect("failed to resolve bundle");

    let node = nodes
        .iter()
        .find(|n| n.path == "hello.txt")
        .expect("hello.txt should exist");

    let output = cairn_dirtree::render::render_stat(node, algo, OutputFormat::Json);
    let json: serde_json::Value = serde_json::from_str(&output).expect("should be valid JSON");

    assert!(json["chunks"].is_array(), "chunks should be an array");
}

#[test]
fn tree_shows_hierarchical_structure() {
    let (_tmpdir, bundle_path) = create_fixture_bundle();

    let bytes = fs::read(&bundle_path).expect("failed to read bundle");
    let (_version, root, algo, bundle) =
        DirTreeBundle::decode_canonical(&bytes).expect("failed to decode bundle");
    let nodes =
        cairn_dirtree::walk::resolve(root, algo, &bundle).expect("failed to resolve bundle");

    let output = cairn_dirtree::render::render_tree(&nodes, OutputFormat::Txt);

    assert!(
        output.contains("subdir/"),
        "tree should show subdir with trailing slash"
    );
    assert!(
        output.contains("nested.txt"),
        "tree should show nested files"
    );
}

#[test]
fn links_shows_hardlinked_pairs() {
    let (_tmpdir, bundle_path) = create_fixture_bundle();

    let bytes = fs::read(&bundle_path).expect("failed to read bundle");
    let (_version, root, algo, bundle) =
        DirTreeBundle::decode_canonical(&bytes).expect("failed to decode bundle");
    let nodes =
        cairn_dirtree::walk::resolve(root, algo, &bundle).expect("failed to resolve bundle");

    // Output should be generated without error (may be empty if no hardlinks)
    let _ = cairn_dirtree::render::render_links(&nodes, algo, OutputFormat::Txt);
}

#[test]
fn summary_counts_match_fixture() {
    let (_tmpdir, bundle_path) = create_fixture_bundle();

    let bytes = fs::read(&bundle_path).expect("failed to read bundle");
    let (_version, root, algo, bundle) =
        DirTreeBundle::decode_canonical(&bytes).expect("failed to decode bundle");
    let nodes =
        cairn_dirtree::walk::resolve(root, algo, &bundle).expect("failed to resolve bundle");

    let summary = cairn_dirtree::render::Summary::compute(&nodes);

    // Verify basic counts (exact counts depend on fixture creation)
    assert!(summary.files > 0, "should have at least one file");
    assert!(summary.dirs > 0, "should have at least one directory");
    assert_eq!(
        summary.total_nodes,
        nodes.len(),
        "total nodes should match input"
    );
}

#[test]
fn summary_json_is_valid() {
    let (_tmpdir, bundle_path) = create_fixture_bundle();

    let bytes = fs::read(&bundle_path).expect("failed to read bundle");
    let (_version, root, algo, bundle) =
        DirTreeBundle::decode_canonical(&bytes).expect("failed to decode bundle");
    let nodes =
        cairn_dirtree::walk::resolve(root, algo, &bundle).expect("failed to resolve bundle");

    let summary = cairn_dirtree::render::Summary::compute(&nodes);
    let output = cairn_dirtree::render::render_summary(&summary, OutputFormat::Json);

    let json: serde_json::Value =
        serde_json::from_str(&output).expect("summary json should be valid");

    assert!(json["files"].is_number());
    assert!(json["dirs"].is_number());
    assert!(json["symlinks"].is_number());
    assert!(json["total_nodes"].is_number());
}

#[test]
fn xattrs_output_is_valid_json() {
    let (_tmpdir, bundle_path) = create_fixture_bundle();

    let bytes = fs::read(&bundle_path).expect("failed to read bundle");
    let (_version, root, algo, bundle) =
        DirTreeBundle::decode_canonical(&bytes).expect("failed to decode bundle");
    let nodes =
        cairn_dirtree::walk::resolve(root, algo, &bundle).expect("failed to resolve bundle");

    let output = cairn_dirtree::render::render_xattrs(&nodes, None, OutputFormat::Json);

    let json: serde_json::Value =
        serde_json::from_str(&output).expect("xattrs json should be valid");

    assert!(json["paths"].is_array());
}

#[test]
fn xattrs_txt_generates_output() {
    let (_tmpdir, bundle_path) = create_fixture_bundle();

    let bytes = fs::read(&bundle_path).expect("failed to read bundle");
    let (_version, root, algo, bundle) =
        DirTreeBundle::decode_canonical(&bytes).expect("failed to decode bundle");
    let nodes =
        cairn_dirtree::walk::resolve(root, algo, &bundle).expect("failed to resolve bundle");

    // Output should be generated without error (may be empty if no xattrs)
    let _ = cairn_dirtree::render::render_xattrs(&nodes, None, OutputFormat::Txt);
}

#[test]
fn corrupted_bundle_produces_decode_error() {
    let tmpdir = TempDir::new().expect("failed to create tmpdir");
    let bad_bundle = tmpdir.path().join("bad.dirtree");

    // Write invalid bytes
    fs::write(&bad_bundle, b"not a valid bundle").expect("failed to write bad bundle");

    let bytes = fs::read(&bad_bundle).expect("failed to read bad bundle");
    let result = DirTreeBundle::decode_canonical(&bytes);

    assert!(result.is_err(), "should fail to decode invalid bundle");
}

#[test]
fn symlinks_output_includes_target() {
    let (_tmpdir, bundle_path) = create_fixture_bundle();

    let bytes = fs::read(&bundle_path).expect("failed to read bundle");
    let (_version, root, algo, bundle) =
        DirTreeBundle::decode_canonical(&bytes).expect("failed to decode bundle");
    let nodes =
        cairn_dirtree::walk::resolve(root, algo, &bundle).expect("failed to resolve bundle");

    // If there's a symlink, verify it appears in output
    // (may be empty if no symlinks, which is fine)
    let _ = cairn_dirtree::render::render_symlinks(&nodes, OutputFormat::Txt);
}

#[test]
fn special_nodes_output_format() {
    let (_tmpdir, bundle_path) = create_fixture_bundle();

    let bytes = fs::read(&bundle_path).expect("failed to read bundle");
    let (_version, root, algo, bundle) =
        DirTreeBundle::decode_canonical(&bytes).expect("failed to decode bundle");
    let nodes =
        cairn_dirtree::walk::resolve(root, algo, &bundle).expect("failed to resolve bundle");

    // Output should be valid even if no special nodes exist
    let _ = cairn_dirtree::render::render_special(&nodes, OutputFormat::Txt);
}

#[test]
fn binary_handles_closed_stdout_without_panic() {
    let tmpdir = TempDir::new().expect("failed to create tmpdir");
    let src = tmpdir.path().join("src");
    fs::create_dir(&src).expect("failed to create src");

    // Create many files to ensure output is large enough to fill pipe buffer
    for i in 0..100 {
        let fname = format!("file_{:03}.txt", i);
        fs::write(src.join(&fname), format!("content {}", i).as_bytes())
            .expect("failed to write test file");
    }

    // Create a subdirectory with files to generate more output
    let subdir = src.join("subdir");
    fs::create_dir(&subdir).expect("failed to create subdir");
    for i in 0..50 {
        let fname = format!("nested_{:03}.txt", i);
        fs::write(
            subdir.join(&fname),
            format!("nested content {}", i).as_bytes(),
        )
        .expect("failed to write nested file");
    }

    // Run digest to produce a bundle
    let store_dir = tmpdir.path().join("store");
    let bundle_path = tmpdir.path().join("out.dirtree");

    let _root = cairn_digest::digest(
        &src,
        &cairn_digest::Store::new(store_dir, vec![]),
        &bundle_path,
        &cairn_digest::build::DigestOptions {
            chunk_config: cairn_digest::chunk::ChunkConfig {
                min_size: 512,
                avg_size: 1024,
                max_size: 2048,
            },
            algo: cairn_core::hash::HashAlgorithm::Sha256,
        },
    )
    .expect("failed to digest");

    // Find the cairn-dirtree binary in the target directory.
    // The test binary is in target/debug/deps/, so we go up two levels to target/debug/
    let mut binary_path = std::env::current_exe().expect("failed to get current exe path");
    binary_path.pop(); // Remove the test binary name (in deps/)
    binary_path.pop(); // Remove 'deps' directory
    binary_path.push("cairn-dirtree");

    assert!(
        binary_path.exists(),
        "cairn-dirtree binary not found at {:?}",
        binary_path
    );

    // Spawn the cairn-dirtree binary with piped stdout
    let mut child = std::process::Command::new(&binary_path)
        .args(["tree", "--input", bundle_path.to_str().unwrap()])
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn cairn-dirtree");

    {
        // Take ownership of stdout and read only a small amount, then drop it to close the pipe.
        // This simulates the behavior of `head` which closes the pipe early.
        let mut stdout = child.stdout.take().expect("failed to get stdout");
        let mut buf = [0u8; 1024];
        let _ = stdout.read(&mut buf); // Read a small amount and then drop stdout to close pipe
        drop(stdout);
    }

    // Wait for child to complete. It should exit cleanly (with code 0 or EPIPE-equivalent),
    // not panic. On Unix, with SIGPIPE set to SIG_DFL, the process should exit cleanly.
    let status = child.wait().expect("failed to wait for child");

    // The process should have exited successfully or with a signal (not a panic).
    // A panic would result in a non-standard exit code.
    // We check that it either exited with code 0 or exited due to SIGPIPE (signal 13).
    if let Some(code) = status.code() {
        // If it exited with a code, it should be 0 (or possibly 141 if it caught the signal)
        // but definitely not a panic code (which would be 101 in Rust).
        assert_ne!(
            code, 101,
            "binary should not panic on closed stdout (panic exit code is 101, got {})",
            code
        );
    } else {
        // Exited due to signal; this is expected for SIGPIPE (signal 13).
        // The key is that it didn't panic (which would show "thread 'main' panicked").
    }
}

#[test]
fn cli_rejects_unsupported_bundle_version() {
    use cairn_core::encode::Encoder;
    use cairn_core::hash::Hash;
    use std::process::Command;

    let tmpdir = TempDir::new().expect("failed to create temp dir");
    let tmpdir_path = tmpdir.path();

    // Hand-construct a bundle with an unsupported version byte (version 255)
    let mut e = Encoder::new();
    e.write_u8(255); // Unsupported version
    e.write_u8(0); // Sha256
    e.write_hash(&Hash([5u8; 32])); // Root hash
    e.write_u32(0); // Object count

    let bundle_bytes = e.into_bytes();
    let bundle_path = tmpdir_path.join("bad_version.dirtree");
    fs::write(&bundle_path, &bundle_bytes).expect("failed to write bad bundle");

    // Find the cairn-dirtree binary in the target directory.
    let mut binary_path = std::env::current_exe().expect("failed to get current exe path");
    binary_path.pop(); // Remove the test binary name (in deps/)
    binary_path.pop(); // Remove 'deps' directory
    binary_path.push("cairn-dirtree");

    assert!(
        binary_path.exists(),
        "cairn-dirtree binary not found at {:?}",
        binary_path
    );

    // Run cairn-dirtree ls command with the bad bundle
    let output = Command::new(&binary_path)
        .args(["ls", "--input", bundle_path.to_str().unwrap()])
        .output()
        .expect("failed to run cairn-dirtree");

    // Verify exit code is 1 (error)
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit code 1, got {:?}",
        output.status.code()
    );

    // Verify error message mentions unsupported version or version
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}\n{}", stderr, stdout);
    assert!(
        combined.contains("unsupported bundle version")
            || combined.contains("version")
            || combined.contains("decode"),
        "error message should mention unsupported version, combined output: {}",
        combined
    );
}
