# Crawl a large repository into a Project Graph

Use the `crawl-project-graph` skill to build or comprehensively refresh this repository's
Project Graph.

Read these files before acting:

- `.claude/skills/crawl-project-graph/SKILL.md`
- `.claude/skills/crawl-project-graph/references/workflow.md`
- `.claude/skills/crawl-project-graph/references/worker-contract.md`
- the repository's `.project-graph/policy.md` and `.project-graph/config.json`, if present

Follow the full manifest -> coverage -> reconciliation -> review -> validation workflow. Treat
every coverage worker as source-read-only and candidate-only: it may write its isolated candidate
result, but not canonical assertions. The orchestrator is the only actor allowed
to write reviewed assertions, create evidence locks, build SQLite, or report graph completeness.

Do not claim a complete crawl unless every eligible file is assigned a surveyed, excluded, or
deferred disposition. Do not promote claims without re-opening their cited anchor spans. Do not
use line-number evidence, scan blocked paths, expose secrets, or lock evidence without explicit
review.

Use FTS-backed `query` before broad traversal. A reviewed unlocked graph is
queryable, and a `"reused": true` build is successful; lock only after
explicit acceptance or for a handoff checkpoint.
