//! `Metadata` object (§3, §4.4).

use crate::decode::{DecodeError, Decoder};
use crate::encode::Encoder;
use crate::hash::{hash_bytes, HashAlgorithm};
use crate::id::MetadataID;
use crate::kind::METADATA_KIND_TAG;

/// Permission bits, ownership, and extended attributes for a node.
///
/// `mtime`/`atime`/`ctime` are intentionally excluded (§6.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    mode: u32,
    uid: u32,
    gid: u32,
    xattrs: Vec<(String, Vec<u8>)>,
}

impl Metadata {
    /// Creates a `Metadata`, sorting `xattrs` by name (plain byte sort, §4.4) and
    /// dropping duplicate names (keeping the first occurrence after sorting) so
    /// that two logically-equal xattr sets always encode identically regardless
    /// of the order they were supplied in.
    pub fn new(mode: u32, uid: u32, gid: u32, mut xattrs: Vec<(String, Vec<u8>)>) -> Self {
        xattrs.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        xattrs.dedup_by(|a, b| a.0 == b.0);
        Self {
            mode,
            uid,
            gid,
            xattrs,
        }
    }

    /// Encodes this object canonically (§4.4): `u8 object_kind_tag`, then
    /// `u32 mode`, `uid`, `gid`, `xattr_count`, and the xattrs themselves.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        e.write_u8(METADATA_KIND_TAG);
        e.write_u32(self.mode);
        e.write_u32(self.uid);
        e.write_u32(self.gid);
        e.write_u32(self.xattrs.len() as u32);
        for (name, value) in &self.xattrs {
            e.write_str(name);
            e.write_bytes(value);
        }
        e.into_bytes()
    }

    /// Computes this object's content-addressed ID.
    pub fn id(&self, algo: HashAlgorithm) -> MetadataID {
        MetadataID(hash_bytes(algo, &self.encode_canonical()))
    }

    /// Decodes a `Metadata` from its canonical encoding (§4.4), the inverse of
    /// [`Metadata::encode_canonical`]. Rejects trailing bytes or mismatched kind tag.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut d = Decoder::new(bytes);
        let kind_tag = d.read_u8()?;
        if kind_tag != METADATA_KIND_TAG {
            return Err(DecodeError::InvalidTag(kind_tag));
        }
        let mode = d.read_u32()?;
        let uid = d.read_u32()?;
        let gid = d.read_u32()?;
        let xattr_count = d.read_u32()?;
        let mut xattrs = Vec::with_capacity(xattr_count as usize);
        for _ in 0..xattr_count {
            let name = d.read_str()?;
            let value = d.read_bytes()?;
            xattrs.push((name, value));
        }
        d.finish()?;
        // Xattrs were already sorted/deduped by the encoding side; skip
        // `Metadata::new`'s re-sort to preserve the encoded order exactly.
        Ok(Self {
            mode,
            uid,
            gid,
            xattrs,
        })
    }

    // Public API

    /// Permission bits (mode) for this node.
    pub fn mode(&self) -> u32 {
        self.mode
    }

    /// User ID (owner) of this node.
    pub fn uid(&self) -> u32 {
        self.uid
    }

    /// Group ID of this node.
    pub fn gid(&self) -> u32 {
        self.gid
    }

    /// Extended attributes (name-value pairs) for this node.
    pub fn xattrs(&self) -> &[(String, Vec<u8>)] {
        &self.xattrs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_xattrs_encode_with_zero_count_and_no_trailing_bytes() {
        let m = Metadata::new(0o644, 1000, 1000, vec![]);
        let mut expected = vec![METADATA_KIND_TAG];
        expected.extend_from_slice(&0o644u32.to_le_bytes());
        expected.extend_from_slice(&1000u32.to_le_bytes());
        expected.extend_from_slice(&1000u32.to_le_bytes());
        expected.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(m.encode_canonical(), expected);
    }

    #[test]
    fn xattr_insertion_order_does_not_affect_id() {
        let a = Metadata::new(
            0o644,
            0,
            0,
            vec![
                ("user.b".to_string(), vec![2]),
                ("user.a".to_string(), vec![1]),
            ],
        );
        let b = Metadata::new(
            0o644,
            0,
            0,
            vec![
                ("user.a".to_string(), vec![1]),
                ("user.b".to_string(), vec![2]),
            ],
        );
        assert_eq!(a.id(HashAlgorithm::Sha256), b.id(HashAlgorithm::Sha256));
    }

    #[test]
    fn duplicate_xattr_names_are_deduplicated() {
        let m = Metadata::new(
            0o644,
            0,
            0,
            vec![
                ("user.a".to_string(), vec![1]),
                ("user.a".to_string(), vec![2]),
            ],
        );
        assert_eq!(m.xattrs.len(), 1);
    }

    #[test]
    fn decode_rejects_dirtree_encoded_bytes() {
        // Verify that Metadata::decode_canonical rejects DirTree-encoded bytes
        // with a kind-tag mismatch, not a silent misparse.
        use crate::model::DirTree;

        let dirtree = DirTree::new(vec![]);
        let dirtree_bytes = dirtree.encode_canonical();

        let result = Metadata::decode_canonical(&dirtree_bytes);
        assert!(
            result.is_err(),
            "Metadata::decode_canonical should reject DirTree bytes"
        );
        assert!(
            matches!(result, Err(DecodeError::InvalidTag(tag)) if tag == 0),
            "Expected InvalidTag(0) for DirTree tag, got {:?}",
            result
        );
    }

    #[test]
    fn decode_rejects_fileindex_encoded_bytes() {
        // Verify that Metadata::decode_canonical rejects FileIndex-encoded bytes
        // with a kind-tag mismatch, not a silent misparse.
        use crate::model::FileIndex;

        let fileindex = FileIndex::new(vec![]);
        let fileindex_bytes = fileindex.encode_canonical();

        let result = Metadata::decode_canonical(&fileindex_bytes);
        assert!(
            result.is_err(),
            "Metadata::decode_canonical should reject FileIndex bytes"
        );
        assert!(
            matches!(result, Err(DecodeError::InvalidTag(tag)) if tag == 2),
            "Expected InvalidTag(2) for FileIndex tag, got {:?}",
            result
        );
    }
}
