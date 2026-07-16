//! Bottom-up tree assembly (§5, full algorithm).
//!
//! `build_tree` consumes the already-walked in-memory tree and fully-populated
//! [`HardlinkTracker`] produced by `walk::walk_tree` — it performs no further
//! filesystem metadata calls (`readdir`/`lstat`), only opening regular files to
//! chunk their content. Every node's `link_group` is looked up from the
//! tracker, which is already complete by construction (see `walk`'s module doc
//! comment), so correctness here doesn't depend on traversal order.

use crate::chunk::{chunk_file, ChunkConfig};
use crate::error::DigestError;
use crate::hardlink::{HardlinkTracker, Inode};
use crate::metadata::build_metadata;
use crate::store::Store;
use crate::walk::{RawKind, WalkEntry};
use cairn_core::bundle::{DirTreeBundle, ObjectKind};
use cairn_core::hash::HashAlgorithm;
use cairn_core::id::{DirTreeID, FileIndexID};
use cairn_core::model::{DirTree, FileIndex, Node, NodeKind};
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;

/// Tunable parameters for a single digest run.
#[derive(Debug, Clone, Default)]
pub struct DigestOptions {
    /// FastCDC chunk size tuning.
    pub chunk_config: ChunkConfig,
    /// The hash algorithm to use throughout this run.
    pub algo: HashAlgorithm,
}

fn inode_of(entry: &WalkEntry) -> Inode {
    Inode {
        device: entry.metadata.dev(),
        inode: entry.metadata.ino(),
    }
}

/// Builds the bottom-up object tree for `walked` (the already fully-walked
/// source directory from `walk::walk_tree`), writing every referenced Chunk,
/// FileIndex, Metadata, and DirTree object into `store` (deduplicated, §6.1),
/// and returns the root `DirTreeID`.
///
/// Every `DirTree`, `Metadata`, and `FileIndex` object built along the way
/// (everything except raw `Chunk` bytes) is also inserted into `bundle`, so
/// the caller can write a standalone `--out` (§5.7) that's fully inspectable
/// without `store`.
///
/// `tracker` must be the tracker returned alongside `walked` by the same
/// `walk_tree` call. `walked` itself is assumed to represent a directory (the
/// source root); only its `children` become `Node`s in the returned tree.
pub fn build_tree(
    walked: &WalkEntry,
    tracker: &HardlinkTracker,
    store: &Store,
    options: &DigestOptions,
    bundle: &mut DirTreeBundle,
) -> Result<DirTreeID, DigestError> {
    let mut file_index_cache: HashMap<Inode, FileIndexID> = HashMap::new();
    build_dir(walked, tracker, store, options, &mut file_index_cache, bundle)
}

fn build_dir(
    dir_entry: &WalkEntry,
    tracker: &HardlinkTracker,
    store: &Store,
    options: &DigestOptions,
    file_index_cache: &mut HashMap<Inode, FileIndexID>,
    bundle: &mut DirTreeBundle,
) -> Result<DirTreeID, DigestError> {
    let mut nodes = Vec::with_capacity(dir_entry.children.len());
    for child in &dir_entry.children {
        nodes.push(build_node(
            child,
            tracker,
            store,
            options,
            file_index_cache,
            bundle,
        )?);
    }
    let dir_tree = DirTree::new(nodes);
    let id = dir_tree.id(options.algo);
    let encoded = dir_tree.encode_canonical();
    store.write(&id.0, &encoded, options.algo)?;
    bundle.insert(ObjectKind::DirTree, id.0, encoded);
    Ok(id)
}

/// Builds the `Node` for a single walked entry (recursing for `Dir`), and
/// dedup-writes its `Metadata` (and, for files, its `Chunk`s and `FileIndex`)
/// into `store`.
fn build_node(
    entry: &WalkEntry,
    tracker: &HardlinkTracker,
    store: &Store,
    options: &DigestOptions,
    file_index_cache: &mut HashMap<Inode, FileIndexID>,
    bundle: &mut DirTreeBundle,
) -> Result<Node, DigestError> {
    let metadata = build_metadata(&entry.path, &entry.metadata)?;
    let metadata_id = metadata.id(options.algo);
    let encoded_metadata = metadata.encode_canonical();
    store.write(&metadata_id.0, &encoded_metadata, options.algo)?;
    bundle.insert(ObjectKind::Metadata, metadata_id.0, encoded_metadata);

    let inode = inode_of(entry);
    // Only regular files are ever observed by the tracker (walk::walk_entry),
    // so this is always None for Dir/Symlink/Device/Fifo/Socket kinds.
    let link_group = tracker.link_group(inode);

    let kind = match &entry.kind {
        RawKind::File => {
            let file_index_id = match file_index_cache.get(&inode) {
                Some(cached) => *cached,
                None => {
                    let chunks = chunk_file(&entry.path, &options.chunk_config, options.algo)?;
                    for (chunk_id, data) in &chunks {
                        store.write(&chunk_id.0, data, options.algo)?;
                    }
                    let chunk_ids = chunks.into_iter().map(|(id, _)| id).collect();
                    let file_index = FileIndex::new(chunk_ids);
                    let file_index_id = file_index.id(options.algo);
                    let encoded_file_index = file_index.encode_canonical();
                    store.write(&file_index_id.0, &encoded_file_index, options.algo)?;
                    bundle.insert(ObjectKind::FileIndex, file_index_id.0, encoded_file_index);
                    file_index_cache.insert(inode, file_index_id);
                    file_index_id
                }
            };
            NodeKind::File { file_index_id }
        }
        RawKind::Dir => {
            let children_id = build_dir(entry, tracker, store, options, file_index_cache, bundle)?;
            NodeKind::Dir { children_id }
        }
        RawKind::Symlink { target } => NodeKind::Symlink {
            target: target.clone(),
        },
        RawKind::Device { major, minor } => NodeKind::Device {
            major: *major,
            minor: *minor,
        },
        RawKind::Fifo => NodeKind::Fifo,
        RawKind::Socket => NodeKind::Socket,
    };

    Ok(Node::new(entry.name.clone(), metadata_id, link_group, kind))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::walk::walk_tree;
    use std::path::PathBuf;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("cairn-digest-build-test-{label}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Directly exercises `build_node`'s wiring of `tracker.link_group` for two
    /// hardlinked paths in different subdirectories, without needing to decode
    /// anything back out of the store. This is the property the walk/build
    /// split (Fix 1) exists to guarantee: both nodes must carry the same
    /// non-`None` link group.
    #[test]
    fn hardlinked_paths_get_matching_link_group_in_built_nodes() {
        let root = unique_temp_dir("hardlink-nodes");
        std::fs::create_dir(root.join("a")).unwrap();
        std::fs::create_dir(root.join("b")).unwrap();
        std::fs::write(root.join("a/shared.bin"), b"shared content").unwrap();
        std::fs::hard_link(root.join("a/shared.bin"), root.join("b/shared.bin")).unwrap();

        let options = DigestOptions::default();
        let (root_entry, tracker) = walk_tree(&root, options.algo).unwrap();
        let store_dir = unique_temp_dir("hardlink-store");
        let store = Store::new(store_dir.clone(), vec![]);
        let mut cache = HashMap::new();
        let mut bundle = DirTreeBundle::new();

        let dir_a = root_entry.children.iter().find(|c| c.name == "a").unwrap();
        let dir_b = root_entry.children.iter().find(|c| c.name == "b").unwrap();
        let file_a = &dir_a.children[0];
        let file_b = &dir_b.children[0];

        let node_a = build_node(file_a, &tracker, &store, &options, &mut cache, &mut bundle).unwrap();
        let node_b = build_node(file_b, &tracker, &store, &options, &mut cache, &mut bundle).unwrap();

        assert!(node_a.link_group.is_some());
        assert_eq!(node_a.link_group, node_b.link_group);

        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&store_dir).unwrap();
    }
}
