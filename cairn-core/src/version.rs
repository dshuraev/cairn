//! DirTreeBundle format version constant.
//!
//! This is the version value written by the sole producer (`cairn-digest`, via
//! `DirTreeBundle::encode_canonical`). Each consumer binary (`cairn-reconstruct`,
//! `cairn-dirtree`) may independently enforce its own stricter `MAX_SUPPORTED_BUNDLE_VERSION`
//! constant — this module defines only what the writer produces, not what any
//! specific reader accepts.

/// The version number written in `DirTreeBundle` headers produced by this codebase.
/// Currently 0, matching the initial numbered version of the bundle container format.
pub const CURRENT_BUNDLE_VERSION: u8 = 0;
