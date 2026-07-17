//! Bundle file reading and decoding.

use crate::error::ReconstructError;
use cairn_core::bundle::DirTreeBundle;
use cairn_core::hash::HashAlgorithm;
use cairn_core::id::DirTreeID;
use std::fs;
use std::path::Path;

/// Reads `path` and decodes it as a dirtree bundle (cairn-reconstruct.md §4).
pub fn read_bundle(
    path: &Path,
) -> Result<(DirTreeID, HashAlgorithm, DirTreeBundle), ReconstructError> {
    let bytes = fs::read(path)?;
    let (root_id, algo, bundle) = DirTreeBundle::decode_canonical(&bytes)?;
    Ok((root_id, algo, bundle))
}
