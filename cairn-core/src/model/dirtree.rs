//! `DirTree` object (§3, §4.3).

use crate::decode::{DecodeError, Decoder};
use crate::encode::Encoder;
use crate::hash::{hash_bytes, HashAlgorithm};
use crate::id::DirTreeID;
use crate::kind::DIRTREE_KIND_TAG;
use crate::model::node::Node;
use crate::sort::git_tree_cmp;

/// A directory's contents: a sorted list of `Node` entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirTree {
    nodes: Vec<Node>,
}

impl DirTree {
    /// Creates a `DirTree` from an unordered list of nodes.
    pub fn new(nodes: Vec<Node>) -> Self {
        Self { nodes }
    }

    /// Encodes this object canonically (§4.3): a `u8 object_kind_tag`, then
    /// a `u32` node count (consistent with `FileIndex`'s `chunk_count` and
    /// `Metadata`'s `xattr_count` — the spec doesn't restate this for `DirTree`,
    /// but fixed-width, delimiter-free encoding per §4.1 requires *some* way to
    /// know how many variable-length nodes follow), followed by each node's
    /// encoding, in git tree-sort order (§4.3) regardless of the order `nodes`
    /// was constructed with.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut sorted: Vec<&Node> = self.nodes.iter().collect();
        sorted.sort_by(|a, b| git_tree_cmp(&a.name, a.kind.is_dir(), &b.name, b.kind.is_dir()));

        let mut e = Encoder::new();
        e.write_u8(DIRTREE_KIND_TAG);
        e.write_u32(sorted.len() as u32);
        for node in sorted {
            node.encode_canonical(&mut e);
        }
        e.into_bytes()
    }

    /// Computes this object's content-addressed ID.
    pub fn id(&self, algo: HashAlgorithm) -> DirTreeID {
        DirTreeID(hash_bytes(algo, &self.encode_canonical()))
    }

    /// Decodes a `DirTree` from its canonical encoding (§4.3), the inverse of
    /// [`DirTree::encode_canonical`]. Rejects trailing bytes or mismatched kind tag.
    /// The returned `nodes` are in the encoded (git tree-sort) order.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut d = Decoder::new(bytes);
        let kind_tag = d.read_u8()?;
        if kind_tag != DIRTREE_KIND_TAG {
            return Err(DecodeError::InvalidTag(kind_tag));
        }
        let node_count = d.read_u32()?;
        let mut nodes = Vec::with_capacity(node_count as usize);
        for _ in 0..node_count {
            nodes.push(Node::decode_canonical(&mut d)?);
        }
        d.finish()?;
        Ok(Self { nodes })
    }

    /// This tree's nodes, in git tree-sort order.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::Hash;
    use crate::id::MetadataID;
    use crate::model::node::NodeKind;

    fn node(name: &str, kind: NodeKind) -> Node {
        Node::new(name, MetadataID(Hash([0u8; 32])), None, kind)
    }

    #[test]
    fn encodes_nodes_in_git_tree_sort_order_regardless_of_input_order() {
        // "foo" (file) < "foo.bar" (file) < "foo" (dir) -- the classic
        // trailing-slash ambiguity case (§4.3).
        let foo_file = node("foo", NodeKind::Fifo);
        let foo_bar_file = node("foo.bar", NodeKind::Fifo);
        let foo_dir = node(
            "foo",
            NodeKind::Dir {
                children_id: crate::id::DirTreeID(Hash([9u8; 32])),
            },
        );

        let scrambled = DirTree::new(vec![
            foo_dir.clone(),
            foo_file.clone(),
            foo_bar_file.clone(),
        ]);
        let in_order = DirTree::new(vec![foo_file, foo_bar_file, foo_dir]);

        assert_eq!(scrambled.encode_canonical(), in_order.encode_canonical());
    }

    #[test]
    fn same_nodes_different_construction_order_hash_identically() {
        let a = node("a", NodeKind::Fifo);
        let b = node("b", NodeKind::Socket);

        let t1 = DirTree::new(vec![a.clone(), b.clone()]);
        let t2 = DirTree::new(vec![b, a]);

        assert_eq!(t1.id(HashAlgorithm::Sha256), t2.id(HashAlgorithm::Sha256));
    }

    #[test]
    fn empty_dirtree_and_empty_fileindex_have_different_ids() {
        // Regression test for domain-separation bug: empty DirTree and empty
        // FileIndex must hash to different IDs, not collide. This was caught in
        // practice when a tree with both an empty directory and a zero-byte file
        // produced a kind mismatch error during walk.
        use crate::model::FileIndex;

        let empty_dirtree = DirTree::new(vec![]);
        let empty_fileindex = FileIndex::new(vec![]);

        let dt_id = empty_dirtree.id(HashAlgorithm::Sha256);
        let fi_id = empty_fileindex.id(HashAlgorithm::Sha256);

        // These must be different; if they collide, the dedup logic in
        // DirTreeBundle will silently drop one and cause KindMismatch errors.
        // Compare the underlying Hash values since the ID types are different.
        assert_ne!(
            dt_id.0, fi_id.0,
            "empty DirTree and empty FileIndex must have different IDs (domain separation)"
        );
    }

    #[test]
    fn decode_rejects_fileindex_encoded_bytes() {
        // Verify that DirTree::decode_canonical rejects FileIndex-encoded bytes
        // with a kind-tag mismatch, not a silent misparse.
        use crate::model::FileIndex;

        let fileindex = FileIndex::new(vec![]);
        let fileindex_bytes = fileindex.encode_canonical();

        let result = DirTree::decode_canonical(&fileindex_bytes);
        assert!(
            result.is_err(),
            "DirTree::decode_canonical should reject FileIndex bytes"
        );
        assert!(
            matches!(result, Err(DecodeError::InvalidTag(tag)) if tag == 2),
            "Expected InvalidTag(2) for FileIndex tag, got {:?}",
            result
        );
    }

    #[test]
    fn decode_rejects_metadata_encoded_bytes() {
        // Verify that DirTree::decode_canonical rejects Metadata-encoded bytes
        // with a kind-tag mismatch, not a silent misparse.
        use crate::model::Metadata;

        let metadata = Metadata::new(0o644, 0, 0, vec![]);
        let metadata_bytes = metadata.encode_canonical();

        let result = DirTree::decode_canonical(&metadata_bytes);
        assert!(
            result.is_err(),
            "DirTree::decode_canonical should reject Metadata bytes"
        );
        assert!(
            matches!(result, Err(DecodeError::InvalidTag(tag)) if tag == 1),
            "Expected InvalidTag(1) for Metadata tag, got {:?}",
            result
        );
    }
}
