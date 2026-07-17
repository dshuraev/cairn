//! CLI binary for cairn-digest: walk, chunk, and store a directory tree.

use anyhow::Context;
use cairn_core::hash::HashAlgorithm;
use cairn_digest::build::DigestOptions;
use cairn_digest::chunk::ChunkConfig;
use cairn_digest::{digest, Store};
use clap::Parser;
use std::path::PathBuf;

/// FastCDC v2020 minimum chunk size floor.
/// From fastcdc-4.0.1/src/v2020/mod.rs: MINIMUM_MIN = 64.
const FASTCDC_MIN_CHUNK_FLOOR: usize = 64;

/// FastCDC v2020 average chunk size floor.
/// From fastcdc-4.0.1/src/v2020/mod.rs: AVERAGE_MIN = 256.
const FASTCDC_AVG_CHUNK_FLOOR: usize = 256;

/// FastCDC v2020 maximum chunk size floor.
/// From fastcdc-4.0.1/src/v2020/mod.rs: MAXIMUM_MIN = 1024.
const FASTCDC_MAX_CHUNK_FLOOR: usize = 1024;

/// Local wrapper around [`HashAlgorithm`] for clap compatibility.
/// `cairn-core` should remain dependency-light and not depend on clap,
/// so this wrapper is defined locally.
#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum HashAlgoArg {
    /// SHA-256 (default).
    #[value(name = "sha256")]
    Sha256,
    /// BLAKE3.
    #[value(name = "blake3")]
    Blake3,
}

impl HashAlgoArg {
    /// Converts this CLI argument variant to the canonical [`HashAlgorithm`].
    fn to_algorithm(self) -> HashAlgorithm {
        match self {
            HashAlgoArg::Sha256 => HashAlgorithm::Sha256,
            HashAlgoArg::Blake3 => HashAlgorithm::Blake3,
        }
    }
}


/// CLI for cairn-digest: walk a directory, chunk its files, deduplicate,
/// and produce a dirtree bundle and chunk store (§2 of cairn-digest.md).
#[derive(Parser, Debug)]
#[command(name = "cairn-digest")]
#[command(about, long_about = None)]
struct Cli {
    /// Source directory to digest.
    src_dir: PathBuf,

    /// Primary chunk store directory (required). Objects are written here
    /// after deduplication check against primary and seed stores.
    #[arg(long, required = true)]
    store: PathBuf,

    /// Output dirtree bundle path (required). Written last, atomically,
    /// only after all referenced objects are in the store.
    #[arg(long, required = true)]
    out: PathBuf,

    /// Seed store directories (optional, repeatable). Consulted for
    /// deduplication before writing to primary store; objects are never
    /// written to seed stores.
    #[arg(long)]
    seed_store: Vec<PathBuf>,

    /// Minimum chunk size, in bytes (default: 16384 / 16 KiB).
    #[arg(long, default_value = "16384")]
    min_chunk: usize,

    /// Target (average) chunk size, in bytes (default: 65536 / 64 KiB).
    #[arg(long, default_value = "65536")]
    avg_chunk: usize,

    /// Maximum chunk size, in bytes (default: 262144 / 256 KiB).
    #[arg(long, default_value = "262144")]
    max_chunk: usize,

    /// Hash algorithm (default: sha256). Cryptographic algorithms only.
    #[arg(long, default_value = "sha256")]
    hash: HashAlgoArg,
}

/// Runs the digest operation with the given CLI arguments.
///
/// Returns `Ok(())` on success (dirtree written, store populated),
/// or `Err(anyhow::Error)` on validation failure or runtime error.
fn run(cli: Cli) -> anyhow::Result<()> {
    // Validate chunk size absolute bounds (fastcdc library constraints).
    // These must be checked first as they are more fundamental than ordering.
    if cli.min_chunk < FASTCDC_MIN_CHUNK_FLOOR {
        anyhow::bail!(
            "min_chunk ({}) is below fastcdc's minimum of 64 bytes",
            cli.min_chunk
        );
    }
    if cli.avg_chunk < FASTCDC_AVG_CHUNK_FLOOR {
        anyhow::bail!(
            "avg_chunk ({}) is below fastcdc's minimum of 256 bytes",
            cli.avg_chunk
        );
    }
    if cli.max_chunk < FASTCDC_MAX_CHUNK_FLOOR {
        anyhow::bail!(
            "max_chunk ({}) is below fastcdc's minimum of 1024 bytes",
            cli.max_chunk
        );
    }

    // Validate chunk size ordering: min <= avg <= max.
    // Note: This check is not a duplicate — fastcdc's own ordering check is a
    // debug-only `debug_assert!`, compiled out in release builds, so this
    // CLI-level check is the only enforcement in a release build.
    if cli.min_chunk > cli.avg_chunk {
        anyhow::bail!(
            "chunk size ordering violation: min_chunk ({}) > avg_chunk ({})",
            cli.min_chunk,
            cli.avg_chunk
        );
    }
    if cli.avg_chunk > cli.max_chunk {
        anyhow::bail!(
            "chunk size ordering violation: avg_chunk ({}) > max_chunk ({})",
            cli.avg_chunk,
            cli.max_chunk
        );
    }

    // Build chunk configuration and digest options.
    let chunk_config = ChunkConfig {
        min_size: cli.min_chunk,
        avg_size: cli.avg_chunk,
        max_size: cli.max_chunk,
    };

    let algo = cli.hash.to_algorithm();

    let options = DigestOptions {
        chunk_config,
        algo,
    };

    // Create the store.
    let store = Store::new(cli.store.clone(), cli.seed_store.clone());

    // Run the digest operation.
    let root_id = digest(&cli.src_dir, &store, &cli.out, &options)
        .context("cairn-digest failed")?;

    // Print the resulting DirTreeID to stdout via its Display impl.
    println!("{}", root_id);

    Ok(())
}

/// Entry point for the cairn-digest CLI.
fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    run(cli)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper to create a temporary directory with a couple of test files.
    fn create_test_dir() -> (TempDir, PathBuf, PathBuf, PathBuf) {
        let tmpdir = TempDir::new().expect("failed to create tempdir");
        let src = tmpdir.path().join("src");
        let store = tmpdir.path().join("store");
        let out = tmpdir.path().join("out.dirtree");

        fs::create_dir(&src).expect("failed to create src dir");

        // Create a couple of test files.
        fs::write(src.join("file1.txt"), b"hello world").expect("failed to write file1");
        fs::write(src.join("file2.txt"), b"test content").expect("failed to write file2");

        (tmpdir, src, store, out)
    }

    #[test]
    fn run_digests_small_directory_successfully() {
        let (_tmpdir, src, store, out) = create_test_dir();

        let cli = Cli {
            src_dir: src,
            store: store.clone(),
            out: out.clone(),
            seed_store: vec![],
            min_chunk: 512,
            avg_chunk: 1024,
            max_chunk: 2048,
            hash: HashAlgoArg::Sha256,
        };

        let result = run(cli);
        assert!(result.is_ok(), "run failed: {}", result.unwrap_err());

        // Verify that the output file was created.
        assert!(
            out.exists(),
            "output dirtree file was not created at {}",
            out.display()
        );

        // Verify that the store directory is non-empty.
        let entries: Vec<_> = fs::read_dir(&store)
            .expect("failed to read store dir")
            .collect();
        assert!(!entries.is_empty(), "store directory is empty");
    }

    #[test]
    fn cli_rejects_missing_required_args() {
        // Try to parse with only the binary name and no arguments.
        let result = Cli::try_parse_from(vec!["cairn-digest"]);
        assert!(
            result.is_err(),
            "expected parse error for missing required args"
        );
    }

    #[test]
    fn cli_parses_repeated_seed_store_flags() {
        let result = Cli::try_parse_from(vec![
            "cairn-digest",
            "/tmp/src",
            "--store",
            "/tmp/store",
            "--out",
            "/tmp/out",
            "--seed-store",
            "/tmp/seed1",
            "--seed-store",
            "/tmp/seed2",
        ]);

        assert!(result.is_ok(), "parse failed: {}", result.unwrap_err());
        let cli = result.unwrap();
        assert_eq!(cli.seed_store.len(), 2, "expected 2 seed stores");
        assert_eq!(cli.seed_store[0].to_string_lossy(), "/tmp/seed1");
        assert_eq!(cli.seed_store[1].to_string_lossy(), "/tmp/seed2");
    }

    #[test]
    fn run_rejects_chunk_size_ordering_violation() {
        let (_tmpdir, src, store, out) = create_test_dir();

        // Test: min_chunk > avg_chunk
        let cli_bad_min = Cli {
            src_dir: src.clone(),
            store: store.clone(),
            out: out.clone(),
            seed_store: vec![],
            min_chunk: 2048,
            avg_chunk: 1024,
            max_chunk: 4096,
            hash: HashAlgoArg::Sha256,
        };

        let result = run(cli_bad_min);
        assert!(
            result.is_err(),
            "run should reject min_chunk > avg_chunk"
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("chunk size ordering violation"),
            "error message should mention ordering violation"
        );

        // Test: avg_chunk > max_chunk
        let cli_bad_avg = Cli {
            src_dir: src.clone(),
            store: store.clone(),
            out: out.clone(),
            seed_store: vec![],
            min_chunk: 512,
            avg_chunk: 4096,
            max_chunk: 2048,
            hash: HashAlgoArg::Sha256,
        };

        let result = run(cli_bad_avg);
        assert!(
            result.is_err(),
            "run should reject avg_chunk > max_chunk"
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("chunk size ordering violation"),
            "error message should mention ordering violation"
        );
    }

    #[test]
    fn run_rejects_min_chunk_below_fastcdc_minimum() {
        let (_tmpdir, src, store, out) = create_test_dir();

        let cli = Cli {
            src_dir: src,
            store,
            out,
            seed_store: vec![],
            min_chunk: 32,
            avg_chunk: 1024,
            max_chunk: 4096,
            hash: HashAlgoArg::Sha256,
        };

        let result = run(cli);
        assert!(
            result.is_err(),
            "run should reject min_chunk below fastcdc minimum (64)"
        );
        assert!(
            result.unwrap_err().to_string().contains("64"),
            "error message should mention the fastcdc minimum (64)"
        );
    }

    #[test]
    fn run_rejects_avg_chunk_below_fastcdc_minimum() {
        let (_tmpdir, src, store, out) = create_test_dir();

        let cli = Cli {
            src_dir: src,
            store,
            out,
            seed_store: vec![],
            min_chunk: 64,
            avg_chunk: 128,
            max_chunk: 4096,
            hash: HashAlgoArg::Sha256,
        };

        let result = run(cli);
        assert!(
            result.is_err(),
            "run should reject avg_chunk below fastcdc minimum (256)"
        );
        assert!(
            result.unwrap_err().to_string().contains("256"),
            "error message should mention the fastcdc minimum (256)"
        );
    }

    #[test]
    fn run_rejects_max_chunk_below_fastcdc_minimum() {
        let (_tmpdir, src, store, out) = create_test_dir();

        let cli = Cli {
            src_dir: src,
            store,
            out,
            seed_store: vec![],
            min_chunk: 64,
            avg_chunk: 256,
            max_chunk: 512,
            hash: HashAlgoArg::Sha256,
        };

        let result = run(cli);
        assert!(
            result.is_err(),
            "run should reject max_chunk below fastcdc minimum (1024)"
        );
        assert!(
            result.unwrap_err().to_string().contains("1024"),
            "error message should mention the fastcdc minimum (1024)"
        );
    }

}
