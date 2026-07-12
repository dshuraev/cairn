//! Content-addressed chunk store with dedup and atomic writes (§6.1, §8).

use crate::error::DigestError;
use cairn_core::hash::{hash_bytes, Hash, HashAlgorithm};
use std::fs;
use std::path::{Path, PathBuf};

/// A content-addressed object store, backed by a primary directory and an
/// ordered list of read-only seed stores consulted for dedup before writing.
#[derive(Debug, Clone)]
pub struct Store {
    primary: PathBuf,
    seeds: Vec<PathBuf>,
}

impl Store {
    /// Creates a store rooted at `primary`, consulting `seeds` (in order) for
    /// existing objects before writing new ones.
    pub fn new(primary: PathBuf, seeds: Vec<PathBuf>) -> Self {
        Self { primary, seeds }
    }

    /// The path an object with the given ID would occupy within `dir`: a flat
    /// directory, filename = lowercase hex of the ID. §9 defers on-disk layout
    /// entirely; this is the one function to change if sharding is ever added.
    fn object_path(dir: &Path, id: &Hash) -> PathBuf {
        dir.join(id.to_string())
    }

    /// Whether an object with this ID already exists, in the primary store or
    /// any seed store, checked in order.
    pub fn contains(&self, id: &Hash) -> bool {
        Self::object_path(&self.primary, id).exists()
            || self
                .seeds
                .iter()
                .any(|seed| Self::object_path(seed, id).exists())
    }

    /// Writes `bytes` under `id` into the primary store.
    ///
    /// Skips the write if an object with this ID already exists anywhere
    /// (primary or any seed, §6.1). Recomputes the hash of `bytes` under `algo`
    /// and rejects the write if it doesn't match `id` — a caller-supplied ID is
    /// never trusted without recomputing. Uses write-temp → verify → rename so
    /// a process killed mid-write never leaves a partial object in the store
    /// (§8).
    pub fn write(&self, id: &Hash, bytes: &[u8], algo: HashAlgorithm) -> Result<(), DigestError> {
        let actual = hash_bytes(algo, bytes);
        if actual != *id {
            return Err(DigestError::StoreCorrupt {
                expected: *id,
                actual,
            });
        }
        if self.contains(id) {
            return Ok(());
        }
        fs::create_dir_all(&self.primary)?;
        let tmp_path = self.primary.join(format!("{id}.tmp"));
        fs::write(&tmp_path, bytes)?;
        fs::rename(&tmp_path, Self::object_path(&self.primary, id))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("cairn-digest-store-test-{label}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_is_idempotent() {
        let dir = unique_temp_dir("idempotent");
        let store = Store::new(dir.join("primary"), vec![]);
        let bytes = b"hello";
        let id = hash_bytes(HashAlgorithm::Sha256, bytes);

        store.write(&id, bytes, HashAlgorithm::Sha256).unwrap();
        store.write(&id, bytes, HashAlgorithm::Sha256).unwrap();

        assert!(store.contains(&id));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn contains_checks_seed_store() {
        let dir = unique_temp_dir("seed");
        let seed_dir = dir.join("seed");
        let primary_dir = dir.join("primary");
        let seed_store = Store::new(seed_dir.clone(), vec![]);
        let bytes = b"seeded";
        let id = hash_bytes(HashAlgorithm::Sha256, bytes);
        seed_store.write(&id, bytes, HashAlgorithm::Sha256).unwrap();

        let store = Store::new(primary_dir, vec![seed_dir]);
        assert!(store.contains(&id));

        let other_id = hash_bytes(HashAlgorithm::Sha256, b"not present anywhere");
        assert!(!store.contains(&other_id));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_rejects_hash_mismatch_without_writing() {
        let dir = unique_temp_dir("mismatch");
        let primary = dir.join("primary");
        let store = Store::new(primary.clone(), vec![]);
        let wrong_id = Hash([0xffu8; 32]);

        let result = store.write(&wrong_id, b"actual content", HashAlgorithm::Sha256);
        assert!(result.is_err());
        assert!(!store.contains(&wrong_id));
        assert!(!primary.exists() || fs::read_dir(&primary).unwrap().next().is_none());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn no_tmp_file_remains_after_successful_write() {
        let dir = unique_temp_dir("no-tmp");
        let primary = dir.join("primary");
        let store = Store::new(primary.clone(), vec![]);
        let bytes = b"clean write";
        let id = hash_bytes(HashAlgorithm::Sha256, bytes);
        store.write(&id, bytes, HashAlgorithm::Sha256).unwrap();

        let has_tmp = fs::read_dir(&primary).unwrap().any(|entry| {
            entry
                .unwrap()
                .path()
                .extension()
                .is_some_and(|ext| ext == "tmp")
        });
        assert!(!has_tmp);

        fs::remove_dir_all(&dir).unwrap();
    }
}
