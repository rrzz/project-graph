# Changelog

All notable changes will be documented here. This project follows Semantic
Versioning.

## [Unreleased]

### Fixed

- A corrupt or incompatible cached index now causes a rebuild instead of
  failing `build` until the file was deleted by hand; the build fingerprint
  also covers the SQLite schema, so upgraded binaries never reuse an
  old-layout index.
- Query terms that collide with FTS5 operator words (`AND`, `OR`, `NOT`,
  `NEAR`) are treated as plain search terms instead of raising syntax errors.
- `%` and `_` are matched literally during fuzzy node resolution and lexical
  search fallback.
- The evidence lock is replaced atomically (write-then-rename), so a crash
  mid-`lock` can no longer truncate the acceptance record.
- Blocked-path policy matches case-insensitively and is re-checked against the
  canonicalized path, closing case-alias and in-repo symlink bypasses.
- `init` writes the current lock version; locks written by a newer binary are
  refused with an upgrade message instead of being misread.

### Added

- Validation rejects aliases and edge ids that would be shadowed by a node id,
  and evidence keys that look like misspelled selector fields.
- Undirected `neighbors`/`impact` results now include the full induced edge
  set of the returned nodes (edges joining two final-level nodes were omitted).

## [0.1.0] - 2026-07-24

- Initial public Rust implementation.
- Compatible project format and CLI workflow for initialization, validation,
  evidence locking, SQLite indexing, search, traversal, paths, impact analysis,
  diagnostics, deterministic evaluation, and Mermaid export.
- Atomic SQLite index replacement and read-only query connections.
