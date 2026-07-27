---
name: reconcile-project-graph
description: Reconcile candidate Project Graph findings into a minimal, reviewed, evidence-locked canonical graph. Use after crawl, frontier, or system-coverage discovery; when candidate reports need identity resolution or conflict review; or when promoting, rejecting, deferring, and evaluating graph facts.
---

# Reconcile Project Graph

Turn untrusted discovery output into a small, useful canonical graph. This is
the sole write-authority workflow between candidate-producing workers and the
reviewed assertions used by `project-graph lock` and `build`.

Read [workflow.md](references/workflow.md) before beginning.

## Trust boundary

- Candidate reports under `.project-graph/candidates/` are reconciliation
  input, never canonical graph data. They must remain outside `assertion_globs`.
- Do not promote a fact because its JSON is well-formed or because several
  workers repeated it. Re-open the cited source and verify the exact claim.
- Do not inspect configured blocked paths or record secrets, personal data,
  generated dependencies, build products, or database contents.
- Only write `reviewed` records to `.project-graph/assertions/*.jsonl`.
  `lock` and `build` intentionally reject candidate/rejected records.
- Never run `lock` without explicit acceptance of the reviewed assertion diff.

## Workflow

1. Read repository instructions, `.project-graph/config.json`, existing
   assertions, and candidate reports. Start with `--json status`, then run
   `validate`, `stale`, `diagnostics`,
   and relevant `query`/`neighbors` commands to establish the current graph.
   `query` is FTS-backed; use it to find candidate concepts before deterministic
   traversal and evidence inspection.
2. Normalize proposed identities: reuse canonical IDs, types, predicates, and
   aliases wherever possible. Merge duplicates before considering edges.
3. Review every proposed fact against its source anchors and classify it as
   **promote**, **revise**, **reject**, or **defer**. Preserve the reason for
   rejected/deferred findings in the candidate report or coverage log.
4. Promote only durable relationships that make a future engineering question
   easier to answer. Write a minimal reviewed assertion diff organized by
   subsystem; avoid a symbol dump.
5. Add or update gold questions when a promoted fact protects a meaningful
   coupling, impact, state, build, incident, or verification query.
6. Run:

   ```bash
   project-graph --project <root> validate
   project-graph --project <root> stale
   project-graph --project <root> build
   project-graph --project <root> eval
   ```

   Correct assertions or anchors rather than weakening source-policy rules.
   `"reused": true` from `build` is a successful no-op. Reviewed unlocked
   assertions remain usable; locking is not required for normal queries.
7. Present the reviewed diff, decisions, unresolved gaps, and command results.
   After explicit acceptance only, run:

   ```bash
   project-graph --project <root> lock
   project-graph --project <root> build
   project-graph --project <root> eval
   ```

## Completion report

State the promoted, revised, rejected, and deferred counts; affected assertion
files; gold-question changes; validation/staleness/evaluation results; known
coverage gaps; whether the build was reused or rebuilt; and whether the graph
was explicitly locked and rebuilt. Report Git context separately from evidence
freshness when a handoff policy requires it.
