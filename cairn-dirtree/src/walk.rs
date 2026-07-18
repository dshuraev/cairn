//! Bundle walking and node resolution.
//!
//! The core logic: starting from a root `DirTree`, recursively resolve every
//! node in the bundle, building a flat vector of `ResolvedNode` entries in
//! walk order. All downstream rendering functions consume this output.

use cairn_core::bundle::{DirTreeBundle, ObjectKind};
use cairn_core::hash::HashAlgorithm;
use cairn_core::id::{ChunkID, DirTreeID, LinkGroupID};
use cairn_core::model::{DirTree, FileIndex, Metadata, NodeKind};
use cairn_core::decode::DecodeError;
use thiserror::Error;

/// The kind of a resolved node, with already-resolved child/chunk information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedKind {
    /// A regular file, with resolved chunk list.
    File { chunks: Vec<ChunkID> },
    /// A directory, with resolved child count.
    Dir { child_count: usize },
    /// A symbolic link.
    Symlink { target: String },
    /// A device node.
    Device { major: u32, minor: u32 },
    /// A named pipe (FIFO).
    Fifo,
    /// A Unix domain socket.
    Socket,
}

/// One entry in a resolved dirtree: a full path, metadata, and kind-specific fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedNode {
    /// Full slash-joined path from root (no leading slash).
    pub path: String,
    /// Final path component (i.e., `Node.name`).
    pub name: String,
    /// Resolved kind and fields.
    pub kind: ResolvedKind,
    /// Resolved metadata.
    pub metadata: Metadata,
    /// Optional hardlink group ID.
    pub link_group: Option<LinkGroupID>,
}

/// Errors that can occur during bundle walking.
#[derive(Error, Debug)]
pub enum WalkError {
    /// An object referenced by ID was not found in the bundle.
    #[error("missing object {id} referenced by {referenced_by}")]
    MissingObject { id: String, referenced_by: String },

    /// An object kind tag mismatch (e.g., metadata_id pointing to a DirTree).
    #[error("kind mismatch at {path}: expected {expected}, found {found}")]
    KindMismatch {
        path: String,
        expected: String,
        found: String,
    },

    /// A decode error in canonical encoding.
    #[error("decode error at {path}: {source}")]
    DecodeError { path: String, source: DecodeError },

    /// The root was not a DirTree.
    #[error("root object is not a DirTree")]
    NotADirTree,

    /// An ID that should have been a Metadata object was not.
    #[error("object {id} should be Metadata but is not")]
    NotAMetadata { id: String },

    /// An ID that should have been a FileIndex object was not.
    #[error("object {id} should be FileIndex but is not")]
    NotAFileIndex { id: String },
}

/// Collects all ChunkIDs reachable from a dirtree.
///
/// Delegates to `resolve()`, extracting only the `chunks` field from File nodes.
/// This avoids duplicating the tree-walking and error-handling logic.
///
/// # Errors
///
/// Returns `WalkError` if any referenced object is missing from the bundle
/// or has a kind tag mismatch (same validation as `resolve()`).
pub fn reachable_chunks(
    root: DirTreeID,
    algo: HashAlgorithm,
    bundle: &DirTreeBundle,
) -> Result<std::collections::HashSet<ChunkID>, WalkError> {
    Ok(resolve(root, algo, bundle)?
        .into_iter()
        .filter_map(|n| match n.kind {
            ResolvedKind::File { chunks } => Some(chunks),
            _ => None,
        })
        .flatten()
        .collect())
}

/// Resolves a dirtree bundle into a flat vector of `ResolvedNode` entries,
/// in depth-first walk order (mirroring `DirTree::nodes()`'s git-tree-sort order).
///
/// # Errors
///
/// Returns `WalkError` if any referenced object is missing from the bundle,
/// has a kind tag mismatch, or fails to decode.
pub fn resolve(
    root: DirTreeID,
    algo: HashAlgorithm,
    bundle: &DirTreeBundle,
) -> Result<Vec<ResolvedNode>, WalkError> {
    let mut nodes = Vec::new();
    let root_bytes = bundle
        .get(&root.0)
        .ok_or_else(|| WalkError::MissingObject {
            id: root.0.to_string(),
            referenced_by: "root".to_string(),
        })?;

    if root_bytes.0 != ObjectKind::DirTree {
        return Err(WalkError::NotADirTree);
    }

    resolve_dir(&root.0, "", bundle, root_bytes.1, algo, &mut nodes)?;
    Ok(nodes)
}

/// Recursively resolve a directory, appending nodes to the output vector.
fn resolve_dir(
    _dirtree_id: &cairn_core::hash::Hash,
    path_prefix: &str,
    bundle: &DirTreeBundle,
    dirtree_bytes: &[u8],
    algo: HashAlgorithm,
    nodes: &mut Vec<ResolvedNode>,
) -> Result<(), WalkError> {
    let dirtree = DirTree::decode_canonical(dirtree_bytes).map_err(|e| WalkError::DecodeError {
        path: if path_prefix.is_empty() {
            "/".to_string()
        } else {
            path_prefix.to_string()
        },
        source: e,
    })?;

    for node in dirtree.nodes() {
        let node_path = if path_prefix.is_empty() {
            node.name.clone()
        } else {
            format!("{}/{}", path_prefix, node.name)
        };

        // Resolve metadata.
        let metadata_bytes = bundle
            .get(&node.metadata_id.0)
            .ok_or_else(|| WalkError::MissingObject {
                id: node.metadata_id.0.to_string(),
                referenced_by: format!("node at path {}", node_path),
            })?;

        if metadata_bytes.0 != ObjectKind::Metadata {
            return Err(WalkError::KindMismatch {
                path: node_path.clone(),
                expected: "Metadata".to_string(),
                found: format!("{:?}", metadata_bytes.0),
            });
        }

        let metadata = Metadata::decode_canonical(metadata_bytes.1).map_err(|e| {
            WalkError::DecodeError {
                path: node_path.clone(),
                source: e,
            }
        })?;

        // Resolve kind-specific fields.
        let kind = match &node.kind {
            NodeKind::File { file_index_id } => {
                let file_index_bytes = bundle
                    .get(&file_index_id.0)
                    .ok_or_else(|| WalkError::MissingObject {
                        id: file_index_id.0.to_string(),
                        referenced_by: format!("file at path {}", node_path),
                    })?;

                if file_index_bytes.0 != ObjectKind::FileIndex {
                    return Err(WalkError::KindMismatch {
                        path: node_path.clone(),
                        expected: "FileIndex".to_string(),
                        found: format!("{:?}", file_index_bytes.0),
                    });
                }

                let file_index =
                    FileIndex::decode_canonical(file_index_bytes.1).map_err(|e| {
                        WalkError::DecodeError {
                            path: node_path.clone(),
                            source: e,
                        }
                    })?;

                ResolvedKind::File {
                    chunks: file_index.chunks().to_vec(),
                }
            }
            NodeKind::Dir { children_id } => {
                let children_bytes = bundle
                    .get(&children_id.0)
                    .ok_or_else(|| WalkError::MissingObject {
                        id: children_id.0.to_string(),
                        referenced_by: format!("dir at path {}", node_path),
                    })?;

                if children_bytes.0 != ObjectKind::DirTree {
                    return Err(WalkError::KindMismatch {
                        path: node_path.clone(),
                        expected: "DirTree".to_string(),
                        found: format!("{:?}", children_bytes.0),
                    });
                }

                let children_dirtree =
                    DirTree::decode_canonical(children_bytes.1).map_err(|e| {
                        WalkError::DecodeError {
                            path: node_path.clone(),
                            source: e,
                        }
                    })?;

                let child_count = children_dirtree.nodes().len();

                ResolvedKind::Dir { child_count }
            }
            NodeKind::Symlink { target } => ResolvedKind::Symlink {
                target: target.clone(),
            },
            NodeKind::Device { major, minor } => ResolvedKind::Device {
                major: *major,
                minor: *minor,
            },
            NodeKind::Fifo => ResolvedKind::Fifo,
            NodeKind::Socket => ResolvedKind::Socket,
        };

        // Append the node itself first (depth-first order)
        nodes.push(ResolvedNode {
            path: node_path.clone(),
            name: node.name.clone(),
            kind,
            metadata,
            link_group: node.link_group,
        });

        // Then recurse into directories to append their children
        if let NodeKind::Dir { children_id } = &node.kind {
            let children_bytes = bundle
                .get(&children_id.0)
                .ok_or_else(|| WalkError::MissingObject {
                    id: children_id.0.to_string(),
                    referenced_by: format!("dir at path {}", node_path),
                })?;

            resolve_dir(
                &children_id.0,
                &node_path,
                bundle,
                children_bytes.1,
                algo,
                nodes,
            )?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use cairn_core::hash::hash_bytes;
    use cairn_core::id::MetadataID;
    use cairn_core::model::Node;

    /// Helper: creates a Metadata with given mode, uid, gid.
    fn make_metadata(mode: u32, uid: u32, gid: u32) -> Metadata {
        Metadata::new(mode, uid, gid, vec![])
    }

    /// Helper: computes Metadata ID.
    fn metadata_id(mode: u32, uid: u32, gid: u32, algo: HashAlgorithm) -> MetadataID {
        make_metadata(mode, uid, gid).id(algo)
    }

    #[test]
    fn resolve_single_file_root() {
        let algo = HashAlgorithm::Sha256;
        let mut bundle = DirTreeBundle::new();

        // Create a single-file dirtree.
        let file_meta = make_metadata(0o644, 1000, 1000);
        let file_meta_id = file_meta.id(algo);
        let file_index = FileIndex::new(vec![ChunkID(hash_bytes(algo, b"chunk1"))]);
        let file_index_id = file_index.id(algo);

        let node = Node::new(
            "hello.txt",
            file_meta_id,
            None,
            NodeKind::File { file_index_id },
        );
        let dirtree = DirTree::new(vec![node]);
        let root_id = dirtree.id(algo);

        // Insert into bundle.
        bundle.insert(
            ObjectKind::DirTree,
            root_id.0,
            dirtree.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::Metadata,
            file_meta_id.0,
            file_meta.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::FileIndex,
            file_index_id.0,
            file_index.encode_canonical(),
        );

        // Resolve.
        let nodes = resolve(root_id, algo, &bundle).unwrap();

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].path, "hello.txt");
        assert_eq!(nodes[0].name, "hello.txt");
        assert_eq!(nodes[0].metadata.uid(), 1000);
        assert_eq!(nodes[0].metadata.gid(), 1000);
        assert_eq!(nodes[0].link_group, None);
        match &nodes[0].kind {
            ResolvedKind::File { chunks } => {
                assert_eq!(chunks.len(), 1);
            }
            _ => panic!("expected File kind"),
        }
    }

    #[test]
    fn resolve_nested_directories() {
        let algo = HashAlgorithm::Sha256;
        let mut bundle = DirTreeBundle::new();

        // Create nested structure: root/ { dir1/ { file.txt } }
        let file_meta = make_metadata(0o644, 1000, 1000);
        let file_meta_id = file_meta.id(algo);
        let file_index = FileIndex::new(vec![ChunkID(hash_bytes(algo, b"chunk1"))]);
        let file_index_id = file_index.id(algo);

        let file_node = Node::new(
            "file.txt",
            file_meta_id,
            None,
            NodeKind::File { file_index_id },
        );
        let subdir_dirtree = DirTree::new(vec![file_node]);
        let subdir_id = subdir_dirtree.id(algo);

        let dir_meta = make_metadata(0o755, 1000, 1000);
        let dir_meta_id = dir_meta.id(algo);

        let dir_node = Node::new(
            "dir1",
            dir_meta_id,
            None,
            NodeKind::Dir {
                children_id: subdir_id,
            },
        );
        let root_dirtree = DirTree::new(vec![dir_node]);
        let root_id = root_dirtree.id(algo);

        // Insert all objects.
        bundle.insert(
            ObjectKind::DirTree,
            root_id.0,
            root_dirtree.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::Metadata,
            dir_meta_id.0,
            dir_meta.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::DirTree,
            subdir_id.0,
            subdir_dirtree.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::Metadata,
            file_meta_id.0,
            file_meta.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::FileIndex,
            file_index_id.0,
            file_index.encode_canonical(),
        );

        // Resolve.
        let nodes = resolve(root_id, algo, &bundle).unwrap();

        // Should have 2 nodes: dir1, dir1/file.txt
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].path, "dir1");
        assert_eq!(nodes[1].path, "dir1/file.txt");
    }

    #[test]
    fn resolve_symlink() {
        let algo = HashAlgorithm::Sha256;
        let mut bundle = DirTreeBundle::new();

        let link_meta = make_metadata(0o777, 1000, 1000);
        let link_meta_id = link_meta.id(algo);

        let link_node = Node::new(
            "mylink",
            link_meta_id,
            None,
            NodeKind::Symlink {
                target: "/etc/passwd".to_string(),
            },
        );
        let dirtree = DirTree::new(vec![link_node]);
        let root_id = dirtree.id(algo);

        bundle.insert(
            ObjectKind::DirTree,
            root_id.0,
            dirtree.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::Metadata,
            link_meta_id.0,
            link_meta.encode_canonical(),
        );

        let nodes = resolve(root_id, algo, &bundle).unwrap();

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].path, "mylink");
        match &nodes[0].kind {
            ResolvedKind::Symlink { target } => {
                assert_eq!(target, "/etc/passwd");
            }
            _ => panic!("expected Symlink kind"),
        }
    }

    #[test]
    fn resolve_device_node() {
        let algo = HashAlgorithm::Sha256;
        let mut bundle = DirTreeBundle::new();

        let dev_meta = make_metadata(0o666, 0, 0);
        let dev_meta_id = dev_meta.id(algo);

        let dev_node = Node::new(
            "tty",
            dev_meta_id,
            None,
            NodeKind::Device {
                major: 5,
                minor: 0,
            },
        );
        let dirtree = DirTree::new(vec![dev_node]);
        let root_id = dirtree.id(algo);

        bundle.insert(
            ObjectKind::DirTree,
            root_id.0,
            dirtree.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::Metadata,
            dev_meta_id.0,
            dev_meta.encode_canonical(),
        );

        let nodes = resolve(root_id, algo, &bundle).unwrap();

        assert_eq!(nodes.len(), 1);
        match &nodes[0].kind {
            ResolvedKind::Device { major, minor } => {
                assert_eq!(*major, 5);
                assert_eq!(*minor, 0);
            }
            _ => panic!("expected Device kind"),
        }
    }

    #[test]
    fn resolve_fifo_and_socket() {
        let algo = HashAlgorithm::Sha256;
        let mut bundle = DirTreeBundle::new();

        let fifo_meta = make_metadata(0o644, 1000, 1000);
        let fifo_meta_id = fifo_meta.id(algo);

        let socket_meta = make_metadata(0o644, 1000, 1000);
        let socket_meta_id = socket_meta.id(algo);

        let fifo_node = Node::new("myfifo", fifo_meta_id, None, NodeKind::Fifo);
        let socket_node = Node::new("mysocket", socket_meta_id, None, NodeKind::Socket);
        let dirtree = DirTree::new(vec![fifo_node, socket_node]);
        let root_id = dirtree.id(algo);

        bundle.insert(
            ObjectKind::DirTree,
            root_id.0,
            dirtree.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::Metadata,
            fifo_meta_id.0,
            fifo_meta.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::Metadata,
            socket_meta_id.0,
            socket_meta.encode_canonical(),
        );

        let nodes = resolve(root_id, algo, &bundle).unwrap();

        assert_eq!(nodes.len(), 2);
        match &nodes[0].kind {
            ResolvedKind::Fifo => {}
            _ => panic!("expected Fifo kind"),
        }
        match &nodes[1].kind {
            ResolvedKind::Socket => {}
            _ => panic!("expected Socket kind"),
        }
    }

    #[test]
    fn resolve_node_with_link_group() {
        let algo = HashAlgorithm::Sha256;
        let mut bundle = DirTreeBundle::new();

        let file_meta = make_metadata(0o644, 1000, 1000);
        let file_meta_id = file_meta.id(algo);
        let file_index = FileIndex::new(vec![ChunkID(hash_bytes(algo, b"chunk1"))]);
        let file_index_id = file_index.id(algo);
        let link_group_id = LinkGroupID(hash_bytes(algo, b"linkgroup"));

        let node = Node::new(
            "hardlinked.txt",
            file_meta_id,
            Some(link_group_id),
            NodeKind::File { file_index_id },
        );
        let dirtree = DirTree::new(vec![node]);
        let root_id = dirtree.id(algo);

        bundle.insert(
            ObjectKind::DirTree,
            root_id.0,
            dirtree.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::Metadata,
            file_meta_id.0,
            file_meta.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::FileIndex,
            file_index_id.0,
            file_index.encode_canonical(),
        );

        let nodes = resolve(root_id, algo, &bundle).unwrap();

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].link_group, Some(link_group_id));
    }

    #[test]
    fn missing_metadata_is_error() {
        let algo = HashAlgorithm::Sha256;
        let mut bundle = DirTreeBundle::new();

        let missing_meta_id = MetadataID(hash_bytes(algo, b"missing"));
        let file_index = FileIndex::new(vec![ChunkID(hash_bytes(algo, b"chunk1"))]);
        let file_index_id = file_index.id(algo);

        let node = Node::new(
            "file.txt",
            missing_meta_id,
            None,
            NodeKind::File { file_index_id },
        );
        let dirtree = DirTree::new(vec![node]);
        let root_id = dirtree.id(algo);

        bundle.insert(
            ObjectKind::DirTree,
            root_id.0,
            dirtree.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::FileIndex,
            file_index_id.0,
            file_index.encode_canonical(),
        );

        let result = resolve(root_id, algo, &bundle);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), WalkError::MissingObject { .. }));
    }

    #[test]
    fn kind_mismatch_on_metadata() {
        let algo = HashAlgorithm::Sha256;
        let mut bundle = DirTreeBundle::new();

        let file_meta = make_metadata(0o644, 1000, 1000);
        let file_meta_id = file_meta.id(algo);
        let file_index = FileIndex::new(vec![ChunkID(hash_bytes(algo, b"chunk1"))]);
        let file_index_id = file_index.id(algo);

        // Intentionally insert file_meta as DirTree kind (wrong kind tag)
        let node = Node::new(
            "file.txt",
            file_meta_id,
            None,
            NodeKind::File { file_index_id },
        );
        let dirtree = DirTree::new(vec![node]);
        let root_id = dirtree.id(algo);

        bundle.insert(
            ObjectKind::DirTree,
            root_id.0,
            dirtree.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::DirTree, // Wrong! Should be Metadata
            file_meta_id.0,
            file_meta.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::FileIndex,
            file_index_id.0,
            file_index.encode_canonical(),
        );

        let result = resolve(root_id, algo, &bundle);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), WalkError::KindMismatch { .. }));
    }

    #[test]
    fn corrupt_bytes_is_decode_error() {
        let algo = HashAlgorithm::Sha256;
        let mut bundle = DirTreeBundle::new();

        let file_meta = make_metadata(0o644, 1000, 1000);
        let file_meta_id = file_meta.id(algo);
        let file_index = FileIndex::new(vec![ChunkID(hash_bytes(algo, b"chunk1"))]);
        let file_index_id = file_index.id(algo);

        let node = Node::new(
            "file.txt",
            file_meta_id,
            None,
            NodeKind::File { file_index_id },
        );
        let dirtree = DirTree::new(vec![node]);
        let root_id = dirtree.id(algo);

        bundle.insert(
            ObjectKind::DirTree,
            root_id.0,
            dirtree.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::Metadata,
            file_meta_id.0,
            b"corrupt bytes".to_vec(), // Invalid metadata encoding
        );
        bundle.insert(
            ObjectKind::FileIndex,
            file_index_id.0,
            file_index.encode_canonical(),
        );

        let result = resolve(root_id, algo, &bundle);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), WalkError::DecodeError { .. }));
    }

#[test]
    fn walk_order_matches_git_tree_sort() {
        let algo = HashAlgorithm::Sha256;
        let mut bundle = DirTreeBundle::new();

        // Create nodes that git-tree-sort would order as: "a" (file), "b", "b/" (dir)
        let a_meta = make_metadata(0o644, 1000, 1000);
        let a_meta_id = a_meta.id(algo);
        let a_index = FileIndex::new(vec![ChunkID(hash_bytes(algo, b"chunk_a"))]);
        let a_index_id = a_index.id(algo);

        let b_meta = make_metadata(0o644, 1000, 1000);
        let b_meta_id = b_meta.id(algo);
        let b_index = FileIndex::new(vec![ChunkID(hash_bytes(algo, b"chunk_b"))]);
        let b_index_id = b_index.id(algo);

        let b_dir_meta = make_metadata(0o755, 1000, 1000);
        let b_dir_meta_id = b_dir_meta.id(algo);

        let b_subdir_empty = DirTree::new(vec![]);
        let b_subdir_id = b_subdir_empty.id(algo);

        // Create root with all three entries, scrambled on purpose
        let b_dir_node = Node::new(
            "b",
            b_dir_meta_id,
            None,
            NodeKind::Dir {
                children_id: b_subdir_id,
            },
        );
        let b_file_node = Node::new(
            "b",
            b_meta_id,
            None,
            NodeKind::File { file_index_id: b_index_id },
        );
        let a_file_node = Node::new(
            "a",
            a_meta_id,
            None,
            NodeKind::File { file_index_id: a_index_id },
        );

        // Deliberately insert in non-sorted order
        let root_dirtree = DirTree::new(vec![b_dir_node, a_file_node, b_file_node]);
        let root_id = root_dirtree.id(algo);

        // Insert all objects.
        bundle.insert(
            ObjectKind::DirTree,
            root_id.0,
            root_dirtree.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::Metadata,
            a_meta_id.0,
            a_meta.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::FileIndex,
            a_index_id.0,
            a_index.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::Metadata,
            b_meta_id.0,
            b_meta.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::FileIndex,
            b_index_id.0,
            b_index.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::Metadata,
            b_dir_meta_id.0,
            b_dir_meta.encode_canonical(),
        );
        bundle.insert(
            ObjectKind::DirTree,
            b_subdir_id.0,
            b_subdir_empty.encode_canonical(),
        );

        let nodes = resolve(root_id, algo, &bundle).unwrap();

        // After git-tree-sort: "a" (file), "b" (file), "b" (dir)
        // In the resolved output, we should see:
        // - "a" (file)
        // - "b" (file)
        // - "b" (dir, with child_count=0)
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].path, "a");
        assert_eq!(nodes[1].path, "b");
        assert_eq!(nodes[2].path, "b");
        // Verify the kinds are distinct
        matches!(&nodes[0].kind, ResolvedKind::File { .. });
        matches!(&nodes[1].kind, ResolvedKind::File { .. });
        matches!(&nodes[2].kind, ResolvedKind::Dir { child_count: 0 });
    }
}
