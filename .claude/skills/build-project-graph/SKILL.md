---
name: build-project-graph
description: Build, expand, repair, or refresh a repository's evidence-locked Project Graph using the project-graph CLI. Use when asked to crawl a codebase, generate SPO engineering triples, initialize .project-graph files and SQLite, capture architecture or hidden coupling, add provenance, or prepare graph memory for developers or agents.
---

# Build Project Graph

Build a reviewed engineering graph, not a symbol dump.

## Locate the CLI

Prefer `project-graph` on `PATH`. Otherwise build this repository with
`cargo build --release` and use the resulting binary. Confirm the target
repository before writing.

## Read policy first

Read repository agent instructions and `.project-graph/config.json` when it
exists. Never inspect configured blocked paths. Never put secret values,
credentials, personal data, generated dependencies, or build output into graph
records or evidence.

Read [assertion-contract.md](references/assertion-contract.md) before authoring
records.

## Initialize

Run:

```bash
project-graph --project <root> init --name "<project name>"
```

Initialization is additive and must not overwrite existing graph files.

## Use the current graph efficiently

Use `query` first for lexical discovery; it is FTS-backed and returns reviewed
node/alias/edge candidates. Follow it with `neighbors`, `path`, `impact`, and
`evidence` for deterministic, provenance-backed answers. A reviewed graph can
be built and queried while evidence is unlocked. Treat `"reused": true` from
`build` as a successful no-op; changed reviewed inputs still rebuild atomically.
Locking is an explicit acceptance/handoff checkpoint, not a prerequisite for
ordinary development queries.

Start an agent workflow with `project-graph --project <root> --json status` to
learn whether an index exists, evidence is fresh/unlocked/stale, and Git lock
context is available before choosing the next command.

## Survey with agents

When agent delegation is available and the repository is non-trivial, spawn
coverage workers with mutually exclusive concerns:

- architecture and authority boundaries;
- runtime flows and state machines;
- persistence and data ownership;
- build, release, and operations;
- tests, incidents, and decisions;
- performance budgets and shared resources.

Give each worker the contract in [worker-contract.md](references/worker-contract.md).
Coverage workers may write isolated candidate files, but must not edit source or
canonical graph assertions. Keep accepted graph writes in the primary agent to
avoid conflicting IDs and vocabularies.

For a new repository or a repository large enough to require complete coverage, use the
`crawl-project-graph` skill instead. It adds a deterministic manifest, bounded coverage
workers, cross-shard reconciliation, and an explicit coverage gate. This skill remains the
lighter-weight workflow for focused bootstraps and updates.

For a small repository or unavailable delegation, perform the same surveys
sequentially.

## Resolve and assemble

Combine candidates, then:

1. Resolve duplicate entities before adding edges.
2. Reuse existing IDs, types, predicates, and aliases.
3. Extend vocabulary only for important recurring distinctions.
4. Keep only facts with durable repository evidence.
5. Use unique source-text anchors; never use line numbers.
6. Keep unreviewed model discoveries as `candidate` reports outside assertion
   globs; write canonical assertions only after they are reviewed.
7. Organize assertions by human-reviewable subsystem.
8. Seed realistic gold questions for coupling and impact queries.

Do not promote candidates solely because their JSON is valid. Personally
inspect the cited source and the complete assertion.

## Validate and accept

Run `validate`, `stale`, `build`, and `eval`. Correct the records or evidence;
never weaken a blocked-path policy to get a pass. Record whether the build
rebuilt or reused the index.

Present the candidate diff for explicit review. Only after review, change
accepted facts to `reviewed`, then run:

```bash
project-graph --project <root> lock
project-graph --project <root> build
project-graph --project <root> eval
project-graph --project <root> diagnostics
```

The lock is an acceptance action. Do not silently relock changed source. For a
repository policy requiring Git-aware handoff verification, inspect `stale` or
run `verify` with only the explicitly required flags; a Git commit change is
context, not evidence staleness.

## Finish

Report counts, evaluation results, covered domains, unresolved candidates,
known gaps, and the database path. Explain how future agents should query the
SQLite index before risky edits and run `stale` before trusting evidence.
