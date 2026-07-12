use cairn_core::hash::HashAlgorithm;
use cairn_digest::walk::{walk_tree, RawKind, WalkEntry};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("cairn-digest-walk-test-{label}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn child<'a>(entry: &'a WalkEntry, name: &str) -> &'a WalkEntry {
    entry
        .children
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no child named {name} under {:?}", entry.path))
}

#[test]
fn walk_classifies_entries_and_resolves_cross_directory_hardlinks() {
    let root = unique_temp_dir("mixed");

    // 1 plain file.
    std::fs::write(root.join("file.txt"), b"hello").unwrap();

    // 1 subdirectory.
    std::fs::create_dir(root.join("subdir")).unwrap();

    // 1 dangling symlink: recorded, not followed or errored on.
    std::os::unix::fs::symlink("/nonexistent/target", root.join("dangling.symlink")).unwrap();

    // 2 hardlinked files in different subdirectories.
    std::fs::create_dir(root.join("linkA")).unwrap();
    std::fs::create_dir(root.join("linkB")).unwrap();
    std::fs::write(root.join("linkA/shared.bin"), b"shared content").unwrap();
    std::fs::hard_link(root.join("linkA/shared.bin"), root.join("linkB/shared.bin")).unwrap();

    // 1 Unix domain socket (std-only, no new dependency needed).
    let socket_path = root.join("socket.sock");
    let _listener = UnixListener::bind(&socket_path).unwrap();

    let (root_entry, tracker) = walk_tree(&root, HashAlgorithm::Sha256).unwrap();

    // Plain file.
    let file_entry = child(&root_entry, "file.txt");
    assert_eq!(file_entry.kind, RawKind::File);
    let file_inode = cairn_digest::hardlink::Inode {
        device: std::os::unix::fs::MetadataExt::dev(&file_entry.metadata),
        inode: std::os::unix::fs::MetadataExt::ino(&file_entry.metadata),
    };
    assert_eq!(
        tracker.link_group(file_inode),
        None,
        "a standalone file must not get a link group"
    );

    // Subdirectory.
    let subdir_entry = child(&root_entry, "subdir");
    assert_eq!(subdir_entry.kind, RawKind::Dir);
    assert!(subdir_entry.children.is_empty());

    // Dangling symlink.
    let symlink_entry = child(&root_entry, "dangling.symlink");
    assert_eq!(
        symlink_entry.kind,
        RawKind::Symlink {
            target: "/nonexistent/target".to_string()
        }
    );

    // Socket.
    let socket_entry = child(&root_entry, "socket.sock");
    assert_eq!(socket_entry.kind, RawKind::Socket);

    // Cross-directory hardlink pair: this is the case Fix 1 exists for. Both
    // paths must resolve to the same LinkGroupID after the *full* walk,
    // regardless of which directory ("linkA" or "linkB") was visited first.
    let link_a_dir = child(&root_entry, "linkA");
    let link_b_dir = child(&root_entry, "linkB");
    let file_a = child(link_a_dir, "shared.bin");
    let file_b = child(link_b_dir, "shared.bin");

    let inode_a = cairn_digest::hardlink::Inode {
        device: std::os::unix::fs::MetadataExt::dev(&file_a.metadata),
        inode: std::os::unix::fs::MetadataExt::ino(&file_a.metadata),
    };
    let inode_b = cairn_digest::hardlink::Inode {
        device: std::os::unix::fs::MetadataExt::dev(&file_b.metadata),
        inode: std::os::unix::fs::MetadataExt::ino(&file_b.metadata),
    };
    assert_eq!(inode_a, inode_b, "hardlinked files must share one inode");

    let group_a = tracker.link_group(inode_a);
    let group_b = tracker.link_group(inode_b);
    assert!(group_a.is_some(), "hardlinked file must have a link group");
    assert_eq!(group_a, group_b);

    std::fs::remove_dir_all(&root).unwrap();
}
