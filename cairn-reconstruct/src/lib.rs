//! **cairn-reconstruct**: Materialize a DirTree bundle into a directory tree on the filesystem.
//!
//! This crate implements the inverse of `cairn-digest`: it reads a DirTree bundle
//! (structure, permissions, content hashes) and materializes the directory tree on disk,
//! with full control over privileged operations (chown, chmod, xattrs, device nodes).

#![warn(missing_docs)]
#![warn(clippy::unwrap_used, clippy::expect_used)]

pub mod bundle_read;
pub mod error;
pub mod materialize;
pub mod noroot;
pub mod plan;
pub mod walk;

pub use error::ReconstructError;
pub use materialize::{materialize, MaterializeOptions, MaterializeReport};
pub use plan::{check, dry_run, CheckReport, DryRunReport};
