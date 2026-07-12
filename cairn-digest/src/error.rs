//! Error type for cairn-digest operations.

use cairn_core::hash::Hash;
use std::fmt;

/// Errors that can occur while chunking, storing, or building dirtree objects.
#[derive(Debug)]
pub enum DigestError {
    /// An I/O error occurred reading the source directory or writing to the store.
    Io(std::io::Error),
    /// FastCDC chunking failed while reading a file.
    Chunking(cairn_core::fastcdc::v2020::Error),
    /// A store object's content did not hash to its expected ID.
    StoreCorrupt {
        /// The ID the caller expected.
        expected: Hash,
        /// The ID actually computed from the content.
        actual: Hash,
    },
}

impl fmt::Display for DigestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DigestError::Io(e) => write!(f, "I/O error: {e}"),
            DigestError::Chunking(e) => write!(f, "chunking error: {e}"),
            DigestError::StoreCorrupt { expected, actual } => {
                write!(f, "hash mismatch: expected {expected}, got {actual}")
            }
        }
    }
}

impl std::error::Error for DigestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DigestError::Io(e) => Some(e),
            DigestError::Chunking(e) => Some(e),
            DigestError::StoreCorrupt { .. } => None,
        }
    }
}

impl From<std::io::Error> for DigestError {
    fn from(e: std::io::Error) -> Self {
        DigestError::Io(e)
    }
}

impl From<cairn_core::fastcdc::v2020::Error> for DigestError {
    fn from(e: cairn_core::fastcdc::v2020::Error) -> Self {
        DigestError::Chunking(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_variant_display_mentions_underlying_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let err = DigestError::from(io_err);
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn chunking_variant_display_is_non_empty() {
        let err = DigestError::Chunking(cairn_core::fastcdc::v2020::Error::Empty);
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn store_corrupt_display_mentions_both_hashes() {
        let expected = Hash([1u8; 32]);
        let actual = Hash([2u8; 32]);
        let err = DigestError::StoreCorrupt { expected, actual };
        let msg = err.to_string();
        assert!(msg.contains(&expected.to_string()));
        assert!(msg.contains(&actual.to_string()));
    }
}
