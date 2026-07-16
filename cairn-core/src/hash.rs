//! Cryptographic hashing primitives (§7).

use sha2::{Digest, Sha256};
use std::fmt;

/// Helper function to derive a directory name for a given hash algorithm.
pub fn algo_tag_str(algo: HashAlgorithm) -> &'static str {
    match algo {
        HashAlgorithm::Sha256 => "sha256",
        HashAlgorithm::Blake3 => "blake3",
    }
}

/// A 32-byte cryptographic digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hash(pub [u8; 32]);

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// The cryptographic hash algorithm used to identify an object.
///
/// Only cryptographically secure hash functions are permitted here (§7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HashAlgorithm {
    /// SHA-256, the default.
    #[default]
    Sha256,
    /// BLAKE3.
    Blake3,
}

/// Hashes `data` with the given algorithm.
pub fn hash_bytes(algo: HashAlgorithm, data: &[u8]) -> Hash {
    match algo {
        HashAlgorithm::Sha256 => {
            let mut hasher = Sha256::new();
            hasher.update(data);
            let digest = hasher.finalize();
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&digest);
            Hash(bytes)
        }
        HashAlgorithm::Blake3 => {
            let digest = blake3::hash(data);
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(digest.as_bytes());
            Hash(bytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_empty_string() {
        let hash = hash_bytes(HashAlgorithm::Sha256, b"");
        assert_eq!(
            hash.to_string(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hashes_abc() {
        let hash = hash_bytes(HashAlgorithm::Sha256, b"abc");
        assert_eq!(
            hash.to_string(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hashes_with_blake3() {
        let hash = hash_bytes(HashAlgorithm::Blake3, b"test");
        // BLAKE3 should produce a valid 32-byte hash
        assert_eq!(hash.0.len(), 32);
    }

    #[test]
    fn algo_tag_str_returns_distinct_names() {
        assert_ne!(
            algo_tag_str(HashAlgorithm::Sha256),
            algo_tag_str(HashAlgorithm::Blake3)
        );
    }
}
