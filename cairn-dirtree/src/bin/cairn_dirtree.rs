//! CLI binary for cairn-dirtree: inspect dirtree bundles.
//!
//! Exit codes: 0 success, 1 any error (bad bundle, decode failure,
//! path-not-found for stat, etc.).

use anyhow::Context;
use cairn_core::bundle::DirTreeBundle;
use cairn_dirtree::render::{self, OutputFormat};
use cairn_dirtree::walk;
use clap::{Parser, Subcommand, ValueEnum};
use std::fs;
use std::path::{Path, PathBuf};

/// Local wrapper around [`OutputFormat`] for clap compatibility.
#[derive(ValueEnum, Clone, Copy, Debug)]
enum OutputArg {
    /// Human-readable text format.
    #[value(name = "txt")]
    Txt,
    /// Machine-parseable JSON.
    #[value(name = "json")]
    Json,
}

impl OutputArg {
    fn to_format(self) -> OutputFormat {
        match self {
            OutputArg::Txt => OutputFormat::Txt,
            OutputArg::Json => OutputFormat::Json,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "cairn-dirtree")]
#[command(about = "Inspect dirtree bundles")]
#[command(long_about = "Exit codes: 0 success, 1 any error")]
struct Cli {
    /// Output format: txt (default) or json
    #[arg(short, long, value_enum, default_value = "txt")]
    output: OutputArg,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// List files and directories
    Ls {
        /// Path to dirtree bundle
        #[arg(short, long, required = true)]
        input: PathBuf,
    },

    /// Display node metadata
    Stat {
        /// Path to dirtree bundle
        #[arg(short, long, required = true)]
        input: PathBuf,
        /// Path within the bundle
        path: String,
    },

    /// Show hierarchical tree view
    Tree {
        /// Path to dirtree bundle
        #[arg(short, long, required = true)]
        input: PathBuf,
    },

    /// List hardlink groups
    Links {
        /// Path to dirtree bundle
        #[arg(short, long, required = true)]
        input: PathBuf,
    },

    /// List symlinks and their targets
    Symlinks {
        /// Path to dirtree bundle
        #[arg(short, long, required = true)]
        input: PathBuf,
    },

    /// List device/fifo/socket nodes
    Special {
        /// Path to dirtree bundle
        #[arg(short, long, required = true)]
        input: PathBuf,
    },

    /// List extended attributes
    Xattrs {
        /// Path to dirtree bundle
        #[arg(short, long, required = true)]
        input: PathBuf,
        /// Filter by xattr name prefix
        #[arg(long)]
        prefix: Option<String>,
    },

    /// Show summary statistics
    Summary {
        /// Path to dirtree bundle
        #[arg(short, long, required = true)]
        input: PathBuf,
    },
}

/// Reads and decodes a dirtree bundle from a file.
fn read_bundle(
    path: &Path,
) -> anyhow::Result<(cairn_core::id::DirTreeID, cairn_core::hash::HashAlgorithm, DirTreeBundle)> {
    let bytes = fs::read(path).context("failed to read bundle file")?;
    let (root_id, algo, bundle) = DirTreeBundle::decode_canonical(&bytes)
        .context("failed to decode bundle")?;
    Ok((root_id, algo, bundle))
}

/// Runs the inspection operation.
fn run(cli: Cli) -> anyhow::Result<()> {
    let format = cli.output.to_format();

    match cli.command {
        Command::Ls { input } => {
            let (root, algo, bundle) = read_bundle(&input)?;
            let nodes = walk::resolve(root, algo, &bundle)?;
            let output = render::render_ls(&nodes, &root.0, algo, format);
            println!("{}", output);
            Ok(())
        }

        Command::Stat { input, path } => {
            let (root, algo, bundle) = read_bundle(&input)?;
            let nodes = walk::resolve(root, algo, &bundle)?;

            let node = nodes
                .iter()
                .find(|n| n.path == path)
                .ok_or_else(|| anyhow::anyhow!("path not found: {}", path))?;

            let output = render::render_stat(node, algo, format);
            println!("{}", output);
            Ok(())
        }

        Command::Tree { input } => {
            let (root, algo, bundle) = read_bundle(&input)?;
            let nodes = walk::resolve(root, algo, &bundle)?;
            let output = render::render_tree(&nodes, format);
            println!("{}", output);
            Ok(())
        }

        Command::Links { input } => {
            let (root, algo, bundle) = read_bundle(&input)?;
            let nodes = walk::resolve(root, algo, &bundle)?;
            let output = render::render_links(&nodes, algo, format);
            println!("{}", output);
            Ok(())
        }

        Command::Symlinks { input } => {
            let (root, algo, bundle) = read_bundle(&input)?;
            let nodes = walk::resolve(root, algo, &bundle)?;
            let output = render::render_symlinks(&nodes, format);
            println!("{}", output);
            Ok(())
        }

        Command::Special { input } => {
            let (root, algo, bundle) = read_bundle(&input)?;
            let nodes = walk::resolve(root, algo, &bundle)?;
            let output = render::render_special(&nodes, format);
            println!("{}", output);
            Ok(())
        }

        Command::Xattrs { input, prefix } => {
            let (root, algo, bundle) = read_bundle(&input)?;
            let nodes = walk::resolve(root, algo, &bundle)?;
            let output = render::render_xattrs(&nodes, prefix.as_deref(), format);
            println!("{}", output);
            Ok(())
        }

        Command::Summary { input } => {
            let (root, algo, bundle) = read_bundle(&input)?;
            let nodes = walk::resolve(root, algo, &bundle)?;
            let summary = render::Summary::compute(&nodes);
            let output = render::render_summary(&summary, format);
            println!("{}", output);
            Ok(())
        }
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    run(cli)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn cli_parses_ls_with_input() {
        let result = Cli::try_parse_from(vec!["cairn-dirtree", "ls", "--input", "/tmp/bundle"]);
        assert!(result.is_ok());
        let cli = result.unwrap();
        assert_eq!(cli.output.to_format(), OutputFormat::Txt);
        matches!(cli.command, Command::Ls { .. });
    }

    #[test]
    fn cli_parses_stat_with_input_and_path() {
        let result = Cli::try_parse_from(vec![
            "cairn-dirtree",
            "stat",
            "--input",
            "/tmp/bundle",
            "some/file.txt",
        ]);
        assert!(result.is_ok());
        let cli = result.unwrap();
        matches!(cli.command, Command::Stat { .. });
    }

    #[test]
    fn cli_parses_tree_with_input() {
        let result = Cli::try_parse_from(vec!["cairn-dirtree", "tree", "--input", "/tmp/bundle"]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parses_links_with_input() {
        let result = Cli::try_parse_from(vec!["cairn-dirtree", "links", "--input", "/tmp/bundle"]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parses_symlinks_with_input() {
        let result = Cli::try_parse_from(vec![
            "cairn-dirtree",
            "symlinks",
            "--input",
            "/tmp/bundle",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parses_special_with_input() {
        let result = Cli::try_parse_from(vec![
            "cairn-dirtree",
            "special",
            "--input",
            "/tmp/bundle",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parses_xattrs_with_input() {
        let result = Cli::try_parse_from(vec![
            "cairn-dirtree",
            "xattrs",
            "--input",
            "/tmp/bundle",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_parses_xattrs_with_prefix() {
        let result = Cli::try_parse_from(vec![
            "cairn-dirtree",
            "xattrs",
            "--input",
            "/tmp/bundle",
            "--prefix",
            "user.",
        ]);
        assert!(result.is_ok());
        let cli = result.unwrap();
        matches!(cli.command, Command::Xattrs {
            prefix: Some(ref p),
            ..
        } if p == "user.");
    }

    #[test]
    fn cli_parses_summary_with_input() {
        let result = Cli::try_parse_from(vec![
            "cairn-dirtree",
            "summary",
            "--input",
            "/tmp/bundle",
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn cli_defaults_output_to_txt() {
        let result = Cli::try_parse_from(vec!["cairn-dirtree", "ls", "--input", "/tmp/bundle"]);
        assert!(result.is_ok());
        let cli = result.unwrap();
        assert_eq!(cli.output.to_format(), OutputFormat::Txt);
    }

    #[test]
    fn cli_parses_json_output_flag() {
        let result = Cli::try_parse_from(vec![
            "cairn-dirtree",
            "-o",
            "json",
            "ls",
            "--input",
            "/tmp/bundle",
        ]);
        assert!(result.is_ok());
        let cli = result.unwrap();
        assert_eq!(cli.output.to_format(), OutputFormat::Json);
    }

    #[test]
    fn cli_parses_output_flag_long_form() {
        let result = Cli::try_parse_from(vec![
            "cairn-dirtree",
            "--output",
            "json",
            "ls",
            "--input",
            "/tmp/bundle",
        ]);
        assert!(result.is_ok());
        let cli = result.unwrap();
        assert_eq!(cli.output.to_format(), OutputFormat::Json);
    }

    #[test]
    fn cli_rejects_missing_input() {
        let result = Cli::try_parse_from(vec!["cairn-dirtree", "ls"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_rejects_invalid_output_format() {
        let result = Cli::try_parse_from(vec![
            "cairn-dirtree",
            "-o",
            "invalid",
            "ls",
            "--input",
            "/tmp/bundle",
        ]);
        assert!(result.is_err());
    }
}
