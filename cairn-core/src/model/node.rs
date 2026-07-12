//! `Node` and `NodeKind` (§3, §4.3).

use crate::decode::{DecodeError, Decoder};
use crate::encode::Encoder;
use crate::id::{DirTreeID, FileIndexID, LinkGroupID, MetadataID};

/// The type-specific payload of a directory entry (§3). Exactly one payload per
/// `Node`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// A regular file, referencing its ordered chunk list.
    File {
        /// The file's content, as a `FileIndex` ID.
        file_index_id: FileIndexID,
    },
    /// A subdirectory.
    Dir {
        /// The subdirectory's contents, as a `DirTree` ID.
        children_id: DirTreeID,
    },
    /// A symbolic link, storing its target directly (never followed, §6.2).
    Symlink {
        /// The link target, verbatim.
        target: String,
    },
    /// A device node.
    Device {
        /// The device's major number.
        major: u32,
        /// The device's minor number.
        minor: u32,
    },
    /// A named pipe (FIFO).
    Fifo,
    /// A Unix domain socket.
    Socket,
}

impl NodeKind {
    /// The single-byte tag identifying this variant in the canonical encoding
    /// (§4.3). Values are an implementation detail internal to this codec, not
    /// otherwise specified by the spec, but must stay stable for hashes to
    /// remain reproducible.
    fn kind_tag(&self) -> u8 {
        match self {
            NodeKind::File { .. } => 0,
            NodeKind::Dir { .. } => 1,
            NodeKind::Symlink { .. } => 2,
            NodeKind::Device { .. } => 3,
            NodeKind::Fifo => 4,
            NodeKind::Socket => 5,
        }
    }

    /// Whether this is a `Dir` entry, for git tree-sort purposes (§4.3).
    pub fn is_dir(&self) -> bool {
        matches!(self, NodeKind::Dir { .. })
    }

    fn encode_payload(&self, e: &mut Encoder) {
        match self {
            NodeKind::File { file_index_id } => {
                e.write_hash(&file_index_id.0);
            }
            NodeKind::Dir { children_id } => {
                e.write_hash(&children_id.0);
            }
            NodeKind::Symlink { target } => {
                e.write_str(target);
            }
            NodeKind::Device { major, minor } => {
                e.write_u32(*major);
                e.write_u32(*minor);
            }
            NodeKind::Fifo | NodeKind::Socket => {}
        }
    }

    /// Decodes the payload matching `kind_tag`, the inverse of `encode_payload`.
    fn decode_payload(kind_tag: u8, d: &mut Decoder) -> Result<Self, DecodeError> {
        Ok(match kind_tag {
            0 => NodeKind::File {
                file_index_id: FileIndexID(d.read_hash()?),
            },
            1 => NodeKind::Dir {
                children_id: DirTreeID(d.read_hash()?),
            },
            2 => NodeKind::Symlink {
                target: d.read_str()?,
            },
            3 => NodeKind::Device {
                major: d.read_u32()?,
                minor: d.read_u32()?,
            },
            4 => NodeKind::Fifo,
            5 => NodeKind::Socket,
            other => return Err(DecodeError::InvalidTag(other)),
        })
    }
}

/// One directory entry: a name plus its metadata, optional hardlink group, and
/// type-specific payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The path component's name (no separators).
    pub name: String,
    /// This node's `Metadata` ID.
    pub metadata_id: MetadataID,
    /// Present iff this path is one of ≥2 hardlinked paths (§3).
    pub link_group: Option<LinkGroupID>,
    /// The type-specific payload.
    pub kind: NodeKind,
}

impl Node {
    /// Creates a new `Node`.
    pub fn new(
        name: impl Into<String>,
        metadata_id: MetadataID,
        link_group: Option<LinkGroupID>,
        kind: NodeKind,
    ) -> Self {
        Self {
            name: name.into(),
            metadata_id,
            link_group,
            kind,
        }
    }

    /// Encodes this node canonically (§4.3) into `e`, as part of a `DirTree`'s
    /// sorted node list.
    pub(crate) fn encode_canonical(&self, e: &mut Encoder) {
        e.write_str(&self.name);
        e.write_u8(self.kind.kind_tag());
        e.write_hash(&self.metadata_id.0);
        match &self.link_group {
            Some(link_group) => {
                e.write_u8(1);
                e.write_hash(&link_group.0);
            }
            None => {
                e.write_u8(0);
            }
        }
        self.kind.encode_payload(e);
    }

    /// Decodes one `Node` from `d`, the inverse of `encode_canonical`. Does not
    /// consume any bytes beyond this node's own encoding, so callers (e.g.
    /// `DirTree::decode_canonical`) can call this in a loop over a sequence of
    /// nodes.
    pub(crate) fn decode_canonical(d: &mut Decoder) -> Result<Self, DecodeError> {
        let name = d.read_str()?;
        let kind_tag = d.read_u8()?;
        let metadata_id = MetadataID(d.read_hash()?);
        let link_group = match d.read_u8()? {
            0 => None,
            1 => Some(LinkGroupID(d.read_hash()?)),
            other => return Err(DecodeError::InvalidTag(other)),
        };
        let kind = NodeKind::decode_payload(kind_tag, d)?;
        Ok(Node {
            name,
            metadata_id,
            link_group,
            kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::Hash;

    #[test]
    fn file_node_encodes_name_tag_metadata_no_link_group_then_payload() {
        let file_index_id = FileIndexID(Hash([1u8; 32]));
        let metadata_id = MetadataID(Hash([2u8; 32]));
        let node = Node::new("a", metadata_id, None, NodeKind::File { file_index_id });

        let mut e = Encoder::new();
        node.encode_canonical(&mut e);
        let bytes = e.into_bytes();

        let mut expected = vec![1u8, 0, 0, 0, b'a']; // name_len + name
        expected.push(0); // kind_tag = File
        expected.extend_from_slice(&[2u8; 32]); // metadata_id
        expected.push(0); // has_link_group = false
        expected.extend_from_slice(&[1u8; 32]); // file_index_id payload

        assert_eq!(bytes, expected);
    }

    #[test]
    fn node_with_link_group_encodes_flag_and_id() {
        let metadata_id = MetadataID(Hash([2u8; 32]));
        let link_group = LinkGroupID(Hash([3u8; 32]));
        let node = Node::new("a", metadata_id, Some(link_group), NodeKind::Fifo);

        let mut e = Encoder::new();
        node.encode_canonical(&mut e);
        let bytes = e.into_bytes();

        let mut expected = vec![1u8, 0, 0, 0, b'a'];
        expected.push(4); // kind_tag = Fifo
        expected.extend_from_slice(&[2u8; 32]);
        expected.push(1); // has_link_group = true
        expected.extend_from_slice(&[3u8; 32]);
        // Fifo has no payload.

        assert_eq!(bytes, expected);
    }
}
