//! Shared walk logic for enumerating chunks in a DirTree.

use crate::error::ReconstructError;
use cairn_core::bundle::DirTreeBundle;
use cairn_core::id::{ChunkID, DirTreeID};
use cairn_core::model::NodeKind;
use std::collections::HashSet;

/// Walks a DirTree and enumerates all referenced ChunkIDs via a visitor callback.
pub fn walk_chunks<F>(
    bundle: &DirTreeBundle,
    dirtree_id: DirTreeID,
    visit: &mut F,
) -> Result<(), ReconstructError>
where
    F: FnMut(ChunkID),
{
    // Get the DirTree; error if not in bundle (I3 violation)
    let dirtree = bundle
        .get(&dirtree_id.0)
        .ok_or(ReconstructError::MissingBundleObject { id: dirtree_id.0 })?;
    let (_kind, dirtree_bytes) = dirtree;
    let dirtree_obj = cairn_core::model::DirTree::decode_canonical(dirtree_bytes)?;

    // Visit each node
    for node in dirtree_obj.nodes() {
        // Get node's Metadata (for structure, not for chunks)
        let _metadata =
            bundle
                .get(&node.metadata_id.0)
                .ok_or(ReconstructError::MissingBundleObject {
                    id: node.metadata_id.0,
                })?;

        // Process based on NodeKind
        match &node.kind {
            NodeKind::File { file_index_id } => {
                // Get FileIndex
                let file_index_obj =
                    bundle
                        .get(&file_index_id.0)
                        .ok_or(ReconstructError::MissingBundleObject {
                            id: file_index_id.0,
                        })?;
                let (_kind, file_index_bytes) = file_index_obj;
                let file_index = cairn_core::model::FileIndex::decode_canonical(file_index_bytes)?;

                // Visit all chunks in FileIndex
                for chunk_id in file_index.chunks() {
                    visit(*chunk_id);
                }
            }
            NodeKind::Dir { children_id } => {
                // Recurse into subdirectory
                walk_chunks(bundle, *children_id, visit)?;
            }
            NodeKind::Symlink { .. } => {
                // Symlinks have no chunks
            }
            NodeKind::Device { .. } => {
                // Device nodes have no chunks
            }
            NodeKind::Fifo => {
                // FIFOs have no chunks
            }
            NodeKind::Socket => {
                // Sockets have no chunks
            }
        }
    }

    Ok(())
}

/// Collects all unique ChunkIDs referenced by a DirTree.
pub fn collect_chunks(
    bundle: &DirTreeBundle,
    dirtree_id: DirTreeID,
) -> Result<Vec<ChunkID>, ReconstructError> {
    let mut chunks = HashSet::new();
    walk_chunks(bundle, dirtree_id, &mut |chunk_id| {
        chunks.insert(chunk_id);
    })?;
    Ok(chunks.into_iter().collect())
}
