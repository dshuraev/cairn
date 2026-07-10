# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Cairn** is a userspace application packaging and deployment tool targeting Linux systems.
It is built around a **package delta update mechanism** that uses content-aware hashing (chunking) for efficient distribution.

The project is organized as a Rust workspace with four crates:

- **cairn-core**: Common types, utilities, and traits shared across other crates
- **cairn-digest**: Plumbing-level tool for producing chunk stores and directory trees (dirtrees) from source directories
- **cairn-dirtree**: Operations over dirtree objects (diff, inspect, etc.)
- **cairn-pkg**: Higher-level package assembly, manifest hashing, and signing

### Key Architecture Principle

Cairn deduplicates content across packages and releases using **content-addressed chunking**.
The entire system is built on two primitives produced by `cairn-digest`:

1. **Chunk store**: content-addressed blobs (variable-sized, deduplicated via FastCDC)
2. **Dirtree**: canonical hierarchical description of directory structure and contents

This clean separation allows `cairn-digest` to be a pure, mechanistic tool (no signing, encryption, or policy)
while `cairn-pkg` and higher layers build trust and deployment policy on top.

## Specification Document

**Read `cairn-digest.md` before implementing anything.** It is normative and covers:

- **§3**: Object model (Chunk, FileIndex, Metadata, Node, DirTree)
- **§4**: Canonical encoding rules — two identical objects *must* hash identically for dedup to work
- **§5**: Algorithm walkthrough (hardlink detection, FastCDC chunking, tree construction)
- **§6**: Deduplication, symlinks, device/fifo/socket nodes, timestamp exclusion
- **§7**: Hash constraints (cryptographic only, default SHA-256)
- **§8**: Atomicity guarantees (write temp → verify hash → rename into place)

Key non-goals: no network access, no signing/key material, no policy decisions — pure mechanism only.

## Build & Test

```bash
cargo build              # Build all crates
cargo test               # Run tests
cargo test -p cairn-digest   # Test specific crate
cargo run -p cairn-digest -- --help   # Run a binary (when added)
```

## Crate Responsibilities

**cairn-core**: Hash types, ID types, common error handling, canonical encoding/decoding primitives

**cairn-digest**: creates chunk store from a directory

- Walk source directory, track hardlinks via (device, inode)
- FastCDC chunking with configurable min/avg/max
- Build FileIndex, Metadata, DirTree objects
- Store deduplication logic (check existing before writing)
- Atomic writes (temp file → verify hash → rename)

**cairn-dirtree**: Diff, inspect, and manipulation operations over dirtree objects

**cairn-pkg**: Package assembly, manifest signing, version management (built on top of cairn-digest)

## Implementation Notes

- **Canonical encoding**: See §4 — fixed-width integers, explicit field widths, git tree-sort for directories
- **Deduplication**: Before any write, check both `--store` and `--seed-store` by ID only — never re-derive without recomputing
- **Sort order**: Use git tree-sort (implicit `/` on dir names for comparison only, per §4.3)
- **Atomicity**: Write to `.tmp`, verify hash matches computed ID, rename into place; only write `--out` after all
  referenced objects exist in store
- **Hardlink detection**: Any inode seen >1 time gets `LinkGroupID` derived deterministically from content hash

## References

- `cairn-digest.md` — normative specification
- `README.md` — project overview
- FastCDC: USENIX ATC 2016 paper for gear-hash boundary detection
- Git tree-sort: understand git's directory entry ordering (not naive byte sort)
- `STYLEGUIDE.md` — rust styleguide
