---
name: system-coverage-project-graph
description: Build a whole-system, evidence-locked Project Graph through a mandatory system-surface coverage matrix, bounded coverage workers, frontier investigations, and one evidence-owning orchestrator. Use for deployed applications or services when complete coverage must include ingress, runtime, persistence, security, operations, observability, and delivery boundaries.
---

# System Coverage Project Graph

Use this skill when a graph must explain the whole running system rather than
only its application code. It combines deterministic repository coverage with
mandatory architectural surfaces, then deepens the risky boundaries with
frontier investigations.

Read [workflow.md](references/workflow.md) and
[coverage-matrix.md](references/coverage-matrix.md) before starting. Reuse the
candidate-only worker rules from `crawl-project-graph` and
`frontier-project-graph`.

## Trust boundary

- Coverage workers may read source and write only isolated `candidate` reports.
- The orchestrator alone resolves identities, promotes facts, writes canonical
  assertions, locks evidence, and claims completion.
- Every in-scope matrix surface must be `covered`, `deferred`, or `absent`; no
  surface may remain unassigned.
- Re-open every span before promotion. Evidence anchors are source text, never
  line numbers. Do not inspect blocked paths or record secrets.

## Required outcome

1. A checked-in coverage matrix maps all in-scope system surfaces to their
   source owners and status.
2. Each covered surface has evidence-backed facts for ownership, inbound and
   outbound data, authority/state, controls, and verification.
3. Cross-boundary facts explain the public request path, real-time state path,
   persistence/lifecycle path, and delivery/recovery path where applicable.
4. Gold questions prove that an editor can retrieve those paths before a risky
   change.
5. Validation, staleness, build, and evaluation succeed before the graph is
   called complete.

Use FTS-backed `query` before broad traversal. A reviewed graph is usable while
unlocked, and `"reused": true` from `build` is successful. Lock only for
explicit acceptance/handoff; Git context is additional provenance and does not
turn an otherwise fresh evidence span stale.
