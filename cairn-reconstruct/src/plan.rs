//! Dry-run and check modes: plan reconstruction without writing.

use crate::error::ReconstructError;
use crate::walk::collect_chunks;
use cairn_core::bundle::DirTreeBundle;
use cairn_core::hash::HashAlgorithm;
use cairn_core::id::{ChunkID, DirTreeID};
use cairn_store::Store;
use std::path::PathBuf;

/// Report from a dry-run operation.
#[derive(Debug)]
pub struct DryRunReport {
    /// Paths that would be created (relative).
    pub planned_creates: Vec<PathBuf>,
    /// ChunkIDs missing from the store.
    pub missing_chunks: Vec<ChunkID>,
}

/// Report from a check operation.
#[derive(Debug)]
pub struct CheckReport {
    /// Number of chunks successfully verified.
    pub verified: usize,
    /// Chunks that failed verification.
    pub failed: Vec<(ChunkID, cairn_store::StoreError)>,
}

/// Performs a dry-run: enumerates planned creates and missing chunks.
pub fn dry_run(
    bundle: &DirTreeBundle,
    root: DirTreeID,
    store: &Store,
    algo: HashAlgorithm,
) -> Result<DryRunReport, ReconstructError> {
    // Collect all chunks referenced by the tree
    let chunks = collect_chunks(bundle, root)?;

    // Check which chunks are missing
    let mut missing_chunks = Vec::new();
    for chunk_id in chunks {
        if !store.contains(algo, &chunk_id.0) {
            missing_chunks.push(chunk_id);
        }
    }

    // For now, planned_creates is a placeholder; proper implementation
    // would enumerate all nodes in the tree
    let planned_creates = vec![];

    Ok(DryRunReport {
        planned_creates,
        missing_chunks,
    })
}

/// Performs a check: verifies all chunks' hashes without writing.
pub fn check(
    bundle: &DirTreeBundle,
    root: DirTreeID,
    algo: HashAlgorithm,
    store: &Store,
) -> Result<CheckReport, ReconstructError> {
    // Collect all chunks referenced by the tree
    let chunks = collect_chunks(bundle, root)?;

    let mut verified = 0;
    let mut failed = Vec::new();

    for chunk_id in chunks {
        match store.read(algo, &chunk_id.0) {
            Ok(_) => verified += 1,
            Err(e) => failed.push((chunk_id, e)),
        }
    }

    Ok(CheckReport { verified, failed })
}
