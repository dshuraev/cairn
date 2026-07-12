//! Git tree-sort comparator (§4.3).

use std::cmp::Ordering;

/// Compares two directory entry names using git's tree-sort convention: as if
/// every `Dir`-kind entry's name had an implicit trailing `/` appended, for
/// comparison purposes only (never stored).
///
/// This avoids the `foo` vs `foo.bar` vs `foo/` ambiguity a naive byte-sort
/// produces, since `.` (0x2e) sorts before `/` (0x2f) in ASCII.
pub fn git_tree_cmp(a_name: &str, a_is_dir: bool, b_name: &str, b_is_dir: bool) -> Ordering {
    let a_bytes = a_name
        .as_bytes()
        .iter()
        .copied()
        .chain(a_is_dir.then_some(b'/'));
    let b_bytes = b_name
        .as_bytes()
        .iter()
        .copied()
        .chain(b_is_dir.then_some(b'/'));
    a_bytes.cmp(b_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_sorts_before_similarly_prefixed_file() {
        // "a" (dir) compares as "a/", which sorts before "aa" (file).
        assert_eq!(git_tree_cmp("a", true, "aa", false), Ordering::Less);
    }

    #[test]
    fn file_before_dotted_file_before_same_named_dir() {
        // '.' (0x2e) < '/' (0x2f) in ASCII, so:
        // "foo" (file) < "foo.bar" (file) < "foo" (dir, compares as "foo/")
        assert_eq!(git_tree_cmp("foo", false, "foo.bar", false), Ordering::Less);
        assert_eq!(git_tree_cmp("foo.bar", false, "foo", true), Ordering::Less);
        assert_eq!(git_tree_cmp("foo", false, "foo", true), Ordering::Less);
    }

    #[test]
    fn equal_names_and_kinds_are_equal() {
        assert_eq!(git_tree_cmp("foo", false, "foo", false), Ordering::Equal);
        assert_eq!(git_tree_cmp("foo", true, "foo", true), Ordering::Equal);
    }
}
