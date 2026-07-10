# `cairn-digest`

## 1. Purpose

`cairn-digest` is a plumbing-level tool. Given a directory, it produces:

1. A **chunk store** — a content-addressed set of blobs, deduplicated across the
   directory and (optionally) across a pre-existing store.
2. A **dirtree** — a canonical, content-addressed, hierarchical description of the
   directory's structure, file contents (as ordered chunk references), and metadata.

`cairn-digest` is exclusively concerned with **reconstruction integrity**: given its
output, the original directory can be rebuilt byte-for-byte, permission-for-permission.
It performs no signing, no encryption, no trust decisions, and makes no claims about
provenance. Those are the responsibility of tools built on top (e.g. `cairn-pkg`).

Non-goals, explicitly:

- No network access. `cairn-digest` operates on local paths only.
- No signing or key material. Nothing here should touch a private key.
- No policy (what should be excluded, what needs a reboot, etc). Pure mechanism.

## 2. CLI interface (initial)

```txt
cairn-digest <SRC_DIR> --store <STORE_DIR> --out <DIRTREE_PATH> [options]

Options:
  --seed-store <DIR>        existing store(s) to check for chunk reuse before 
                            writing new chunks (repeatable)
  --min-chunk <BYTES>       FastCDC minimum chunk size   (default: 16KiB)
  --avg-chunk <BYTES>       FastCDC target chunk size    (default: 64KiB)
  --max-chunk <BYTES>       FastCDC maximum chunk size   (default: 256KiB)
  --hash <ALGO>             cryptographic hash algorithm  (default: sha256; only
                            cryptographically secure algorithms are permitted — see §7)
  --threads <N>             parallelism for chunking/hashing independent files
```

Output: exit 0 and a dirtree object written to `--out`, plus new chunks written into
`--store`. Non-zero exit and no partial writes to `--store`/`--out` on any error
(§8, atomicity).

## 3. Object model

Five object kinds, each independently content-addressed by a cryptographic hash of its
canonical encoding (§4). All hashes are 32-byte digests (SHA-256 by default).

```txt
Chunk
  bytes                     — raw content, arbitrary length within [min-chunk, max-chunk]
  id = H(bytes)

FileIndex
  chunks: [ChunkID, ...]    — ORDERED (concatenation order), fixed-width 32B entries
  id = H(canonical(chunks))

Metadata
  mode:  u32                — permission bits + type bits
  uid:   u32
  gid:   u32
  xattrs: [(name: string, value: bytes), ...]   — sorted by name, no duplicates
  id = H(canonical(mode, uid, gid, xattrs))
  NOTE: mtime/atime/ctime are intentionally excluded (see §6.4)

Node                         — one uniform shape for every directory entry
  name: string               — path component only, no separators
  metadata_id: MetadataID
  link_group: LinkGroupID?   — present iff this path is one of ≥2 hardlinked paths
  kind: NodeKind

NodeKind (tagged union; exactly one payload per Node)
  File    { file_index_id: FileIndexID }
  Dir     { children_id: DirTreeID }
  Symlink { target: string }
  Device  { major: u32, minor: u32 }
  Fifo    {}
  Socket  {}

DirTree
  nodes: [Node, ...]        — sorted by name (§4.3)
  id = H(canonical(nodes))
```

`LinkGroupID` is a stable identifier derived from the source inode number at walk
time (§6.2) — it is *not* itself a hash of content, since its only job is to mark
"these Nodes must be re-linked, not independently materialized" on reconstruction.

The **root dirtree ID** output by a single `cairn-digest` run is the identity of the
entire directory's structure + content + metadata, excluding anything outside this
model (timestamps, absolute source paths, etc).

## 4. Canonical encoding

Every object above must serialize to **exactly one** byte sequence for a given logical
value — two semantically identical objects must hash identically, or dedup silently
breaks. Rules:

### 4.1 Primitives

- Integers: fixed-width, little-endian, explicit width per field (u32 = 4 bytes, no
  variable-length ints anywhere in the format).
- Strings (names, xattr names, symlink targets): UTF-8 bytes, `u32` length prefix, no
  terminator, no escaping.
- Byte blobs (xattr values): `u32` length prefix + raw bytes.
- Hash IDs (ChunkID, FileIndexID, MetadataID, DirTreeID, LinkGroupID): fixed 32 bytes,
  no length prefix needed anywhere they appear.

### 4.2 FileIndex encoding

`u32 chunk_count` followed by `chunk_count × 32-byte ChunkID`, concatenated in order.
No delimiters needed — fixed width makes this unambiguous.

### 4.3 DirTree encoding and sort order

Entries are sorted by `name` using **git's tree-sort convention**: compare names as if
every `Dir`-kind entry's name has an implicit trailing `/` appended for comparison
purposes only (not stored). This avoids the `foo` vs `foo.bar` vs `foo/` ambiguity that
a naive byte-sort produces. Encoding per node:

```txt
u32 name_len, name bytes,
u8  kind_tag,
32B metadata_id,
u8  has_link_group, [32B link_group_id if present],
<kind-specific payload>
```

Kind-specific payload:

- `File`: 32B file_index_id
- `Dir`: 32B children_id
- `Symlink`: u32 target_len, target bytes
- `Device`: u32 major, u32 minor
- `Fifo` / `Socket`: no payload

### 4.4 Metadata encoding

`u32 mode, u32 uid, u32 gid, u32 xattr_count`, followed by
`xattr_count × (u32 name_len, name, u32 value_len, value)`, with xattrs sorted by name
bytes ascending (plain byte sort — no directory-suffix rule applies here).

## 5. Algorithm

Given `SRC_DIR`:

1. **Walk** `SRC_DIR` recursively, depth-first, following no symlinks (symlinks are
   recorded as `Symlink` nodes, never traversed into).
2. **Identify hardlinks**: for every regular file, record `(device, inode)`. Any inode
   seen more than once is assigned a `LinkGroupID` (derived deterministically from the
   first-seen path's content, e.g. `H("linkgroup" || device || inode)` — the exact
   derivation doesn't need to be stable across runs on different source trees, only
   consistent *within* one run).
3. **Chunk regular files** (FastCDC, gear-hash boundary detection, tunable
   min/avg/max from §2) into `Chunk` objects, hash each (§7), write to `--store`
   (skip write if `--seed-store` or `--store` already has that ID — see §6.1).
4. **Build `FileIndex`** per regular file from its ordered chunk ID list; hash, dedup
   against store.
5. **Build `Metadata`** per node (file, dir, symlink, device, fifo, socket) from mode
   /uid/gid/xattrs; hash, dedup against store.
6. **Build `DirTree` objects bottom-up** (post-order: every child must be fully
   hashed before its parent is constructed), producing one root `DirTreeID`.
7. **Write** the root dirtree (and all referenced sub-dirtrees) to `--out`; ensure
   all referenced Chunk/FileIndex/Metadata objects are present in `--store`.

### 6.1 Store deduplication

Before writing any object (chunk, file-index, metadata, or dirtree) to `--store`,
check whether an object with that ID already exists (in `--store` or any
`--seed-store`). If present, skip the write — this is where cross-file,
cross-release, and cross-package dedup actually happens. This check must be by ID
only; never re-derive or trust a caller-supplied ID without recomputing it from
content during any operation that writes into a store used for verification
elsewhere (write-path trust is local to this tool; it does not extend to fetched
data from an untrusted source — that's `cairn-pkg`'s / the installer's problem, not
this one's).

### 6.2 Symlinks

Store the target string directly in the `Node`, per §3/§4.3. Do not chunk it, do not
follow it.

### 6.3 Device/Fifo/Socket nodes

Recorded for completeness (a vendor bundle could technically contain one, e.g. a named
pipe for IPC bootstrap). Not expected to be exercised in the primary app-bundle use
case; kept simple (no payload beyond metadata, or major/minor for devices) rather than
elaborated.

### 6.4 Excluded metadata

`mtime`/`atime`/`ctime` are never hashed or stored. Rationale: including them means a
`touch` with no content change produces a new object graph, defeating dedup for no
benefit to this tool's stated purpose (reconstruction integrity, not backup/audit
history — a tool built on top that cares about timestamps can record them
out-of-band).

## 7. Hash function

Default: SHA-256. Only cryptographically secure hash functions are permitted here,
even though `cairn-digest` itself performs no security checks (see prior discussion:
downstream signing verifies a manifest hash that is only meaningful if nothing beneath
it can be collided against). `--hash` may allow selecting among a small allow-list of
modern cryptographic hashes (e.g. SHA-256, BLAKE3) for performance tuning; it must
never expose non-cryptographic hash functions (xxHash, CityHash, etc.) as an option,
by design, to remove the possibility of silently weakening every downstream
verification.

## 8. Atomicity and failure behavior

- All new objects are written to temporary names within `--store` and renamed into
  place only after their content is fully written and hash-verified against the
  computed ID (protects against partial-write corruption if the process is killed
  mid-run).
- The `--out` dirtree file is written last, only after every object it (transitively)
  references is confirmed present in `--store`. A `--out` file existing is therefore
  a promise that the store is complete for that tree — nothing should ever consume a
  dirtree without this invariant holding.
- On any error, `cairn-digest` exits non-zero. Chunks already committed to `--store`
  from a failed run are harmless (content-addressed, no dangling references possible)
  and may be left in place — cleanup, if desired, is a separate `cairn` GC concern,
  not this tool's.

## 9. Explicitly deferred to later specs

- `cairn-dirtree` (diff/inspect operations over dirtree objects)
- `cairn-pkg` (package assembly, manifest/scripts hashing, signing)
- Chunk store garbage collection / reference counting
- On-disk layout of `--store` (flat hash-named files vs. sharded directories, etc.)
