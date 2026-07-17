//! CLI binary for cairn-reconstruct: materialize a dirtree bundle into a directory.

use anyhow::Context;
use cairn_reconstruct::bundle_read::read_bundle;
use cairn_reconstruct::materialize::MaterializeOptions;
use cairn_reconstruct::{check, dry_run, materialize};
use cairn_store::Store;
use clap::Parser;
use std::path::PathBuf;

/// CLI for cairn-reconstruct: read a dirtree bundle and materialize a directory tree.
/// Exit code: 0 = full faithful reconstruction (or successful -n/-c), 2 = successful --no-root
/// with skips (I1 not held), 1 = error.
#[derive(Parser, Debug)]
#[command(name = "cairn-reconstruct")]
#[command(about, long_about = None)]
struct Cli {
    /// Input dirtree bundle file.
    #[arg(long = "input", short = 'i', required = true)]
    input: PathBuf,

    /// Primary chunk store directory (required).
    #[arg(long = "store", short = 's', required = true)]
    store: PathBuf,

    /// Output directory path (required). Must not already exist.
    #[arg(long = "out", short = 'o', required = true)]
    out: PathBuf,

    /// Seed store directories (optional, repeatable). Consulted for chunks
    /// before primary store; chunks are never written to seed stores.
    #[arg(long)]
    seed_store: Vec<PathBuf>,

    /// Dry-run: enumerate chunks and paths, check store presence, do not write.
    #[arg(long = "dry-run", short = 'n')]
    dry_run: bool,

    /// Check: verify all chunks' hashes without writing.
    #[arg(long = "check", short = 'c')]
    check: bool,

    /// Skip privileged operations: chown, device mknod, security.*/trusted.* xattrs,
    /// setuid/setgid bits. Emits a manifest of skips to stderr.
    #[arg(long = "no-root")]
    no_root: bool,
}

/// Runs the reconstruction with the given CLI arguments.
fn run(cli: Cli) -> anyhow::Result<cairn_reconstruct::MaterializeReport> {
    // Read and decode bundle
    let (_root_id, algo, bundle) = read_bundle(&cli.input)
        .context("failed to read bundle")?;

    // Create store
    let store = Store::new(cli.store.clone(), cli.seed_store.clone());

    // If dry-run or check, just inspect; don't materialize
    if cli.dry_run || cli.check {
        if cli.dry_run {
            let report = dry_run(&bundle, _root_id, &store, algo)
                .context("dry-run failed")?;
            println!("Planned creates: {:?}", report.planned_creates);
            println!("Missing chunks: {}", report.missing_chunks.len());
            for chunk in &report.missing_chunks {
                println!("  {:?}", chunk);
            }
        }

        if cli.check {
            let report = check(&bundle, _root_id, algo, &store)
                .context("check failed")?;
            println!("Verified: {} chunks", report.verified);
            println!("Failed: {} chunks", report.failed.len());
            for (chunk, err) in &report.failed {
                println!("  {:?}: {}", chunk, err);
            }
        }

        // Return empty report for -n/-c (they don't write)
        return Ok(cairn_reconstruct::MaterializeReport { skips: vec![] });
    }

    // Real materialization
    let options = MaterializeOptions {
        no_root: cli.no_root,
    };

    materialize(&bundle, _root_id, algo, &store, &cli.out, &options)
        .context("materialization failed")
}

/// Entry point for the cairn-reconstruct CLI.
fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let report = run(cli)?;

    // Emit skips to stderr if present (--no-root mode)
    if !report.skips.is_empty() {
        for skip in &report.skips {
            eprintln!(
                "SKIP {} {}: recorded={} applied={}",
                skip.kind.name(),
                skip.path.display(),
                skip.recorded,
                skip.applied
            );
        }
        // Exit code 2 for successful --no-root with skips
        std::process::exit(2);
    }

    // Print root ID to stdout and exit 0
    println!("Reconstruction successful");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_rejects_missing_required_args() {
        let result = Cli::try_parse_from(vec!["cairn-reconstruct"]);
        assert!(result.is_err(), "should reject missing required args");
    }

    #[test]
    fn cli_parses_repeated_seed_store_flags() {
        let result = Cli::try_parse_from(vec![
            "cairn-reconstruct",
            "--input",
            "/tmp/bundle",
            "--store",
            "/tmp/store",
            "--out",
            "/tmp/out",
            "--seed-store",
            "/tmp/seed1",
            "--seed-store",
            "/tmp/seed2",
        ]);

        assert!(result.is_ok(), "should parse repeated seed-store flags");
        let cli = result.unwrap();
        assert_eq!(cli.seed_store.len(), 2);
    }

    #[test]
    fn cli_parses_dry_run_flag() {
        let result = Cli::try_parse_from(vec![
            "cairn-reconstruct",
            "--input",
            "/tmp/bundle",
            "--store",
            "/tmp/store",
            "--out",
            "/tmp/out",
            "--dry-run",
        ]);

        assert!(result.is_ok());
        assert!(result.unwrap().dry_run);
    }

    #[test]
    fn cli_parses_check_flag() {
        let result = Cli::try_parse_from(vec![
            "cairn-reconstruct",
            "--input",
            "/tmp/bundle",
            "--store",
            "/tmp/store",
            "--out",
            "/tmp/out",
            "--check",
        ]);

        assert!(result.is_ok());
        assert!(result.unwrap().check);
    }

    #[test]
    fn cli_parses_no_root_flag() {
        let result = Cli::try_parse_from(vec![
            "cairn-reconstruct",
            "--input",
            "/tmp/bundle",
            "--store",
            "/tmp/store",
            "--out",
            "/tmp/out",
            "--no-root",
        ]);

        assert!(result.is_ok());
        assert!(result.unwrap().no_root);
    }
}
