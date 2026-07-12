//! `FileIndex` object (§3, §4.2).

use crate::encode::Encoder;
use crate::hash::{hash_bytes, HashAlgorithm};
use crate::id::{ChunkID, FileIndexID};

/// An ordered list of chunk IDs making up a regular file's content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIndex {
    chunks: Vec<ChunkID>,
}

impl FileIndex {
    /// Creates a `FileIndex` from an ordered list of chunk IDs.
    pub fn new(chunks: Vec<ChunkID>) -> Self {
        Self { chunks }
    }

    /// Encodes this object canonically (§4.2): `u32 chunk_count` followed by
    /// `chunk_count` 32-byte chunk IDs, concatenated in order.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        e.write_u32(self.chunks.len() as u32);
        for chunk_id in &self.chunks {
            e.write_hash(&chunk_id.0);
        }
        e.into_bytes()
    }

    /// Computes this object's content-addressed ID.
    pub fn id(&self, algo: HashAlgorithm) -> FileIndexID {
        FileIndexID(hash_bytes(algo, &self.encode_canonical()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::Hash;

    #[test]
    fn encodes_count_then_chunk_ids_in_order() {
        let a = ChunkID(Hash([1u8; 32]));
        let b = ChunkID(Hash([2u8; 32]));
        let file_index = FileIndex::new(vec![a, b]);

        let mut expected = vec![2u8, 0, 0, 0];
        expected.extend_from_slice(&[1u8; 32]);
        expected.extend_from_slice(&[2u8; 32]);

        assert_eq!(file_index.encode_canonical(), expected);
    }

    #[test]
    fn id_matches_hash_of_canonical_encoding() {
        let a = ChunkID(Hash([1u8; 32]));
        let b = ChunkID(Hash([2u8; 32]));
        let file_index = FileIndex::new(vec![a, b]);

        let expected = FileIndexID(hash_bytes(
            HashAlgorithm::Sha256,
            &file_index.encode_canonical(),
        ));
        assert_eq!(file_index.id(HashAlgorithm::Sha256), expected);
    }
}
