//! **cairn-dirtree**: Inspection and diff/merge operations over dirtree bundles.
//!
//! This crate is store-free: all operations consume `.dirtree` bundle files
//! (self-contained collections of `DirTree`, `Metadata`, and `FileIndex` objects)
//! without any dependency on chunk stores. Currently scoped to inspection only
//! (ls, stat, tree, links, symlinks, special, xattrs, summary subcommands); diff
//! and merge are deferred to future plans.
//!
//! All decode-side machinery is reused from `cairn-core` (which already implements
//! `DirTreeBundle::decode_canonical`, `DirTree::decode_canonical`, etc.); this crate
//! adds the walk logic (`walk::resolve`) and formatting (`render::*`) layer.

pub mod render;
pub mod walk;
