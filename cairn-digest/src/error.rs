//! Error type for cairn-digest operations.

use std::fmt;

/// Errors that can occur while chunking, storing, or building dirtree objects.
#[derive(Debug)]
pub enum DigestError {
    /// An I/O error occurred reading the source directory or writing to the store.
    Io(std::io::Error),
    /// FastCDC chunking failed while reading a file.
    Chunking(cairn_core::fastcdc::v2020::Error),
    /// A store operation failed (read, write, or hash verification).
    Store(cairn_store::StoreError),
}

impl fmt::Display for DigestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DigestError::Io(e) => write!(f, "I/O error: {e}"),
            DigestError::Chunking(e) => write!(f, "chunking error: {e}"),
            DigestError::Store(e) => write!(f, "store error: {e}"),
        }
    }
}

impl std::error::Error for DigestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DigestError::Io(e) => Some(e),
            DigestError::Chunking(e) => Some(e),
            DigestError::Store(e) => Some(e),
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

impl From<cairn_store::StoreError> for DigestError {
    fn from(e: cairn_store::StoreError) -> Self {
        DigestError::Store(e)
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
    fn store_variant_display_is_non_empty() {
        let err = DigestError::Store(cairn_store::StoreError::NotFound {
            id: cairn_core::hash::Hash([1u8; 32]),
        });
        assert!(!err.to_string().is_empty());
    }
}
