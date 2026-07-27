# Worker contract

Survey only the assigned concern. Remain read-only. Respect repository
instructions and configured blocked paths.

Return one JSON object with:

- `scope`
- `nodes`
- `edges`
- `aliases`
- `gold_questions`
- `coverage_gaps`

Use Project Graph assertion shapes. Mark every proposal and evidence item
`candidate`; use evidence method `model`. Use repository-relative paths and
unique source anchors, never line numbers.

Prefer authority, data flow, state transitions, invariants, shared resources,
incidents, decisions, and verification relationships. Do not enumerate every
function. Do not infer relationships from names alone. Omit unsupported claims
and identify them as coverage gaps instead.

Do not write `.project-graph` files. The primary agent resolves identities,
vocabulary, conflicts, and final evidence.
