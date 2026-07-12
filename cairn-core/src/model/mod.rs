//! The §3 object model: `Chunk`, `FileIndex`, `Metadata`, `Node`, `DirTree`.
//!
//! `Chunk` has no struct here: per §3, a chunk's identity is `H(bytes)` — its raw
//! bytes *are* its canonical encoding, so [`crate::id::ChunkID`] (from a hash of
//! those bytes) is all that's needed. There is nothing else to construct or
//! encode for a chunk.

pub mod dirtree;
pub mod file_index;
pub mod metadata;
pub mod node;

pub use dirtree::DirTree;
pub use file_index::FileIndex;
pub use metadata::Metadata;
pub use node::{Node, NodeKind};
