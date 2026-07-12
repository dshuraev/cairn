//! FastCDC chunking of a single file (§5.3).

use crate::error::DigestError;
use cairn_core::fastcdc::v2020::StreamCDC;
use cairn_core::hash::{hash_bytes, HashAlgorithm};
use cairn_core::id::ChunkID;
use std::fs::File;
use std::path::Path;

/// FastCDC chunk size tuning parameters (§2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkConfig {
    /// Minimum chunk size, in bytes.
    pub min_size: usize,
    /// Target (average) chunk size, in bytes.
    pub avg_size: usize,
    /// Maximum chunk size, in bytes.
    pub max_size: usize,
}

impl Default for ChunkConfig {
    /// §2 defaults: 16KiB / 64KiB / 256KiB.
    fn default() -> Self {
        Self {
            min_size: 16 * 1024,
            avg_size: 64 * 1024,
            max_size: 256 * 1024,
        }
    }
}

/// Splits the file at `path` into content-defined chunks (§5.3) and hashes each.
///
/// Returns the chunks in file order, paired with their content-addressed ID.
pub fn chunk_file(
    path: &Path,
    config: &ChunkConfig,
    algo: HashAlgorithm,
) -> Result<Vec<(ChunkID, Vec<u8>)>, DigestError> {
    let file = File::open(path)?;
    let chunker = StreamCDC::new(file, config.min_size, config.avg_size, config.max_size);
    let mut chunks = Vec::new();
    for result in chunker {
        let chunk_data = result?;
        let id = ChunkID(hash_bytes(algo, &chunk_data.data));
        chunks.push((id, chunk_data.data));
    }
    Ok(chunks)
}
