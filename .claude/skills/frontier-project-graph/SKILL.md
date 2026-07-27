---
name: frontier-project-graph
description: Build or deepen a Project Graph with a small set of frontier-model architectural workers and one evidence-owning orchestrator. Use for high-risk, deeply coupled, poorly documented, or ambiguous codebases where broad judgment matters more than broad file coverage.
---

# Frontier Project Graph

Use this companion to `crawl-project-graph` when the hard problem is not
enumerating files but understanding hidden coupling, ownership, invariants, and
failure history. It is a high-judgment survey, not a substitute for the
deterministic whole-repository manifest.

Read [workflow.md](references/workflow.md) before starting and give each
worker [worker-contract.md](references/worker-contract.md).

## Trust boundary

- Frontier workers are source-read-only and write only isolated `candidate`
  findings.
- The orchestrator alone resolves IDs, writes assertions, changes review state,
  builds SQLite, and requests/records acceptance.
- A strong model is not evidence. Every promoted claim needs re-opened,
  durable repository anchors; never use line numbers.
- Do not inspect blocked paths, place secrets or personal data in facts, or
  lock evidence before explicit review.

## When to use it

Use 3–7 frontier workers when a focused change crosses architectural boundaries,
when diagnostics show a graph with shallow but unhelpful coverage, or after an
incident exposes missing design knowledge. Use `crawl-project-graph` first when
file coverage itself is unknown. The two skills can be combined: crawl workers
map the repository; frontier workers explain its highest-risk regions.

Use FTS-backed `query` to seed an investigation, then use graph traversal and
evidence for the answer. A reviewed unlocked graph is valid for this work; a
`"reused": true` build is a successful no-op. Reserve locking and optional Git
verification for explicit acceptance or handoff checkpoints.

## Required outcome

1. Each worker owns a distinct question, not merely a directory.
2. The orchestrator reconciles disagreements and rechecks all promoted spans.
3. Accepted facts emphasize authority, data/state transitions, resource limits,
   invariants, decisions, incidents, and verification gates.
4. Gold questions prove the graph answers the risky editor questions that
   motivated the survey.
