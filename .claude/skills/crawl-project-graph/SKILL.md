---
name: crawl-project-graph
description: Build a complete, evidence-locked Project Graph for a large repository using an orchestrator and bounded coverage workers. Use when a repository is too large for one context window, when bootstrapping or refreshing broad graph coverage, or when agents need durable shared engineering memory.
---

# Crawl Project Graph

Use this as the canonical large-repository workflow. The goal is broad,
reviewable engineering coverage—not a symbol index and not autonomous truth.

Read [workflow.md](references/workflow.md) before starting. Read
[worker-contract.md](references/worker-contract.md) before delegating.

## Non-negotiable trust boundary

- Coverage workers are read-only with respect to repository source, but write
  only isolated `candidate` graph findings.
- The orchestrator is the only agent that writes assertions, resolves IDs, or
  changes review status.
- No worker or orchestrator runs `lock` until a human or explicitly authorized
  primary review has accepted the candidate diff.
- Every accepted relationship needs durable repository evidence with text
  anchors, never line numbers.
- Do not inspect blocked paths or place credentials, personal data, generated
  dependencies, build products, or database contents into facts.

## Execution modes

Use the full crawl when the repository has more than roughly 150 relevant
source/document files, several independently-owned modules, or no existing
graph. Use the existing `build-project-graph` skill for focused refreshes.

The orchestrator must keep the manifest, candidate queue, and coverage matrix
small in its context. Workers receive a shard manifest and relevant contracts,
not the entire repository.

Use FTS-backed `query` as the lexical entry point to the reviewed graph. A
reviewed graph may be built and queried while unlocked; `lock` is an explicit
acceptance/handoff checkpoint, not a normal-development prerequisite. Treat a
`"reused": true` build result as successful.

## Required completion state

1. Every eligible source/document file is assigned to a covered shard or an
   explicit exclusion reason.
2. Candidate facts are deduplicated, evidence-checked, and either promoted,
   rejected, or recorded as a coverage gap.
3. The graph validates; staleness is reported truthfully.
4. Gold questions cover each high-risk subsystem and at least one cross-cutting
   coupling per major domain.
5. The final report states coverage, candidate/reviewed counts, known gaps,
   and whether evidence has been explicitly locked.
