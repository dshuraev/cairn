//! Content-addressed ID newtypes wrapping [`crate::hash::Hash`].

use crate::hash::Hash;
use std::fmt;

macro_rules! define_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(pub Hash);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

define_id!(
    /// Identifies a `Chunk` object (§3).
    ChunkID
);
define_id!(
    /// Identifies a `FileIndex` object (§3).
    FileIndexID
);
define_id!(
    /// Identifies a `Metadata` object (§3).
    MetadataID
);
define_id!(
    /// Identifies a `DirTree` object (§3).
    DirTreeID
);
define_id!(
    /// Identifies a hardlink group (§3, §5.2). Not itself a content hash.
    LinkGroupID
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_display_matches_inner_hash() {
        let hash = Hash([7u8; 32]);
        assert_eq!(ChunkID(hash).to_string(), hash.to_string());
        assert_eq!(FileIndexID(hash).to_string(), hash.to_string());
        assert_eq!(MetadataID(hash).to_string(), hash.to_string());
        assert_eq!(DirTreeID(hash).to_string(), hash.to_string());
        assert_eq!(LinkGroupID(hash).to_string(), hash.to_string());
    }
}
