//! Hardlink detection: tracking `(device, inode)` across a full directory walk
//! (§5.2, §6.2).
//!
//! The tracker must be fully populated (every inode counted) *before* any object
//! is hashed — see `walk::walk_tree`'s doc comment for why interleaving walking
//! and building would silently corrupt an already-hashed ancestor `DirTree`.

use cairn_core::encode::Encoder;
use cairn_core::hash::{hash_bytes, HashAlgorithm};
use cairn_core::id::{FileIndexID, LinkGroupID};
use std::collections::HashMap;

/// A regular file's identity for hardlink purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Inode {
    /// The device number the file resides on.
    pub device: u64,
    /// The inode number.
    pub inode: u64,
}

/// Counts how many times each inode is observed across a directory walk.
#[derive(Debug, Clone)]
pub struct HardlinkTracker {
    counts: HashMap<Inode, u32>,
    algo: HashAlgorithm,
    /// Maps FileIndexID to LinkGroupID for hardlinked inodes.
    /// This ensures LinkGroupID is derived from content, not inode numbers,
    /// making it stable across digest runs (satisfying I1 invariant).
    file_index_to_link_group: HashMap<FileIndexID, LinkGroupID>,
}

impl HardlinkTracker {
    /// Creates an empty tracker that will derive `LinkGroupID`s using `algo`.
    pub fn new(algo: HashAlgorithm) -> Self {
        Self {
            counts: HashMap::new(),
            algo,
            file_index_to_link_group: HashMap::new(),
        }
    }

    /// Records one more observation of `inode` (called once per regular file
    /// encountered during the walk).
    pub fn observe(&mut self, inode: Inode) {
        *self.counts.entry(inode).or_insert(0) += 1;
    }

    /// Registers a FileIndex for a hardlinked inode and derives/caches its LinkGroupID.
    /// Returns the LinkGroupID for this file_index_id if the inode is hardlinked (seen >1 time),
    /// or None for standalone files.
    ///
    /// For hardlinked files, LinkGroupID is derived from the FileIndex hash (file content),
    /// not the inode number. This makes LinkGroupID stable across digest runs, satisfying
    /// the I1 invariant: digest(reconstruct(bundle, store)) == bundle.root.
    pub fn register_file_index(
        &mut self,
        inode: Inode,
        file_index_id: FileIndexID,
    ) -> Option<LinkGroupID> {
        if *self.counts.get(&inode)? < 2 {
            return None;
        }

        // Check if we've already computed LinkGroupID for this FileIndex
        if let Some(link_group) = self.file_index_to_link_group.get(&file_index_id) {
            return Some(*link_group);
        }

        // Derive LinkGroupID from file content (FileIndex), not inode
        let mut e = Encoder::new();
        e.write_bytes(b"linkgroup");
        e.write_hash(&file_index_id.0);
        let link_group_id = LinkGroupID(hash_bytes(self.algo, &e.into_bytes()));

        self.file_index_to_link_group
            .insert(file_index_id, link_group_id);
        Some(link_group_id)
    }

    /// This inode's `LinkGroupID`, if it was observed more than once during the
    /// walk this tracker was built from, or `None` for a standalone file.
    ///
    /// **DEPRECATED**: Use `register_file_index()` instead, which derives LinkGroupID
    /// from file content for I1 invariant compliance.
    ///
    /// This old method derived from `H("linkgroup" || device || inode)`, which changes
    /// across digest runs and violates I1 when hardlinks are involved.
    pub fn link_group(&self, inode: Inode) -> Option<LinkGroupID> {
        if *self.counts.get(&inode)? < 2 {
            return None;
        }
        let mut e = Encoder::new();
        e.write_bytes(b"linkgroup");
        e.write_bytes(&inode.device.to_le_bytes());
        e.write_bytes(&inode.inode.to_le_bytes());
        Some(LinkGroupID(hash_bytes(self.algo, &e.into_bytes())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_inode_has_no_link_group() {
        let mut tracker = HardlinkTracker::new(HashAlgorithm::Sha256);
        let inode = Inode {
            device: 1,
            inode: 42,
        };
        tracker.observe(inode);
        assert_eq!(tracker.link_group(inode), None);
    }

    #[test]
    fn repeated_inode_gets_a_stable_link_group() {
        let mut tracker = HardlinkTracker::new(HashAlgorithm::Sha256);
        let inode = Inode {
            device: 1,
            inode: 42,
        };
        tracker.observe(inode);
        tracker.observe(inode);
        let a = tracker.link_group(inode);
        let b = tracker.link_group(inode);
        assert!(a.is_some());
        assert_eq!(a, b);
    }

    #[test]
    fn unobserved_inode_has_no_link_group() {
        let tracker = HardlinkTracker::new(HashAlgorithm::Sha256);
        let inode = Inode {
            device: 1,
            inode: 42,
        };
        assert_eq!(tracker.link_group(inode), None);
    }
}
