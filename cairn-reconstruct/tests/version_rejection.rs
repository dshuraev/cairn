//! Integration test: unsupported bundle version rejection.

#![allow(clippy::unwrap_used)]

use cairn_core::encode::Encoder;
use cairn_core::hash::Hash;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn cli_rejects_unsupported_bundle_version() {
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

    // Create a minimal store (not needed since bundle has no objects, but required by CLI)
    let store_path = tmpdir_path.join("store");
    fs::create_dir(&store_path).expect("failed to create store");

    let out_path = tmpdir_path.join("output");

    // Find the cairn-reconstruct binary in the target directory.
    let mut binary_path = std::env::current_exe().expect("failed to get current exe path");
    binary_path.pop(); // Remove the test binary name (in deps/)
    binary_path.pop(); // Remove 'deps' directory
    binary_path.push("cairn-reconstruct");

    // Run cairn-reconstruct binary with the bad bundle
    let output = Command::new(&binary_path)
        .arg("--input")
        .arg(&bundle_path)
        .arg("--store")
        .arg(&store_path)
        .arg("--out")
        .arg(&out_path)
        .output()
        .expect("failed to run cairn-reconstruct");

    // Verify exit code is 1 (error)
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit code 1, got {:?}",
        output.status.code()
    );

    // Verify error message mentions unsupported version
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported bundle version") || stderr.contains("version"),
        "error message should mention unsupported version, stderr: {}",
        stderr
    );

    // Verify output directory was not created
    assert!(
        !out_path.exists(),
        "output directory should not be created on error"
    );
}
