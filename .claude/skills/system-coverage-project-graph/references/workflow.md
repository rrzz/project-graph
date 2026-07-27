# Whole-system graph workflow

## 1. Establish scope and inventory

Read graph policy and repository documentation first. Run a deterministic file
manifest (the `crawl-project-graph` workflow) and classify every eligible file
as a matrix owner, supporting evidence, generated/duplicate artifact, blocked,
or intentionally out of scope. Write the mandatory coverage matrix before
delegating work.

Run `project-graph --project <root> --json status` first. It works before an
index exists and gives the orchestrator one machine-readable evidence and Git
baseline.

## 2. Assign bounded surface surveys

Assign coverage workers by matrix surface, not arbitrary folders. Each worker
may inspect its source scope and write a candidate report only. It must state:

- covered files and excluded files;
- components and responsibility boundaries;
- inbound/outbound data paths;
- authority, state, persistence, controls, and verification;
- exact text anchors for candidate facts; and
- gaps, uncertainty, or cross-surface dependencies.

## 3. Investigate high-risk boundaries

Use 3–7 frontier questions for relationships that are easy to miss: public
client to authoritative runtime, runtime to renderer, lifecycle to persistence,
and build to delivery/recovery. A worker may describe a concern but may not
promote it from model confidence alone.

## 4. Reconcile centrally

The orchestrator re-opens every cited span, resolves identities, and promotes
only reviewed, minimal assertions. Reconcile contradictions before writing. A
gap, decision candidate, or incident investigation is preferable to a
confidently wrong edge.

## 5. Apply completion gates

Before calling the graph whole-system complete, verify:

- no matrix row is unassigned;
- each covered row has ownership, entry/exit data, authority/state, controls,
  verification, and a gold question;
- gold questions cover at least these applicable paths:
  - public client → ingress/gateway → authoritative runtime;
  - authoritative runtime → real-time transport → client renderer;
  - lifecycle → persistence → cleanup/recovery;
  - build → smoke verification → deploy → rollback/migration;
- candidates are still isolated from assertion globs; and
- unresolved gaps are visible in the matrix or graph as deferred work.

Run `validate`, `stale`, `build`, `eval`, and `diagnostics`. Use FTS-backed
`query` before broad traversal, and treat `"reused": true` from `build` as a
successful unchanged index. Review the assertion diff and matrix. A reviewed
unlocked graph is usable; lock evidence only after explicit acceptance, then
rebuild and rerun evaluation. If a handoff needs Git context, report it with
`verify`; do not equate commit drift with stale source evidence.
