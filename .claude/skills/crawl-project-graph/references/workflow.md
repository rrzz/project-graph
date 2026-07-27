# Orchestrated whole-repository crawl

## 0. Establish policy and scope

Read project agent instructions, `.project-graph/config.json` when present,
and the existing graph diagnostics. Treat `blocked_paths` as an allowlist
boundary: exclude matching paths before any worker reads them.

Run or locate the CLI, then initialize only if needed:

```bash
project-graph --project <root> init --name "<project>"
project-graph --project <root> --json status
```

Use `query` as the FTS-backed lexical entry point before broader traversal.
`build` is valid for reviewed unlocked assertions and may report `"reused":
true` when inputs are unchanged; report that as a successful no-op. Locking is
reserved for explicit acceptance or handoff, not required to inspect the
current reviewed graph.

## 1. Build a deterministic crawl manifest

The orchestrator inventories candidate files with `rg --files`, applies the
blocked-path policy and ordinary non-source exclusions, then creates a manifest
outside assertion globs (for example `.project-graph/candidates/manifest.json`).
Each eligible file has exactly one of: `assigned`, `excluded`, or `deferred`.

Classify by extension/path and assign bounded shards. Start with these passes:

1. **Topology pass** — root manifests, package/build files, entry points,
   configuration, maintained architecture docs, tests, and deployment files.
2. **Module passes** — contiguous directory/module shards. Keep a worker's
   assigned source set small enough to read carefully (normally 15–40 files).
3. **Cross-cutting passes** — persistence, APIs/protocols, security, build and
   release, tests, performance/concurrency, shared resources, and incidents.
4. **Reconciliation pass** — only cross-shard dependencies reported by workers.

Generated/vendor/build artifacts are normally exclusions. A generator and its
generated contract may be graph facts, but generated output is not crawled as
independent source of truth.

## 2. Delegate coverage work

Use the smallest capable model for coverage workers. Give every worker the worker
contract, a single concern, its shard paths, and only the small existing-vocab
slice needed to avoid obvious duplicate IDs.

Coverage workers may run read-only search and inspect assigned source. They
write one candidate result file, but must not:

- edit repository source or canonical graph assertion files;
- run `lock`, `build`, or any deployment;
- inspect blocked paths;
- receive other workers' raw context;
- claim broad coverage for files they did not survey.

Use a bounded concurrency level. As workers finish, have them write raw JSON
output under `.project-graph/candidates/<shard>.json` for local audit and reconciliation;
that directory must remain outside assertion globs. Candidate artifacts are ignored
by default: reviewed assertions, not raw model output, are the committed source of truth.

## 3. Reconcile candidates centrally

The orchestrator—not a worker—does all of the following:

1. Deduplicate nodes by actual identity, not lexical similarity.
2. Resolve aliases against existing canonical IDs.
3. Reject edges whose endpoints, predicate, or evidence do not precisely match.
4. Personally re-open every cited span intended for promotion.
5. Convert accepted candidates to `reviewed` JSONL assertions organized by
   subsystem. Preserve rejected/deferred items in the candidate report.
6. Convert cross-shard dependency reports into a small reconciliation task;
   avoid a second full crawl.

Do not ask an LLM to perform silent global entity resolution. It may propose
aliases, but canonicalization is an orchestrator review decision.

## 4. Measure coverage and graph health

Build a coverage matrix from the manifest:

| Status | Meaning |
| --- | --- |
| covered | worker returned a valid result for the file's shard |
| excluded | explicit policy/reason; not a gap |
| deferred | intentionally omitted; report as a gap |
| unassigned | workflow failure; do not claim complete coverage |

Create 5–15 gold questions from actual developer work. Each important domain
should have one question, and cross-cutting/high-risk domains should have at
least one question that traverses a coupling, invariant, incident, or test.

Run:

```bash
project-graph --project <root> validate
project-graph --project <root> stale
project-graph --project <root> build
project-graph --project <root> eval
project-graph --project <root> diagnostics
```

`build` may occur with unlocked evidence to inspect traversal; report its
evidence state accurately rather than treating unlocked as unusable. Do not run
`lock` just to eliminate a stale report.

## 5. Acceptance and refresh

Present the reviewed assertion diff, coverage matrix, candidate/rejected list,
stale evidence, and evaluation result. Only after explicit acceptance:

```bash
project-graph --project <root> lock
project-graph --project <root> build
project-graph --project <root> eval
```

For an explicitly Git-aware release/handoff policy, use `verify` to report or
enforce only the required freshness, commit, or worktree condition. A different
Git commit alone does not invalidate unchanged evidence.

Later refreshes are incremental: cover changed modules plus their one-hop
dependencies, run `stale`, repair affected facts, and add a gold question when
a new coupling or incident is discovered. Do not repeat the whole crawl unless
the repository structure or graph schema has materially changed.
