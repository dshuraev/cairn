//! Top-level `digest()` entrypoint with atomic `--out` write (§8).

use crate::build::{build_tree, DigestOptions};
use crate::error::DigestError;
use crate::store::Store;
use crate::walk::walk_tree;
use cairn_core::bundle::DirTreeBundle;
use cairn_core::id::DirTreeID;
use std::fs;
use std::path::{Path, PathBuf};

/// Walks `src_dir`, chunks and dedup-writes every referenced object into
/// `store`, and writes a standalone dirtree bundle to `out_path` (§5.7): the
/// root `DirTreeID` plus every `DirTree`, `Metadata`, and `FileIndex` object
/// it transitively references, inlined so the tree — structure, permissions,
/// ownership, per-file chunk lists — can be inspected without `store`. Raw
/// chunk bytes stay store-only.
///
/// `out_path` is written last, atomically (write-temp → rename, §8), and only
/// after `build_tree` has synchronously dedup-written every object the
/// returned tree transitively references into `store` — so `out_path`
/// existing is a promise that the store is complete for that tree. On any
/// error, `out_path` is left untouched: it's never partially written.
pub fn digest(
    src_dir: &Path,
    store: &Store,
    out_path: &Path,
    options: &DigestOptions,
) -> Result<DirTreeID, DigestError> {
    let (walked, tracker) = walk_tree(src_dir, options.algo)?;
    let mut bundle = DirTreeBundle::new();
    let root_id = build_tree(&walked, &tracker, store, options, &mut bundle)?;

    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut tmp_name = out_path.as_os_str().to_os_string();
    tmp_name.push(".tmp");
    let tmp_path = PathBuf::from(tmp_name);
    fs::write(&tmp_path, bundle.encode_canonical(root_id, options.algo))?;
    fs::rename(&tmp_path, out_path)?;

    Ok(root_id)
}
