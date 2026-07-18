//! Output formatting for inspection subcommands.
//!
//! Provides rendering functions for each subcommand in both `txt` (human-readable)
//! and `json` (machine-parseable) formats. All functions operate on resolved
//! node lists and a summary structure as needed.

use crate::walk::ResolvedNode;
use cairn_core::hash::HashAlgorithm;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Output format for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable text.
    Txt,
    /// Machine-parseable JSON.
    Json,
}

/// Helper: render an ID as `algo:hex` string.
fn id_str(algo: HashAlgorithm, hash: &cairn_core::hash::Hash) -> String {
    let algo_str = match algo {
        cairn_core::hash::HashAlgorithm::Sha256 => "sha256",
        cairn_core::hash::HashAlgorithm::Blake3 => "blake3",
    };
    format!("{}:{}", algo_str, hash)
}

/// Helper: mask mode to permission bits only (stored_mode & 0o7777).
fn mask_mode(mode: u32) -> u32 {
    mode & 0o7777
}

// ==============================================================================
// LS SUBCOMMAND
// ==============================================================================

#[derive(Debug, Serialize, Deserialize)]
struct LsEntry {
    path: String,
    kind: String,
    mode: u32,
    uid: u32,
    gid: u32,
    chunk_count: Option<u32>,
}

#[derive(Debug, Serialize)]
struct LsJson {
    root: String,
    entries: Vec<LsEntry>,
}

/// Renders `ls` output (one line per path).
pub fn render_ls(
    nodes: &[ResolvedNode],
    root: &cairn_core::hash::Hash,
    algo: HashAlgorithm,
    format: OutputFormat,
) -> String {
    match format {
        OutputFormat::Txt => render_ls_txt(nodes),
        OutputFormat::Json => render_ls_json(nodes, root, algo),
    }
}

fn render_ls_txt(nodes: &[ResolvedNode]) -> String {
    if nodes.is_empty() {
        return String::new();
    }

    // Calculate column widths
    let mut max_path_len = 4; // "path" header
    let mut max_kind_len = 4; // "kind" header
    for node in nodes {
        max_path_len = max_path_len.max(node.path.len());
        let kind_str = kind_name(&node.kind);
        max_kind_len = max_kind_len.max(kind_str.len());
    }

    let mut output = String::new();

    // Header
    output.push_str("path");
    output.push_str(&" ".repeat(max_path_len - 4 + 2));
    output.push_str("type");
    output.push_str(&" ".repeat(max_kind_len - 4 + 2));
    output.push_str("mode  uid    gid    chunks\n");

    // Rows
    for node in nodes {
        output.push_str(&node.path);
        output.push_str(&" ".repeat(max_path_len - node.path.len() + 2));

        let kind_str = kind_name(&node.kind);
        output.push_str(kind_str);
        output.push_str(&" ".repeat(max_kind_len - kind_str.len() + 2));

        let mode = mask_mode(node.metadata.mode());
        output.push_str(&format!("{:04o}", mode));
        output.push_str("  ");

        output.push_str(&format!("{:5}", node.metadata.uid()));
        output.push_str("  ");

        output.push_str(&format!("{:5}", node.metadata.gid()));
        output.push_str("  ");

        // Chunk count
        match &node.kind {
            crate::walk::ResolvedKind::File { chunks } => {
                output.push_str(&format!("{} chunks", chunks.len()));
            }
            _ => output.push_str("-"),
        }

        output.push('\n');
    }

    output
}

fn render_ls_json(
    nodes: &[ResolvedNode],
    root: &cairn_core::hash::Hash,
    algo: HashAlgorithm,
) -> String {
    let entries: Vec<LsEntry> = nodes
        .iter()
        .map(|node| {
            let kind_str = kind_name(&node.kind);
            let chunk_count = match &node.kind {
                crate::walk::ResolvedKind::File { chunks } => Some(chunks.len() as u32),
                _ => None,
            };
            LsEntry {
                path: node.path.clone(),
                kind: kind_str.to_string(),
                mode: mask_mode(node.metadata.mode()),
                uid: node.metadata.uid(),
                gid: node.metadata.gid(),
                chunk_count,
            }
        })
        .collect();

    let output = LsJson {
        root: id_str(algo, root),
        entries,
    };

    serde_json::to_string_pretty(&output).unwrap_or_default()
}

// ==============================================================================
// STAT SUBCOMMAND
// ==============================================================================

#[derive(Debug, Serialize)]
struct StatJson {
    path: String,
    kind: String,
    mode: u32,
    uid: u32,
    gid: u32,
    link_group: Option<String>,
    xattrs: Vec<StatXattr>,
    target: Option<String>,
    major: Option<u32>,
    minor: Option<u32>,
    chunks: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct StatXattr {
    name: String,
    value_len: usize,
}

/// Renders `stat <path>` output for a single node.
pub fn render_stat(
    node: &ResolvedNode,
    algo: HashAlgorithm,
    format: OutputFormat,
) -> String {
    match format {
        OutputFormat::Txt => render_stat_txt(node),
        OutputFormat::Json => render_stat_json(node, algo),
    }
}

fn render_stat_txt(node: &ResolvedNode) -> String {
    let mut output = String::new();

    output.push_str("path:        ");
    output.push_str(&node.path);
    output.push('\n');

    output.push_str("kind:        ");
    output.push_str(kind_name(&node.kind));
    output.push('\n');

    output.push_str("mode:        ");
    let mode = mask_mode(node.metadata.mode());
    output.push_str(&format!("{:04o}", mode));
    output.push('\n');

    output.push_str("uid:         ");
    output.push_str(&node.metadata.uid().to_string());
    output.push('\n');

    output.push_str("gid:         ");
    output.push_str(&node.metadata.gid().to_string());
    output.push('\n');

    output.push_str("link_group:  ");
    match node.link_group {
        Some(lg) => output.push_str(&id_str(cairn_core::hash::HashAlgorithm::Sha256, &lg.0)),
        None => output.push_str("-"),
    }
    output.push('\n');

    output.push_str("xattrs:      ");
    if node.metadata.xattrs().is_empty() {
        output.push_str("(none)");
    }
    output.push('\n');

    for (name, value) in node.metadata.xattrs() {
        output.push_str("  ");
        output.push_str(name);
        output.push_str(": ");
        output.push_str(&format!("{} bytes", value.len()));
        output.push('\n');
    }

    // Kind-specific fields
    match &node.kind {
        crate::walk::ResolvedKind::File { chunks } => {
            output.push_str("chunks:\n");
            for chunk in chunks {
                output.push_str("  ");
                output.push_str(&id_str(cairn_core::hash::HashAlgorithm::Sha256, &chunk.0));
                output.push('\n');
            }
        }
        crate::walk::ResolvedKind::Symlink { target } => {
            output.push_str("target:      ");
            output.push_str(target);
            output.push('\n');
        }
        crate::walk::ResolvedKind::Device { major, minor } => {
            output.push_str("major:       ");
            output.push_str(&major.to_string());
            output.push('\n');
            output.push_str("minor:       ");
            output.push_str(&minor.to_string());
            output.push('\n');
        }
        _ => {}
    }

    output
}

fn render_stat_json(node: &ResolvedNode, algo: HashAlgorithm) -> String {
    let (target, major, minor, chunks) = match &node.kind {
        crate::walk::ResolvedKind::File { chunks } => (
            None,
            None,
            None,
            Some(
                chunks
                    .iter()
                    .map(|c| id_str(algo, &c.0))
                    .collect::<Vec<_>>(),
            ),
        ),
        crate::walk::ResolvedKind::Symlink { target } => (Some(target.clone()), None, None, None),
        crate::walk::ResolvedKind::Device { major: maj, minor: min } => {
            (None, Some(*maj), Some(*min), None)
        }
        _ => (None, None, None, None),
    };

    let xattrs: Vec<StatXattr> = node
        .metadata
        .xattrs()
        .iter()
        .map(|(name, value)| StatXattr {
            name: name.clone(),
            value_len: value.len(),
        })
        .collect();

    let output = StatJson {
        path: node.path.clone(),
        kind: kind_name(&node.kind).to_string(),
        mode: mask_mode(node.metadata.mode()),
        uid: node.metadata.uid(),
        gid: node.metadata.gid(),
        link_group: node.link_group.map(|lg| id_str(algo, &lg.0)),
        xattrs,
        target,
        major,
        minor,
        chunks,
    };

    serde_json::to_string_pretty(&output).unwrap_or_default()
}

// ==============================================================================
// TREE SUBCOMMAND
// ==============================================================================

#[derive(Debug, Serialize, Clone)]
struct TreeNode {
    name: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    children: Option<Vec<TreeNode>>,
}

/// Renders `tree` output (hierarchical).
pub fn render_tree(nodes: &[ResolvedNode], format: OutputFormat) -> String {
    match format {
        OutputFormat::Txt => render_tree_txt(nodes),
        OutputFormat::Json => render_tree_json(nodes),
    }
}

fn render_tree_txt(nodes: &[ResolvedNode]) -> String {
    let mut output = String::new();
    for node in nodes {
        let depth = node.path.matches('/').count();
        let indent = "  ".repeat(depth);
        output.push_str(&indent);
        output.push_str(&node.name);
        match &node.kind {
            crate::walk::ResolvedKind::Dir { .. } => output.push('/'),
            _ => {}
        }
        output.push('\n');
    }
    output
}

fn render_tree_json(nodes: &[ResolvedNode]) -> String {
    // Build tree structure from flat list recursively.
    fn build_tree(
        nodes: &[ResolvedNode],
        prefix: &str,
        depth: usize,
    ) -> Vec<TreeNode> {
        let mut result = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for node in nodes {
            // Only process nodes at the next depth level
            if !node.path.starts_with(prefix) {
                continue;
            }

            let rel_path = if prefix.is_empty() {
                &node.path
            } else {
                &node.path[prefix.len() + 1..]
            };

            let parts: Vec<&str> = rel_path.split('/').collect();
            if parts.len() == 1 && !seen.contains(parts[0]) {
                // This is a direct child at this level
                seen.insert(parts[0]);
                let kind_str = kind_name(&node.kind);

                let next_prefix = if prefix.is_empty() {
                    node.path.clone()
                } else {
                    format!("{}/{}", prefix, parts[0])
                };

                let children = if matches!(node.kind, crate::walk::ResolvedKind::Dir { .. }) {
                    let child_list = build_tree(nodes, &next_prefix, depth + 1);
                    if child_list.is_empty() {
                        None
                    } else {
                        Some(child_list)
                    }
                } else {
                    None
                };

                result.push(TreeNode {
                    name: parts[0].to_string(),
                    kind: kind_str.to_string(),
                    children,
                });
            }
        }

        result
    }

    let children = build_tree(nodes, "", 0);
    let root = TreeNode {
        name: String::new(),
        kind: "dir".to_string(),
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    };

    serde_json::to_string_pretty(&root).unwrap_or_default()
}

// ==============================================================================
// LINKS SUBCOMMAND
// ==============================================================================

#[derive(Debug, Serialize)]
struct LinkGroup {
    link_group: String,
    paths: Vec<String>,
}

#[derive(Debug, Serialize)]
struct LinksJson {
    groups: Vec<LinkGroup>,
}

/// Renders `links` output (hardlink groups).
pub fn render_links(nodes: &[ResolvedNode], algo: HashAlgorithm, format: OutputFormat) -> String {
    match format {
        OutputFormat::Txt => render_links_txt(nodes, algo),
        OutputFormat::Json => render_links_json(nodes, algo),
    }
}

fn render_links_txt(nodes: &[ResolvedNode], algo: HashAlgorithm) -> String {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for node in nodes {
        if let Some(lg) = node.link_group {
            let lg_str = id_str(algo, &lg.0);
            groups.entry(lg_str).or_default().push(node.path.clone());
        }
    }

    let mut output = String::new();
    for (lg, mut paths) in groups {
        output.push_str(&lg);
        output.push('\n');
        paths.sort();
        for path in paths {
            output.push_str("  ");
            output.push_str(&path);
            output.push('\n');
        }
    }
    output
}

fn render_links_json(nodes: &[ResolvedNode], algo: HashAlgorithm) -> String {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for node in nodes {
        if let Some(lg) = node.link_group {
            let lg_str = id_str(algo, &lg.0);
            groups.entry(lg_str).or_default().push(node.path.clone());
        }
    }

    let mut group_vec: Vec<LinkGroup> = groups
        .into_iter()
        .map(|(link_group, mut paths)| {
            paths.sort();
            LinkGroup { link_group, paths }
        })
        .collect();

    group_vec.sort_by(|a, b| a.link_group.cmp(&b.link_group));

    let output = LinksJson {
        groups: group_vec,
    };

    serde_json::to_string_pretty(&output).unwrap_or_default()
}

// ==============================================================================
// SYMLINKS SUBCOMMAND
// ==============================================================================

#[derive(Debug, Serialize)]
struct SymlinkEntry {
    path: String,
    target: String,
}

#[derive(Debug, Serialize)]
struct SymlinksJson {
    symlinks: Vec<SymlinkEntry>,
}

/// Renders `symlinks` output (paths and their targets).
pub fn render_symlinks(nodes: &[ResolvedNode], format: OutputFormat) -> String {
    match format {
        OutputFormat::Txt => render_symlinks_txt(nodes),
        OutputFormat::Json => render_symlinks_json(nodes),
    }
}

fn render_symlinks_txt(nodes: &[ResolvedNode]) -> String {
    let mut output = String::new();
    for node in nodes {
        if let crate::walk::ResolvedKind::Symlink { target } = &node.kind {
            output.push_str(&node.path);
            output.push_str(" -> ");
            output.push_str(target);
            output.push('\n');
        }
    }
    output
}

fn render_symlinks_json(nodes: &[ResolvedNode]) -> String {
    let symlinks: Vec<SymlinkEntry> = nodes
        .iter()
        .filter_map(|node| {
            if let crate::walk::ResolvedKind::Symlink { target } = &node.kind {
                Some(SymlinkEntry {
                    path: node.path.clone(),
                    target: target.clone(),
                })
            } else {
                None
            }
        })
        .collect();

    let output = SymlinksJson { symlinks };
    serde_json::to_string_pretty(&output).unwrap_or_default()
}

// ==============================================================================
// SPECIAL SUBCOMMAND
// ==============================================================================

#[derive(Debug, Serialize)]
struct SpecialEntry {
    path: String,
    kind: String,
    major: Option<u32>,
    minor: Option<u32>,
}

#[derive(Debug, Serialize)]
struct SpecialJson {
    special: Vec<SpecialEntry>,
}

/// Renders `special` output (device/fifo/socket nodes).
pub fn render_special(nodes: &[ResolvedNode], format: OutputFormat) -> String {
    match format {
        OutputFormat::Txt => render_special_txt(nodes),
        OutputFormat::Json => render_special_json(nodes),
    }
}

fn render_special_txt(nodes: &[ResolvedNode]) -> String {
    let mut output = String::new();
    for node in nodes {
        match &node.kind {
            crate::walk::ResolvedKind::Device { major, minor } => {
                output.push_str(&node.path);
                output.push_str("  device  ");
                output.push_str(&format!("{}:{}", major, minor));
                output.push('\n');
            }
            crate::walk::ResolvedKind::Fifo => {
                output.push_str(&node.path);
                output.push_str("  fifo  -\n");
            }
            crate::walk::ResolvedKind::Socket => {
                output.push_str(&node.path);
                output.push_str("  socket  -\n");
            }
            _ => {}
        }
    }
    output
}

fn render_special_json(nodes: &[ResolvedNode]) -> String {
    let special: Vec<SpecialEntry> = nodes
        .iter()
        .filter_map(|node| {
            match &node.kind {
                crate::walk::ResolvedKind::Device { major, minor } => {
                    Some(SpecialEntry {
                        path: node.path.clone(),
                        kind: "device".to_string(),
                        major: Some(*major),
                        minor: Some(*minor),
                    })
                }
                crate::walk::ResolvedKind::Fifo => {
                    Some(SpecialEntry {
                        path: node.path.clone(),
                        kind: "fifo".to_string(),
                        major: None,
                        minor: None,
                    })
                }
                crate::walk::ResolvedKind::Socket => {
                    Some(SpecialEntry {
                        path: node.path.clone(),
                        kind: "socket".to_string(),
                        major: None,
                        minor: None,
                    })
                }
                _ => None,
            }
        })
        .collect();

    let output = SpecialJson { special };
    serde_json::to_string_pretty(&output).unwrap_or_default()
}

// ==============================================================================
// XATTRS SUBCOMMAND
// ==============================================================================

#[derive(Debug, Serialize)]
struct XattrEntry {
    name: String,
    value_len: usize,
}

#[derive(Debug, Serialize)]
struct XattrPathEntry {
    path: String,
    xattrs: Vec<XattrEntry>,
}

#[derive(Debug, Serialize)]
struct XattrsJson {
    paths: Vec<XattrPathEntry>,
}

/// Renders `xattrs [--prefix <name>]` output.
pub fn render_xattrs(
    nodes: &[ResolvedNode],
    prefix: Option<&str>,
    format: OutputFormat,
) -> String {
    match format {
        OutputFormat::Txt => render_xattrs_txt(nodes, prefix),
        OutputFormat::Json => render_xattrs_json(nodes, prefix),
    }
}

fn render_xattrs_txt(nodes: &[ResolvedNode], prefix: Option<&str>) -> String {
    let mut output = String::new();
    for node in nodes {
        let matching_xattrs: Vec<_> = node
            .metadata
            .xattrs()
            .iter()
            .filter(|(name, _)| match prefix {
                Some(p) => name.starts_with(p),
                None => true,
            })
            .collect();

        if !matching_xattrs.is_empty() {
            output.push_str(&node.path);
            output.push('\n');
            for (name, value) in matching_xattrs {
                output.push_str("  ");
                output.push_str(name);
                output.push_str(": ");
                output.push_str(&format!("{} bytes", value.len()));
                output.push('\n');
            }
        }
    }
    output
}

fn render_xattrs_json(nodes: &[ResolvedNode], prefix: Option<&str>) -> String {
    let mut paths = Vec::new();
    for node in nodes {
        let matching_xattrs: Vec<XattrEntry> = node
            .metadata
            .xattrs()
            .iter()
            .filter(|(name, _)| match prefix {
                Some(p) => name.starts_with(p),
                None => true,
            })
            .map(|(name, value)| XattrEntry {
                name: name.clone(),
                value_len: value.len(),
            })
            .collect();

        if !matching_xattrs.is_empty() {
            paths.push(XattrPathEntry {
                path: node.path.clone(),
                xattrs: matching_xattrs,
            });
        }
    }

    let output = XattrsJson { paths };
    serde_json::to_string_pretty(&output).unwrap_or_default()
}

// ==============================================================================
// SUMMARY SUBCOMMAND
// ==============================================================================

#[derive(Debug, Serialize)]
struct SummaryJson {
    files: usize,
    dirs: usize,
    symlinks: usize,
    specials: usize,
    hardlink_groups: usize,
    distinct_chunk_ids: usize,
    total_nodes: usize,
}

/// Summary statistics computed from nodes.
pub struct Summary {
    pub files: usize,
    pub dirs: usize,
    pub symlinks: usize,
    pub specials: usize,
    pub hardlink_groups: usize,
    pub distinct_chunk_ids: usize,
    pub total_nodes: usize,
}

impl Summary {
    /// Computes summary statistics from a node list.
    pub fn compute(nodes: &[ResolvedNode]) -> Self {
        let mut files = 0;
        let mut dirs = 0;
        let mut symlinks = 0;
        let mut specials = 0;
        let mut chunk_ids = std::collections::HashSet::new();
        let mut link_groups = std::collections::HashSet::new();

        for node in nodes {
            match &node.kind {
                crate::walk::ResolvedKind::File { chunks } => {
                    files += 1;
                    for chunk in chunks {
                        chunk_ids.insert(chunk.0);
                    }
                }
                crate::walk::ResolvedKind::Dir { .. } => dirs += 1,
                crate::walk::ResolvedKind::Symlink { .. } => symlinks += 1,
                crate::walk::ResolvedKind::Device { .. }
                | crate::walk::ResolvedKind::Fifo
                | crate::walk::ResolvedKind::Socket => specials += 1,
            }

            if let Some(lg) = node.link_group {
                link_groups.insert(lg.0);
            }
        }

        Summary {
            files,
            dirs,
            symlinks,
            specials,
            hardlink_groups: link_groups.len(),
            distinct_chunk_ids: chunk_ids.len(),
            total_nodes: nodes.len(),
        }
    }
}

/// Renders `summary` output.
pub fn render_summary(summary: &Summary, format: OutputFormat) -> String {
    match format {
        OutputFormat::Txt => render_summary_txt(summary),
        OutputFormat::Json => render_summary_json(summary),
    }
}

fn render_summary_txt(summary: &Summary) -> String {
    let mut output = String::new();
    output.push_str("files:              ");
    output.push_str(&summary.files.to_string());
    output.push('\n');
    output.push_str("dirs:               ");
    output.push_str(&summary.dirs.to_string());
    output.push('\n');
    output.push_str("symlinks:           ");
    output.push_str(&summary.symlinks.to_string());
    output.push('\n');
    output.push_str("specials:           ");
    output.push_str(&summary.specials.to_string());
    output.push('\n');
    output.push_str("hardlink groups:    ");
    output.push_str(&summary.hardlink_groups.to_string());
    output.push('\n');
    output.push_str("distinct chunk IDs: ");
    output.push_str(&summary.distinct_chunk_ids.to_string());
    output.push('\n');
    output.push_str("total nodes:        ");
    output.push_str(&summary.total_nodes.to_string());
    output.push('\n');
    output
}

fn render_summary_json(summary: &Summary) -> String {
    let output = SummaryJson {
        files: summary.files,
        dirs: summary.dirs,
        symlinks: summary.symlinks,
        specials: summary.specials,
        hardlink_groups: summary.hardlink_groups,
        distinct_chunk_ids: summary.distinct_chunk_ids,
        total_nodes: summary.total_nodes,
    };

    serde_json::to_string_pretty(&output).unwrap_or_default()
}

// ==============================================================================
// HELPERS
// ==============================================================================

/// Returns the kind name as a string.
fn kind_name(kind: &crate::walk::ResolvedKind) -> &'static str {
    match kind {
        crate::walk::ResolvedKind::File { .. } => "file",
        crate::walk::ResolvedKind::Dir { .. } => "dir",
        crate::walk::ResolvedKind::Symlink { .. } => "symlink",
        crate::walk::ResolvedKind::Device { .. } => "device",
        crate::walk::ResolvedKind::Fifo => "fifo",
        crate::walk::ResolvedKind::Socket => "socket",
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use cairn_core::hash::hash_bytes;
    use cairn_core::id::{ChunkID, LinkGroupID};
    use cairn_core::model::Metadata;

    /// Helper: create a Metadata with mode, uid, gid, and optional xattrs.
    fn make_metadata(mode: u32, uid: u32, gid: u32, xattrs: Vec<(String, Vec<u8>)>) -> Metadata {
        Metadata::new(mode, uid, gid, xattrs)
    }

    /// Helper: create a ResolvedNode for testing.
    fn make_node(
        path: &str,
        name: &str,
        kind: crate::walk::ResolvedKind,
        mode: u32,
        uid: u32,
        gid: u32,
        link_group: Option<LinkGroupID>,
    ) -> ResolvedNode {
        ResolvedNode {
            path: path.to_string(),
            name: name.to_string(),
            kind,
            metadata: make_metadata(mode, uid, gid, vec![]),
            link_group,
        }
    }

    #[test]
    fn render_ls_txt_single_file() {
        let algo = HashAlgorithm::Sha256;
        let chunk = ChunkID(hash_bytes(algo, b"chunk1"));
        let node = make_node(
            "hello.txt",
            "hello.txt",
            crate::walk::ResolvedKind::File {
                chunks: vec![chunk],
            },
            0o100644,
            1000,
            1000,
            None,
        );

        let root = hash_bytes(algo, b"root");
        let txt = render_ls(&[node], &root, algo, OutputFormat::Txt);

        assert!(txt.contains("hello.txt"));
        assert!(txt.contains("file"));
        assert!(txt.contains("0644"));
        assert!(txt.contains("1000"));
        assert!(txt.contains("1 chunks"));
    }

    #[test]
    fn render_ls_json_single_file() {
        let algo = HashAlgorithm::Sha256;
        let chunk = ChunkID(hash_bytes(algo, b"chunk1"));
        let node = make_node(
            "hello.txt",
            "hello.txt",
            crate::walk::ResolvedKind::File {
                chunks: vec![chunk],
            },
            0o100644,
            1000,
            1000,
            None,
        );

        let root = hash_bytes(algo, b"root");
        let json_str = render_ls(&[node], &root, algo, OutputFormat::Json);
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(json["entries"][0]["path"], "hello.txt");
        assert_eq!(json["entries"][0]["kind"], "file");
        assert_eq!(json["entries"][0]["mode"], 0o644);
        assert_eq!(json["entries"][0]["chunk_count"], 1);
    }

    #[test]
    fn render_stat_txt_file() {
        let algo = HashAlgorithm::Sha256;
        let chunk = ChunkID(hash_bytes(algo, b"chunk1"));
        let node = make_node(
            "hello.txt",
            "hello.txt",
            crate::walk::ResolvedKind::File {
                chunks: vec![chunk],
            },
            0o100644,
            1000,
            1000,
            None,
        );

        let txt = render_stat(&node, algo, OutputFormat::Txt);

        assert!(txt.contains("path:        hello.txt"));
        assert!(txt.contains("kind:        file"));
        assert!(txt.contains("0644"));
    }

    #[test]
    fn render_stat_json_file() {
        let algo = HashAlgorithm::Sha256;
        let chunk = ChunkID(hash_bytes(algo, b"chunk1"));
        let node = make_node(
            "hello.txt",
            "hello.txt",
            crate::walk::ResolvedKind::File {
                chunks: vec![chunk],
            },
            0o100644,
            1000,
            1000,
            None,
        );

        let json_str = render_stat(&node, algo, OutputFormat::Json);
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(json["path"], "hello.txt");
        assert_eq!(json["kind"], "file");
        assert_eq!(json["mode"], 0o644);
    }

    #[test]
    fn mode_masking_in_render_ls_txt() {
        let algo = HashAlgorithm::Sha256;
        let node = make_node(
            "regular",
            "regular",
            crate::walk::ResolvedKind::File { chunks: vec![] },
            0o100644, // raw st_mode with S_IFREG bits
            0,
            0,
            None,
        );

        let root = hash_bytes(algo, b"root");
        let txt = render_ls(&[node], &root, algo, OutputFormat::Txt);

        // Should display 0644, not 0o100644
        assert!(txt.contains("0644"));
        assert!(!txt.contains("100644"));
    }

    #[test]
    fn mode_masking_in_render_ls_json() {
        let algo = HashAlgorithm::Sha256;
        let node = make_node(
            "regular",
            "regular",
            crate::walk::ResolvedKind::File { chunks: vec![] },
            0o100644, // raw st_mode with S_IFREG bits
            0,
            0,
            None,
        );

        let root = hash_bytes(algo, b"root");
        let json_str = render_ls(&[node], &root, algo, OutputFormat::Json);
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        // Mode in JSON should be masked decimal value (0o644 = 420 in decimal)
        assert_eq!(json["entries"][0]["mode"], 0o644 as u32);
        assert!(json["entries"][0]["mode"] != 0o100644 as u32);
    }

    #[test]
    fn mode_masking_in_render_stat_txt() {
        let algo = HashAlgorithm::Sha256;
        let node = make_node(
            "regular",
            "regular",
            crate::walk::ResolvedKind::File { chunks: vec![] },
            0o100644,
            0,
            0,
            None,
        );

        let txt = render_stat(&node, algo, OutputFormat::Txt);

        // Should display 0644, not 0o100644
        assert!(txt.contains("0644"));
        assert!(!txt.contains("100644"));
    }

    #[test]
    fn render_tree_txt() {
        let nodes = vec![
            make_node(
                "dir1",
                "dir1",
                crate::walk::ResolvedKind::Dir { child_count: 1 },
                0o40755,
                0,
                0,
                None,
            ),
            make_node(
                "dir1/file.txt",
                "file.txt",
                crate::walk::ResolvedKind::File { chunks: vec![] },
                0o100644,
                0,
                0,
                None,
            ),
        ];

        let txt = render_tree(&nodes, OutputFormat::Txt);

        assert!(txt.contains("dir1/"));
        assert!(txt.contains("  file.txt"));
    }

    #[test]
    fn render_symlinks_txt() {
        let node = make_node(
            "mylink",
            "mylink",
            crate::walk::ResolvedKind::Symlink {
                target: "/etc/passwd".to_string(),
            },
            0o120777,
            0,
            0,
            None,
        );

        let txt = render_symlinks(&[node], OutputFormat::Txt);

        assert!(txt.contains("mylink -> /etc/passwd"));
    }

    #[test]
    fn render_special_txt() {
        let dev_node = make_node(
            "tty",
            "tty",
            crate::walk::ResolvedKind::Device {
                major: 5,
                minor: 0,
            },
            0o20666,
            0,
            0,
            None,
        );
        let fifo_node = make_node(
            "myfifo",
            "myfifo",
            crate::walk::ResolvedKind::Fifo,
            0o10644,
            0,
            0,
            None,
        );

        let txt = render_special(&[dev_node, fifo_node], OutputFormat::Txt);

        assert!(txt.contains("tty") && txt.contains("device") && txt.contains("5:0"));
        assert!(txt.contains("myfifo") && txt.contains("fifo"));
    }

    #[test]
    fn render_links_txt_multiple_groups() {
        let algo = HashAlgorithm::Sha256;
        let lg1 = LinkGroupID(hash_bytes(algo, b"lg1"));
        let lg2 = LinkGroupID(hash_bytes(algo, b"lg2"));

        let node1 = ResolvedNode {
            path: "file1a".to_string(),
            name: "file1a".to_string(),
            kind: crate::walk::ResolvedKind::File { chunks: vec![] },
            metadata: make_metadata(0o644, 0, 0, vec![]),
            link_group: Some(lg1),
        };
        let node2 = ResolvedNode {
            path: "file1b".to_string(),
            name: "file1b".to_string(),
            kind: crate::walk::ResolvedKind::File { chunks: vec![] },
            metadata: make_metadata(0o644, 0, 0, vec![]),
            link_group: Some(lg1),
        };
        let node3 = ResolvedNode {
            path: "file2a".to_string(),
            name: "file2a".to_string(),
            kind: crate::walk::ResolvedKind::File { chunks: vec![] },
            metadata: make_metadata(0o644, 0, 0, vec![]),
            link_group: Some(lg2),
        };

        let txt = render_links(&[node1, node2, node3], algo, OutputFormat::Txt);

        // Should contain both groups
        assert!(txt.contains("file1a"));
        assert!(txt.contains("file1b"));
        assert!(txt.contains("file2a"));
    }

    #[test]
    fn render_xattrs_txt_with_prefix_filter() {
        let _algo = HashAlgorithm::Sha256;
        let node = ResolvedNode {
            path: "file.txt".to_string(),
            name: "file.txt".to_string(),
            kind: crate::walk::ResolvedKind::File { chunks: vec![] },
            metadata: make_metadata(
                0o644,
                0,
                0,
                vec![
                    ("user.foo".to_string(), vec![1, 2, 3]),
                    ("user.bar".to_string(), vec![4, 5]),
                    ("system.attr".to_string(), vec![6]),
                ],
            ),
            link_group: None,
        };

        let txt_all = render_xattrs(&[node.clone()], None, OutputFormat::Txt);
        assert!(txt_all.contains("user.foo"));
        assert!(txt_all.contains("user.bar"));
        assert!(txt_all.contains("system.attr"));

        let txt_user = render_xattrs(&[node], Some("user."), OutputFormat::Txt);
        assert!(txt_user.contains("user.foo"));
        assert!(txt_user.contains("user.bar"));
        assert!(!txt_user.contains("system.attr"));
    }

    #[test]
    fn render_summary_computes_correct_stats() {
        let algo = HashAlgorithm::Sha256;
        let chunk1 = ChunkID(hash_bytes(algo, b"chunk1"));
        let chunk2 = ChunkID(hash_bytes(algo, b"chunk2"));
        let lg = LinkGroupID(hash_bytes(algo, b"lg"));

        let nodes = vec![
            make_node(
                "file1.txt",
                "file1.txt",
                crate::walk::ResolvedKind::File {
                    chunks: vec![chunk1, chunk2],
                },
                0o644,
                0,
                0,
                Some(lg),
            ),
            make_node(
                "file2.txt",
                "file2.txt",
                crate::walk::ResolvedKind::File {
                    chunks: vec![chunk1], // shared chunk
                },
                0o644,
                0,
                0,
                Some(lg),
            ),
            make_node(
                "dir",
                "dir",
                crate::walk::ResolvedKind::Dir { child_count: 2 },
                0o755,
                0,
                0,
                None,
            ),
            make_node(
                "link",
                "link",
                crate::walk::ResolvedKind::Symlink {
                    target: "/etc".to_string(),
                },
                0o777,
                0,
                0,
                None,
            ),
        ];

        let summary = Summary::compute(&nodes);

        assert_eq!(summary.files, 2);
        assert_eq!(summary.dirs, 1);
        assert_eq!(summary.symlinks, 1);
        assert_eq!(summary.specials, 0);
        assert_eq!(summary.hardlink_groups, 1);
        assert_eq!(summary.distinct_chunk_ids, 2); // chunk1 and chunk2
        assert_eq!(summary.total_nodes, 4);

        let txt = render_summary(&summary, OutputFormat::Txt);
        assert!(txt.contains("files:              2"));
        assert!(txt.contains("dirs:               1"));
        assert!(txt.contains("symlinks:           1"));
        assert!(txt.contains("hardlink groups:    1"));
        assert!(txt.contains("distinct chunk IDs: 2"));
        assert!(txt.contains("total nodes:        4"));
    }
}
