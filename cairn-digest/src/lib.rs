//! **cairn-digest**: Core mechanism for chunking directories into deduplicated chunk stores.
//!
//! This crate is pure: no signing, encryption, or policy. It produces two primitives:
//!
//! - **Chunk store**: content-addressed blobs identified by their cryptographic hash.
//! - **DirTree**: canonical hierarchical description of directory structure and contents.
//!
//! See `cairn-digest.md` (the spec) for details on object model and encoding.

#![warn(missing_docs)]
#![warn(clippy::unwrap_used, clippy::expect_used)]

pub mod build;
pub mod chunk;
pub mod digest;
pub mod error;
pub mod hardlink;
pub mod metadata;
pub mod store;
pub mod walk;

pub use build::DigestOptions;
pub use digest::digest;
pub use error::DigestError;
pub use store::Store;
