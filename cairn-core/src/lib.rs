//! **cairn-core**: Common types, canonical encoding, and hashing primitives shared
//! across the Cairn workspace.
//!
//! This crate is pure: no filesystem or network I/O. It defines the §3/§4 object
//! model (`Chunk` identity, `FileIndex`, `Metadata`, `Node`, `DirTree`), their
//! canonical byte encoding, content-addressed ID newtypes, and the git tree-sort
//! comparator used to order directory entries deterministically.
//!
//! See `cairn-digest.md` (the spec) for details on object model and encoding.

#![warn(missing_docs)]
#![warn(clippy::unwrap_used, clippy::expect_used)]

pub use fastcdc;

pub mod encode;
pub mod hash;
pub mod id;
pub mod model;
pub mod sort;

#[cfg(test)]
mod tests {
    #[test]
    fn fastcdc_reexport_resolves() {
        let data = b"hello world";
        let _chunker = crate::fastcdc::v2020::StreamCDC::new(&data[..], 64, 256, 1024);
    }
}
