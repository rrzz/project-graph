# Mandatory system-surface coverage matrix

Create `.project-graph/coverage-matrix.json` before worker assignment. Each
surface must have exactly one status: `covered`, `deferred`, or `absent`.
`deferred` requires an owner and a concrete reason; `absent` requires an
evidence-backed explanation. Do not treat an uninspected surface as absent.

Use this minimum matrix, removing only rows demonstrably outside the project
boundary:

1. `client-presentation` — browser, app UI, renderer, local input/state.
2. `edge-ingress` — DNS, CDN, WAF, TLS, rate limits, origin authentication.
3. `gateway-proxy` — reverse proxy, routing, request limits, headers.
4. `identity-api` — authentication, authorization, HTTP/RPC entry points.
5. `realtime-transport` — WebSockets or other session transport, protocol,
   reconnect and delivery semantics.
6. `authoritative-runtime` — state machines, simulation, concurrency, resource
   caps, ownership and cleanup.
7. `persistence-lifecycle` — databases, retention, migrations, recovery,
   durable versus ephemeral state.
8. `background-status` — queues, jobs, presence, lobby/status aggregation.
9. `service-connectivity` — private networks, external services, dependencies.
10. `security-abuse` — secrets, control plane, input validation, anti-abuse,
    administrative boundaries.
11. `observability` — metrics, logs, alerts, incident signals and failure modes.
12. `build-supply-chain` — source build, generated artifacts, images,
    configuration inputs.
13. `delivery-recovery` — deploy, migrations, rollback, backup and restoration.
14. `verification` — unit/integration/smoke checks and operational validation.
15. `external-vendors` — providers and managed services with material runtime
    responsibility.

For every `covered` row record:

- source owners and evidence-backed graph nodes;
- inbound data/control and outbound effects;
- authority and durable/ephemeral state;
- security or resource controls;
- verification and at least one question the graph must answer.

The matrix is an audit artifact, not generated graph truth. Candidate reports
remain under `.project-graph/candidates/` and must not be added to assertion
globs.
