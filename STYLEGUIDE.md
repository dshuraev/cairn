# Rust Style Guide for Cairn

This guide codifies conventions and patterns for the Cairn codebase. Follow these rules unless there's a documented reason not to.

**Target**: Rust 2021 edition. All code should compile with `cargo check` and pass `cargo clippy -- -D warnings`.

---

## Table of Contents

1. [File and Module Organization](#file-and-module-organization)
2. [Naming Conventions](#naming-conventions)
3. [Error Handling](#error-handling)
4. [Functions and Types](#functions-and-types)
5. [Testing](#testing)
6. [Documentation](#documentation)
7. [Cairn-Specific Patterns](#cairn-specific-patterns)
8. [Performance and Safety](#performance-and-safety)

---

## File and Module Organization

### Module Layout

- Keep modules focused: one major responsibility per module.
- Private by default; expose only what callers need via `pub`.
- Use `mod foo;` in `lib.rs` / `main.rs`; put implementation in `foo.rs` or `foo/mod.rs`.
- For complex modules, use a directory with `mod.rs` and submodules.

**Example:**
```
cairn-digest/src/
├── lib.rs          (pub mod chunking; pub mod tree; ...)
├── chunking.rs     (FastCDC implementation)
├── tree/
│   ├── mod.rs      (tree construction)
│   ├── walk.rs     (directory traversal)
│   └── sort.rs     (git tree-sort)
└── store.rs        (chunk deduplication and writing)
```

### Workspace Crates

Each crate has a clear role (per `CLAUDE.md`):

- **cairn-core**: Types, error definitions, canonical encoding primitives. No external I/O.
- **cairn-digest**: Chunk store and dirtree generation. Pure mechanism, no signing/encryption.
- **cairn-dirtree**: Operations over dirtrees (diff, inspect, reconstruct).
- **cairn-pkg**: Manifest assembly, signing, versioning (built on digest).

Keep dependencies minimal and acyclic. If crate-A needs crate-B's types, those types belong in cairn-core.

---

## Naming Conventions

### General Rules

- Use `snake_case` for variables, functions, and modules.
- Use `CamelCase` for types, traits, and enum variants.
- Use `SCREAMING_SNAKE_CASE` for constants and `const` items.
- Abbreviations in names should be lowercase or title-case in CamelCase (e.g., `FastCDC`, not `FastCdC`; `sha256_hash`, not `SHA256Hash`).

### Type and Field Naming

- Prefix boolean fields and functions with `is_`, `has_`, or `can_` where intent is unclear.
  ```rust
  struct File {
      is_symlink: bool,
      has_content: bool,
  }
  ```

- Suffix ID types with `ID` (e.g., `LinkGroupID`, `ChunkID`). These are values that identify objects.
  ```rust
  pub struct ChunkID(pub Hash);
  pub struct LinkGroupID(pub Hash);
  ```

- Suffix "result of an operation" types with `Result` only if they carry error information. Use descriptive names otherwise.
  ```rust
  // Good: operation result
  pub struct ChunkingResult {
      chunks: Vec<Chunk>,
      dedup_ratio: f64,
  }

  // Good: just a set of values
  pub struct Chunk {
      offset: u64,
      len: usize,
  }
  ```

---

## Error Handling

### Use Result<T, E> Everywhere

All fallible operations return `Result<T, E>`. Prefer explicit errors over `panic!()` outside of tests and truly unrecoverable states.

### Error Types

Use **thiserror** for defining custom error types:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DigestError {
    #[error("failed to read source directory: {0}")]
    ReadError(String),

    #[error("chunk {id} not found in store")]
    ChunkNotFound { id: ChunkID },

    #[error("hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: Hash, actual: Hash },

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
```

### Error Context with anyhow

For user-facing code or cross-crate boundaries, use **anyhow** to add context:

```rust
use anyhow::{Context, Result};

pub fn write_chunk_store(path: &Path, chunks: &[Chunk]) -> Result<()> {
    fs::create_dir_all(path)
        .context("failed to create chunk store directory")?;

    for chunk in chunks {
        write_chunk(path, chunk)
            .context(format!("failed to write chunk {}", chunk.id))?;
    }

    Ok(())
}
```

### Never Unwrap in Library Code

Library code (non-test, non-binary) should not call `.unwrap()`, `.expect()`, or `.panic!()`. If you want to terminate early, return an error and let the caller decide.

**Exception**: In `main()` and integration tests, `.context()?.` or `.unwrap_or_else()` is acceptable for setup.

```rust
// library code: bad
pub fn parse_manifest(data: &[u8]) -> Manifest {
    serde_json::from_slice(data).expect("invalid JSON")
}

// library code: good
pub fn parse_manifest(data: &[u8]) -> Result<Manifest, serde_json::Error> {
    serde_json::from_slice(data)
}

// binary code: acceptable
fn main() -> anyhow::Result<()> {
    let manifest = parse_manifest(data)?;
    Ok(())
}
```

---

## Functions and Types

### Signatures

- Keep function signatures readable. If a function takes 3+ non-trivial arguments, consider a builder or config struct.
  ```rust
  // Less readable
  pub fn chunk(
      data: &[u8],
      min_chunk_size: usize,
      avg_chunk_size: usize,
      max_chunk_size: usize,
  ) -> Result<Vec<Chunk>>

  // Better
  pub struct ChunkConfig {
      pub min_size: usize,
      pub avg_size: usize,
      pub max_size: usize,
  }

  pub fn chunk(data: &[u8], config: &ChunkConfig) -> Result<Vec<Chunk>>
  ```

- Prefer `&str` and `&[T]` over `&String` and `&Vec<T>` in function parameters.

- Prefer owned types in struct fields unless there's a lifetime reason (e.g., references to the source directory during a walk).

### Impl Blocks

- Group related `impl` blocks together; separate by blank lines.
- Order methods logically (constructors, then public API, then private helpers).
  ```rust
  impl Chunk {
      // Constructors
      pub fn new(offset: u64, len: usize) -> Self { ... }

      // Public API
      pub fn hash(&self) -> Hash { ... }
      pub fn size(&self) -> usize { self.len }

      // Private helpers
      fn validate_offset(&self) -> Result<()> { ... }
  }
  ```

### Traits

- Implement `Debug`, `Clone`, and `PartialEq` for public types (derive where possible).
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
  pub struct ChunkID(pub Hash);
  ```

- Use `Copy` for small value types (IDs, offsets, counts).

- Implement `Display` for user-facing types:
  ```rust
  impl fmt::Display for ChunkID {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
          write!(f, "{}", hex::encode(&self.0.0))
      }
  }
  ```

---

## Testing

### Test Organization

- Place unit tests in the same file as the code they test, in a `#[cfg(test)] mod tests` block at the end.
- Use `#[test]` for individual tests and descriptive names:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn test_fastcdc_splits_at_gear_boundaries() {
          // ...
      }

      #[test]
      fn test_empty_input_returns_single_chunk() {
          // ...
      }
  }
  ```

- For integration tests, create `tests/` directory at crate root with one file per major feature:
  ```
  cairn-digest/tests/
  ├── end_to_end.rs      (full pipeline: directory → chunks → store)
  ├── chunking.rs        (FastCDC behavior)
  └── deduplication.rs   (seed-store logic)
  ```

### Test Data

- Use small, deterministic test inputs.
- For large or complex data, generate it programmatically rather than checking in files.
- Include both happy-path and error cases:
  ```rust
  #[test]
  fn test_handles_empty_directory() { ... }

  #[test]
  fn test_handles_permission_denied() { ... }

  #[test]
  fn test_handles_symlink_to_directory() { ... }
  ```

### Assertions

- Use `assert_eq!` and `assert!` for test assertions.
- For complex comparisons, use `pretty_assertions` crate or custom assertion helpers.
- Avoid generic "panic" messages; make assertions self-documenting:
  ```rust
  // Bad
  assert!(result.is_ok());

  // Good
  assert!(result.is_ok(), "expected digest to succeed but got: {:?}", result.err());
  ```

---

## Documentation

### Doc Comments

Every public item (type, function, trait, module) must have a doc comment:

```rust
/// Computes FastCDC boundaries for a byte stream.
///
/// # Arguments
///
/// * `data` — the input byte slice
/// * `config` — chunking parameters (min/avg/max sizes)
///
/// # Returns
///
/// A vector of `Chunk` offsets and lengths. Chunks partition the input without overlap.
///
/// # Example
///
/// ```
/// let config = ChunkConfig { min_size: 4096, avg_size: 8192, max_size: 16384 };
/// let chunks = chunk(&data, &config)?;
/// assert!(chunks.iter().map(|c| c.len).sum::<usize>() == data.len());
/// ```
pub fn chunk(data: &[u8], config: &ChunkConfig) -> Result<Vec<Chunk>>
```

### Comment Style

- Use `///` for public items, `//` for internal comments.
- Explain **why**, not what. The code shows what; comments explain intent.
  ```rust
  // Bad: restates the code
  // increment counter
  counter += 1;

  // Good: explains the "why"
  // track total bytes processed so far; resets per digest operation
  total_processed += chunk.len;
  ```

- Keep comments concise. If an explanation spans 3+ lines, consider renaming the code instead.

### Module Documentation

Document each crate's purpose at the top of `lib.rs`:

```rust
//! **cairn-digest**: Core mechanism for chunking directories into deduplicated chunk stores.
//!
//! This crate is pure: no signing, encryption, or policy. It produces two primitives:
//!
//! - **Chunk store**: content-addressed blobs identified by their cryptographic hash.
//! - **DirTree**: canonical hierarchical description of directory structure and contents.
//!
//! See `cairn-digest.md` (the spec) for details on object model and encoding.
```

---

## Cairn-Specific Patterns

### 1. Atomicity and Writes

Always use the **write-temp-verify-rename** pattern:

```rust
/// Writes a chunk to the store atomically.
///
/// - Writes to `.tmp` file first
/// - Verifies the hash matches the expected ID
/// - Atomically renames into the final location
/// - Returns error if hash mismatch (data corruption or wrong data)
pub fn write_chunk(store: &Path, chunk_id: &ChunkID, data: &[u8]) -> Result<()> {
    let tmp_path = store.join(format!("{}.tmp", chunk_id));
    let final_path = store.join(chunk_id.to_string());

    // Write to temp file
    fs::write(&tmp_path, data)
        .context("failed to write temporary chunk file")?;

    // Verify hash
    let actual_hash = hash_data(data)?;
    if actual_hash != chunk_id.0 {
        fs::remove_file(&tmp_path).ok();
        return Err(anyhow!(
            "hash mismatch: expected {}, got {}",
            chunk_id,
            actual_hash
        ));
    }

    // Atomic rename into place
    fs::rename(&tmp_path, &final_path)
        .context("failed to move chunk into store")?;

    Ok(())
}
```

### 2. Deduplication Checks

Before writing any chunk, check both primary and seed stores by **ID only**—never re-derive:

```rust
pub fn store_chunk(
    store: &Path,
    seed_store: Option<&Path>,
    chunk_id: &ChunkID,
    data: &[u8],
) -> Result<()> {
    // Check primary store first
    let final_path = store.join(chunk_id.to_string());
    if final_path.exists() {
        return Ok(()); // Already present, skip
    }

    // Check seed store
    if let Some(seed) = seed_store {
        let seed_path = seed.join(chunk_id.to_string());
        if seed_path.exists() {
            return Ok(()); // Found in seed, no need to write
        }
    }

    // Not found anywhere, write it
    write_chunk(store, chunk_id, data)
}
```

### 3. Hardlink Detection

Track hardlinks via `(device, inode)` and derive `LinkGroupID` deterministically from content hash:

```rust
use std::os::unix::fs::MetadataExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Inode {
    device: u64,
    inode: u64,
}

impl Inode {
    fn from_metadata(meta: &Metadata) -> Self {
        Inode {
            device: meta.dev(),
            inode: meta.ino(),
        }
    }
}

pub struct HardlinkTracker {
    seen: HashMap<Inode, ChunkID>,
}

impl HardlinkTracker {
    pub fn record(&mut self, inode: Inode, chunk_id: ChunkID) {
        self.seen.insert(inode, chunk_id);
    }

    pub fn lookup(&self, inode: Inode) -> Option<ChunkID> {
        self.seen.get(&inode).copied()
    }
}
```

### 4. Canonical Encoding and Sorting

- Use **git tree-sort** for directory entries (not naive byte sort). See `cairn-digest.md` §4.3.
- Implement `Encode` / `Decode` traits for types that must be hashed identically across invocations.

```rust
/// Encodes this tree in canonical form for hashing.
///
/// Directory entries are sorted in git tree order:
/// entries with a trailing `/` conceptually sort first,
/// allowing `a/` to sort before `aa` even though '/' > 'a'.
pub fn encode_canonical(&self) -> Vec<u8> {
    let mut buf = Vec::new();
    
    let mut entries = self.entries.clone();
    entries.sort_by(|a, b| git_tree_sort(&a.name, &b.name));

    for entry in entries {
        // Fixed-width integer encoding for each field
        buf.extend_from_slice(&entry.name.as_bytes());
        buf.push(0); // null terminator
        buf.extend_from_slice(&entry.hash.0); // 32 bytes for SHA-256
    }

    buf
}
```

### 5. Specification-Driven Implementation

All encoding and object model decisions must reference `cairn-digest.md`. When implementing a feature:

1. Check the spec first (especially §3–§8).
2. Document deviations explicitly.
3. Add a comment linking to the spec if the code is non-obvious:
   ```rust
   // Per cairn-digest.md §4.2: fixed-width integer encoding ensures
   // two identical objects hash identically regardless of platform.
   buf.extend_from_slice(&len.to_le_bytes()); // 8 bytes, little-endian
   ```

---

## Performance and Safety

### Memory

- Avoid unnecessary allocations. Use iterators and borrowed data where possible.
  ```rust
  // Allocates a Vec unnecessarily
  let sorted: Vec<_> = entries.iter().map(|e| e.name.clone()).collect();

  // Better: operate on borrowed data
  let sorted: Vec<_> = entries.iter().map(|e| &e.name).collect();
  ```

- For large data (files, chunk stores), use buffered I/O:
  ```rust
  use std::io::{BufReader, BufWriter};

  let file = File::open(path)?;
  let reader = BufReader::new(file);
  for line in reader.lines() { ... }
  ```

### Unsafe Code

Avoid `unsafe` unless absolutely necessary. If you use it:

1. Document why it's safe with a `// SAFETY: ...` comment.
2. Minimize the unsafe block to the smallest piece.
3. Run `cargo test` and `cargo miri` to verify.

```rust
// SAFETY: `ptr` is guaranteed to point to valid, aligned memory
// initialized by this thread. We do not access `ptr` from any other thread.
unsafe {
    ptr::write(ptr, value);
}
```

### Optimization

- Measure first. Use `cargo bench` or `perf` before optimizing.
- Prefer clarity over micro-optimizations.
- Profile hot paths (chunking loop, directory walk, deduplication check).

### Clippy Lints

All code must pass `cargo clippy -- -D warnings`. Common lints to watch:

- `clippy::unwrap_used` — don't use `.unwrap()` in libraries.
- `clippy::expect_used` — same for `.expect()`.
- `clippy::too_many_arguments` — refactor into a config struct.
- `clippy::missing_errors_doc` — document error cases.

---

## Summary

- **Modules**: Focused, private by default, acyclic dependencies.
- **Naming**: snake_case, CamelCase, SCREAMING_SNAKE_CASE; prefer descriptive names.
- **Errors**: `Result<T, E>` everywhere; use `thiserror` and `anyhow`.
- **Testing**: Unit tests in same file, integration tests in `tests/`, descriptive names.
- **Docs**: Doc comments for all public items; explain why, not what.
- **Cairn patterns**: Atomicity, dedup checks, hardlink tracking, canonical encoding, spec-driven.
- **Performance**: Measure first, avoid allocations, use safe code by default.

