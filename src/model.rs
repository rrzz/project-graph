use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CONFIG_DIR: &str = ".project-graph";
pub const DEFAULT_DB: &str = ".project-graph/cache/graph.sqlite";
pub const LOCK_FILE: &str = ".project-graph/evidence-lock.json";
pub const GOLD_FILE: &str = ".project-graph/gold-questions.json";

fn default_assertion_globs() -> Vec<String> {
    vec![".project-graph/assertions/*.jsonl".into()]
}

fn default_database() -> String {
    DEFAULT_DB.into()
}

fn default_true() -> bool {
    true
}

fn default_review() -> String {
    "reviewed".into()
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub schema_version: Value,
    pub project: String,
    #[serde(default = "default_database")]
    pub database: String,
    #[serde(default = "default_assertion_globs")]
    pub assertion_globs: Vec<String>,
    #[serde(default = "default_true")]
    pub require_edge_evidence: bool,
    pub node_types: Vec<String>,
    pub predicates: Vec<String>,
    #[serde(default)]
    pub impact_predicates: Vec<String>,
    #[serde(default)]
    pub blocked_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub path: String,
    #[serde(default)]
    pub start_anchor: Option<String>,
    #[serde(default)]
    pub anchor: Option<String>,
    #[serde(default)]
    pub end_anchor: Option<String>,
    #[serde(default)]
    pub start_occurrence: Option<usize>,
    #[serde(default)]
    pub end_occurrence: Option<usize>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub review: Option<String>,
    #[serde(default)]
    pub extractor_version: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Record {
    Node(Node),
    Edge(Edge),
    Alias(Alias),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_review")]
    pub review: String,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default = "empty_object")]
    pub attributes: Value,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub predicate: String,
    pub target: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_review")]
    pub review: String,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub valid_from: Option<String>,
    #[serde(default)]
    pub valid_to: Option<String>,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default = "empty_object")]
    pub attributes: Value,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alias {
    pub alias: String,
    pub node: String,
    #[serde(default = "default_review")]
    pub review: String,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

impl Record {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Node(_) => "node",
            Self::Edge(_) => "edge",
            Self::Alias(_) => "alias",
        }
    }

    pub fn subject_id(&self) -> &str {
        match self {
            Self::Node(value) => &value.id,
            Self::Edge(value) => &value.id,
            Self::Alias(value) => &value.alias,
        }
    }

    pub fn review(&self) -> &str {
        match self {
            Self::Node(value) => &value.review,
            Self::Edge(value) => &value.review,
            Self::Alias(value) => &value.review,
        }
    }

    pub fn evidence(&self) -> &[Evidence] {
        match self {
            Self::Node(value) => &value.evidence,
            Self::Edge(value) => &value.evidence,
            Self::Alias(value) => &value.evidence,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadedRecord {
    pub record: Record,
    pub origin: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceSelector {
    pub start_anchor: String,
    pub end_anchor: String,
    pub start_occurrence: Option<usize>,
    pub end_occurrence: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct EvidenceSnapshot {
    pub key: String,
    pub path: String,
    pub selector: EvidenceSelector,
    pub line_start: usize,
    pub line_end: usize,
    pub file_sha256: String,
    pub span_sha256: String,
    pub quote: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvidenceLock {
    #[serde(default = "lock_version")]
    pub version: u32,
    #[serde(default)]
    pub updated_at: Option<String>,
    /// Optional repository context captured when the lock was accepted. This is
    /// provenance metadata, not a replacement for per-span evidence hashes.
    #[serde(default)]
    pub git: Option<GitContext>,
    #[serde(default)]
    pub evidence: BTreeMap<String, LockedEvidence>,
}

/// Current evidence-lock format version. `init`, `lock`, the serde default,
/// and the newer-than-supported check in `load_lock` all reference this one
/// constant so the number cannot drift between writers.
pub const LOCK_VERSION: u32 = 3;

const fn lock_version() -> u32 {
    LOCK_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitContext {
    pub head: String,
    pub tree: String,
    pub branch: Option<String>,
    pub worktree_clean: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedEvidence {
    pub path: String,
    pub selector: EvidenceSelector,
    pub span_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldFile {
    pub version: u32,
    pub questions: Vec<GoldQuestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldQuestion {
    pub question: String,
    #[serde(default)]
    pub seeds: Vec<String>,
    #[serde(default = "default_depth")]
    pub depth: usize,
    #[serde(default = "default_direction")]
    pub direction: String,
    #[serde(default)]
    pub predicates: Option<Vec<String>>,
    #[serde(default)]
    pub expected_nodes: Vec<String>,
}

const fn default_depth() -> usize {
    2
}

fn default_direction() -> String {
    "both".into()
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BuildCounts {
    pub nodes: usize,
    pub edges: usize,
    pub aliases: usize,
    pub evidence: usize,
    pub reused: bool,
}

#[derive(Debug, Clone)]
pub struct InitializedProject {
    pub root: PathBuf,
    pub created: Vec<PathBuf>,
}
