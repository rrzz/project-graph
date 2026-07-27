use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use chrono::{SecondsFormat, Utc};
use glob::glob;
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, Row};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::error::{GraphError, Result};
use crate::model::{
    BuildCounts, Config, Evidence, EvidenceLock, EvidenceSelector, EvidenceSnapshot, GitContext,
    GoldFile, InitializedProject, LoadedRecord, LockedEvidence, Record, CONFIG_DIR, DEFAULT_DB,
    GOLD_FILE, LOCK_FILE, LOCK_VERSION,
};

const REVIEWS: [&str; 3] = ["reviewed", "candidate", "rejected"];
const METHODS: [&str; 3] = ["deterministic", "human", "model"];

pub fn utc_now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn git_output(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    Some(value.trim().to_owned())
}

/// Return repository context when this project is inside a Git worktree. Git is
/// deliberately optional: Project Graph remains usable for exported trees and
/// non-Git projects.
pub fn git_context(root: &Path) -> Option<GitContext> {
    if git_output(root, &["rev-parse", "--is-inside-work-tree"]).as_deref() != Some("true") {
        return None;
    }
    let head = git_output(root, &["rev-parse", "HEAD"])?;
    let tree = git_output(root, &["rev-parse", "HEAD^{tree}"])?;
    let branch = git_output(root, &["symbolic-ref", "--short", "-q", "HEAD"])
        .filter(|value| !value.is_empty());
    let worktree_clean = git_output(root, &["status", "--porcelain", "--untracked-files=normal"])
        .is_some_and(|value| value.is_empty());
    Some(GitContext {
        head,
        tree,
        branch,
        worktree_clean,
    })
}

/// Compare the optional Git metadata captured by a lock with the current
/// worktree. A commit change does not make source evidence stale; callers use
/// this as additional handoff/release context.
pub fn git_lock_status(root: &Path, lock: &EvidenceLock) -> Value {
    let current = git_context(root);
    let status = match (&lock.git, &current) {
        (None, _) => "not_recorded",
        (Some(_), None) => "unavailable",
        (Some(locked), Some(now)) if locked.head == now.head => "same_commit",
        (Some(_), Some(_)) => "different_commit",
    };
    json!({"status": status, "locked": lock.git, "current": current})
}

/// Compact JSON for storage columns. NOT canonical across shapes: structs
/// serialize in field-declaration order (only `Value` maps sort). Anything
/// that must be byte-stable across versions (the evidence key) builds its
/// string explicitly instead of calling this.
fn stored_json(value: &impl serde::Serialize) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}

fn sha256(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn valid_id(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || "_.:/-".contains(c))
}

pub fn project_root(path: impl AsRef<Path>) -> Result<PathBuf> {
    let root = fs::canonicalize(path.as_ref())?;
    if !root.join(CONFIG_DIR).join("config.json").is_file() {
        return Err(GraphError::msg(format!(
            "{} is not initialized; run `project-graph --project {} init`",
            root.display(),
            root.display()
        )));
    }
    Ok(root)
}

pub fn load_config(root: &Path) -> Result<Config> {
    let path = root.join(CONFIG_DIR).join("config.json");
    if !path.is_file() {
        return Err(GraphError::MissingFile(path));
    }
    let config: Config = serde_json::from_slice(&fs::read(&path)?).map_err(|error| {
        GraphError::msg(format!(
            "{}: invalid configuration: {error}",
            path.display()
        ))
    })?;
    if config.project.trim().is_empty() {
        return Err(GraphError::msg("config project must be non-empty"));
    }
    if config.node_types.is_empty() {
        return Err(GraphError::msg(
            "config node_types must be a non-empty list",
        ));
    }
    if config.predicates.is_empty() {
        return Err(GraphError::msg(
            "config predicates must be a non-empty list",
        ));
    }
    Ok(config)
}

pub fn assertion_paths(root: &Path, config: &Config) -> Result<Vec<PathBuf>> {
    let mut paths = HashSet::new();
    for pattern in &config.assertion_globs {
        let absolute = root.join(pattern);
        let pattern = absolute.to_string_lossy();
        for entry in glob(&pattern)? {
            let path = entry?;
            if path.is_file() {
                paths.insert(path);
            }
        }
    }
    let mut paths: Vec<_> = paths.into_iter().collect();
    paths.sort();
    Ok(paths)
}

pub fn load_records(root: &Path, config: &Config) -> Result<Vec<LoadedRecord>> {
    let mut records = Vec::new();
    for path in assertion_paths(root, config)? {
        let relative = relative_slash(root, &path)?;
        let content = fs::read_to_string(&path)?;
        for (index, raw) in content.lines().enumerate() {
            let line = index + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let value: Value =
                serde_json::from_str(trimmed).map_err(|error| GraphError::JsonLine {
                    path: relative.clone(),
                    line,
                    message: error.to_string(),
                })?;
            if !value.is_object() {
                return Err(GraphError::msg(format!(
                    "{relative}:{line}: expected a JSON object"
                )));
            }
            let record: Record =
                serde_json::from_value(value).map_err(|error| GraphError::JsonLine {
                    path: relative.clone(),
                    line,
                    message: error.to_string(),
                })?;
            records.push(LoadedRecord {
                record,
                origin: relative.clone(),
                line,
            });
        }
    }
    Ok(records)
}

fn relative_slash(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| GraphError::msg(format!("{} escapes project root", path.display())))?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn blocked_set(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        // Case-insensitive: on case-insensitive filesystems (Windows, macOS
        // default) `OVH.TXT` opens the same file as `ovh.txt`, so a
        // case-sensitive block policy would be trivially bypassable.
        builder.add(
            GlobBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .map_err(|error| GraphError::msg(format!("invalid blocked path glob: {error}")))?,
        );
    }
    builder
        .build()
        .map_err(|error| GraphError::msg(format!("invalid blocked path policy: {error}")))
}

fn safe_source_path_with(root: &Path, blocked: &GlobSet, relative: &str) -> Result<PathBuf> {
    let normalized = relative
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_owned();
    let candidate = Path::new(&normalized);
    if normalized.is_empty()
        || candidate.is_absolute()
        || candidate.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(GraphError::msg(format!(
            "Unsafe evidence path: {relative:?}"
        )));
    }
    if blocked.is_match(&normalized) {
        return Err(GraphError::msg(format!(
            "Evidence path is blocked by project policy: {normalized}"
        )));
    }
    let joined = root.join(&normalized);
    let resolved = fs::canonicalize(&joined)
        .map_err(|_| GraphError::msg(format!("Evidence source does not exist: {normalized}")))?;
    if !resolved.starts_with(root) {
        return Err(GraphError::msg(format!(
            "Evidence path escapes project root: {relative:?}"
        )));
    }
    // Re-check the policy against the CANONICAL relative path: an in-repo
    // symlink whose own name is unblocked must not become a read alias for a
    // blocked file.
    let canonical_relative = relative_slash(root, &resolved)?;
    if blocked.is_match(&canonical_relative) {
        return Err(GraphError::msg(format!(
            "Evidence path is blocked by project policy: {canonical_relative}"
        )));
    }
    Ok(resolved)
}

fn evidence_selector(evidence: &Evidence) -> Result<EvidenceSelector> {
    if evidence.extra.contains_key("line")
        || evidence.extra.contains_key("line_start")
        || evidence.extra.contains_key("line_end")
    {
        return Err(GraphError::msg(
            "Line-number evidence is not supported; use start_anchor/end_anchor",
        ));
    }
    let start = evidence
        .start_anchor
        .as_ref()
        .or(evidence.anchor.as_ref())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| GraphError::msg("Evidence requires a non-empty start_anchor"))?
        .clone();
    let end = evidence.end_anchor.clone().unwrap_or_else(|| start.clone());
    if end.trim().is_empty() {
        return Err(GraphError::msg(
            "Evidence end_anchor must be a non-empty string",
        ));
    }
    if start.contains('\n') || end.contains('\n') {
        return Err(GraphError::msg(
            "Evidence anchors must each fit on one source line",
        ));
    }
    if evidence.start_occurrence == Some(0) || evidence.end_occurrence == Some(0) {
        return Err(GraphError::msg(
            "evidence occurrences must be positive integers",
        ));
    }
    Ok(EvidenceSelector {
        start_anchor: start,
        end_anchor: end,
        start_occurrence: evidence.start_occurrence,
        end_occurrence: evidence.end_occurrence,
    })
}

fn select_occurrence(
    lines: &[&str],
    anchor: &str,
    occurrence: Option<usize>,
    start_at: usize,
) -> Result<usize> {
    let matches: Vec<_> = lines
        .iter()
        .enumerate()
        .skip(start_at)
        .filter_map(|(index, line)| line.contains(anchor).then_some(index))
        .collect();
    match occurrence {
        None if matches.is_empty() => {
            Err(GraphError::msg(format!("Anchor {anchor:?} was not found")))
        }
        None if matches.len() != 1 => Err(GraphError::msg(format!(
            "Anchor {anchor:?} is ambiguous (found {}); set an explicit occurrence",
            matches.len()
        ))),
        None => Ok(matches[0]),
        Some(n) if matches.len() < n => Err(GraphError::msg(format!(
            "Anchor {anchor:?} occurrence {n} was not found (found {})",
            matches.len()
        ))),
        Some(n) => Ok(matches[n - 1]),
    }
}

fn evidence_key(path: &str, selector: &EvidenceSelector) -> Result<String> {
    // Match the original portable format exactly: sorted keys, compact JSON,
    // and explicit null occurrence values.
    let selector_json = format!(
        "{{\"end_anchor\":{},\"end_occurrence\":{},\"start_anchor\":{},\"start_occurrence\":{}}}",
        serde_json::to_string(&selector.end_anchor)?,
        serde_json::to_string(&selector.end_occurrence)?,
        serde_json::to_string(&selector.start_anchor)?,
        serde_json::to_string(&selector.start_occurrence)?,
    );
    let digest = sha256(selector_json.as_bytes());
    Ok(format!(
        "{}#anchor-{}",
        path.replace('\\', "/"),
        &digest[..20]
    ))
}

pub fn snapshot_evidence(
    root: &Path,
    evidence: &Evidence,
    blocked_patterns: &[String],
) -> Result<EvidenceSnapshot> {
    EvidenceResolver::new(root, blocked_patterns)?.snapshot(evidence)
}

struct SourceText {
    raw: Vec<u8>,
    lines: Vec<String>,
}

/// Resolves all evidence for one operation. It compiles blocked-path policy
/// once, reads each source file once, and reuses repeated anchor snapshots.
struct EvidenceResolver {
    root: PathBuf,
    blocked: GlobSet,
    sources: HashMap<String, SourceText>,
    snapshots: HashMap<String, EvidenceSnapshot>,
}

impl EvidenceResolver {
    fn new(root: &Path, blocked_patterns: &[String]) -> Result<Self> {
        Ok(Self {
            root: fs::canonicalize(root)?,
            blocked: blocked_set(blocked_patterns)?,
            sources: HashMap::new(),
            snapshots: HashMap::new(),
        })
    }

    fn source(&mut self, path: &str) -> Result<&SourceText> {
        let normalized = path.replace('\\', "/");
        if !self.sources.contains_key(&normalized) {
            let source = safe_source_path_with(&self.root, &self.blocked, path)?;
            let raw = fs::read(&source)?;
            let text = String::from_utf8(raw.clone()).map_err(|_| {
                GraphError::msg(format!("Evidence source is not UTF-8 text: {path}"))
            })?;
            self.sources.insert(
                normalized.clone(),
                SourceText {
                    raw,
                    lines: text.lines().map(str::to_owned).collect(),
                },
            );
        }
        self.sources
            .get(&normalized)
            .ok_or_else(|| GraphError::msg("evidence source cache entry was not created"))
    }

    fn snapshot(&mut self, evidence: &Evidence) -> Result<EvidenceSnapshot> {
        let selector = evidence_selector(evidence)?;
        let path = evidence.path.replace('\\', "/");
        let key = evidence_key(&path, &selector)?;
        if let Some(snapshot) = self.snapshots.get(&key) {
            return Ok(snapshot.clone());
        }
        let source = self.source(&path)?;
        let lines: Vec<_> = source.lines.iter().map(String::as_str).collect();
        let start =
            select_occurrence(&lines, &selector.start_anchor, selector.start_occurrence, 0)?;
        let end = if selector.end_anchor == selector.start_anchor
            && selector.end_occurrence.is_none_or(|value| value == 1)
        {
            start
        } else {
            select_occurrence(&lines, &selector.end_anchor, selector.end_occurrence, start)?
        };
        let quote = lines[start..=end].join("\n");
        let snapshot = EvidenceSnapshot {
            key: key.clone(),
            path,
            selector,
            line_start: start + 1,
            line_end: end + 1,
            file_sha256: sha256(&source.raw),
            span_sha256: sha256(quote.as_bytes()),
            quote,
        };
        self.snapshots.insert(key, snapshot.clone());
        Ok(snapshot)
    }
}

fn collect_snapshots(
    records: &[LoadedRecord],
    resolver: &mut EvidenceResolver,
) -> Result<BTreeMap<String, EvidenceSnapshot>> {
    let mut snapshots = BTreeMap::new();
    for loaded in records {
        for evidence in loaded.record.evidence() {
            let snapshot = resolver.snapshot(evidence)?;
            snapshots.insert(snapshot.key.clone(), snapshot);
        }
    }
    Ok(snapshots)
}

fn location(record: &LoadedRecord) -> String {
    format!("{}:{}", record.origin, record.line)
}

/// The SQLite index and evidence lock are canonical, reviewed-only artifacts.
/// Candidate and rejected assertions may live elsewhere (for example in raw
/// coverage-worker reports), but must never be included by `assertion_globs`.
fn canonical_records(records: &[LoadedRecord]) -> Result<()> {
    let noncanonical: Vec<_> = records
        .iter()
        .filter(|loaded| loaded.record.review() != "reviewed")
        .map(|loaded| {
            format!(
                "{}: {} {:?} is {:?}; canonical lock/build accepts only reviewed records",
                location(loaded),
                loaded.record.kind(),
                loaded.record.subject_id(),
                loaded.record.review()
            )
        })
        .collect();
    if noncanonical.is_empty() {
        Ok(())
    } else {
        Err(GraphError::msg(noncanonical.join("\n")))
    }
}

pub fn validate_records(
    root: &Path,
    config: &Config,
    records: &[LoadedRecord],
    inspect_sources: bool,
) -> Vec<String> {
    let mut resolver = if inspect_sources {
        match EvidenceResolver::new(root, &config.blocked_paths) {
            Ok(resolver) => Some(resolver),
            Err(error) => return vec![error.to_string()],
        }
    } else {
        None
    };
    validate_records_with_resolver(config, records, resolver.as_mut())
}

fn validate_records_with_resolver(
    config: &Config,
    records: &[LoadedRecord],
    mut resolver: Option<&mut EvidenceResolver>,
) -> Vec<String> {
    let mut errors = Vec::new();
    let node_types: HashSet<_> = config.node_types.iter().map(String::as_str).collect();
    let predicates: HashSet<_> = config.predicates.iter().map(String::as_str).collect();
    let mut nodes = HashMap::new();
    let mut edges = HashMap::new();
    let mut aliases = HashMap::new();

    for loaded in records {
        let where_ = location(loaded);
        match &loaded.record {
            Record::Node(node) => {
                if !valid_id(&node.id) {
                    errors.push(format!("{where_}: invalid or missing id"));
                }
                if nodes.insert(node.id.as_str(), loaded).is_some() {
                    errors.push(format!("{where_}: duplicate node id {:?}", node.id));
                }
                if !node_types.contains(node.node_type.as_str()) {
                    errors.push(format!("{where_}: unknown node type {:?}", node.node_type));
                }
                if node.name.trim().is_empty() {
                    errors.push(format!("{where_}: node name must be non-empty"));
                }
            }
            Record::Edge(edge) => {
                if !valid_id(&edge.id) {
                    errors.push(format!("{where_}: invalid or missing id"));
                }
                if edges.insert(edge.id.as_str(), loaded).is_some() {
                    errors.push(format!("{where_}: duplicate edge id {:?}", edge.id));
                }
                if !predicates.contains(edge.predicate.as_str()) {
                    errors.push(format!("{where_}: unknown predicate {:?}", edge.predicate));
                }
                if config.require_edge_evidence && edge.evidence.is_empty() {
                    errors.push(format!("{where_}: edge requires evidence"));
                }
            }
            Record::Alias(alias) => {
                if alias.alias.trim().is_empty() {
                    errors.push(format!("{where_}: alias must be non-empty"));
                }
                let folded = alias.alias.to_lowercase();
                if aliases.insert(folded, loaded).is_some() {
                    errors.push(format!("{where_}: duplicate alias {:?}", alias.alias));
                }
            }
        }
        if !REVIEWS.contains(&loaded.record.review()) {
            errors.push(format!(
                "{where_}: invalid review state {:?}",
                loaded.record.review()
            ));
        }
        for (index, evidence) in loaded.record.evidence().iter().enumerate() {
            let method = evidence.method.as_deref().unwrap_or("human");
            let review = evidence
                .review
                .as_deref()
                .unwrap_or_else(|| loaded.record.review());
            if !METHODS.contains(&method) {
                errors.push(format!(
                    "{where_}: evidence[{index}] invalid method {method:?}"
                ));
            }
            if !REVIEWS.contains(&review) {
                errors.push(format!(
                    "{where_}: evidence[{index}] invalid review state {review:?}"
                ));
            }
            if loaded.record.review() == "reviewed" && review != "reviewed" {
                errors.push(format!(
                    "{where_}: evidence[{index}] on a reviewed record must be reviewed"
                ));
            }
            // Extra keys are an extension point, but a typo'd selector field
            // ("start_occurence", "anchor_start", "line") would silently
            // change which span gets locked. Reject near-misses outright.
            for key in evidence.extra.keys() {
                let folded = key.to_lowercase();
                if folded.contains("anchor")
                    || folded.contains("occur")
                    || folded == "line"
                    || folded.starts_with("line_")
                {
                    errors.push(format!(
                        "{where_}: evidence[{index}] unknown key {key:?} resembles a selector field; \
                         use start_anchor/end_anchor/start_occurrence/end_occurrence"
                    ));
                }
            }
            if let Some(resolver) = resolver.as_deref_mut() {
                if let Err(error) = resolver.snapshot(evidence) {
                    errors.push(format!("{where_}: evidence[{index}]: {error}"));
                }
            }
        }
    }
    // Identifier namespaces must not shadow each other: resolution prefers an
    // exact node id, then aliases (`resolve_node`), and evidence lookup
    // prefers a node id over an edge id (`evidence_for`). A collision would
    // make one of the two records silently unreachable.
    let node_ids_folded: HashSet<_> = nodes.keys().map(|id| id.to_lowercase()).collect();
    for loaded in edges.values() {
        if let Record::Edge(edge) = &loaded.record {
            for (label, endpoint) in [("source", &edge.source), ("target", &edge.target)] {
                if !nodes.contains_key(endpoint.as_str()) {
                    errors.push(format!(
                        "{}: edge {:?} references missing {label} node {endpoint:?}",
                        location(loaded),
                        edge.id
                    ));
                }
            }
            if nodes.contains_key(edge.id.as_str()) {
                errors.push(format!(
                    "{}: edge id {:?} collides with a node id; the node would shadow it",
                    location(loaded),
                    edge.id
                ));
            }
        }
    }
    for loaded in aliases.values() {
        if let Record::Alias(alias) = &loaded.record {
            if !nodes.contains_key(alias.node.as_str()) {
                errors.push(format!(
                    "{}: alias {:?} references missing node {:?}",
                    location(loaded),
                    alias.alias,
                    alias.node
                ));
            }
            if node_ids_folded.contains(&alias.alias.to_lowercase()) {
                errors.push(format!(
                    "{}: alias {:?} collides with a node id; the node would shadow it",
                    location(loaded),
                    alias.alias
                ));
            }
        }
    }
    errors
}

pub fn load_lock(root: &Path) -> Result<EvidenceLock> {
    let path = root.join(LOCK_FILE);
    if !path.exists() {
        return Ok(EvidenceLock::default());
    }
    let lock: EvidenceLock = serde_json::from_slice(&fs::read(&path)?)?;
    if lock.version > LOCK_VERSION {
        return Err(GraphError::msg(format!(
            "{}: lock version {} is newer than this binary supports ({LOCK_VERSION}); upgrade project-graph",
            path.display(),
            lock.version
        )));
    }
    Ok(lock)
}

/// Replace a source-of-truth file without a truncation window: the new content
/// is fully written under a temporary name first, then swapped in by renames.
/// (Windows cannot rename over an existing file, hence the backup step.) The
/// evidence lock is an acceptance record, not a rebuildable cache, so a crash
/// mid-write must never leave it half-written.
fn replace_file(path: &Path, contents: &str) -> Result<()> {
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, contents)?;
    if path.exists() {
        let backup = path.with_extension("json.bak");
        if backup.exists() {
            fs::remove_file(&backup)?;
        }
        fs::rename(path, &backup)?;
        if let Err(error) = fs::rename(&temp, path) {
            let _ = fs::rename(&backup, path);
            let _ = fs::remove_file(&temp);
            return Err(error.into());
        }
        let _ = fs::remove_file(&backup);
    } else {
        fs::rename(&temp, path)?;
    }
    Ok(())
}

pub fn write_lock(
    root: &Path,
    config: &Config,
    records: &[LoadedRecord],
    require_clean_worktree: bool,
) -> Result<usize> {
    canonical_records(records)?;
    let git = git_context(root);
    if require_clean_worktree && !git.as_ref().is_some_and(|context| context.worktree_clean) {
        return Err(GraphError::msg(
            "cannot lock: --require-clean-worktree requires a clean Git worktree",
        ));
    }
    let mut resolver = EvidenceResolver::new(root, &config.blocked_paths)?;
    let snapshots = collect_snapshots(records, &mut resolver)?;
    let snapshots = snapshots
        .into_values()
        .map(|snapshot| {
            Ok((
                snapshot.key,
                LockedEvidence {
                    path: snapshot.path,
                    selector: snapshot.selector,
                    span_sha256: snapshot.span_sha256,
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let count = snapshots.len();
    let lock = EvidenceLock {
        version: LOCK_VERSION,
        updated_at: Some(utc_now()),
        git,
        evidence: snapshots,
    };
    let path = root.join(LOCK_FILE);
    replace_file(
        &path,
        &format!("{}\n", serde_json::to_string_pretty(&lock)?),
    )?;
    Ok(count)
}

pub fn verify_lock(
    root: &Path,
    config: &Config,
    records: &[LoadedRecord],
    require_fresh: bool,
    require_same_commit: bool,
    require_clean_worktree: bool,
) -> Result<Value> {
    let evidence = audit_evidence(root, config, records)?;
    let nonfresh: Vec<_> = evidence
        .iter()
        .filter(|row| row["lock_status"] != "fresh")
        .cloned()
        .collect();
    let git = git_lock_status(root, &load_lock(root)?);
    let mut failures = Vec::new();
    if require_fresh && !nonfresh.is_empty() {
        failures.push("evidence is not fresh");
    }
    if require_same_commit && git["status"] != "same_commit" {
        failures.push("Git HEAD differs from the lock or was not recorded");
    }
    if require_clean_worktree && git["current"]["worktree_clean"] != Value::Bool(true) {
        failures.push("current Git worktree is not clean");
    }
    Ok(json!({
        "valid": failures.is_empty(),
        "requirements": {
            "fresh_evidence": require_fresh,
            "same_commit": require_same_commit,
            "clean_worktree": require_clean_worktree,
        },
        "nonfresh_evidence": nonfresh.len(),
        "git": git,
        "failures": failures,
    }))
}

fn snapshot_status(snapshot: &EvidenceSnapshot, locked: Option<&LockedEvidence>) -> &'static str {
    match locked {
        None => "unlocked",
        Some(value) if value.span_sha256 != snapshot.span_sha256 => "stale",
        Some(_) => "fresh",
    }
}

pub fn audit_evidence(
    root: &Path,
    config: &Config,
    records: &[LoadedRecord],
) -> Result<Vec<Value>> {
    let lock = load_lock(root)?;
    let mut resolver = EvidenceResolver::new(root, &config.blocked_paths)?;
    let mut rows = Vec::new();
    for loaded in records {
        for evidence in loaded.record.evidence() {
            match resolver.snapshot(evidence) {
                Ok(snapshot) => {
                    let locked = lock.evidence.get(&snapshot.key);
                    rows.push(json!({
                        "subject_kind": loaded.record.kind(),
                        "subject_id": loaded.record.subject_id(),
                        "path": snapshot.path,
                        "selector": snapshot.selector,
                        "lock_status": snapshot_status(&snapshot, locked),
                        "locked_span_sha256": locked.map(|value| &value.span_sha256),
                        "actual_span_sha256": snapshot.span_sha256,
                        "resolved_line_start": snapshot.line_start,
                        "resolved_line_end": snapshot.line_end,
                    }));
                }
                Err(error) => rows.push(json!({
                    "subject_kind": loaded.record.kind(),
                    "subject_id": loaded.record.subject_id(),
                    "path": evidence.path,
                    "lock_status": "unresolved",
                    "error": error.to_string(),
                })),
            }
        }
    }
    Ok(rows)
}

const SCHEMA_SQL: &str = r"
PRAGMA foreign_keys = ON;
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE nodes (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    review TEXT NOT NULL CHECK (review = 'reviewed'),
    attributes_json TEXT NOT NULL
);
CREATE TABLE aliases (
    alias TEXT PRIMARY KEY COLLATE NOCASE,
    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE
);
CREATE TABLE edges (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES nodes(id),
    predicate TEXT NOT NULL,
    target_id TEXT NOT NULL REFERENCES nodes(id),
    description TEXT NOT NULL DEFAULT '',
    review TEXT NOT NULL CHECK (review = 'reviewed'),
    confidence REAL,
    valid_from TEXT,
    valid_to TEXT,
    attributes_json TEXT NOT NULL
);
CREATE TABLE evidence (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    subject_kind TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    path TEXT NOT NULL,
    selector_json TEXT NOT NULL,
    line_start INTEGER NOT NULL,
    line_end INTEGER NOT NULL,
    method TEXT NOT NULL,
    review TEXT NOT NULL CHECK (review = 'reviewed'),
    extractor_version TEXT,
    lock_status TEXT NOT NULL,
    locked_span_sha256 TEXT,
    actual_span_sha256 TEXT NOT NULL,
    file_sha256 TEXT NOT NULL,
    quote TEXT NOT NULL
);
CREATE INDEX edges_source_idx ON edges(source_id);
CREATE INDEX edges_target_idx ON edges(target_id);
CREATE INDEX edges_predicate_idx ON edges(predicate);
CREATE INDEX evidence_subject_idx ON evidence(subject_kind, subject_id);
CREATE INDEX evidence_status_idx ON evidence(lock_status);
CREATE VIRTUAL TABLE search_index USING fts5(
    kind UNINDEXED,
    graph_id UNINDEXED,
    text,
    tokenize = 'porter unicode61 remove_diacritics 2'
);
";

fn database_path(root: &Path, config: &Config) -> PathBuf {
    root.join(&config.database)
}

fn fingerprint_part(digest: &mut Sha256, label: &str, value: &[u8]) {
    digest.update(label.as_bytes());
    digest.update([0]);
    digest.update(value.len().to_le_bytes());
    digest.update(value);
    digest.update([0]);
}

fn build_fingerprint(
    root: &Path,
    config: &Config,
    snapshots: &BTreeMap<String, EvidenceSnapshot>,
    lock: &EvidenceLock,
) -> Result<String> {
    let mut digest = Sha256::new();
    // The schema text participates in the fingerprint so upgrading to a binary
    // with a different schema can never silently reuse an old-layout index.
    fingerprint_part(&mut digest, "schema", SCHEMA_SQL.as_bytes());
    fingerprint_part(
        &mut digest,
        "config",
        &fs::read(root.join(CONFIG_DIR).join("config.json"))?,
    );
    for assertion in assertion_paths(root, config)? {
        fingerprint_part(
            &mut digest,
            &format!("assertion:{}", relative_slash(root, &assertion)?),
            &fs::read(assertion)?,
        );
    }
    for (key, snapshot) in snapshots {
        fingerprint_part(
            &mut digest,
            &format!("snapshot:{key}"),
            snapshot.file_sha256.as_bytes(),
        );
        fingerprint_part(
            &mut digest,
            &format!("span:{key}"),
            snapshot.span_sha256.as_bytes(),
        );
    }
    for (key, evidence) in &lock.evidence {
        fingerprint_part(
            &mut digest,
            &format!("lock:{key}"),
            evidence.span_sha256.as_bytes(),
        );
    }
    Ok(format!("{:x}", digest.finalize()))
}

/// A cached index is reusable only when it is readable and records the same
/// build fingerprint. Every failure here — a missing file, a corrupt database,
/// an index written by an older schema — means "rebuild". The cache is
/// disposable by contract, so it must never be able to make `build` fail.
fn reusable_build_counts(path: &Path, fingerprint: &str) -> Option<BuildCounts> {
    let attempt = || -> Result<Option<BuildCounts>> {
        if !path.is_file() {
            return Ok(None);
        }
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let previous: Option<String> = connection
            .query_row(
                "SELECT value FROM meta WHERE key = 'build_fingerprint'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if previous.as_deref() != Some(fingerprint) {
            return Ok(None);
        }
        let count = |table: &str| -> Result<usize> {
            Ok(
                connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })?,
            )
        };
        Ok(Some(BuildCounts {
            nodes: count("nodes")?,
            edges: count("edges")?,
            aliases: count("aliases")?,
            evidence: count("evidence")?,
            reused: true,
        }))
    };
    attempt().ok().flatten()
}

pub fn build_database(
    root: &Path,
    config: &Config,
    records: &[LoadedRecord],
) -> Result<(PathBuf, BuildCounts)> {
    let mut resolver = EvidenceResolver::new(root, &config.blocked_paths)?;
    let errors = validate_records_with_resolver(config, records, Some(&mut resolver));
    if !errors.is_empty() {
        return Err(GraphError::msg(errors.join("\n")));
    }
    // Reject before opening/removing a temporary index so a failed build leaves
    // the last known-good reviewed index intact.
    canonical_records(records)?;
    let lock = load_lock(root)?;
    let snapshots = collect_snapshots(records, &mut resolver)?;
    let path = database_path(root, config);
    let fingerprint = build_fingerprint(root, config, &snapshots, &lock)?;
    if let Some(counts) = reusable_build_counts(&path, &fingerprint) {
        return Ok((path, counts));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!("sqlite.tmp-{}", std::process::id()));
    if temp.exists() {
        fs::remove_file(&temp)?;
    }
    let result = build_database_at(config, records, &lock, &snapshots, &fingerprint, &temp);
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    let counts = result?;
    let backup = path.with_extension("sqlite.bak");
    if backup.exists() {
        fs::remove_file(&backup)?;
    }
    if path.exists() {
        fs::rename(&path, &backup)?;
    }
    if let Err(error) = fs::rename(&temp, &path) {
        if backup.exists() {
            let _ = fs::rename(&backup, &path);
        }
        return Err(error.into());
    }
    if backup.exists() {
        fs::remove_file(backup)?;
    }
    Ok((path, counts))
}

fn build_database_at(
    config: &Config,
    records: &[LoadedRecord],
    lock: &EvidenceLock,
    snapshots: &BTreeMap<String, EvidenceSnapshot>,
    fingerprint: &str,
    path: &Path,
) -> Result<BuildCounts> {
    let mut connection = Connection::open(path)?;
    connection.execute_batch(SCHEMA_SQL)?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT INTO meta VALUES (?1, ?2)",
        params!["project", config.project],
    )?;
    transaction.execute(
        "INSERT INTO meta VALUES (?1, ?2)",
        params![
            "schema_version",
            config
                .schema_version
                .as_str()
                .map_or_else(|| config.schema_version.to_string(), str::to_owned)
        ],
    )?;
    transaction.execute(
        "INSERT INTO meta VALUES (?1, ?2)",
        params!["built_at", utc_now()],
    )?;
    transaction.execute(
        "INSERT INTO meta VALUES (?1, ?2)",
        params!["build_fingerprint", fingerprint],
    )?;
    let mut counts = BuildCounts::default();
    let node_names: HashMap<_, _> = records
        .iter()
        .filter_map(|loaded| match &loaded.record {
            Record::Node(node) => Some((node.id.as_str(), node.name.as_str())),
            _ => None,
        })
        .collect();
    for loaded in records {
        if let Record::Node(node) = &loaded.record {
            transaction.execute(
                "INSERT INTO nodes VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    node.id,
                    node.node_type,
                    node.name,
                    node.description,
                    node.review,
                    stored_json(&node.attributes)?
                ],
            )?;
            transaction.execute(
                "INSERT INTO search_index (kind, graph_id, text) VALUES ('node', ?1, ?2)",
                params![
                    node.id,
                    format!(
                        "{} {} {} {}",
                        node.id, node.node_type, node.name, node.description
                    )
                ],
            )?;
            counts.nodes += 1;
        }
    }
    for loaded in records {
        if let Record::Alias(alias) = &loaded.record {
            transaction.execute(
                "INSERT INTO aliases VALUES (?1, ?2)",
                params![alias.alias, alias.node],
            )?;
            transaction.execute(
                "INSERT INTO search_index (kind, graph_id, text) VALUES ('alias', ?1, ?2)",
                params![alias.node, alias.alias],
            )?;
            counts.aliases += 1;
        }
    }
    for loaded in records {
        if let Record::Edge(edge) = &loaded.record {
            transaction.execute(
                "INSERT INTO edges VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    edge.id,
                    edge.source,
                    edge.predicate,
                    edge.target,
                    edge.description,
                    edge.review,
                    edge.confidence,
                    edge.valid_from,
                    edge.valid_to,
                    stored_json(&edge.attributes)?
                ],
            )?;
            transaction.execute(
                "INSERT INTO search_index (kind, graph_id, text) VALUES ('edge', ?1, ?2)",
                params![
                    edge.id,
                    format!(
                        "{} {} {} {} {} {} {}",
                        edge.id,
                        edge.predicate,
                        edge.description,
                        edge.source,
                        node_names
                            .get(edge.source.as_str())
                            .copied()
                            .unwrap_or_default(),
                        edge.target,
                        node_names
                            .get(edge.target.as_str())
                            .copied()
                            .unwrap_or_default(),
                    )
                ],
            )?;
            counts.edges += 1;
        }
    }
    for loaded in records {
        for evidence in loaded.record.evidence() {
            let selector = evidence_selector(evidence)?;
            let key = evidence_key(&evidence.path, &selector)?;
            let snapshot = snapshots.get(&key).ok_or_else(|| {
                GraphError::msg(format!("missing cached evidence snapshot for {key}"))
            })?;
            let locked = lock.evidence.get(&snapshot.key);
            transaction.execute(
                r"INSERT INTO evidence (
                    subject_kind, subject_id, path, selector_json, line_start, line_end,
                    method, review, extractor_version, lock_status,
                    locked_span_sha256, actual_span_sha256, file_sha256, quote
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    loaded.record.kind(),
                    loaded.record.subject_id(),
                    snapshot.path,
                    stored_json(&snapshot.selector)?,
                    snapshot.line_start,
                    snapshot.line_end,
                    evidence.method.as_deref().unwrap_or("human"),
                    evidence
                        .review
                        .as_deref()
                        .unwrap_or_else(|| loaded.record.review()),
                    evidence.extractor_version,
                    snapshot_status(snapshot, locked),
                    locked.map(|value| value.span_sha256.as_str()),
                    snapshot.span_sha256,
                    snapshot.file_sha256,
                    snapshot.quote,
                ],
            )?;
            counts.evidence += 1;
        }
    }
    transaction.commit()?;
    connection.execute_batch("PRAGMA optimize;")?;
    drop(connection);
    Ok(counts)
}

pub fn connect_database(root: &Path, config: &Config) -> Result<Connection> {
    let path = database_path(root, config);
    if !path.is_file() {
        return Err(GraphError::msg(format!(
            "Graph index does not exist: {}; run `project-graph build`",
            path.display()
        )));
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA query_only = ON;")?;
    Ok(connection)
}

/// Compact operational state for humans and agents. This never requires an
/// index to exist, so it is safe as the first command in a workflow.
pub fn project_status(root: &Path, config: &Config, records: &[LoadedRecord]) -> Result<Value> {
    let evidence = audit_evidence(root, config, records)?;
    let mut evidence_counts = BTreeMap::<String, usize>::new();
    for row in &evidence {
        let status = row["lock_status"].as_str().unwrap_or("unresolved");
        *evidence_counts.entry(status.to_owned()).or_default() += 1;
    }
    let path = database_path(root, config);
    let index = if !path.is_file() {
        json!({"state": "missing", "path": path})
    } else {
        match connect_database(root, config) {
            Ok(connection) => {
                let metadata = query_values(
                    &connection,
                    "SELECT key, value FROM meta WHERE key IN ('built_at', 'build_fingerprint') ORDER BY key",
                    [],
                )?;
                let health = diagnostics(&connection)?;
                json!({
                    "state": "ready",
                    "path": path,
                    "metadata": metadata,
                    "nodes": health["nodes"],
                    "edges": health["edges"],
                })
            }
            Err(error) => json!({"state": "unreadable", "path": path, "error": error.to_string()}),
        }
    };
    Ok(json!({
        "project": config.project,
        "records": records.len(),
        "index": index,
        "evidence": {
            "total": evidence.len(),
            "counts": evidence_counts,
        },
        "git": git_lock_status(root, &load_lock(root)?),
    }))
}

fn sql_value(value: rusqlite::types::ValueRef<'_>) -> Value {
    use rusqlite::types::ValueRef;
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => json!(value),
        ValueRef::Real(value) => json!(value),
        ValueRef::Text(value) => Value::String(String::from_utf8_lossy(value).into_owned()),
        ValueRef::Blob(value) => Value::String(format!("<{} bytes>", value.len())),
    }
}

fn row_json(row: &Row<'_>) -> rusqlite::Result<Value> {
    let mut value = Map::new();
    let reference = row.as_ref();
    for index in 0..reference.column_count() {
        let name = reference.column_name(index)?.to_owned();
        value.insert(name, sql_value(row.get_ref(index)?));
    }
    Ok(Value::Object(value))
}

fn query_values(
    connection: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<Value>> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement
        .query_map(params, row_json)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn object_string<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn fts_query(term: &str) -> Option<String> {
    // Every term is quoted so FTS5 treats it as a plain token: an unquoted
    // uppercase AND/OR/NOT/NEAR in user input is otherwise parsed as a query
    // operator and raises a syntax error. Terms contain only alphanumerics
    // here (the split guarantees it), so no embedded-quote escaping is needed.
    let terms: Vec<_> = term
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .take(16)
        .map(|term| format!("\"{term}\""))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" AND "))
}

/// A contains-match LIKE pattern with `%`, `_`, and `\` treated as literal
/// characters (pair with `ESCAPE '\'`). Underscores are ubiquitous in code
/// identifiers; without escaping every `_` is a one-character wildcard.
fn like_pattern(term: &str) -> String {
    let mut escaped = String::with_capacity(term.len() + 2);
    for character in term.chars() {
        if matches!(character, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    format!("%{escaped}%")
}

pub fn resolve_node(connection: &Connection, value: &str) -> Result<Value> {
    let exact = query_values(
        connection,
        "SELECT * FROM nodes WHERE id = ?1 AND review = 'reviewed'",
        params![value],
    )?;
    if let Some(row) = exact.into_iter().next() {
        return Ok(row);
    }
    let alias = query_values(
        connection,
        r"SELECT n.* FROM aliases a JOIN nodes n ON n.id = a.node_id
          WHERE a.alias = ?1 COLLATE NOCASE AND n.review = 'reviewed'",
        params![value],
    )?;
    if let Some(row) = alias.into_iter().next() {
        return Ok(row);
    }
    let like = like_pattern(value);
    let rows = query_values(
        connection,
        r"SELECT * FROM nodes WHERE review = 'reviewed'
          AND (name LIKE ?1 ESCAPE '\' OR id LIKE ?1 ESCAPE '\') ORDER BY id LIMIT 11",
        params![like],
    )?;
    match rows.len() {
        0 => Err(GraphError::msg(format!("No node matches {value:?}"))),
        1 => Ok(rows.into_iter().next().expect("one row")),
        _ => Err(GraphError::msg(format!(
            "Ambiguous node {value:?}; matches: {}",
            rows.iter()
                .take(10)
                .map(|row| object_string(row, "id"))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

pub fn search(connection: &Connection, term: &str, limit: usize) -> Result<Value> {
    let like = like_pattern(term);
    let (nodes, edges) = if let Some(match_query) = fts_query(term) {
        (
            query_values(
                connection,
                r"SELECT DISTINCT n.* FROM search_index si
                  JOIN nodes n ON n.id = si.graph_id
                  WHERE search_index MATCH ?1 AND si.kind IN ('node', 'alias')
                    AND n.review = 'reviewed'
                  ORDER BY bm25(search_index), n.type, n.name LIMIT ?2",
                params![&match_query, limit],
            )?,
            query_values(
                connection,
                r"SELECT DISTINCT e.*, s.name AS source_name, t.name AS target_name
                  FROM search_index si
                  JOIN edges e ON e.id = si.graph_id
                  JOIN nodes s ON s.id = e.source_id JOIN nodes t ON t.id = e.target_id
                  WHERE search_index MATCH ?1 AND si.kind = 'edge'
                    AND e.review = 'reviewed' AND s.review = 'reviewed' AND t.review = 'reviewed'
                  ORDER BY bm25(search_index), e.predicate, e.id LIMIT ?2",
                params![&match_query, limit],
            )?,
        )
    } else {
        (
            query_values(
                connection,
                r"SELECT DISTINCT n.* FROM nodes n
                  LEFT JOIN aliases a ON a.node_id = n.id
                  WHERE n.review = 'reviewed' AND
                    (n.id LIKE ?1 ESCAPE '\' OR n.name LIKE ?1 ESCAPE '\'
                     OR n.description LIKE ?1 ESCAPE '\' OR a.alias LIKE ?1 ESCAPE '\')
                  ORDER BY n.type, n.name LIMIT ?2",
                params![like, limit],
            )?,
            query_values(
                connection,
                r"SELECT e.*, s.name AS source_name, t.name AS target_name
                  FROM edges e JOIN nodes s ON s.id = e.source_id JOIN nodes t ON t.id = e.target_id
                  WHERE e.review = 'reviewed' AND s.review = 'reviewed' AND t.review = 'reviewed' AND
                    (e.id LIKE ?1 ESCAPE '\' OR e.predicate LIKE ?1 ESCAPE '\'
                     OR e.description LIKE ?1 ESCAPE '\')
                  ORDER BY e.predicate, e.id LIMIT ?2",
                params![like, limit],
            )?,
        )
    };
    Ok(json!({"nodes": nodes, "edges": edges}))
}

fn graph_edges(connection: &Connection) -> Result<Vec<Value>> {
    query_values(
        connection,
        r"SELECT e.*, s.name AS source_name, s.type AS source_type,
          t.name AS target_name, t.type AS target_type
          FROM edges e JOIN nodes s ON s.id = e.source_id
          JOIN nodes t ON t.id = e.target_id
          WHERE e.review = 'reviewed' AND s.review = 'reviewed' AND t.review = 'reviewed'
          ORDER BY e.id",
        [],
    )
}

fn nodes_by_ids(connection: &Connection, ids: &HashSet<String>) -> Result<Vec<Value>> {
    let mut rows = query_values(
        connection,
        "SELECT * FROM nodes WHERE review = 'reviewed' ORDER BY type, name",
        [],
    )?;
    rows.retain(|row| ids.contains(object_string(row, "id")));
    Ok(rows)
}

pub fn traverse(
    connection: &Connection,
    center: &str,
    depth: usize,
    direction: &str,
    predicates: Option<&HashSet<String>>,
) -> Result<Value> {
    if !["in", "out", "both"].contains(&direction) {
        return Err(GraphError::msg("direction must be in, out, or both"));
    }
    let start = resolve_node(connection, center)?;
    let start_id = object_string(&start, "id").to_owned();
    // The whole edge table is loaded once and rescanned per level: O(depth x
    // edges) with string comparisons. That is deliberate — graphs here are
    // curated (hundreds of edges, not millions) and one query keeps SQLite
    // read-only and trivially correct. Revisit only if profiles say so.
    let rows = graph_edges(connection)?;
    let mut visited = HashSet::from([start_id.clone()]);
    let mut frontier = HashSet::from([start_id]);
    let mut selected = BTreeMap::new();
    for _ in 0..depth {
        let mut next = HashSet::new();
        for row in &rows {
            let predicate = object_string(row, "predicate");
            if predicates.is_some_and(|items| !items.contains(predicate)) {
                continue;
            }
            let source = object_string(row, "source_id");
            let target = object_string(row, "target_id");
            let neighbor = if ["out", "both"].contains(&direction) && frontier.contains(source) {
                Some(target)
            } else if ["in", "both"].contains(&direction) && frontier.contains(target) {
                Some(source)
            } else {
                None
            };
            if let Some(neighbor) = neighbor {
                selected.insert(object_string(row, "id").to_owned(), row.clone());
                if !visited.contains(neighbor) {
                    next.insert(neighbor.to_owned());
                }
            }
        }
        visited.extend(next.iter().cloned());
        frontier = next;
        if frontier.is_empty() {
            break;
        }
    }
    // For undirected neighborhoods, complete the edge set to the induced
    // subgraph: two nodes both discovered in the FINAL level can be joined by
    // an edge the level scan never walked (neither endpoint was in a frontier
    // while the other existed). Directed traversals keep discovery edges only,
    // because an induced completion would smuggle in wrong-direction edges.
    if direction == "both" {
        for row in &rows {
            let predicate = object_string(row, "predicate");
            if predicates.is_some_and(|items| !items.contains(predicate)) {
                continue;
            }
            if visited.contains(object_string(row, "source_id"))
                && visited.contains(object_string(row, "target_id"))
            {
                selected.insert(object_string(row, "id").to_owned(), row.clone());
            }
        }
    }
    Ok(json!({
        "center": start,
        "nodes": nodes_by_ids(connection, &visited)?,
        "edges": selected.into_values().collect::<Vec<_>>(),
    }))
}

pub fn shortest_path(
    connection: &Connection,
    source: &str,
    target: &str,
    max_depth: usize,
    direction: &str,
) -> Result<Value> {
    if !["in", "out", "both"].contains(&direction) {
        return Err(GraphError::msg("direction must be in, out, or both"));
    }
    let start = resolve_node(connection, source)?;
    let goal = resolve_node(connection, target)?;
    let start_id = object_string(&start, "id").to_owned();
    let goal_id = object_string(&goal, "id").to_owned();
    let rows = graph_edges(connection)?;
    let mut adjacency: HashMap<String, Vec<(String, Value)>> = HashMap::new();
    for row in rows {
        let source_id = object_string(&row, "source_id").to_owned();
        let target_id = object_string(&row, "target_id").to_owned();
        if ["out", "both"].contains(&direction) {
            adjacency
                .entry(source_id.clone())
                .or_default()
                .push((target_id.clone(), row.clone()));
        }
        if ["in", "both"].contains(&direction) {
            adjacency
                .entry(target_id)
                .or_default()
                .push((source_id, row));
        }
    }
    let mut queue = VecDeque::from([(start_id.clone(), Vec::<(String, Value)>::new())]);
    let mut visited = HashSet::from([start_id.clone()]);
    let mut found = None;
    while let Some((current, path)) = queue.pop_front() {
        if current == goal_id {
            found = Some(path);
            break;
        }
        if path.len() >= max_depth {
            continue;
        }
        for (neighbor, edge) in adjacency.get(&current).into_iter().flatten() {
            if visited.insert(neighbor.clone()) {
                let mut next = path.clone();
                next.push((neighbor.clone(), edge.clone()));
                queue.push_back((neighbor.clone(), next));
            }
        }
    }
    let found = found.ok_or_else(|| {
        GraphError::msg(format!(
            "No path from {start_id} to {goal_id} within {max_depth} hops"
        ))
    })?;
    let ids: Vec<_> = std::iter::once(start_id)
        .chain(found.iter().map(|(node, _)| node.clone()))
        .collect();
    let selected: HashSet<_> = ids.iter().cloned().collect();
    let by_id: HashMap<_, _> = nodes_by_ids(connection, &selected)?
        .into_iter()
        .map(|row| (object_string(&row, "id").to_owned(), row))
        .collect();
    Ok(json!({
        "nodes": ids.iter().filter_map(|id| by_id.get(id)).collect::<Vec<_>>(),
        "edges": found.iter().map(|(_, edge)| edge).collect::<Vec<_>>(),
    }))
}

pub fn evidence_for(connection: &Connection, subject: &str) -> Result<Value> {
    let node = query_values(
        connection,
        "SELECT id FROM nodes WHERE id = ?1 AND review = 'reviewed'",
        params![subject],
    )?;
    let (kind, identifier) = if node.is_empty() {
        let edge = query_values(
            connection,
            "SELECT id FROM edges WHERE id = ?1 AND review = 'reviewed'",
            params![subject],
        )?;
        if edge.is_empty() {
            let resolved = resolve_node(connection, subject)?;
            ("node", object_string(&resolved, "id").to_owned())
        } else {
            ("edge", subject.to_owned())
        }
    } else {
        ("node", subject.to_owned())
    };
    Ok(Value::Array(query_values(
        connection,
        "SELECT * FROM evidence WHERE subject_kind = ?1 AND subject_id = ?2 AND review = 'reviewed' ORDER BY path, line_start",
        params![kind, identifier],
    )?))
}

pub fn diagnostics(connection: &Connection) -> Result<Value> {
    let scalar =
        |sql: &str| -> Result<i64> { Ok(connection.query_row(sql, [], |row| row.get(0))?) };
    let nodes = scalar("SELECT COUNT(*) FROM nodes WHERE review='reviewed'")?;
    let edges = scalar("SELECT COUNT(*) FROM edges WHERE review='reviewed'")?;
    let candidates = scalar(
        "SELECT (SELECT COUNT(*) FROM nodes WHERE review='candidate') +
         (SELECT COUNT(*) FROM edges WHERE review='candidate')",
    )?;
    let nonfresh = scalar("SELECT COUNT(*) FROM evidence WHERE lock_status != 'fresh'")?;
    let top_degree = query_values(
        connection,
        r"SELECT n.id, n.name, n.type, COUNT(e.id) AS degree FROM nodes n
          LEFT JOIN edges e ON (e.source_id=n.id OR e.target_id=n.id) AND e.review='reviewed'
          WHERE n.review='reviewed'
          GROUP BY n.id ORDER BY degree DESC, n.id LIMIT 10",
        [],
    )?;
    Ok(json!({
        "nodes": nodes,
        "edges": edges,
        "edge_node_ratio": if nodes == 0 { 0.0 } else {
            ((edges as f64 / nodes as f64) * 1000.0).round() / 1000.0
        },
        "candidate_facts": candidates,
        "nonfresh_evidence": nonfresh,
        "top_degree": top_degree,
    }))
}

pub fn evaluate_gold(root: &Path, connection: &Connection) -> Result<Value> {
    let path = root.join(GOLD_FILE);
    if !path.is_file() {
        return Err(GraphError::MissingFile(path));
    }
    let gold: GoldFile = serde_json::from_slice(&fs::read(&path)?)?;
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;
    let mut results = Vec::new();
    for (index, question) in gold.questions.iter().enumerate() {
        let where_ = format!("{GOLD_FILE}: question {}", index + 1);
        if question.question.trim().is_empty() {
            return Err(GraphError::msg(format!(
                "{where_} requires a non-empty question"
            )));
        }
        if question.seeds.is_empty() || question.expected_nodes.is_empty() {
            skipped += 1;
            results.push(json!({
                "question": question.question,
                "status": "skipped",
                "reason": "seeds and expected_nodes are required for an executable check",
            }));
            continue;
        }
        if !["in", "out", "both"].contains(&question.direction.as_str()) {
            return Err(GraphError::msg(format!(
                "{where_} direction must be in, out, or both"
            )));
        }
        let predicates = question
            .predicates
            .as_ref()
            .map(|values| values.iter().cloned().collect::<HashSet<_>>());
        let seeds = question
            .seeds
            .iter()
            .map(|seed| {
                resolve_node(connection, seed).map(|row| object_string(&row, "id").to_owned())
            })
            .collect::<Result<Vec<_>>>()?;
        let expected = question
            .expected_nodes
            .iter()
            .map(|node| {
                resolve_node(connection, node).map(|row| object_string(&row, "id").to_owned())
            })
            .collect::<Result<HashSet<_>>>()?;
        let mut retrieved = HashSet::new();
        for seed in &seeds {
            let neighborhood = traverse(
                connection,
                seed,
                question.depth,
                &question.direction,
                predicates.as_ref(),
            )?;
            for node in neighborhood["nodes"].as_array().into_iter().flatten() {
                retrieved.insert(object_string(node, "id").to_owned());
            }
        }
        let mut missing: Vec<_> = expected.difference(&retrieved).cloned().collect();
        missing.sort();
        let status = if missing.is_empty() {
            passed += 1;
            "passed"
        } else {
            failed += 1;
            "failed"
        };
        let coverage = ((expected.len() - missing.len()) as f64 / expected.len() as f64 * 1000.0)
            .round()
            / 1000.0;
        results.push(json!({
            "question": question.question,
            "status": status,
            "seeds": seeds,
            "depth": question.depth,
            "direction": question.direction,
            "expected": expected.len(),
            "retrieved": retrieved.len(),
            "coverage": coverage,
            "missing_nodes": missing,
        }));
    }
    Ok(json!({
        "questions": gold.questions.len(),
        "passed": passed,
        "failed": failed,
        "skipped": skipped,
        "results": results,
    }))
}

fn mermaid_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "&quot;")
        .replace('\n', "<br/>")
}

pub fn mermaid(connection: &Connection, selected: Option<&HashSet<String>>) -> Result<String> {
    let mut nodes = query_values(
        connection,
        "SELECT * FROM nodes WHERE review = 'reviewed' ORDER BY type, name",
        [],
    )?;
    let mut edges = graph_edges(connection)?;
    if let Some(selected) = selected {
        nodes.retain(|node| selected.contains(object_string(node, "id")));
        edges.retain(|edge| {
            selected.contains(object_string(edge, "source_id"))
                && selected.contains(object_string(edge, "target_id"))
        });
    }
    let graph_ids: HashMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(index, row)| {
            (
                object_string(row, "id").to_owned(),
                format!("N{}", index + 1),
            )
        })
        .collect();
    let mut by_type: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for node in &nodes {
        by_type
            .entry(object_string(node, "type").to_owned())
            .or_default()
            .push(node);
    }
    let mut lines = vec!["flowchart LR".to_owned()];
    for (index, (node_type, typed)) in by_type.iter().enumerate() {
        lines.push(format!(
            "    subgraph G{}[\"{}\"]",
            index + 1,
            mermaid_escape(node_type)
        ));
        for node in typed {
            let id = object_string(node, "id");
            let label = format!("{}<br/><small>{}</small>", object_string(node, "name"), id);
            lines.push(format!(
                "        {}[\"{}\"]",
                graph_ids[id],
                mermaid_escape(&label)
            ));
        }
        lines.push("    end".to_owned());
    }
    for edge in &edges {
        lines.push(format!(
            "    {} -->|\"{}\"| {}",
            graph_ids[object_string(edge, "source_id")],
            mermaid_escape(object_string(edge, "predicate")),
            graph_ids[object_string(edge, "target_id")]
        ));
    }
    Ok(format!("{}\n", lines.join("\n")))
}

pub fn init_project(root: &Path, name: Option<&str>) -> Result<InitializedProject> {
    fs::create_dir_all(root)?;
    let root = fs::canonicalize(root)?;
    let graph = root.join(CONFIG_DIR);
    let assertions = graph.join("assertions");
    fs::create_dir_all(graph.join("cache"))?;
    fs::create_dir_all(&assertions)?;
    let config_path = graph.join("config.json");
    let assertions_path = assertions.join("core.jsonl");
    let gold_path = root.join(GOLD_FILE);
    let ignore_path = graph.join(".gitignore");
    let lock_path = root.join(LOCK_FILE);
    let mut created = Vec::new();
    if !config_path.exists() {
        let project = name
            .map(str::to_owned)
            .or_else(|| {
                root.file_name()
                    .map(|value| value.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "project".to_owned());
        let config = json!({
            "schema_version": "1.0",
            "project": project,
            "database": DEFAULT_DB,
            "assertion_globs": [".project-graph/assertions/*.jsonl"],
            "require_edge_evidence": true,
            "node_types": ["Component", "File", "Symbol", "Interface", "State",
                "DataStore", "Invariant", "Flow", "Decision", "Incident", "Release",
                "Verification"],
            "predicates": ["DEFINES", "CALLS", "SENDS", "HANDLES", "TRANSITIONS_TO",
                "READS", "WRITES", "DEPENDS_ON", "IMPLEMENTS", "GUARDED_BY",
                "DOCUMENTED_IN", "VERIFIED_BY", "VIOLATES", "FIXED_BY", "SHIPPED_IN"],
            "impact_predicates": ["CALLS", "SENDS", "HANDLES", "READS", "WRITES",
                "DEPENDS_ON", "IMPLEMENTS", "GUARDED_BY", "VERIFIED_BY"],
            "blocked_paths": [".git/**", "**/.git/**", ".secrets/**", "**/.secrets/**",
                "*secret*", "**/*secret*", "*credential*", "**/*credential*",
                "*.tfstate", "*.tfstate.*", "**/*.tfstate", "**/*.tfstate.*",
                "*.tfplan", "**/*.tfplan", ".env", ".env.*", "**/.env", "**/.env.*",
                "ovh.txt", "**/ovh.txt"]
        });
        fs::write(
            &config_path,
            format!("{}\n", serde_json::to_string_pretty(&config)?),
        )?;
        created.push(config_path);
    }
    if !assertions_path.exists() {
        fs::write(
            &assertions_path,
            "# One JSON object per line. Records may be node, edge, or alias.\n",
        )?;
        created.push(assertions_path);
    }
    if !gold_path.exists() {
        fs::write(
            &gold_path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&json!({
                    "version": 1,
                    "questions": [{
                        "question": "Which component implements this behavior?",
                        "seeds": [],
                        "depth": 2,
                        "direction": "both",
                        "expected_nodes": []
                    }]
                }))?
            ),
        )?;
        created.push(gold_path);
    }
    if !ignore_path.exists() {
        fs::write(
            &ignore_path,
            "cache/\n# Raw worker findings are disposable reconciliation input, not graph truth.\ncandidates/\n",
        )?;
        created.push(ignore_path);
    }
    if !lock_path.exists() {
        fs::write(
            &lock_path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&json!({
                    "version": LOCK_VERSION,
                    "updated_at": null,
                    "evidence": {}
                }))?
            ),
        )?;
        created.push(lock_path);
    }
    Ok(InitializedProject { root, created })
}
