//! `FileIndex` object (§3, §4.2).

use crate::decode::{DecodeError, Decoder};
use crate::encode::Encoder;
use crate::hash::{hash_bytes, HashAlgorithm};
use crate::id::{ChunkID, FileIndexID};
use crate::kind::FILEINDEX_KIND_TAG;

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

    /// Encodes this object canonically (§4.2): `u8 object_kind_tag`, then
    /// `u32 chunk_count` followed by `chunk_count` 32-byte chunk IDs,
    /// concatenated in order.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        e.write_u8(FILEINDEX_KIND_TAG);
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

    /// Decodes a `FileIndex` from its canonical encoding (§4.2), the inverse of
    /// [`FileIndex::encode_canonical`]. Rejects trailing bytes or mismatched kind tag.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut d = Decoder::new(bytes);
        let kind_tag = d.read_u8()?;
        if kind_tag != FILEINDEX_KIND_TAG {
            return Err(DecodeError::InvalidTag(kind_tag));
        }
        let chunk_count = d.read_u32()?;
        let mut chunks = Vec::with_capacity(chunk_count as usize);
        for _ in 0..chunk_count {
            chunks.push(ChunkID(d.read_hash()?));
        }
        d.finish()?;
        Ok(Self { chunks })
    }

    /// This file's content, as an ordered list of chunk IDs.
    pub fn chunks(&self) -> &[ChunkID] {
        &self.chunks
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

        let mut expected = vec![FILEINDEX_KIND_TAG, 2u8, 0, 0, 0];
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

    #[test]
    fn decode_rejects_dirtree_encoded_bytes() {
        // Verify that FileIndex::decode_canonical rejects DirTree-encoded bytes
        // with a kind-tag mismatch, not a silent misparse.
        use crate::model::DirTree;

        let dirtree = DirTree::new(vec![]);
        let dirtree_bytes = dirtree.encode_canonical();

        let result = FileIndex::decode_canonical(&dirtree_bytes);
        assert!(
            result.is_err(),
            "FileIndex::decode_canonical should reject DirTree bytes"
        );
        assert!(
            matches!(result, Err(DecodeError::InvalidTag(tag)) if tag == 0),
            "Expected InvalidTag(0) for DirTree tag, got {:?}",
            result
        );
    }

    #[test]
    fn decode_rejects_metadata_encoded_bytes() {
        // Verify that FileIndex::decode_canonical rejects Metadata-encoded bytes
        // with a kind-tag mismatch, not a silent misparse.
        use crate::model::Metadata;

        let metadata = Metadata::new(0o644, 0, 0, vec![]);
        let metadata_bytes = metadata.encode_canonical();

        let result = FileIndex::decode_canonical(&metadata_bytes);
        assert!(
            result.is_err(),
            "FileIndex::decode_canonical should reject Metadata bytes"
        );
        assert!(
            matches!(result, Err(DecodeError::InvalidTag(tag)) if tag == 1),
            "Expected InvalidTag(1) for Metadata tag, got {:?}",
            result
        );
    }
}
