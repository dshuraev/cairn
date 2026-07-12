//! Directory walk (§5.1) with an integrated hardlink prescan (§5.2).
//!
//! `walk_tree` performs one full recursive pass over the source directory,
//! collecting an in-memory tree of [`WalkEntry`] and simultaneously populating a
//! [`HardlinkTracker`] with every regular file's `(device, inode)`. By the time
//! it returns, the tracker is complete: every inode that will ever repeat is
//! already known, regardless of which path was visited first.
//!
//! This matters because `cairn-digest.md` §5 lists "1. Walk" / "2. Identify
//! hardlinks" as steps *before* "3. Chunk" / "6. Build DirTree bottom-up" for a
//! reason: if hardlink identification were interleaved with bottom-up hashing
//! instead, the first-seen path in a cross-directory hardlink pair could have
//! its containing `DirTree` already hashed and dedup-written to the store
//! *before* the second occurrence was discovered elsewhere in the tree — at
//! which point that `DirTree`'s encoding (missing `link_group`, §4.3) would be
//! permanently wrong. Finishing the whole prescan first, then building
//! afterward purely from the in-memory result (see `build::build_tree`), avoids
//! that ordering hazard entirely.

use crate::error::DigestError;
use crate::hardlink::{HardlinkTracker, Inode};
use cairn_core::hash::HashAlgorithm;
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

/// A directory entry's type, classified without following symlinks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawKind {
    /// A regular file.
    File,
    /// A directory.
    Dir,
    /// A symbolic link, recorded but never followed (§6.2).
    Symlink {
        /// The link's target, verbatim (may be dangling).
        target: String,
    },
    /// A block or character device node.
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

/// One entry discovered during the walk, with its subtree already walked if it's
/// a directory.
#[derive(Debug)]
pub struct WalkEntry {
    /// The path component's name (no separators).
    pub name: String,
    /// The entry's full path on disk.
    pub path: PathBuf,
    /// The entry's classified kind.
    pub kind: RawKind,
    /// The entry's own filesystem metadata (never followed through symlinks).
    pub metadata: fs::Metadata,
    /// This entry's children, populated only for `RawKind::Dir`.
    pub children: Vec<WalkEntry>,
}

/// Extracts `(major, minor)` from a raw `st_rdev` value using the standard
/// Linux/glibc encoding (`gnu_dev_major`/`gnu_dev_minor`); Rust's `std` has no
/// equivalent helper, and this repo targets Linux only.
fn major_minor(rdev: u64) -> (u32, u32) {
    let major = (((rdev >> 8) & 0xfff) as u32) | (((rdev >> 32) as u32) & !0xfffu32);
    let minor = ((rdev & 0xff) as u32) | (((rdev >> 12) as u32) & !0xffu32);
    (major, minor)
}

fn classify(metadata: &fs::Metadata, path: &Path) -> Result<RawKind, DigestError> {
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        let target = fs::read_link(path)?;
        Ok(RawKind::Symlink {
            target: target.to_string_lossy().into_owned(),
        })
    } else if file_type.is_dir() {
        Ok(RawKind::Dir)
    } else if file_type.is_file() {
        Ok(RawKind::File)
    } else if file_type.is_fifo() {
        Ok(RawKind::Fifo)
    } else if file_type.is_socket() {
        Ok(RawKind::Socket)
    } else {
        // Only block/char devices remain among Unix file types (§6.3).
        let (major, minor) = major_minor(metadata.rdev());
        Ok(RawKind::Device { major, minor })
    }
}

fn walk_entry(path: &Path, tracker: &mut HardlinkTracker) -> Result<WalkEntry, DigestError> {
    let metadata = fs::symlink_metadata(path)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let kind = classify(&metadata, path)?;

    if kind == RawKind::File {
        tracker.observe(Inode {
            device: metadata.dev(),
            inode: metadata.ino(),
        });
    }

    let mut children = Vec::new();
    if kind == RawKind::Dir {
        let mut dir_entries: Vec<_> = fs::read_dir(path)?.collect::<Result<_, _>>()?;
        dir_entries.sort_by_key(|e| e.file_name());
        for dir_entry in dir_entries {
            children.push(walk_entry(&dir_entry.path(), tracker)?);
        }
    }

    Ok(WalkEntry {
        name,
        path: path.to_path_buf(),
        kind,
        metadata,
        children,
    })
}

/// Walks `root` recursively, depth-first, never following symlinks (§5.1),
/// returning the fully-populated tree alongside a fully-populated
/// [`HardlinkTracker`] (§5.2). See the module doc comment for why the tracker
/// must be complete before any object is built from this tree.
pub fn walk_tree(
    root: &Path,
    algo: HashAlgorithm,
) -> Result<(WalkEntry, HardlinkTracker), DigestError> {
    let mut tracker = HardlinkTracker::new(algo);
    let entry = walk_entry(root, &mut tracker)?;
    Ok((entry, tracker))
}
