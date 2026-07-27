# Review project graph candidates

Review candidate assertions against the repository before evidence is locked.

For each record:

1. Confirm node identity and resolve aliases.
2. Confirm that both edge endpoints exist and mean what their IDs claim.
3. Confirm the predicate precisely describes the cited source span.
4. Resolve every evidence selector against current source.
5. Reject evidence that is incidental, over-broad, generated, secret-bearing,
   or dependent on line numbers.
6. Check for a contradictory existing assertion.
7. Promote `candidate` to `reviewed` only when all checks pass.

Then run `validate`, inspect `stale`, and examine `diagnostics`. Add or update
gold questions for important developer workflows. Lock and build only after
the reviewed diff is acceptable.

Use FTS-backed `query` to find existing reviewed concepts before resolving
duplicates. A reviewed unlocked graph remains usable, and `"reused": true`
from `build` is success; Git context is separate from evidence freshness.

Return a summary grouped into promoted, corrected, rejected, and deferred
records. Explain deferred coverage gaps without inventing replacement facts.
