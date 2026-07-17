//! Content-addressed object store: a pure mechanism for reading/writing
//! hash-verified, immutable chunks of data. No policy, no signing, no encryption.
//!
//! See `cairn-core` for the data model; this crate provides only storage and
//! retrieval with hash verification (I2).

#![warn(missing_docs)]
#![warn(clippy::unwrap_used, clippy::expect_used)]

pub mod error;
pub mod store;

pub use error::StoreError;
pub use store::Store;
