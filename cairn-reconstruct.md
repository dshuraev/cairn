# `cairn-reconstruct`

## 1. Purpose

`cairn-reconstruct` is a plumbing-level tool. Given a `.dirtree` bundle (§3 of
`cairn-digest.md`) and a chunk store, it materializes the original directory tree
onto the filesystem.

`cairn-reconstruct` is the inverse of `cairn-digest`: it is exclusively concerned
with **reconstruction integrity** — given a bundle and a complete store, the
resulting directory must re-digest to the same root `DirTreeID` the bundle carries.
It performs no signing, no encryption, no trust decisions, and makes no claims
about provenance. Those are the responsibility of tools built on top (e.g.
`cairn-pkg`).

Non-goals, explicitly:

- No network access. `cairn-reconstruct` operates on local paths only.
- No signing or key material. Nothing here should touch a private key.
- No policy (what to overwrite, what needs a reboot, uid/gid remapping, etc). Pure
  mechanism.

## 2. CLI interface (initial)

```txt
cairn-reconstruct --input <DIRTREE> --store <STORE_DIR> --out <DIR> [options]

Options:
  --input, -i DIRTREE       path to dirtree bundle (cairn-digest.md --out parameter)
  --out, -o DIR             directory to reconstruct the filetree in
  --store, -s <DIR>         additional store(s) to check for chunks, in order after
                            --store (repeatable), mirroring cairn-digest.md §2/§6.1
  --dry-run, -n             presence check only; see §7
  --check, -c               full hash-verification pass; see §7
  --no-root                 skip privileged operations; see §6
```

`--out DIR` is REQUIRED and MUST NOT already exist; `cairn-reconstruct` creates it.
This is a deliberate departure from `tar -C`-style tools, which require an existing
target directory: the must-not-exist precondition is what makes §5's atomicity
guarantee possible — there is no directory to leave in a half-populated state
in place, only a `.tmp` sibling that either becomes `DIR` in one `rename()` or
never gets that far.

Output: exit 0 and a fully materialized directory tree at `--out`. Non-zero exit
and no `--out` directory left behind on any error (§5, atomicity), except under
`--no-root` where partial privilege is an accepted, disclosed condition of success
(§6).

## 3. Invariants

Everything below is subordinate to I1. Where a requirement would violate I1, the
requirement is wrong or a deliberately disclosed exception (§6).

### I1 — Round-trip

For a bundle with root ID `R` and a store containing every chunk it transitively
references, `digest(reconstruct(bundle, store))` MUST yield `R`.

This is self-consistent with `cairn-digest.md` §6.4: timestamps are excluded from
the object model, so `cairn-reconstruct` does not restore `mtime`/`atime`/`ctime`,
and `cairn-digest` does not read them back out. Neither side has an opinion about
timestamps; I1 holds without either needing one.

### I2 — Verify on read

Every chunk read from the store MUST be hash-verified (recompute `H(bytes)`,
compare to the requested `ChunkID`) before its bytes are used to materialize a
file. The store is untrusted input, exactly symmetric to `cairn-digest.md` §8's
verify-on-write: that spec trusts nothing it writes without recomputing the hash
first; this one trusts nothing it reads without the same check.

### I3 — Single source per object kind

`DirTree`, `Metadata`, and `FileIndex` objects come from the **bundle only**, never
from the store. Only `Chunk` bytes come from the store. The bundle is
self-contained by design (see the doc comment on `DirTreeBundle` in
`cairn-core/src/bundle.rs`) precisely so that structure, permissions, ownership,
and per-file chunk lists can be read and validated without touching a store at
all. Consequently the store read API this tool needs is chunks-only — no
`DirTree`/`Metadata`/`FileIndex` lookup path into the store exists or should be
added.

## 4. Data flow

```txt
.dirtree bundle -> decode (cairn-core) -> root DirTreeID
  -> walk DirTree nodes
      -> Metadata by ID  <- bundle   (I3)
      -> FileIndex by ID <- bundle   (I3)
      -> chunks by ID    <- store    (verify each, I2)
  -> materialize into DIR.tmp -> rename() into DIR
```

Atomicity mirrors `cairn-digest.md` §8: the tree is built up under a temporary
name and only renamed into its final, visible name once fully materialized.
`rename()` is atomic only within a single filesystem, and only usable here
because `DIR` is guaranteed not to exist yet (§2) — so `DIR.tmp` MUST be created
on the same filesystem as the parent of `DIR` (conventionally
`DIR.tmp` alongside `DIR`, i.e. sharing `DIR`'s parent directory), never on a
different mount that would force a copy instead of a rename.

## 5. Ordering requirements

Per node, operations MUST be applied in this order: **create → write content →
chown → chmod → xattrs**. This order is load-bearing, not stylistic; getting it
wrong produces a directory that materializes without error but silently fails I1.

- **`chown()` before `chmod()`.** On Linux, `chown()` clears the setuid and
  setgid bits as a kernel-level security measure against ownership-change races.
  If `chmod()` runs first, the subsequent `chown()` strips the very bits `chmod`
  just set. Get this backwards and a mode `04755` binary reconstructs as `0755` —
  no error, no warning; the store, the bundle, and the walk all report success.
  I1 is the only thing that catches it, and only if it's actually tested end to
  end.
- **`chown()` before xattrs.** `chown()` also clears the `security.capability`
  xattr (Linux capability-set-on-exec semantics tie it to ownership). Applying
  xattrs before `chown()` risks the same silent loss. xattrs MUST be applied
  last, after both `chown()` and `chmod()`.
- **Directory modes are applied post-order.** Create directories with permissive
  temporary modes, populate their children fully, then apply the recorded mode
  on the way back out (post-order, mirroring `cairn-digest.md` §5's post-order
  `DirTree` construction, but in reverse). A directory materialized with its
  final mode up front — e.g. `0500` — cannot have entries written into it.
- **Hardlinks.** Nodes sharing a `LinkGroupID` MUST resolve to a single inode:
  the first occurrence encountered creates the file (chunks, then metadata
  ops in the order above); every subsequent occurrence calls `link()` against
  the first, not an independent materialization. See `cairn-digest.md` §5.2 for
  how `LinkGroupID` is derived on the digest side.
- **Symlinks.** `NodeKind::Symlink { target }` carries the target verbatim.
  Create the symlink with that exact string. MUST NOT resolve, normalize, or
  validate the target in any way — a dangling or relative-and-nonsensical
  symlink is the faithful, correct result.
- **Determinism independent of iteration order.** Reconstruction MUST produce
  an identical result regardless of the order nodes are visited within a
  `DirTree`, with exactly one specified exception: within a link group, which
  member is chosen as the first-writer (the one that actually creates the
  file, with the rest `link()`-ing to it) is unspecified — any member may be
  selected — because the on-disk result (shared inode, identical content and
  metadata) is identical no matter which is chosen.

## 6. Privilege and `--no-root`

Default behavior is **strict**: any operation requiring a privilege the process
does not hold MUST hard-fail the reconstruction, non-zero exit, no `--out` left
behind.

`--no-root` relaxes this: it skips operations that require privilege instead of
failing on them. This trades I1 for the ability to reconstruct as an
unprivileged user (e.g. for inspection, testing, or extraction into a container
build context). Precisely which operations are skipped, and precisely what each
skip breaks:

| Operation | Privilege required | Effect when skipped |
| --- | --- | --- |
| `chown` (uid/gid) | root (`CAP_CHOWN`) | Breaks I1: reconstructed `Metadata` has the invoking user's uid/gid instead of the recorded ones -> `MetadataID` differs -> `DirTreeID` differs |
| `mknod` for char/block device nodes | `CAP_MKNOD` | Breaks I1: the node cannot be created at all |
| `security.*` / `trusted.*` xattrs | `CAP_SYS_ADMIN` (namespace-dependent) | Breaks I1: the node's xattr set differs from what was recorded |
| `chmod` setgid to a group the invoking user is not a member of | root | Breaks I1: recorded mode bits are not reproducible |
| `mkfifo` / FIFO nodes | none — unprivileged | not skipped; FIFOs need no special capability on Linux |
| socket nodes | none — unprivileged | not skipped; `bind()`-less socket-file creation needs no special capability on Linux |

Only `S_IFCHR`/`S_IFBLK` device nodes require `CAP_MKNOD` on Linux; FIFOs and
sockets do not, and `user.*`-namespace xattrs are unprivileged regardless of
file ownership. `--no-root` therefore skips exactly the four privileged rows
above and nothing else.

**`--no-root` voids I1 by construction. Its output is not the tree** — it is a
best-effort, partially-faithful materialization, useful for inspection but not
a substitute for a real reconstruction. Accordingly:

- Under `--no-root`, `cairn-reconstruct` MUST emit a manifest enumerating every
  skipped operation (path, kind of skip, recorded value vs. applied value).
- Under `--no-root`, `cairn-reconstruct` SHOULD exit with a distinct
  non-zero-but-not-error status (or otherwise unambiguous marker in its output)
  so a caller cannot mistake a `--no-root` run for a faithful one by checking
  only the process exit code. See open question (2).

### Security note (open question, not decided here)

Under `--no-root`, the invoking user owns every reconstructed file (since
`chown` is skipped). This means `chmod` to a mode with the setuid or setgid bit
**succeeds** — there is no privilege boundary preventing an unprivileged user
from setting `u+s` on a file they own. A bundle recording a setuid-root binary
therefore reconstructs, without any error, as setuid-to-the-invoking-user. That
is not a missing security property; it is a *different* one, silently
substituted. Three options, undecided:

1. Clear setuid/setgid bits unconditionally under `--no-root` (recommended:
   fails safe — the reconstructed file is merely non-executable-as-privileged
   rather than privileged-as-the-wrong-user, and this is easy to name in the
   skip manifest).
2. Skip the node entirely under `--no-root`.
3. Hard-fail even under `--no-root` when a setuid/setgid node is encountered.

This needs a decision before `--no-root` ships; recommend (1), flagged as open
question (1) below.

## 7. `--dry-run` and `--check`

Both are read-only: neither writes anything to `--out`, and no directory (not
even a `.tmp`) is created.

- **`--dry-run` / `-n`**: presence check by `ChunkID` only. MUST NOT read chunk
  bytes — existence in the store is checked the same way `Store::contains` does
  on the digest side (by ID, across `--store` then each `--seed-store` in
  order), never by opening and hashing. Reports the materialization plan (nodes
  that would be created) and the list of `ChunkID`s absent from every
  configured store. This is the fetch-manifest primitive for delta updates —
  it is the main reason `-n` exists: a caller (e.g. `cairn-pkg`) can diff two
  dry-run outputs to know exactly which chunks a delta update needs to fetch,
  without touching disk I/O beyond directory-entry existence checks.
- **`--check` / `-c`**: reads and hash-verifies every chunk the bundle
  transitively references — a full I2 pass, ahead of and independent from any
  real reconstruction. Writes nothing. Catches store corruption (a chunk file
  present but with the wrong bytes) that `-n`'s existence check cannot.
  Expensive: it is a full read of every byte the reconstruction would need.

The split is deliberate: `-n` is cheap and answers a completeness question ("do
I have everything, by ID?"); `-c` is expensive and answers an integrity
question ("is everything I have actually correct?"). They MAY be combined
(`-nc`) to get both answers without materializing anything.

## 8. Explicitly out of scope

- Partial or subtree reconstruction (reconstructing a single node or path
  prefix rather than a whole bundle) — deferred.
- Merge or update-in-place semantics (reconciling an existing `--out` against
  a new bundle) — that is policy, belongs in `cairn-pkg`.
- uid/gid remapping (e.g. reconstructing under different ownership than
  recorded) — policy, belongs in `cairn-pkg`.
- Delta byte-size estimation. **Not possible with the current format**:
  `FileIndex` stores only an ordered list of `ChunkID`s (`cairn-digest.md`
  §3/§4.2), no sizes. The size of a chunk you do not have is unknowable from
  the bundle alone. `-n` gives a count of missing `ChunkID`s, not a byte count
  or estimate. This is a known, accepted limitation of the current object
  model; providing byte-accurate delta sizing would require a `FileIndex`
  format change (e.g. a parallel length table) and is out of scope for this
  spec.
- mtime/atime/ctime restoration, per `cairn-digest.md` §6.4 — these were never
  recorded, so there is nothing to restore.

## 9. Crate boundary

```txt
cairn-dirtree     -> cairn-core                  # structural ops, NO store dependency
cairn-digest      -> cairn-core + cairn-store     # write side
cairn-reconstruct -> cairn-core + cairn-store     # read side
```

`Store` is extracted from `cairn-digest/src/store.rs` (currently 207 LOC,
write-only: `contains()` + `write()`) into a new `cairn-store` crate, gaining a
chunk `read()` (hash-verified, per I2). It does not move into `cairn-core`,
because `cairn-core`'s charter is "no external I/O" (`STYLEGUIDE.md`), and
`cairn-dirtree` must remain store-free — encoding that as a separate crate
boundary makes the invariant checkable with `cargo tree` rather than merely
documented.

`STYLEGUIDE.md`'s crate table currently lists `cairn-dirtree` as covering
"diff, inspect, reconstruct" — that line predates this spec and is now wrong;
`reconstruct` is its own crate and `STYLEGUIDE.md` needs updating to match.

## 10. Open questions

1. **Setuid/setgid handling under `--no-root`** (§6, security note): clear the
   bits, skip the node, or hard-fail. Recommend clearing (option 1); undecided.
2. **Distinct exit code for `--no-root` runs** (§6): whether a successful
   `--no-root` reconstruction should use an exit code distinct from a normal
   success, so scripts cannot mistake it for a faithful one by exit-code alone.
3. **Missing-chunk behavior during a real (non-dry-run) reconstruct**: if the
   bundle references a `ChunkID` absent from every configured store, should
   `cairn-reconstruct` fail on the first missing chunk it hits, or enumerate
   every missing chunk across the whole tree before failing? Recommend
   enumerate-then-fail — it is more useful to a caller and matches `-n`'s
   purpose (a complete missing-chunk manifest in one run) — but this is not
   yet decided.
