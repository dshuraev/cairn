//! Builds [`cairn_core::model::Metadata`] from filesystem metadata (§3, §4.4).

use cairn_core::model::Metadata;
use std::os::unix::fs::MetadataExt;

/// Builds a `Metadata` object from a file's filesystem metadata.
///
/// `xattrs` is always empty for now; real xattr reading is deferred future work.
pub fn build_metadata(meta: &std::fs::Metadata) -> Metadata {
    Metadata::new(meta.mode(), meta.uid(), meta.gid(), vec![])
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use cairn_core::hash::HashAlgorithm;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("cairn-digest-metadata-test-{label}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn mode_matches_set_permissions() {
        let dir = unique_temp_dir("mode");
        let path = dir.join("f");
        fs::write(&path, b"x").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

        let meta = fs::symlink_metadata(&path).unwrap();
        let built = build_metadata(&meta);
        assert_eq!(built.encode_canonical()[0..4], 0o100640u32.to_le_bytes());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn identical_permissions_hash_identically() {
        let dir = unique_temp_dir("identical");
        let path_a = dir.join("a");
        let path_b = dir.join("b");
        fs::write(&path_a, b"x").unwrap();
        fs::write(&path_b, b"y").unwrap();
        fs::set_permissions(&path_a, fs::Permissions::from_mode(0o644)).unwrap();
        fs::set_permissions(&path_b, fs::Permissions::from_mode(0o644)).unwrap();

        let meta_a = fs::symlink_metadata(&path_a).unwrap();
        let meta_b = fs::symlink_metadata(&path_b).unwrap();
        let built_a = build_metadata(&meta_a);
        let built_b = build_metadata(&meta_b);

        assert_eq!(
            built_a.id(HashAlgorithm::Sha256),
            built_b.id(HashAlgorithm::Sha256)
        );

        fs::remove_dir_all(&dir).unwrap();
    }
}
