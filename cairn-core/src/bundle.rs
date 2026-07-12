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
            ObjectKind::DirTree => 0,
            ObjectKind::Metadata => 1,
            ObjectKind::FileIndex => 2,
        }
    }

    fn from_tag(tag: u8) -> Result<Self, DecodeError> {
        match tag {
            0 => Ok(ObjectKind::DirTree),
            1 => Ok(ObjectKind::Metadata),
            2 => Ok(ObjectKind::FileIndex),
            other => Err(DecodeError::InvalidTag(other)),
        }
    }
}

fn algo_tag(algo: HashAlgorithm) -> u8 {
    match algo {
        HashAlgorithm::Sha256 => 0,
    }
}

fn algo_from_tag(tag: u8) -> Result<HashAlgorithm, DecodeError> {
    match tag {
        0 => Ok(HashAlgorithm::Sha256),
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
    pub fn encode_canonical(&self, root: DirTreeID, algo: HashAlgorithm) -> Vec<u8> {
        let mut e = Encoder::new();
        e.write_u8(algo_tag(algo));
        e.write_hash(&root.0);
        e.write_u32(self.objects.len() as u32);
        for (id, (kind, bytes)) in &self.objects {
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
}
