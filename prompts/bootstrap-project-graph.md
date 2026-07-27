# Bootstrap an evidence-locked project graph

Create a high-value engineering knowledge graph for this repository using the
`project-graph` CLI. The graph must help developers answer coupling, authority,
state-flow, incident, and verification questions. Do not index every symbol.

## Safety and evidence rules

- Read `.project-graph/config.json` and repository agent instructions first.
- Never inspect blocked paths or include credentials, secret values, personal
  data, generated dependencies, build products, or database contents as facts.
- Treat source code and maintained project documentation as evidence.
- Use unique `start_anchor` and `end_anchor` strings, never line numbers.
- Mark model-discovered records `candidate`. Promote them to `reviewed` only
  after checking every endpoint, predicate, description, and evidence span.
- Do not run `project-graph lock` until a human or primary agent has reviewed
  the assertions.
- Preserve provenance. If a claim has no durable evidence, omit it or keep it
  as an explicitly unsupported candidate outside assertion globs.

## Parallel survey

If agents are available, delegate independent read-only surveys. Give every
worker an exclusive concern and require the JSON response described in
`prompts/survey-worker.md`. Useful concerns are:

1. Architecture, process boundaries, and authority.
2. Runtime flows, state machines, and protocol messages.
3. Persistence, records, migrations, and data ownership.
4. Build, release, deployment, and operational gates.
5. Tests, verification, incidents, and documented decisions.
6. Shared resources, performance budgets, concurrency, and hidden coupling.

Workers must not edit graph files. The primary agent owns resolution and all
writes.

## Synthesis

1. Initialize the project if needed:

   ```bash
   project-graph --project . init --name "<project name>"
   ```

2. Inspect the combined candidate set. Resolve aliases and duplicate concepts.
3. Keep the vocabulary bounded. Add project-specific node types or predicates
   only when existing terms cannot express an important recurring distinction.
4. Prefer facts that answer:
   - What owns this behavior?
   - What must change together?
   - Which invariant or resource constrains it?
   - Which test proves it?
   - Which incident or decision explains it?
5. Write cohesive JSONL files under `.project-graph/assertions/`.
6. Add 5–15 realistic bounded checks to
   `.project-graph/gold-questions.json`.
7. Run:

   ```bash
   project-graph --project . validate
   project-graph --project . stale
   ```

8. Review failures against source and correct assertions or anchors. Do not
   weaken policies to make validation pass.
9. After explicit review/acceptance, run:

   ```bash
   project-graph --project . lock
   project-graph --project . build
   project-graph --project . eval
   project-graph --project . diagnostics
   ```

`query` is FTS-backed and should seed normal graph investigation. A reviewed
unlocked graph is usable; treat `"reused": true` from `build` as a successful
no-op. Reserve locking and optional Git `verify` checks for explicit acceptance
or handoff rather than everyday use.

## Completion report

Report node/edge/evidence counts, candidate counts, evaluation results, domains
covered, known coverage gaps, and any assertions left awaiting review.
