# Frontier-worker orchestration

## 1. Frame the questions

The orchestrator reads policy, config, graph diagnostics, existing gold
questions, incident/design documents, and the requested change or risk. Define
3–7 non-overlapping questions. Assign each a source scope and a concrete output
decision. Good questions are about a boundary or invariant, not "understand
directory X."

Start with `project-graph --project <root> --json status`, then use
`diagnostics` only when the current index is present and deeper graph health is
needed.

Suggested frontier roles:

1. **Architecture owner** — authority boundaries, entry points, lifecycle.
2. **Runtime owner** — state machines, data flow, protocols, concurrency.
3. **Constraint owner** — invariants, performance/resource budgets, shared
   mutable resources, hardware limits.
4. **Durability owner** — persistence, migrations, compatibility, recovery.
5. **Delivery owner** — build, generated contracts, deployment, observability.
6. **Evidence owner** — tests, decisions, incidents, regressions, verification.

Omit roles that do not fit; never create workers merely to fill the list.

## 2. Run high-judgment read-only surveys

Use a frontier-capable model. Give each worker its question and contract, not
the other workers' raw reports. Bounded scope still matters: broad interface
reading is allowed, unbounded repository crawling is not.

Save raw reports under `.project-graph/candidates/frontier-<question-id>.json`.
They are local reconciliation input and must remain outside assertion globs.

## 3. Hold an adversarial reconciliation

The orchestrator compares reports by concern. For each claim selected for
promotion:

1. Re-open the cited evidence span.
2. Check whether another report contradicts, narrows, or changes its ownership.
3. Resolve identities and aliases against existing assertions.
4. Write only the minimal reviewed node/edge set needed to preserve the
   relationship.
5. Record unresolved disagreement as a coverage gap, decision candidate, or
   investigation—not an asserted fact.

This stage is where frontier workers earn their cost: they surface questions and
cross-boundary implications; the orchestrator turns only evidence-backed parts
into durable knowledge.

## 4. Prove usefulness

Add 3–10 gold questions that test actual risky decisions, for example:

- What must change together when this record/protocol changes?
- Which resource users share the constrained bank/budget?
- Which state owns the transition and which test verifies it?
- What incident or decision explains this guard?

Run:

```bash
project-graph --project <root> validate
project-graph --project <root> stale
project-graph --project <root> build
project-graph --project <root> eval
project-graph --project <root> diagnostics
```

Use FTS-backed `query` to find the relevant reviewed concepts before expanding
with graph traversal. `build` may safely reuse an unchanged index; record
`"reused": true` as success. An unlocked reviewed graph remains queryable;
locking is only an explicit acceptance or handoff action.

Present the reviewed diff, report disagreements, and state remaining gaps. Lock
only after explicit acceptance, then rebuild and rerun evaluation. Use `verify`
only when a Git-aware checkpoint is specifically required; commit drift alone
does not make evidence stale.
