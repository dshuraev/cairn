//! Error type for reconstruction operations.

use std::fmt;
use std::path::PathBuf;

/// Errors that can occur during directory reconstruction.
#[derive(Debug)]
pub enum ReconstructError {
    /// An I/O error occurred.
    Io(std::io::Error),
    /// A decode error occurred reading the bundle.
    Decode(cairn_core::decode::DecodeError),
    /// A store error occurred.
    Store(cairn_store::StoreError),
    /// Referenced by a Node but absent from the bundle (I3 violation).
    MissingBundleObject {
        /// The missing ID.
        id: cairn_core::hash::Hash,
    },
    /// Chunks were absent from every configured store during real reconstruct.
    MissingChunks {
        /// The missing chunk IDs.
        ids: Vec<cairn_core::id::ChunkID>,
    },
    /// A privileged operation was required but --no-root was not passed.
    PrivilegeRequired {
        /// The path where the operation failed.
        path: PathBuf,
        /// Description of the required operation.
        op: String,
    },
    /// `--out` already exists.
    OutputExists {
        /// The path that already exists.
        path: PathBuf,
    },
    /// Bundle version is not supported by this binary.
    UnsupportedBundleVersion {
        /// The version found in the bundle.
        found: u8,
        /// The maximum version this binary supports.
        max_supported: u8,
    },
}

impl fmt::Display for ReconstructError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReconstructError::Io(e) => write!(f, "I/O error: {e}"),
            ReconstructError::Decode(e) => write!(f, "decode error: {e}"),
            ReconstructError::Store(e) => write!(f, "store error: {e}"),
            ReconstructError::MissingBundleObject { id } => {
                write!(f, "missing bundle object: {id}")
            }
            ReconstructError::MissingChunks { ids } => {
                write!(f, "missing {} chunks from store", ids.len())
            }
            ReconstructError::PrivilegeRequired { path, op } => {
                write!(f, "privileged operation required at {}: {}", path.display(), op)
            }
            ReconstructError::OutputExists { path } => {
                write!(f, "output path already exists: {}", path.display())
            }
            ReconstructError::UnsupportedBundleVersion {
                found,
                max_supported,
            } => write!(
                f,
                "unsupported bundle version: {} (maximum supported: {})",
                found, max_supported
            ),
        }
    }
}

impl std::error::Error for ReconstructError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ReconstructError::Io(e) => Some(e),
            ReconstructError::Decode(e) => Some(e),
            ReconstructError::Store(e) => Some(e),
            ReconstructError::UnsupportedBundleVersion { .. } => None,
            _ => None,
        }
    }
}

impl From<std::io::Error> for ReconstructError {
    fn from(e: std::io::Error) -> Self {
        ReconstructError::Io(e)
    }
}

impl From<cairn_core::decode::DecodeError> for ReconstructError {
    fn from(e: cairn_core::decode::DecodeError) -> Self {
        ReconstructError::Decode(e)
    }
}

impl From<cairn_store::StoreError> for ReconstructError {
    fn from(e: cairn_store::StoreError) -> Self {
        ReconstructError::Store(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_variant_display_mentions_underlying_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let err = ReconstructError::from(io_err);
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn missing_chunks_display_mentions_count() {
        let ids = vec![
            cairn_core::id::ChunkID(cairn_core::hash::Hash([1u8; 32])),
            cairn_core::id::ChunkID(cairn_core::hash::Hash([2u8; 32])),
        ];
        let err = ReconstructError::MissingChunks { ids };
        assert!(err.to_string().contains("2 chunks"));
    }

    #[test]
    fn output_exists_display_mentions_path() {
        let path = PathBuf::from("/tmp/test");
        let err = ReconstructError::OutputExists { path };
        assert!(err.to_string().contains("test"));
    }
}
