//! `Metadata` object (§3, §4.4).

use crate::encode::Encoder;
use crate::hash::{hash_bytes, HashAlgorithm};
use crate::id::MetadataID;

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

    /// Encodes this object canonically (§4.4).
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut e = Encoder::new();
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_xattrs_encode_with_zero_count_and_no_trailing_bytes() {
        let m = Metadata::new(0o644, 1000, 1000, vec![]);
        let mut expected = vec![];
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
}
