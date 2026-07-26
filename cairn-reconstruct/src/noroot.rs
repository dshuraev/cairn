//! Skip decision logic and manifest for --no-root mode.

use std::path::PathBuf;

/// One skipped privileged operation under --no-root, for the manifest.
#[derive(Debug, Clone)]
pub struct Skip {
    /// The path where the operation was skipped.
    pub path: PathBuf,
    /// The kind of operation that was skipped.
    pub kind: SkipKind,
    /// Human-readable representation of what was recorded.
    pub recorded: String,
    /// Human-readable representation of what was actually applied.
    pub applied: String,
}

/// The kind of privileged operation that was skipped.
#[derive(Debug, Clone)]
pub enum SkipKind {
    /// A chown operation.
    Chown,
    /// A device node could not be created (--no-root).
    Mknod,
    /// A privileged xattr (security.* or trusted.*) could not be set.
    PrivilegedXattr,
    /// A setuid or setgid bit could not be set.
    SetgidBit,
}

impl SkipKind {
    /// Human-readable name of the skip kind.
    pub fn name(&self) -> &'static str {
        match self {
            SkipKind::Chown => "CHOWN",
            SkipKind::Mknod => "MKNOD",
            SkipKind::PrivilegedXattr => "XATTR",
            SkipKind::SetgidBit => "CHMOD_SETUID_SETGID",
        }
    }
}

/// Decides whether to skip a chown operation under --no-root.
/// Under --no-root, always skips; under strict mode, always attempts.
pub fn decide_chown(no_root: bool, path: PathBuf, uid: u32, gid: u32) -> Option<Skip> {
    if no_root {
        Some(Skip {
            path,
            kind: SkipKind::Chown,
            recorded: format!("uid={} gid={}", uid, gid),
            applied: "invoking process uid/gid".to_string(),
        })
    } else {
        None
    }
}

/// Decides whether to skip a chmod setuid/setgid bit under --no-root.
/// Unconditionally clears them under --no-root; under strict mode, attempts chmod.
pub fn decide_chmod_setbits(no_root: bool, path: PathBuf, mode: u32) -> (u32, Option<Skip>) {
    const S_ISUID: u32 = 0o4000;
    const S_ISGID: u32 = 0o2000;

    let has_set_bits = (mode & (S_ISUID | S_ISGID)) != 0;

    if no_root && has_set_bits {
        (
            mode & !(S_ISUID | S_ISGID),
            Some(Skip {
                path,
                kind: SkipKind::SetgidBit,
                recorded: format!("mode 0o{:o}", mode),
                applied: format!("mode 0o{:o}", mode & !(S_ISUID | S_ISGID)),
            }),
        )
    } else {
        (mode, None)
    }
}

/// Decides whether to skip setting a privileged xattr under --no-root.
/// Privileged: security.* or trusted.*
pub fn is_privileged_xattr(name: &str) -> bool {
    name.starts_with("security.") || name.starts_with("trusted.")
}

/// Decides whether to skip a mknod operation (device creation) under --no-root.
/// Under --no-root, always skips; under strict mode, always attempts.
pub fn decide_mknod(no_root: bool, path: PathBuf, major: u32, minor: u32) -> Option<Skip> {
    if no_root {
        Some(Skip {
            path,
            kind: SkipKind::Mknod,
            recorded: format!("device major={} minor={}", major, minor),
            applied: "node creation skipped".to_string(),
        })
    } else {
        None
    }
}
