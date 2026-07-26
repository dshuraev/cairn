//! End-to-end tests for cairn-dirtree chunks command.
//!
//! These tests use `cairn_digest::digest` to build real bundle fixtures with
//! overlapping and disjoint chunk sets, then test the chunks command logic.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use cairn_core::bundle::DirTreeBundle;
use cairn_core::hash::HashAlgorithm;
use cairn_dirtree::chunks::{self, RequestedSets};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper: creates a temporary source directory with given content,
/// runs cairn_digest to produce a bundle, and returns the bundle path.
fn create_fixture_bundle(content: &[(&str, &[u8])], algo: HashAlgorithm) -> (TempDir, PathBuf) {
    let tmpdir = TempDir::new().expect("failed to create tmpdir");
    let src = tmpdir.path().join("src");
    fs::create_dir(&src).expect("failed to create src");

    for (filename, data) in content {
        fs::write(src.join(filename), data).expect("failed to write file");
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
            algo,
        },
    )
    .expect("failed to digest");

    (tmpdir, bundle_path)
}

#[test]
fn chunks_single_source_target_fully_overlapping() {
    let algo = HashAlgorithm::Sha256;
    let (_tmpdir_source, source_path) =
        create_fixture_bundle(&[("file.txt", b"hello world")], algo);
    let (_tmpdir_target, target_path) =
        create_fixture_bundle(&[("file.txt", b"hello world")], algo);

    let source_bytes = fs::read(&source_path).expect("failed to read source");
    let (_version_source, source_root, source_algo, source_bundle) =
        DirTreeBundle::decode_canonical(&source_bytes).expect("failed to decode source");

    let target_bytes = fs::read(&target_path).expect("failed to read target");
    let (_version_target, target_root, target_algo, target_bundle) =
        DirTreeBundle::decode_canonical(&target_bytes).expect("failed to decode target");

    let sources = vec![(source_root, source_algo, source_bundle, source_path.clone())];
    let target = (target_root, target_algo, target_bundle, target_path.clone());

    let result = chunks::compute(
        &sources,
        &target,
        RequestedSets {
            new: true,
            old: true,
            common: true,
        },
    )
    .expect("failed to compute chunks");

    // Fully overlapping: new and old should be empty, common should have chunks
    assert_eq!(result.new, Some(vec![]));
    assert_eq!(result.old, Some(vec![]));
    assert!(result.common.is_some());
    assert!(!result.common.as_ref().unwrap().is_empty());
}

#[test]
fn chunks_single_source_target_fully_disjoint() {
    let algo = HashAlgorithm::Sha256;
    let (_tmpdir_source, source_path) =
        create_fixture_bundle(&[("source.txt", b"source content")], algo);
    let (_tmpdir_target, target_path) =
        create_fixture_bundle(&[("target.txt", b"target content different")], algo);

    let source_bytes = fs::read(&source_path).expect("failed to read source");
    let (_version_source, source_root, source_algo, source_bundle) =
        DirTreeBundle::decode_canonical(&source_bytes).expect("failed to decode source");

    let target_bytes = fs::read(&target_path).expect("failed to read target");
    let (_version_target, target_root, target_algo, target_bundle) =
        DirTreeBundle::decode_canonical(&target_bytes).expect("failed to decode target");

    let sources = vec![(source_root, source_algo, source_bundle, source_path.clone())];
    let target = (target_root, target_algo, target_bundle, target_path.clone());

    let result = chunks::compute(
        &sources,
        &target,
        RequestedSets {
            new: true,
            old: true,
            common: true,
        },
    )
    .expect("failed to compute chunks");

    // Fully disjoint: new should have all target chunks, old should have all source chunks, common empty
    assert!(result.new.is_some());
    assert!(!result.new.as_ref().unwrap().is_empty());
    assert!(result.old.is_some());
    assert!(!result.old.as_ref().unwrap().is_empty());
    assert_eq!(result.common, Some(vec![]));
}

#[test]
fn chunks_partial_overlap() {
    let algo = HashAlgorithm::Sha256;
    // Create files that will deduplicate
    let shared_content = b"shared content here";
    let (_tmpdir_source, source_path) = create_fixture_bundle(
        &[
            ("shared.txt", shared_content),
            ("source_only.txt", b"source only"),
        ],
        algo,
    );
    let (_tmpdir_target, target_path) = create_fixture_bundle(
        &[
            ("shared.txt", shared_content),
            ("target_only.txt", b"target only"),
        ],
        algo,
    );

    let source_bytes = fs::read(&source_path).expect("failed to read source");
    let (_version_source, source_root, source_algo, source_bundle) =
        DirTreeBundle::decode_canonical(&source_bytes).expect("failed to decode source");

    let target_bytes = fs::read(&target_path).expect("failed to read target");
    let (_version_target, target_root, target_algo, target_bundle) =
        DirTreeBundle::decode_canonical(&target_bytes).expect("failed to decode target");

    let sources = vec![(source_root, source_algo, source_bundle, source_path.clone())];
    let target = (target_root, target_algo, target_bundle, target_path.clone());

    let result = chunks::compute(
        &sources,
        &target,
        RequestedSets {
            new: true,
            old: true,
            common: true,
        },
    )
    .expect("failed to compute chunks");

    // Partial overlap: all three sets should be non-empty
    assert!(result.new.is_some());
    assert!(!result.new.as_ref().unwrap().is_empty());
    assert!(result.old.is_some());
    assert!(!result.old.as_ref().unwrap().is_empty());
    assert!(result.common.is_some());
    assert!(!result.common.as_ref().unwrap().is_empty());
}

#[test]
fn chunks_multiple_sources_union() {
    let algo = HashAlgorithm::Sha256;
    let shared_content = b"shared";

    let (_tmpdir_source1, source1_path) =
        create_fixture_bundle(&[("s1.txt", shared_content)], algo);
    let (_tmpdir_source2, source2_path) =
        create_fixture_bundle(&[("s2.txt", shared_content)], algo);
    let (_tmpdir_target, target_path) = create_fixture_bundle(
        &[("s1.txt", shared_content), ("s2.txt", shared_content)],
        algo,
    );

    let source1_bytes = fs::read(&source1_path).expect("failed to read source1");
    let (_version_source1, source1_root, source1_algo, source1_bundle) =
        DirTreeBundle::decode_canonical(&source1_bytes).expect("failed to decode source1");

    let source2_bytes = fs::read(&source2_path).expect("failed to read source2");
    let (_version_source2, source2_root, source2_algo, source2_bundle) =
        DirTreeBundle::decode_canonical(&source2_bytes).expect("failed to decode source2");

    let target_bytes = fs::read(&target_path).expect("failed to read target");
    let (_version_target, target_root, target_algo, target_bundle) =
        DirTreeBundle::decode_canonical(&target_bytes).expect("failed to decode target");

    let sources = vec![
        (
            source1_root,
            source1_algo,
            source1_bundle,
            source1_path.clone(),
        ),
        (
            source2_root,
            source2_algo,
            source2_bundle,
            source2_path.clone(),
        ),
    ];
    let target = (target_root, target_algo, target_bundle, target_path.clone());

    let result = chunks::compute(
        &sources,
        &target,
        RequestedSets {
            new: true,
            old: true,
            common: true,
        },
    )
    .expect("failed to compute chunks");

    // Multiple sources union: new should be empty (sources cover everything), old empty, common non-empty
    assert_eq!(result.new, Some(vec![]));
    assert_eq!(result.old, Some(vec![]));
    assert!(result.common.is_some());
    assert!(!result.common.as_ref().unwrap().is_empty());
}

#[test]
fn chunks_only_new_requested() {
    let algo = HashAlgorithm::Sha256;
    let (_tmpdir_source, source_path) = create_fixture_bundle(&[("a.txt", b"aaa")], algo);
    let (_tmpdir_target, target_path) =
        create_fixture_bundle(&[("a.txt", b"aaa"), ("b.txt", b"bbb")], algo);

    let source_bytes = fs::read(&source_path).expect("failed to read source");
    let (_version_source, source_root, source_algo, source_bundle) =
        DirTreeBundle::decode_canonical(&source_bytes).expect("failed to decode source");

    let target_bytes = fs::read(&target_path).expect("failed to read target");
    let (_version_target, target_root, target_algo, target_bundle) =
        DirTreeBundle::decode_canonical(&target_bytes).expect("failed to decode target");

    let sources = vec![(source_root, source_algo, source_bundle, source_path.clone())];
    let target = (target_root, target_algo, target_bundle, target_path.clone());

    let result = chunks::compute(
        &sources,
        &target,
        RequestedSets {
            new: true,
            old: false,
            common: false,
        },
    )
    .expect("failed to compute chunks");

    // Only new requested
    assert!(result.new.is_some());
    assert!(!result.new.as_ref().unwrap().is_empty());
    assert_eq!(result.old, None);
    assert_eq!(result.common, None);
}

#[test]
fn chunks_no_flags_defaults_to_new() {
    let algo = HashAlgorithm::Sha256;
    let (_tmpdir_source, source_path) = create_fixture_bundle(&[("a.txt", b"aaa")], algo);
    let (_tmpdir_target, target_path) =
        create_fixture_bundle(&[("a.txt", b"aaa"), ("b.txt", b"bbb")], algo);

    let source_bytes = fs::read(&source_path).expect("failed to read source");
    let (_version_source, source_root, source_algo, source_bundle) =
        DirTreeBundle::decode_canonical(&source_bytes).expect("failed to decode source");

    let target_bytes = fs::read(&target_path).expect("failed to read target");
    let (_version_target, target_root, target_algo, target_bundle) =
        DirTreeBundle::decode_canonical(&target_bytes).expect("failed to decode target");

    let sources = vec![(source_root, source_algo, source_bundle, source_path.clone())];
    let target = (target_root, target_algo, target_bundle, target_path.clone());

    let result = chunks::compute(
        &sources,
        &target,
        RequestedSets {
            new: false,
            old: false,
            common: false,
        },
    )
    .expect("failed to compute chunks");

    // No flags: all None
    assert_eq!(result.new, None);
    assert_eq!(result.old, None);
    assert_eq!(result.common, None);
}

#[test]
fn chunks_algorithm_mismatch_sha256_vs_blake3() {
    let source_algo = HashAlgorithm::Sha256;
    let target_algo = HashAlgorithm::Blake3;

    let (_tmpdir_source, source_path) =
        create_fixture_bundle(&[("file.txt", b"content")], source_algo);
    let (_tmpdir_target, target_path) =
        create_fixture_bundle(&[("file.txt", b"content")], target_algo);

    let source_bytes = fs::read(&source_path).expect("failed to read source");
    let (_version_source, source_root, _, source_bundle) =
        DirTreeBundle::decode_canonical(&source_bytes).expect("failed to decode source");

    let target_bytes = fs::read(&target_path).expect("failed to read target");
    let (_version_target, target_root, _, target_bundle) =
        DirTreeBundle::decode_canonical(&target_bytes).expect("failed to decode target");

    let sources = vec![(source_root, source_algo, source_bundle, source_path.clone())];
    let target = (target_root, target_algo, target_bundle, target_path.clone());

    let result = chunks::compute(
        &sources,
        &target,
        RequestedSets {
            new: true,
            old: false,
            common: false,
        },
    );

    assert!(matches!(
        result,
        Err(cairn_dirtree::chunks::ChunksError::AlgorithmMismatch {
            expected: HashAlgorithm::Blake3,
            found: HashAlgorithm::Sha256,
            ..
        })
    ));
}

#[test]
fn chunks_all_algorithms_matching_sha256() {
    let algo = HashAlgorithm::Sha256;
    let (_tmpdir_source, source_path) = create_fixture_bundle(&[("file.txt", b"content")], algo);
    let (_tmpdir_target, target_path) = create_fixture_bundle(&[("file.txt", b"content")], algo);

    let source_bytes = fs::read(&source_path).expect("failed to read source");
    let (_version_source, source_root, source_algo, source_bundle) =
        DirTreeBundle::decode_canonical(&source_bytes).expect("failed to decode source");

    let target_bytes = fs::read(&target_path).expect("failed to read target");
    let (_version_target, target_root, target_algo, target_bundle) =
        DirTreeBundle::decode_canonical(&target_bytes).expect("failed to decode target");

    let sources = vec![(source_root, source_algo, source_bundle, source_path.clone())];
    let target = (target_root, target_algo, target_bundle, target_path.clone());

    let result = chunks::compute(
        &sources,
        &target,
        RequestedSets {
            new: true,
            old: true,
            common: true,
        },
    );

    assert!(result.is_ok());
}

#[test]
fn chunks_all_algorithms_matching_blake3() {
    let algo = HashAlgorithm::Blake3;
    let (_tmpdir_source, source_path) = create_fixture_bundle(&[("file.txt", b"content")], algo);
    let (_tmpdir_target, target_path) = create_fixture_bundle(&[("file.txt", b"content")], algo);

    let source_bytes = fs::read(&source_path).expect("failed to read source");
    let (_version_source, source_root, source_algo, source_bundle) =
        DirTreeBundle::decode_canonical(&source_bytes).expect("failed to decode source");

    let target_bytes = fs::read(&target_path).expect("failed to read target");
    let (_version_target, target_root, target_algo, target_bundle) =
        DirTreeBundle::decode_canonical(&target_bytes).expect("failed to decode target");

    let sources = vec![(source_root, source_algo, source_bundle, source_path.clone())];
    let target = (target_root, target_algo, target_bundle, target_path.clone());

    let result = chunks::compute(
        &sources,
        &target,
        RequestedSets {
            new: true,
            old: true,
            common: true,
        },
    );

    assert!(result.is_ok());
}
