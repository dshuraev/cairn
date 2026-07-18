//! Compute chunk set differences between source and target dirtree bundles.
//!
//! This module provides pure content-addressed reachability computation over
//! DirTree/FileIndex objects, computing set differences (new/old/common) of
//! ChunkIDs reachable from source vs. target bundles. Never touches chunk bytes
//! or a chunk store.

use cairn_core::bundle::DirTreeBundle;
use cairn_core::hash::HashAlgorithm;
use cairn_core::id::{ChunkID, DirTreeID};
use std::collections::HashSet;
use std::path::PathBuf;

/// Errors that can occur during chunk computation.
#[derive(Debug)]
pub enum ChunksError {
    /// Algorithm mismatch between bundles.
    AlgorithmMismatch {
        expected: HashAlgorithm,
        found: HashAlgorithm,
        source: PathBuf,
    },

    /// Walk-level error (missing object, kind mismatch, etc.).
    Walk(crate::walk::WalkError),
}

impl std::fmt::Display for ChunksError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChunksError::AlgorithmMismatch {
                expected,
                found,
                source,
            } => {
                write!(
                    f,
                    "algorithm mismatch: expected {:?} (from target), found {:?} in {}",
                    expected, found, source.display()
                )
            }
            ChunksError::Walk(err) => write!(f, "{}", err),
        }
    }
}

impl std::error::Error for ChunksError {}

impl From<crate::walk::WalkError> for ChunksError {
    fn from(err: crate::walk::WalkError) -> Self {
        ChunksError::Walk(err)
    }
}

/// Returns the raw 32-byte digest of a ChunkID (extracts Hash::0 from ChunkID::0).
fn chunk_id_bytes(id: &ChunkID) -> &[u8; 32] {
    &id.0.0
}

/// Requested chunk sets to compute.
#[derive(Debug, Clone)]
pub struct RequestedSets {
    /// Include new chunks (in target but not in any source).
    pub new: bool,
    /// Include old chunks (in sources but not in target).
    pub old: bool,
    /// Include common chunks (in both sources and target).
    pub common: bool,
}

/// Result of chunk set computation.
#[derive(Debug, Clone)]
pub struct ChunksResult {
    /// Chunks in target but not in any source (new).
    /// None if not requested; Some(vec![]) if requested but empty.
    pub new: Option<Vec<ChunkID>>,

    /// Chunks in some source but not in target (old).
    /// None if not requested; Some(vec![]) if requested but empty.
    pub old: Option<Vec<ChunkID>>,

    /// Chunks in both sources and target (common).
    /// None if not requested; Some(vec![]) if requested but empty.
    pub common: Option<Vec<ChunkID>>,
}

/// Compute chunk set differences between sources and target.
///
/// # Algorithm
///
/// 1. Establish target's `HashAlgorithm` as the reference.
/// 2. Check each source's algorithm matches the target's; return error on mismatch.
/// 3. Resolve `target_ids` via `reachable_chunks` over target.
/// 4. Resolve `source_ids` as the union of `reachable_chunks` over each source.
/// 5. Compute set differences: new = target - sources, old = sources - target, common = target ∩ sources.
/// 6. Sort each populated set and return only requested subsets.
///
/// # Errors
///
/// Returns `ChunksError::AlgorithmMismatch` if any source's algorithm differs from
/// the target's, or `ChunksError::Walk` if any referenced object is missing from
/// a bundle or has a kind tag mismatch.
pub fn compute(
    sources: &[(DirTreeID, HashAlgorithm, DirTreeBundle, PathBuf)],
    target: &(DirTreeID, HashAlgorithm, DirTreeBundle, PathBuf),
    want: RequestedSets,
) -> Result<ChunksResult, ChunksError> {
    // Helper: sort and collect ChunkIDs by their raw 32-byte hash digest.
    fn sorted_chunk_ids(ids: impl Iterator<Item = ChunkID>) -> Vec<ChunkID> {
        let mut v: Vec<ChunkID> = ids.collect();
        v.sort_unstable_by(|a, b| chunk_id_bytes(&a).cmp(chunk_id_bytes(&b)));
        v
    }

    let target_algo = target.1;

    // Check algorithm mismatch before any resolution
    for (_, source_algo, _, source_path) in sources {
        if *source_algo != target_algo {
            return Err(ChunksError::AlgorithmMismatch {
                expected: target_algo,
                found: *source_algo,
                source: source_path.clone(),
            });
        }
    }

    // Resolve target chunks
    let target_ids = crate::walk::reachable_chunks(
        target.0,
        target.1,
        &target.2,
    )?;

    // Resolve source chunks (union)
    let mut source_ids = HashSet::new();
    for (source_root, source_algo, bundle, _) in sources {
        let reachable = crate::walk::reachable_chunks(*source_root, *source_algo, bundle)?;
        source_ids.extend(reachable);
    }

    // Compute set differences
    let new_set = want.new.then(|| sorted_chunk_ids(target_ids.difference(&source_ids).copied()));

    let old_set = want.old.then(|| sorted_chunk_ids(source_ids.difference(&target_ids).copied()));

    let common_set = want.common.then(|| sorted_chunk_ids(target_ids.intersection(&source_ids).copied()));

    Ok(ChunksResult {
        new: new_set,
        old: old_set,
        common: common_set,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use cairn_core::bundle::ObjectKind;
    use cairn_core::hash::hash_bytes;
    use cairn_core::id::MetadataID;
    use cairn_core::model::{DirTree, FileIndex, Metadata, Node, NodeKind};

    fn make_metadata(mode: u32, uid: u32, gid: u32) -> Metadata {
        Metadata::new(mode, uid, gid, vec![])
    }

    fn metadata_id(mode: u32, uid: u32, gid: u32, algo: HashAlgorithm) -> MetadataID {
        make_metadata(mode, uid, gid).id(algo)
    }

    fn build_bundle_with_chunks(
        algo: HashAlgorithm,
        chunks: &[&[u8]],
    ) -> (DirTreeID, DirTreeBundle) {
        let mut bundle = DirTreeBundle::new();

        // Create FileIndex with specified chunks
        let chunk_ids: Vec<ChunkID> = chunks
            .iter()
            .map(|c| ChunkID(hash_bytes(algo, c)))
            .collect();
        let file_index = FileIndex::new(chunk_ids);
        let file_index_id = file_index.id(algo);

        let file_meta = make_metadata(0o644, 1000, 1000);
        let file_meta_id = file_meta.id(algo);

        let file_node = Node::new(
            "file.txt",
            file_meta_id,
            None,
            NodeKind::File { file_index_id },
        );

        let dirtree = DirTree::new(vec![file_node]);
        let root_id = dirtree.id(algo);

        bundle.insert(ObjectKind::DirTree, root_id.0, dirtree.encode_canonical());
        bundle.insert(ObjectKind::Metadata, file_meta_id.0, file_meta.encode_canonical());
        bundle.insert(
            ObjectKind::FileIndex,
            file_index_id.0,
            file_index.encode_canonical(),
        );

        (root_id, bundle)
    }

    #[test]
    fn single_source_target_fully_overlapping() {
        let algo = HashAlgorithm::Sha256;
        let (target_root, target_bundle) = build_bundle_with_chunks(algo, &[b"chunk1", b"chunk2"]);
        let (source_root, source_bundle) =
            build_bundle_with_chunks(algo, &[b"chunk1", b"chunk2"]);

        let sources = vec![(
            source_root,
            algo,
            source_bundle,
            PathBuf::from("/tmp/source"),
        )];
        let target = (target_root, algo, target_bundle, PathBuf::from("/tmp/target"));

        let result = compute(
            &sources,
            &target,
            RequestedSets {
                new: true,
                old: true,
                common: true,
            },
        )
        .unwrap();

        assert_eq!(result.new, Some(vec![]));
        assert_eq!(result.common.as_ref().map(|v| v.len()), Some(2));
        assert_eq!(result.old, Some(vec![]));
    }

    #[test]
    fn single_source_target_fully_disjoint() {
        let algo = HashAlgorithm::Sha256;
        let (target_root, target_bundle) = build_bundle_with_chunks(algo, &[b"chunk1", b"chunk2"]);
        let (source_root, source_bundle) = build_bundle_with_chunks(algo, &[b"chunk3", b"chunk4"]);

        let sources = vec![(
            source_root,
            algo,
            source_bundle,
            PathBuf::from("/tmp/source"),
        )];
        let target = (target_root, algo, target_bundle, PathBuf::from("/tmp/target"));

        let result = compute(
            &sources,
            &target,
            RequestedSets {
                new: true,
                old: true,
                common: true,
            },
        )
        .unwrap();

        assert_eq!(result.new.as_ref().map(|v| v.len()), Some(2));
        assert_eq!(result.common, Some(vec![]));
        assert_eq!(result.old.as_ref().map(|v| v.len()), Some(2));
    }

    #[test]
    fn partial_overlap() {
        let algo = HashAlgorithm::Sha256;
        let (target_root, target_bundle) =
            build_bundle_with_chunks(algo, &[b"chunk1", b"chunk2", b"chunk3"]);
        let (source_root, source_bundle) =
            build_bundle_with_chunks(algo, &[b"chunk2", b"chunk3", b"chunk4"]);

        let sources = vec![(
            source_root,
            algo,
            source_bundle,
            PathBuf::from("/tmp/source"),
        )];
        let target = (target_root, algo, target_bundle, PathBuf::from("/tmp/target"));

        let result = compute(
            &sources,
            &target,
            RequestedSets {
                new: true,
                old: true,
                common: true,
            },
        )
        .unwrap();

        // new: chunk1 (in target but not source)
        assert_eq!(result.new.as_ref().map(|v| v.len()), Some(1));
        // common: chunk2, chunk3
        assert_eq!(result.common.as_ref().map(|v| v.len()), Some(2));
        // old: chunk4 (in source but not target)
        assert_eq!(result.old.as_ref().map(|v| v.len()), Some(1));
    }

    #[test]
    fn only_new_requested() {
        let algo = HashAlgorithm::Sha256;
        let (target_root, target_bundle) =
            build_bundle_with_chunks(algo, &[b"chunk1", b"chunk2"]);
        let (source_root, source_bundle) = build_bundle_with_chunks(algo, &[b"chunk2"]);

        let sources = vec![(
            source_root,
            algo,
            source_bundle,
            PathBuf::from("/tmp/source"),
        )];
        let target = (target_root, algo, target_bundle, PathBuf::from("/tmp/target"));

        let result = compute(
            &sources,
            &target,
            RequestedSets {
                new: true,
                old: false,
                common: false,
            },
        )
        .unwrap();

        assert_eq!(result.new.as_ref().map(|v| v.len()), Some(1));
        assert_eq!(result.old, None);
        assert_eq!(result.common, None);
    }

    #[test]
    fn requested_but_empty_set() {
        let algo = HashAlgorithm::Sha256;
        let (target_root, target_bundle) =
            build_bundle_with_chunks(algo, &[b"chunk1", b"chunk2"]);
        let (source_root, source_bundle) =
            build_bundle_with_chunks(algo, &[b"chunk1", b"chunk2"]);

        let sources = vec![(
            source_root,
            algo,
            source_bundle,
            PathBuf::from("/tmp/source"),
        )];
        let target = (target_root, algo, target_bundle, PathBuf::from("/tmp/target"));

        let result = compute(
            &sources,
            &target,
            RequestedSets {
                new: true,
                old: true,
                common: true,
            },
        )
        .unwrap();

        // new and old should be Some(vec![]) (requested but empty), not None
        assert!(result.new.is_some());
        assert!(result.old.is_some());
        assert_eq!(result.new.as_ref().map(|v| v.is_empty()), Some(true));
        assert_eq!(result.old.as_ref().map(|v| v.is_empty()), Some(true));
    }

    #[test]
    fn no_flags_defaults_to_new() {
        let algo = HashAlgorithm::Sha256;
        let (target_root, target_bundle) =
            build_bundle_with_chunks(algo, &[b"chunk1", b"chunk2"]);
        let (source_root, source_bundle) = build_bundle_with_chunks(algo, &[b"chunk2"]);

        let sources = vec![(
            source_root,
            algo,
            source_bundle,
            PathBuf::from("/tmp/source"),
        )];
        let target = (target_root, algo, target_bundle, PathBuf::from("/tmp/target"));

        let result = compute(
            &sources,
            &target,
            RequestedSets {
                new: false,
                old: false,
                common: false,
            },
        )
        .unwrap();

        // Should have only None fields
        assert_eq!(result.new, None);
        assert_eq!(result.old, None);
        assert_eq!(result.common, None);
    }

    #[test]
    fn multiple_sources_union() {
        let algo = HashAlgorithm::Sha256;
        let (target_root, target_bundle) =
            build_bundle_with_chunks(algo, &[b"chunk1", b"chunk2", b"chunk3"]);
        let (source1_root, source1_bundle) = build_bundle_with_chunks(algo, &[b"chunk1"]);
        let (source2_root, source2_bundle) = build_bundle_with_chunks(algo, &[b"chunk2"]);

        let sources = vec![
            (
                source1_root,
                algo,
                source1_bundle,
                PathBuf::from("/tmp/source1"),
            ),
            (
                source2_root,
                algo,
                source2_bundle,
                PathBuf::from("/tmp/source2"),
            ),
        ];
        let target = (target_root, algo, target_bundle, PathBuf::from("/tmp/target"));

        let result = compute(
            &sources,
            &target,
            RequestedSets {
                new: true,
                old: true,
                common: true,
            },
        )
        .unwrap();

        // new: chunk3 (only in target)
        assert_eq!(result.new.as_ref().map(|v| v.len()), Some(1));
        // old: nothing (sources are subset of target)
        assert_eq!(result.old, Some(vec![]));
        // common: chunk1, chunk2
        assert_eq!(result.common.as_ref().map(|v| v.len()), Some(2));
    }

    #[test]
    fn determinism_sorted_by_raw_bytes() {
        let algo = HashAlgorithm::Sha256;
        let (target_root, target_bundle) =
            build_bundle_with_chunks(algo, &[b"a", b"c", b"b"]);
        let (source_root, source_bundle) = build_bundle_with_chunks(algo, &[]);

        let sources = vec![(
            source_root,
            algo,
            source_bundle,
            PathBuf::from("/tmp/source"),
        )];
        let target = (target_root, algo, target_bundle, PathBuf::from("/tmp/target"));

        let result1 = compute(
            &sources,
            &target,
            RequestedSets {
                new: true,
                old: false,
                common: false,
            },
        )
        .unwrap();

        let result2 = compute(
            &sources,
            &target,
            RequestedSets {
                new: true,
                old: false,
                common: false,
            },
        )
        .unwrap();

        // Both results should have sorted new chunks
        assert_eq!(result1.new, result2.new);
        if let Some(new_chunks) = &result1.new {
            let is_sorted = new_chunks.windows(2).all(|w| w[0].0.0 <= w[1].0.0);
            assert!(is_sorted, "new chunks should be sorted by raw bytes");
        }
    }

    #[test]
    fn algorithm_mismatch_sha256_vs_blake3_in_sources() {
        let target = {
            let (root, bundle) = build_bundle_with_chunks(HashAlgorithm::Sha256, &[b"chunk1"]);
            (root, HashAlgorithm::Sha256, bundle, PathBuf::from("/tmp/target"))
        };

        let sources = vec![(
            {
                let (root, _) = build_bundle_with_chunks(HashAlgorithm::Blake3, &[b"chunk1"]);
                root
            },
            HashAlgorithm::Blake3,
            {
                let (_, bundle) = build_bundle_with_chunks(HashAlgorithm::Blake3, &[b"chunk1"]);
                bundle
            },
            PathBuf::from("/tmp/source"),
        )];

        let result = compute(
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
            Err(ChunksError::AlgorithmMismatch {
                expected: HashAlgorithm::Sha256,
                found: HashAlgorithm::Blake3,
                ..
            })
        ));
    }

    #[test]
    fn algorithm_mismatch_mixed_sources() {
        let target = {
            let (root, bundle) = build_bundle_with_chunks(HashAlgorithm::Sha256, &[b"chunk1"]);
            (root, HashAlgorithm::Sha256, bundle, PathBuf::from("/tmp/target"))
        };

        let (source1_root, source1_bundle) =
            build_bundle_with_chunks(HashAlgorithm::Sha256, &[b"chunk1"]);
        let (source2_root, source2_bundle) =
            build_bundle_with_chunks(HashAlgorithm::Blake3, &[b"chunk1"]);

        let sources = vec![
            (
                source1_root,
                HashAlgorithm::Sha256,
                source1_bundle,
                PathBuf::from("/tmp/source1"),
            ),
            (
                source2_root,
                HashAlgorithm::Blake3,
                source2_bundle,
                PathBuf::from("/tmp/source2"),
            ),
        ];

        let result = compute(
            &sources,
            &target,
            RequestedSets {
                new: true,
                old: false,
                common: false,
            },
        );

        assert!(matches!(result, Err(ChunksError::AlgorithmMismatch { .. })));
    }

    #[test]
    fn all_algorithms_matching() {
        let algo = HashAlgorithm::Sha256;
        let (target_root, target_bundle) = build_bundle_with_chunks(algo, &[b"chunk1"]);
        let (source1_root, source1_bundle) = build_bundle_with_chunks(algo, &[b"chunk2"]);
        let (source2_root, source2_bundle) = build_bundle_with_chunks(algo, &[b"chunk3"]);

        let sources = vec![
            (
                source1_root,
                algo,
                source1_bundle,
                PathBuf::from("/tmp/source1"),
            ),
            (
                source2_root,
                algo,
                source2_bundle,
                PathBuf::from("/tmp/source2"),
            ),
        ];
        let target = (target_root, algo, target_bundle, PathBuf::from("/tmp/target"));

        let result = compute(
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
    fn missing_referenced_object_propagates() {
        let algo = HashAlgorithm::Sha256;
        let mut target_bundle = DirTreeBundle::new();

        // Create a file node without actually inserting the FileIndex
        let file_meta = make_metadata(0o644, 1000, 1000);
        let file_meta_id = file_meta.id(algo);
        let file_index = FileIndex::new(vec![ChunkID(hash_bytes(algo, b"chunk1"))]);
        let file_index_id = file_index.id(algo);

        let file_node = Node::new(
            "file.txt",
            file_meta_id,
            None,
            NodeKind::File { file_index_id },
        );

        let dirtree = DirTree::new(vec![file_node]);
        let root_id = dirtree.id(algo);

        target_bundle.insert(ObjectKind::DirTree, root_id.0, dirtree.encode_canonical());
        target_bundle.insert(ObjectKind::Metadata, file_meta_id.0, file_meta.encode_canonical());
        // NOTE: intentionally NOT inserting file_index

        let (source_root, source_bundle) = build_bundle_with_chunks(algo, &[]);

        let sources = vec![(
            source_root,
            algo,
            source_bundle,
            PathBuf::from("/tmp/source"),
        )];
        let target = (root_id, algo, target_bundle, PathBuf::from("/tmp/target"));

        let result = compute(
            &sources,
            &target,
            RequestedSets {
                new: true,
                old: false,
                common: false,
            },
        );

        assert!(matches!(result, Err(ChunksError::Walk(..))));
    }
}
