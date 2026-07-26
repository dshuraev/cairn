//! Content-addressed chunk store with dedup and atomic writes (§6.1, §8).

use crate::error::StoreError;
use cairn_core::hash::{algo_tag_str, hash_bytes, Hash, HashAlgorithm};
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

    /// The path an object with the given ID would occupy within `dir`: a
    /// per-algorithm subdirectory (sha256/ or blake3/), then filename =
    /// lowercase hex of the ID. This namespacing ensures objects from different
    /// algorithms do not collide (§9). §9 defers on-disk layout entirely; this
    /// is the one function to change if sharding is ever added.
    fn object_path(dir: &Path, algo: HashAlgorithm, id: &Hash) -> PathBuf {
        let algo_dir = algo_tag_str(algo);
        dir.join(algo_dir).join(id.to_string())
    }

    /// Whether an object with this ID already exists, in the primary store or
    /// any seed store (checked in order), under the given hash algorithm.
    pub fn contains(&self, algo: HashAlgorithm, id: &Hash) -> bool {
        Self::object_path(&self.primary, algo, id).exists()
            || self
                .seeds
                .iter()
                .any(|seed| Self::object_path(seed, algo, id).exists())
    }

    /// Writes `bytes` under `id` into the primary store.
    ///
    /// Skips the write if an object with this ID already exists anywhere
    /// (primary or any seed, §6.1). Recomputes the hash of `bytes` under `algo`
    /// and rejects the write if it doesn't match `id` — a caller-supplied ID is
    /// never trusted without recomputing. Uses write-temp → verify → rename so
    /// a process killed mid-write never leaves a partial object in the store
    /// (§8). Tmp files are stored under the algorithm-specific subdirectory to
    /// avoid namespace pollution.
    pub fn write(&self, id: &Hash, bytes: &[u8], algo: HashAlgorithm) -> Result<(), StoreError> {
        let actual = hash_bytes(algo, bytes);
        if actual != *id {
            return Err(StoreError::HashMismatch {
                expected: *id,
                actual,
            });
        }
        if self.contains(algo, id) {
            return Ok(());
        }
        let algo_dir = self.primary.join(algo_tag_str(algo));
        fs::create_dir_all(&algo_dir)?;

        // Use a unique tmp filename per call to avoid TOCTOU race when concurrent
        // calls target the same ID. Derive uniqueness from thread ID + system time nanos.
        let thread_id = std::thread::current().id();
        let time_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let unique_suffix = format!("{:?}-{:x}", thread_id, time_nanos);

        let tmp_path = algo_dir.join(format!("{id}.{unique_suffix}.tmp"));
        fs::write(&tmp_path, bytes)?;
        fs::rename(&tmp_path, Self::object_path(&self.primary, algo, id))?;
        Ok(())
    }

    /// Reads and hash-verifies the object with `id` under `algo` (I2): checks
    /// the primary store then each seed (in order, mirroring `contains`),
    /// reads the first hit, recomputes `H(bytes)` under `algo`, and errors if
    /// it doesn't match `id`. Returns `StoreError::NotFound` if `id` is absent
    /// from every configured store.
    pub fn read(&self, algo: HashAlgorithm, id: &Hash) -> Result<Vec<u8>, StoreError> {
        // Check primary store first
        let primary_path = Self::object_path(&self.primary, algo, id);
        if primary_path.exists() {
            let bytes = fs::read(&primary_path)?;
            let actual = hash_bytes(algo, &bytes);
            if actual != *id {
                return Err(StoreError::HashMismatch {
                    expected: *id,
                    actual,
                });
            }
            return Ok(bytes);
        }

        // Check seed stores in order
        for seed in &self.seeds {
            let seed_path = Self::object_path(seed, algo, id);
            if seed_path.exists() {
                let bytes = fs::read(&seed_path)?;
                let actual = hash_bytes(algo, &bytes);
                if actual != *id {
                    return Err(StoreError::HashMismatch {
                        expected: *id,
                        actual,
                    });
                }
                return Ok(bytes);
            }
        }

        Err(StoreError::NotFound { id: *id })
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
        let dir = std::env::temp_dir().join(format!("cairn-store-test-{label}-{nanos}"));
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

        assert!(store.contains(HashAlgorithm::Sha256, &id));
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
        assert!(store.contains(HashAlgorithm::Sha256, &id));

        let other_id = hash_bytes(HashAlgorithm::Sha256, b"not present anywhere");
        assert!(!store.contains(HashAlgorithm::Sha256, &other_id));

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
        assert!(!store.contains(HashAlgorithm::Sha256, &wrong_id));
        let algo_subdir = primary.join("sha256");
        assert!(!algo_subdir.exists() || fs::read_dir(&algo_subdir).unwrap().next().is_none());

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

        let algo_subdir = primary.join("sha256");
        let has_tmp = fs::read_dir(&algo_subdir).unwrap().any(|entry| {
            entry
                .unwrap()
                .path()
                .extension()
                .is_some_and(|ext| ext == "tmp")
        });
        assert!(!has_tmp);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn concurrent_writes_same_id_are_safe() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let dir = unique_temp_dir("concurrent-write");
        let primary = dir.join("primary");
        let store = Arc::new(Store::new(primary, vec![]));

        let bytes = b"shared content for concurrent test";
        let id = hash_bytes(HashAlgorithm::Sha256, bytes);

        let num_threads = 16;
        let barrier = Arc::new(Barrier::new(num_threads));

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let store_clone = Arc::clone(&store);
                let barrier_clone = Arc::clone(&barrier);
                let id_copy = id;
                let bytes_copy = bytes.to_vec();

                thread::spawn(move || {
                    // Synchronize to maximize chance of concurrent interleaving
                    barrier_clone.wait();
                    store_clone.write(&id_copy, &bytes_copy, HashAlgorithm::Sha256)
                })
            })
            .collect();

        // Collect results from all threads
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // All threads should complete successfully (or harmlessly if object already exists)
        for result in results {
            assert!(result.is_ok());
        }

        // Verify the final committed object is not corrupted:
        // re-hash the bytes on disk and compare against expected ID
        let final_obj_path = Store::object_path(&store.primary, HashAlgorithm::Sha256, &id);
        assert!(
            final_obj_path.exists(),
            "Object should exist after concurrent writes"
        );

        let stored_bytes = fs::read(&final_obj_path).unwrap();
        let stored_hash = hash_bytes(HashAlgorithm::Sha256, &stored_bytes);
        assert_eq!(
            stored_hash, id,
            "Stored object must have correct hash; concurrent writes corrupted it"
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn object_paths_are_namespaced_by_algorithm() {
        let dir = unique_temp_dir("algo-namespace");
        let _store = Store::new(dir.join("primary"), vec![]);

        // Test that object paths differ based on algorithm, preventing collisions.
        // We use the same ID bytes but different algorithms to prove the namespace
        // separation works correctly.
        let test_id = Hash([0x42u8; 32]);

        // Compute paths for this ID under different algorithms
        let sha256_path = Store::object_path(&dir.join("primary"), HashAlgorithm::Sha256, &test_id);
        let blake3_path = Store::object_path(&dir.join("primary"), HashAlgorithm::Blake3, &test_id);

        // Paths must differ (one under sha256/, one under blake3/)
        assert_ne!(
            sha256_path, blake3_path,
            "Paths for the same ID under different algorithms must differ (different subdirectories)"
        );

        // Verify they have different directory components
        assert!(sha256_path.to_string_lossy().contains("sha256"));
        assert!(blake3_path.to_string_lossy().contains("blake3"));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn read_returns_bytes_matching_written_id() {
        let dir = unique_temp_dir("read-write");
        let store = Store::new(dir.join("primary"), vec![]);
        let bytes = b"test content";
        let id = hash_bytes(HashAlgorithm::Sha256, bytes);

        store.write(&id, bytes, HashAlgorithm::Sha256).unwrap();
        let read_bytes = store.read(HashAlgorithm::Sha256, &id).unwrap();

        assert_eq!(read_bytes, bytes);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn read_detects_hash_mismatch() {
        let dir = unique_temp_dir("read-mismatch");
        let primary = dir.join("primary");
        let store = Store::new(primary.clone(), vec![]);
        let bytes = b"original";
        let id = hash_bytes(HashAlgorithm::Sha256, bytes);

        store.write(&id, bytes, HashAlgorithm::Sha256).unwrap();

        // Corrupt the file by writing different bytes directly
        let path = Store::object_path(&primary, HashAlgorithm::Sha256, &id);
        fs::write(&path, b"corrupted").unwrap();

        let result = store.read(HashAlgorithm::Sha256, &id);
        assert!(matches!(result, Err(StoreError::HashMismatch { .. })));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn read_checks_seed_store_in_order() {
        let dir = unique_temp_dir("read-seed");
        let seed_dir = dir.join("seed");
        let primary_dir = dir.join("primary");
        let seed_store = Store::new(seed_dir.clone(), vec![]);
        let bytes = b"seeded content";
        let id = hash_bytes(HashAlgorithm::Sha256, bytes);

        seed_store.write(&id, bytes, HashAlgorithm::Sha256).unwrap();

        let store = Store::new(primary_dir, vec![seed_dir]);
        let read_bytes = store.read(HashAlgorithm::Sha256, &id).unwrap();

        assert_eq!(read_bytes, bytes);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn read_returns_not_found_for_absent_id() {
        let dir = unique_temp_dir("read-not-found");
        let store = Store::new(dir.join("primary"), vec![]);
        let missing_id = Hash([0x99u8; 32]);

        let result = store.read(HashAlgorithm::Sha256, &missing_id);
        assert!(matches!(result, Err(StoreError::NotFound { .. })));

        fs::remove_dir_all(&dir).unwrap();
    }
}
