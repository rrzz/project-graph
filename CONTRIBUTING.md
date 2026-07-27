# Contributing

Thank you for improving Project Graph.

## Workflow

1. Open an issue for behavior or format changes that affect compatibility.
2. Add or update tests before changing observable behavior.
3. Run formatting, Clippy with warnings denied, and the full test suite.
4. Keep commits focused and explain compatibility implications.

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## Compatibility contract

Existing `.project-graph` directories are a public data format. Changes must
preserve older valid configurations and assertions unless accompanied by an
explicit schema-version migration.

In particular:

- SQLite remains generated and disposable.
- Evidence acceptance remains an explicit operation.
- `stale` must inspect source directly, not trust the cache.
- Blocked paths must be rejected before file content is read.
- Alias resolution remains case-insensitive.

## Pull requests

Include:

- the user-visible outcome;
- tests covering the change;
- security or compatibility considerations;
- documentation changes when CLI behavior changes.

By contributing, you agree that your contribution is licensed under the
project's MIT license.
