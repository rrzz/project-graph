# Reconciliation workflow

## Classify each candidate

| Decision | Use when | Action |
| --- | --- | --- |
| Promote | The claim is durable, source-supported, non-duplicative, and useful. | Write a minimal `reviewed` assertion with reviewed evidence. |
| Revise | The underlying insight is useful but ID, predicate, scope, or anchor is wrong. | Correct it and re-check the source before promotion. |
| Reject | The claim is unsupported, contradicted, redundant, or too implementation-local. | Preserve a concise reason in candidate material. |
| Defer | Evidence is incomplete or the design question remains open. | Record a coverage gap, decision candidate, or investigation needed. |

## Review checklist

For every proposed node, edge, alias, and evidence item:

1. Is the source path allowed by `blocked_paths` and repository instructions?
2. Does the anchored source span support this exact claim, not merely a related one?
3. Does the record reuse an existing canonical ID, type, predicate, or alias?
4. Does it contradict a reviewed assertion or another candidate?
5. Will it answer a durable engineering question better than source search alone?
6. Is its evidence review state `reviewed` and its anchor unique without line numbers?
7. Does it require a gold question or adjustment to an existing one?

## Canonical artifact rules

`assertions/*.jsonl` contains reviewed source-of-truth facts. The evidence lock
is an explicit acceptance record. SQLite is a generated read-only query index.
Raw candidates remain outside assertion globs, including rejected and deferred
items. If a candidate appears in the canonical assertion set, stop and move it
before running `lock` or `build`.
