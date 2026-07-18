//! Standalone dirtree bundle (§5.7, §8).
//!
//! Inlines every `DirTree`, `Metadata`, and `FileIndex` object reachable from a
//! root `DirTree` into one self-contained, content-addressed container, so
//! `--out` can be inspected — structure, permissions, ownership, per-file
//! chunk lists — without the store. Raw chunk bytes are deliberately not
//! included, only their IDs (via `FileIndex`): bulk content belongs in the
//! store, not the tree description.

use crate::decode::{DecodeError, Decoder};
use crate::encode::Encoder;
use crate::hash::{Hash, HashAlgorithm};
use crate::id::DirTreeID;
use crate::kind::{DIRTREE_KIND_TAG, FILEINDEX_KIND_TAG, METADATA_KIND_TAG};
use std::collections::HashMap;

/// Which §3 object a bundle entry's bytes decode as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    /// A `DirTree` object.
    DirTree,
    /// A `Metadata` object.
    Metadata,
    /// A `FileIndex` object.
    FileIndex,
}

impl ObjectKind {
    fn tag(self) -> u8 {
        match self {
            ObjectKind::DirTree => DIRTREE_KIND_TAG,
            ObjectKind::Metadata => METADATA_KIND_TAG,
            ObjectKind::FileIndex => FILEINDEX_KIND_TAG,
        }
    }

    fn from_tag(tag: u8) -> Result<Self, DecodeError> {
        match tag {
            DIRTREE_KIND_TAG => Ok(ObjectKind::DirTree),
            METADATA_KIND_TAG => Ok(ObjectKind::Metadata),
            FILEINDEX_KIND_TAG => Ok(ObjectKind::FileIndex),
            other => Err(DecodeError::InvalidTag(other)),
        }
    }
}

fn algo_tag(algo: HashAlgorithm) -> u8 {
    match algo {
        HashAlgorithm::Sha256 => 0,
        HashAlgorithm::Blake3 => 1,
    }
}

fn algo_from_tag(tag: u8) -> Result<HashAlgorithm, DecodeError> {
    match tag {
        0 => Ok(HashAlgorithm::Sha256),
        1 => Ok(HashAlgorithm::Blake3),
        other => Err(DecodeError::InvalidTag(other)),
    }
}

/// A self-contained bundle: a root `DirTreeID` plus every `DirTree`,
/// `Metadata`, and `FileIndex` object (canonically encoded) it transitively
/// references, keyed by content-addressed ID.
#[derive(Debug, Clone, Default)]
pub struct DirTreeBundle {
    objects: HashMap<Hash, (ObjectKind, Vec<u8>)>,
}

impl DirTreeBundle {
    /// Creates an empty bundle.
    pub fn new() -> Self {
        Self {
            objects: HashMap::new(),
        }
    }

    /// Inserts an object's canonical encoding under `id`, deduplicating by ID
    /// (mirrors the store's dedup, §6.1) — a second insert under an ID already
    /// present is a no-op.
    pub fn insert(&mut self, kind: ObjectKind, id: Hash, bytes: Vec<u8>) {
        self.objects.entry(id).or_insert((kind, bytes));
    }

    /// The number of distinct objects in this bundle.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Whether this bundle has no objects.
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Looks up an object's kind and raw canonical encoding by ID.
    pub fn get(&self, id: &Hash) -> Option<(ObjectKind, &[u8])> {
        self.objects
            .get(id)
            .map(|(kind, bytes)| (*kind, bytes.as_slice()))
    }

    /// Encodes this bundle, together with `root` and `algo`, into a single
    /// self-contained byte stream suitable for writing to `--out`: an algo
    /// tag, the root ID, an object count, then each object as
    /// `kind_tag || id || len_prefixed bytes`.
    ///
    /// Objects are encoded in strictly ascending order of their ID (hash bytes)
    /// to ensure canonical (deterministic) encoding for deduplication (§4.1).
    pub fn encode_canonical(&self, root: DirTreeID, algo: HashAlgorithm) -> Vec<u8> {
        let mut e = Encoder::new();
        e.write_u8(algo_tag(algo));
        e.write_hash(&root.0);
        e.write_u32(self.objects.len() as u32);

        // Sort objects by ID for canonical encoding
        let mut sorted_objects: Vec<_> = self.objects.iter().collect();
        sorted_objects.sort_by_key(|(id_a, _)| id_a.0);

        for (id, (kind, bytes)) in sorted_objects {
            e.write_u8(kind.tag());
            e.write_hash(id);
            e.write_bytes(bytes);
        }
        e.into_bytes()
    }

    /// Decodes a bundle previously produced by `encode_canonical`, returning
    /// the root ID, hash algorithm, and the bundle itself. Rejects trailing
    /// bytes.
    pub fn decode_canonical(
        bytes: &[u8],
    ) -> Result<(DirTreeID, HashAlgorithm, Self), DecodeError> {
        let mut d = Decoder::new(bytes);
        let algo = algo_from_tag(d.read_u8()?)?;
        let root = DirTreeID(d.read_hash()?);
        let count = d.read_u32()?;
        let mut objects = HashMap::with_capacity(count as usize);
        for _ in 0..count {
            let kind = ObjectKind::from_tag(d.read_u8()?)?;
            let id = d.read_hash()?;
            let payload = d.read_bytes()?;
            objects.insert(id, (kind, payload));
        }
        d.finish()?;
        Ok((root, algo, Self { objects }))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::hash::hash_bytes;

    #[test]
    fn round_trips_root_algo_and_objects() {
        let mut bundle = DirTreeBundle::new();
        let a = hash_bytes(HashAlgorithm::Sha256, b"a");
        let b = hash_bytes(HashAlgorithm::Sha256, b"b");
        bundle.insert(ObjectKind::DirTree, a, b"dirtree-bytes".to_vec());
        bundle.insert(ObjectKind::Metadata, b, b"metadata-bytes".to_vec());

        let root = DirTreeID(a);
        let encoded = bundle.encode_canonical(root, HashAlgorithm::Sha256);
        let (decoded_root, algo, decoded) = DirTreeBundle::decode_canonical(&encoded).unwrap();

        assert_eq!(decoded_root, root);
        assert_eq!(algo, HashAlgorithm::Sha256);
        assert_eq!(decoded.len(), 2);
        assert_eq!(
            decoded.get(&a),
            Some((ObjectKind::DirTree, b"dirtree-bytes".as_slice()))
        );
        assert_eq!(
            decoded.get(&b),
            Some((ObjectKind::Metadata, b"metadata-bytes".as_slice()))
        );
    }

    #[test]
    fn second_insert_under_same_id_is_a_no_op() {
        let mut bundle = DirTreeBundle::new();
        let id = hash_bytes(HashAlgorithm::Sha256, b"x");
        bundle.insert(ObjectKind::FileIndex, id, b"first".to_vec());
        bundle.insert(ObjectKind::FileIndex, id, b"second".to_vec());

        assert_eq!(bundle.len(), 1);
        assert_eq!(
            bundle.get(&id),
            Some((ObjectKind::FileIndex, b"first".as_slice()))
        );
    }

    #[test]
    fn empty_bundle_round_trips() {
        let bundle = DirTreeBundle::new();
        let root = DirTreeID(hash_bytes(HashAlgorithm::Sha256, b"root"));
        let encoded = bundle.encode_canonical(root, HashAlgorithm::Sha256);
        let (decoded_root, _algo, decoded) = DirTreeBundle::decode_canonical(&encoded).unwrap();
        assert_eq!(decoded_root, root);
        assert!(decoded.is_empty());
    }

    #[test]
    fn objects_encoded_in_sorted_id_order() {
        // Create multiple objects with distinct IDs.
        // We'll create hashes from different inputs and verify they
        // appear in the encoded output in strictly ascending order by ID.
        let mut bundle = DirTreeBundle::new();

        // Create 6 objects with different inputs to get different hashes
        let h1 = hash_bytes(HashAlgorithm::Sha256, b"object_1");
        let h2 = hash_bytes(HashAlgorithm::Sha256, b"object_2");
        let h3 = hash_bytes(HashAlgorithm::Sha256, b"object_3");
        let h4 = hash_bytes(HashAlgorithm::Sha256, b"object_4");
        let h5 = hash_bytes(HashAlgorithm::Sha256, b"object_5");
        let h6 = hash_bytes(HashAlgorithm::Sha256, b"object_6");

        // Insert them in a deliberately non-sorted order
        bundle.insert(ObjectKind::DirTree, h5, b"bytes_5".to_vec());
        bundle.insert(ObjectKind::Metadata, h2, b"bytes_2".to_vec());
        bundle.insert(ObjectKind::FileIndex, h6, b"bytes_6".to_vec());
        bundle.insert(ObjectKind::DirTree, h1, b"bytes_1".to_vec());
        bundle.insert(ObjectKind::Metadata, h4, b"bytes_4".to_vec());
        bundle.insert(ObjectKind::FileIndex, h3, b"bytes_3".to_vec());

        let root = DirTreeID(h1);
        let encoded = bundle.encode_canonical(root, HashAlgorithm::Sha256);

        // Decode and extract the object IDs in the order they appear
        let (_, _, decoded) = DirTreeBundle::decode_canonical(&encoded).unwrap();

        // Manually parse the encoded bytes to check ordering of objects
        // Format: 1 byte (algo) + 32 bytes (root) + 4 bytes (count) + objects
        let mut offset = 1 + 32 + 4;
        let mut extracted_ids = Vec::new();

        for _ in 0..6 {
            // Skip kind tag (1 byte)
            offset += 1;
            // Extract the 32-byte object ID
            let id_bytes = &encoded[offset..offset + 32];
            let mut id_array = [0u8; 32];
            id_array.copy_from_slice(id_bytes);
            extracted_ids.push(Hash(id_array));
            offset += 32;
            // Skip length-prefixed bytes
            let len = u32::from_le_bytes([
                encoded[offset],
                encoded[offset + 1],
                encoded[offset + 2],
                encoded[offset + 3],
            ]) as usize;
            offset += 4 + len;
        }

        // Verify that extracted IDs are in strictly ascending order
        for i in 0..extracted_ids.len() - 1 {
            assert!(
                extracted_ids[i].0 < extracted_ids[i + 1].0,
                "Objects not sorted: id[{}] = {} >= id[{}] = {}",
                i,
                extracted_ids[i],
                i + 1,
                extracted_ids[i + 1]
            );
        }

        // Also verify that we got all the expected objects
        assert_eq!(decoded.len(), 6);
    }

    #[test]
    fn objectkind_tag_matches_constants() {
        // Cross-check: verify that ObjectKind::tag() returns values matching
        // the constants defined in kind.rs. This ensures the two schemes stay
        // in sync for domain separation in canonical encodings (§4.1).
        assert_eq!(ObjectKind::DirTree.tag(), DIRTREE_KIND_TAG);
        assert_eq!(ObjectKind::Metadata.tag(), METADATA_KIND_TAG);
        assert_eq!(ObjectKind::FileIndex.tag(), FILEINDEX_KIND_TAG);
    }
}
