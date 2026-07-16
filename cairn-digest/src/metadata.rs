//! Builds [`cairn_core::model::Metadata`] from filesystem metadata (§3, §4.4).

use cairn_core::model::Metadata;
use crate::error::DigestError;
use std::os::unix::fs::MetadataExt;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// Builds a `Metadata` object from a file's filesystem metadata and extended attributes.
///
/// Reads the file's extended attributes (xattrs) without following symlinks, which is
/// consistent with the use of `symlink_metadata`/`lstat` semantics throughout the codebase.
/// The xattrs are sorted by name as part of the `Metadata::new` constructor.
pub fn build_metadata(path: &Path, meta: &std::fs::Metadata) -> Result<Metadata, DigestError> {
    // Read xattrs without following symlinks
    let mut xattrs = Vec::new();

    // List all xattr names for this path (without following symlinks)
    match xattr::list(path) {
        Ok(attr_iter) => {
            for attr_name_osstring in attr_iter {
                // Convert OsString to bytes using Unix semantics
                let attr_name_bytes = OsStr::as_bytes(&attr_name_osstring);
                let attr_name = String::from_utf8_lossy(attr_name_bytes).into_owned();
                // Get the value for this attribute (without following symlinks)
                if let Ok(Some(value)) = xattr::get(path, &attr_name_osstring) {
                    xattrs.push((attr_name, value));
                }
            }
        }
        Err(_) => {
            // If xattr listing fails (e.g., no xattrs supported on this filesystem),
            // proceed with empty xattrs list
        }
    }

    Ok(Metadata::new(meta.mode(), meta.uid(), meta.gid(), xattrs))
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
        let built = build_metadata(&path, &meta).unwrap();
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
        let built_a = build_metadata(&path_a, &meta_a).unwrap();
        let built_b = build_metadata(&path_b, &meta_b).unwrap();

        assert_eq!(
            built_a.id(HashAlgorithm::Sha256),
            built_b.id(HashAlgorithm::Sha256)
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn build_metadata_captures_xattrs() {
        let dir = unique_temp_dir("xattr-test");
        let path = dir.join("file_with_xattr");
        fs::write(&path, b"content").unwrap();

        // Try to set an xattr; skip test if filesystem doesn't support it
        let test_attr_name = "user.test_attr";
        let test_attr_value = b"test_value";
        match xattr::set(&path, test_attr_name, test_attr_value) {
            Ok(()) => {
                // Successfully set xattr; verify build_metadata captures it
                let meta = fs::symlink_metadata(&path).unwrap();
                let built = build_metadata(&path, &meta).unwrap();

                // Expected metadata with the xattr included
                let expected = Metadata::new(
                    meta.mode(),
                    meta.uid(),
                    meta.gid(),
                    vec![(test_attr_name.to_string(), test_attr_value.to_vec())],
                );

                // Both should encode and hash identically
                assert_eq!(
                    built.id(HashAlgorithm::Sha256),
                    expected.id(HashAlgorithm::Sha256),
                    "build_metadata should read xattrs from the file"
                );
            }
            Err(e) => {
                // Filesystem doesn't support xattrs; skip test gracefully
                eprintln!(
                    "Skipping xattr test: filesystem doesn't support xattrs ({})",
                    e
                );
            }
        }

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn build_metadata_without_xattrs_matches_before() {
        let dir = unique_temp_dir("no-xattr-test");
        let path = dir.join("file_no_xattr");
        fs::write(&path, b"content").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        let meta = fs::symlink_metadata(&path).unwrap();
        let built = build_metadata(&path, &meta).unwrap();

        // Metadata without any xattrs should match when constructed directly
        let expected = Metadata::new(meta.mode(), meta.uid(), meta.gid(), vec![]);

        assert_eq!(
            built.id(HashAlgorithm::Sha256),
            expected.id(HashAlgorithm::Sha256),
            "Files without xattrs should still work correctly"
        );

        fs::remove_dir_all(&dir).unwrap();
    }
}
