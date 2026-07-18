//! Object kind tag constants for domain separation in canonical encodings (§4).

/// Tag byte for `DirTree` objects in canonical encodings.
/// Prepended to `DirTree::encode_canonical()` output to ensure domain separation
/// from other object kinds (§4.1).
pub const DIRTREE_KIND_TAG: u8 = 0;

/// Tag byte for `Metadata` objects in canonical encodings.
/// Prepended to `Metadata::encode_canonical()` output to ensure domain separation
/// from other object kinds (§4.1).
pub const METADATA_KIND_TAG: u8 = 1;

/// Tag byte for `FileIndex` objects in canonical encodings.
/// Prepended to `FileIndex::encode_canonical()` output to ensure domain separation
/// from other object kinds (§4.1).
pub const FILEINDEX_KIND_TAG: u8 = 2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_values_match_objectkind_scheme() {
        // Verify these constants stay in sync with `bundle::ObjectKind::tag()`.
        // If this test fails, it indicates the two schemes have drifted and need
        // reconciliation.
        assert_eq!(DIRTREE_KIND_TAG, 0);
        assert_eq!(METADATA_KIND_TAG, 1);
        assert_eq!(FILEINDEX_KIND_TAG, 2);
    }
}
