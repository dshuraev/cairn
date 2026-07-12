//! `DirTree` object (§3, §4.3).

use crate::encode::Encoder;
use crate::hash::{hash_bytes, HashAlgorithm};
use crate::id::DirTreeID;
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

    /// Encodes this object canonically (§4.3): a `u32` node count (consistent
    /// with `FileIndex`'s `chunk_count` and `Metadata`'s `xattr_count` — the
    /// spec doesn't restate this for `DirTree`, but fixed-width, delimiter-free
    /// encoding per §4.1 requires *some* way to know how many variable-length
    /// nodes follow), followed by each node's encoding, in git tree-sort order
    /// (§4.3) regardless of the order `nodes` was constructed with.
    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut sorted: Vec<&Node> = self.nodes.iter().collect();
        sorted.sort_by(|a, b| git_tree_cmp(&a.name, a.kind.is_dir(), &b.name, b.kind.is_dir()));

        let mut e = Encoder::new();
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
}
