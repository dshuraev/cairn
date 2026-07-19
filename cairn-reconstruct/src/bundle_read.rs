//! Bundle file reading and decoding.

use crate::error::ReconstructError;
use cairn_core::bundle::DirTreeBundle;
use cairn_core::hash::HashAlgorithm;
use cairn_core::id::DirTreeID;
use std::fs;
use std::path::Path;

/// Maximum bundle version this binary can handle.
pub const MAX_SUPPORTED_BUNDLE_VERSION: u8 = 0;

/// Reads `path` and decodes it as a dirtree bundle (cairn-reconstruct.md §4).
pub fn read_bundle(
    path: &Path,
) -> Result<(DirTreeID, HashAlgorithm, DirTreeBundle), ReconstructError> {
    let bytes = fs::read(path)?;
    let (version, root_id, algo, bundle) = DirTreeBundle::decode_canonical(&bytes)?;
    if version > MAX_SUPPORTED_BUNDLE_VERSION {
        return Err(ReconstructError::UnsupportedBundleVersion {
            found: version,
            max_supported: MAX_SUPPORTED_BUNDLE_VERSION,
        });
    }
    Ok((root_id, algo, bundle))
}
