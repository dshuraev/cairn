//! Error type for store operations.

use cairn_core::hash::Hash;
use std::fmt;

/// Errors that can occur while reading from or writing to a store.
#[derive(Debug)]
pub enum StoreError {
    /// An I/O error occurred reading or writing the store.
    Io(std::io::Error),
    /// A read or write's recomputed hash didn't match the requested/claimed ID.
    HashMismatch {
        /// The ID expected.
        expected: Hash,
        /// The ID actually computed from the content.
        actual: Hash,
    },
    /// `read()` was called for an ID present in no configured store.
    NotFound {
        /// The missing ID.
        id: Hash,
    },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "I/O error: {e}"),
            StoreError::HashMismatch { expected, actual } => {
                write!(f, "hash mismatch: expected {expected}, got {actual}")
            }
            StoreError::NotFound { id } => {
                write!(f, "object not found: {id}")
            }
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StoreError::Io(e) => Some(e),
            StoreError::HashMismatch { .. } => None,
            StoreError::NotFound { .. } => None,
        }
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_variant_display_mentions_underlying_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let err = StoreError::from(io_err);
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn hash_mismatch_display_mentions_both_hashes() {
        let expected = Hash([1u8; 32]);
        let actual = Hash([2u8; 32]);
        let err = StoreError::HashMismatch { expected, actual };
        let msg = err.to_string();
        assert!(msg.contains(&expected.to_string()));
        assert!(msg.contains(&actual.to_string()));
    }

    #[test]
    fn not_found_display_mentions_id() {
        let id = Hash([3u8; 32]);
        let err = StoreError::NotFound { id };
        assert!(err.to_string().contains(&id.to_string()));
    }
}
