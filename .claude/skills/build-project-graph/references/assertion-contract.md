# Assertion contract

Assertions are JSON objects, one per non-comment JSONL line.

Node:

```json
{"kind":"node","id":"component:api","type":"Component","name":"API","description":"Owns authoritative requests.","review":"candidate"}
```

Edge/SPO triple:

```json
{"kind":"edge","id":"edge:api-writes-store","source":"component:api","predicate":"WRITES","target":"store:primary","description":"The API persists authoritative state.","review":"candidate","evidence":[{"path":"src/api.rs","start_anchor":"fn persist(","end_anchor":"transaction.commit()?;","method":"model","review":"candidate"}]}
```

Alias:

```json
{"kind":"alias","alias":"Primary DB","node":"store:primary","review":"candidate"}
```

IDs use letters, digits, `_`, `.`, `:`, `/`, and `-`, beginning with a letter
or digit. Edge endpoints must reference declared nodes. Types and predicates
must exist in `.project-graph/config.json`.

Evidence anchors select complete source lines inclusively. Each anchor must be
unique unless the corresponding positive occurrence is supplied. Never use
line-number evidence. Keep spans narrow enough to prove the claim but broad
enough to survive harmless formatting changes.

`deterministic` means a mechanical extractor established the fact. `human`
means a person authored it. `model` means an LLM proposed it. Extraction method
does not imply review status.

Review states are `candidate`, `reviewed`, and `rejected`. Only inspected facts
become `reviewed`.
