# Project graph coverage worker

Perform coverage work for the assigned repository concern. Do not edit source
files or inspect paths blocked by `.project-graph/config.json`. You may write
your one `candidate` result under `.project-graph/candidates/`; do not edit
canonical graph assertions.

Return one JSON object:

```json
{
  "scope": "assigned concern",
  "nodes": [],
  "edges": [],
  "aliases": [],
  "gold_questions": [],
  "coverage_gaps": []
}
```

Use the normal assertion shapes for `nodes`, `edges`, and `aliases`. Mark every
record `"review":"candidate"`. Every proposed edge must cite at least one
source span:

```json
{
  "path": "relative/source/path",
  "start_anchor": "unique text on the first source line",
  "end_anchor": "unique text on the last source line",
  "method": "model",
  "review": "candidate"
}
```

Requirements:

- Use repository-relative paths.
- Use no line numbers.
- Verify that each anchor resolves uniquely; supply a positive occurrence only
  when uniqueness is impossible and the surrounding construct is stable.
- Prefer durable architectural meaning over exhaustive symbol inventory.
- Record negative findings in `coverage_gaps`, not as facts.
- Never include source excerpts containing secrets or personal data.
- Do not infer an edge merely because two symbols have similar names.
- Keep descriptions factual, compact, and limited to what the evidence proves.
