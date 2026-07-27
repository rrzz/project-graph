use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use project_knowledge_graph::core::{
    audit_evidence, build_database, connect_database, evaluate_gold, git_lock_status, init_project,
    load_config, load_lock, load_records, project_status, search, shortest_path, traverse,
    validate_records, verify_lock, write_lock,
};
use serde_json::{json, Value};
use tempfile::TempDir;

struct Fixture {
    temp: TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("temporary directory");
        let root = temp.path().join("fixture");
        let root = init_project(&root, Some("fixture"))
            .expect("initialize fixture")
            .root;
        fs::write(
            root.join("client.js"),
            "function send() {\n  socket.send('serve');\n}\n",
        )
        .expect("write source");
        let records = [
            json!({"kind":"node","id":"symbol:send","type":"Symbol","name":"send"}),
            json!({"kind":"node","id":"interface:serve","type":"Interface","name":"serve"}),
            json!({
                "kind":"edge","id":"edge:send-serve","source":"symbol:send",
                "predicate":"SENDS","target":"interface:serve",
                "evidence":[{
                    "path":"client.js","start_anchor":"function send() {","end_anchor":"}",
                    "method":"deterministic","review":"reviewed"
                }]
            }),
            json!({"kind":"alias","alias":"OP_SERVE","node":"interface:serve"}),
        ];
        write_jsonl(&root.join(".project-graph/assertions/core.jsonl"), &records);
        Self { temp, root }
    }

    fn load(
        &self,
    ) -> (
        project_knowledge_graph::model::Config,
        Vec<project_knowledge_graph::model::LoadedRecord>,
    ) {
        let config = load_config(&self.root).expect("load config");
        let records = load_records(&self.root, &config).expect("load records");
        (config, records)
    }
}

fn write_jsonl(path: &Path, values: &[Value]) {
    let content = values
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(path, format!("{content}\n")).expect("write assertions");
}

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .status()
        .expect("run git");
    assert!(status.success(), "git {:?} failed", args);
}

#[test]
fn builds_queries_aliases_and_finds_paths() {
    let fixture = Fixture::new();
    let (config, records) = fixture.load();
    assert!(validate_records(&fixture.root, &config, &records, true).is_empty());
    write_lock(&fixture.root, &config, &records, false).expect("lock");
    build_database(&fixture.root, &config, &records).expect("build");
    let connection = connect_database(&fixture.root, &config).expect("connect");

    let path = shortest_path(&connection, "symbol:send", "OP_SERVE", 8, "both").expect("path");
    let ids: Vec<_> = path["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|node| node["id"].as_str().expect("id"))
        .collect();
    assert_eq!(ids, ["symbol:send", "interface:serve"]);

    let neighborhood = traverse(&connection, "OP_SERVE", 1, "both", None).expect("neighborhood");
    assert_eq!(neighborhood["edges"].as_array().expect("edges").len(), 1);
    let lexical = search(&connection, "serve", 25).expect("FTS search");
    assert_eq!(lexical["nodes"].as_array().expect("nodes").len(), 1);
    assert_eq!(lexical["edges"].as_array().expect("edges").len(), 1);
    let audit = audit_evidence(&fixture.root, &config, &records).expect("audit");
    assert!(audit.iter().all(|row| row["lock_status"] == "fresh"));
}

#[test]
fn build_reuses_the_index_only_when_assertions_evidence_and_lock_are_unchanged() {
    let fixture = Fixture::new();
    let (config, records) = fixture.load();
    write_lock(&fixture.root, &config, &records, false).expect("lock");
    let (_, initial) = build_database(&fixture.root, &config, &records).expect("initial build");
    assert!(!initial.reused);

    let (_, unchanged) = build_database(&fixture.root, &config, &records).expect("reused build");
    assert!(unchanged.reused);

    fs::write(
        fixture.root.join("client.js"),
        "function send() {\n  socket.send('serve again');\n}\n",
    )
    .expect("change evidence source");
    let (_, rebuilt) =
        build_database(&fixture.root, &config, &records).expect("rebuild stale source");
    assert!(!rebuilt.reused);
}

#[test]
fn status_reports_a_usable_unlocked_graph_and_a_ready_index() {
    let fixture = Fixture::new();
    let (config, records) = fixture.load();
    let before = project_status(&fixture.root, &config, &records).expect("status before build");
    assert_eq!(before["index"]["state"], "missing");
    assert_eq!(before["evidence"]["counts"]["unlocked"], 1);

    build_database(&fixture.root, &config, &records).expect("build unlocked graph");
    let after = project_status(&fixture.root, &config, &records).expect("status after build");
    assert_eq!(after["index"]["state"], "ready");
    assert_eq!(after["index"]["nodes"], 2);
    assert_eq!(after["evidence"]["counts"]["unlocked"], 1);
}

#[test]
fn canonical_lock_and_build_reject_unreviewed_records_without_replacing_the_index() {
    let fixture = Fixture::new();
    let (config, reviewed_records) = fixture.load();
    write_lock(&fixture.root, &config, &reviewed_records, false).expect("lock reviewed graph");
    let (database, _) =
        build_database(&fixture.root, &config, &reviewed_records).expect("build reviewed graph");
    let before = fs::read(&database).expect("read reviewed index");

    write_jsonl(
        &fixture
            .root
            .join(".project-graph/assertions/unreviewed.jsonl"),
        &[json!({
            "kind":"node",
            "id":"component:coverage-draft",
            "type":"Component",
            "name":"coverage draft",
            "review":"candidate"
        })],
    );
    let records = load_records(&fixture.root, &config).expect("reload with candidate");
    assert!(validate_records(&fixture.root, &config, &records, true).is_empty());

    let lock_error = write_lock(&fixture.root, &config, &records, false)
        .expect_err("candidate records must not enter the evidence lock");
    assert!(lock_error.to_string().contains("only reviewed records"));
    let build_error = build_database(&fixture.root, &config, &records)
        .expect_err("candidate records must not enter the SQLite index");
    assert!(build_error.to_string().contains("only reviewed records"));
    assert_eq!(fs::read(database).expect("read preserved index"), before);
}

#[test]
fn reviewed_records_cannot_cite_unreviewed_evidence() {
    let fixture = Fixture::new();
    write_jsonl(
        &fixture
            .root
            .join(".project-graph/assertions/bad-evidence.jsonl"),
        &[json!({
            "kind":"node",
            "id":"component:bad-evidence",
            "type":"Component",
            "name":"bad evidence",
            "review":"reviewed",
            "evidence":[{
                "path":"client.js",
                "start_anchor":"function send() {",
                "end_anchor":"}",
                "method":"deterministic",
                "review":"candidate"
            }]
        })],
    );
    let (config, records) = fixture.load();
    assert!(validate_records(&fixture.root, &config, &records, true)
        .iter()
        .any(|error| error.contains("reviewed record must be reviewed")));
}

#[test]
fn lock_captures_git_context_without_making_commit_drift_evidence_staleness() {
    let fixture = Fixture::new();
    git(&fixture.root, &["init", "--quiet"]);
    git(
        &fixture.root,
        &["config", "user.email", "tests@example.invalid"],
    );
    git(
        &fixture.root,
        &["config", "user.name", "Project Graph tests"],
    );
    git(&fixture.root, &["add", "."]);
    git(&fixture.root, &["commit", "--quiet", "-m", "initial"]);

    let (config, records) = fixture.load();
    write_lock(&fixture.root, &config, &records, false).expect("lock with Git metadata");
    let lock = load_lock(&fixture.root).expect("load lock");
    let captured = lock.git.as_ref().expect("captured Git context");
    assert!(captured.worktree_clean);
    assert_eq!(
        git_lock_status(&fixture.root, &lock)["status"],
        "same_commit"
    );

    git(&fixture.root, &["add", ".project-graph/evidence-lock.json"]);
    git(&fixture.root, &["commit", "--quiet", "-m", "accept lock"]);
    assert_eq!(
        git_lock_status(&fixture.root, &lock)["status"],
        "different_commit"
    );
    let report = verify_lock(&fixture.root, &config, &records, true, false, true)
        .expect("verify evidence independently of commit context");
    assert_eq!(report["valid"], true);
    let strict =
        verify_lock(&fixture.root, &config, &records, true, true, true).expect("strict verify");
    assert_eq!(strict["valid"], false);
}

#[test]
fn moving_an_intact_span_is_fresh_but_changing_it_is_stale() {
    let fixture = Fixture::new();
    let (config, records) = fixture.load();
    write_lock(&fixture.root, &config, &records, false).expect("lock");
    fs::write(
        fixture.root.join("client.js"),
        "\n\n// preface\nfunction send() {\n  socket.send('serve');\n}\n",
    )
    .expect("move span");
    let moved = audit_evidence(&fixture.root, &config, &records).expect("audit moved");
    assert_eq!(moved[0]["lock_status"], "fresh");
    assert_eq!(moved[0]["resolved_line_start"], 4);

    fs::write(
        fixture.root.join("client.js"),
        "function send() {\n  socket.send('changed');\n}\n",
    )
    .expect("change span");
    let changed = audit_evidence(&fixture.root, &config, &records).expect("audit changed");
    assert_eq!(changed[0]["lock_status"], "stale");
}

#[test]
fn missing_and_ambiguous_anchors_are_reported() {
    let fixture = Fixture::new();
    let (config, records) = fixture.load();
    write_lock(&fixture.root, &config, &records, false).expect("lock");
    fs::write(fixture.root.join("client.js"), "function renamed() {}\n").expect("rename");
    let audit = audit_evidence(&fixture.root, &config, &records).expect("audit");
    assert_eq!(audit[0]["lock_status"], "unresolved");

    fs::write(
        fixture.root.join("client.js"),
        "socket one\nsocket two\n}\n",
    )
    .expect("ambiguous");
    let mut raw: Vec<Value> = records
        .iter()
        .map(|loaded| serde_json::to_value(&loaded.record).expect("serialize"))
        .collect();
    raw[2]["evidence"][0]["start_anchor"] = json!("socket");
    write_jsonl(
        &fixture.root.join(".project-graph/assertions/core.jsonl"),
        &raw,
    );
    let records = load_records(&fixture.root, &config).expect("reload");
    let errors = validate_records(&fixture.root, &config, &records, true);
    assert!(errors.iter().any(|error| error.contains("ambiguous")));
}

#[test]
fn blocked_source_is_rejected() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("ovh.txt"), "do not read\n").expect("blocked fixture");
    let (config, records) = fixture.load();
    let mut raw: Vec<Value> = records
        .iter()
        .map(|loaded| serde_json::to_value(&loaded.record).expect("serialize"))
        .collect();
    raw[2]["evidence"][0]["path"] = json!("ovh.txt");
    write_jsonl(
        &fixture.root.join(".project-graph/assertions/core.jsonl"),
        &raw,
    );
    let records = load_records(&fixture.root, &config).expect("reload");
    assert!(validate_records(&fixture.root, &config, &records, true)
        .iter()
        .any(|error| error.contains("blocked")));
}

#[test]
fn gold_questions_check_bounded_retrieval() {
    let fixture = Fixture::new();
    let (config, records) = fixture.load();
    write_lock(&fixture.root, &config, &records, false).expect("lock");
    build_database(&fixture.root, &config, &records).expect("build");
    fs::write(
        fixture.root.join(".project-graph/gold-questions.json"),
        serde_json::to_vec(&json!({
            "version":1,
            "questions":[{
                "question":"What does send reach?",
                "seeds":["symbol:send"],
                "depth":1,
                "direction":"out",
                "expected_nodes":["symbol:send","OP_SERVE"]
            }]
        }))
        .expect("gold json"),
    )
    .expect("write gold");
    let connection = connect_database(&fixture.root, &config).expect("connect");
    let report = evaluate_gold(&fixture.root, &connection).expect("evaluate");
    assert_eq!(report["passed"], 1);
    assert_eq!(report["failed"], 0);
    assert_eq!(report["results"][0]["coverage"], 1.0);
}

#[test]
fn fixture_owns_its_temporary_directory_for_the_whole_test() {
    let fixture = Fixture::new();
    assert!(fixture.temp.path().exists());
}

#[test]
fn corrupt_cache_is_rebuilt_not_fatal() {
    let fixture = Fixture::new();
    let (config, records) = fixture.load();
    let (database, first) = build_database(&fixture.root, &config, &records).expect("first build");
    assert!(!first.reused);
    fs::write(&database, b"this is not a sqlite database").expect("corrupt cache");
    let (_, rebuilt) =
        build_database(&fixture.root, &config, &records).expect("corrupt cache must rebuild");
    assert!(!rebuilt.reused);
    let connection = connect_database(&fixture.root, &config).expect("reconnect");
    let found = search(&connection, "send", 25).expect("query after recovery");
    assert_eq!(found["nodes"].as_array().expect("nodes").len(), 1);
}

#[test]
fn fts_operator_words_are_plain_search_terms() {
    let fixture = Fixture::new();
    let (config, records) = fixture.load();
    build_database(&fixture.root, &config, &records).expect("build");
    let connection = connect_database(&fixture.root, &config).expect("connect");
    for term in ["OR", "AND", "NOT", "send NOT serve", "NEAR(send)"] {
        search(&connection, term, 25)
            .unwrap_or_else(|error| panic!("query {term:?} must not be FTS syntax: {error}"));
    }
    let hit = search(&connection, "send", 25).expect("plain term");
    assert_eq!(hit["nodes"].as_array().expect("nodes").len(), 1);
}

#[test]
fn like_wildcards_are_literal_in_fuzzy_resolution() {
    let fixture = Fixture::new();
    let (config, records) = fixture.load();
    build_database(&fixture.root, &config, &records).expect("build");
    let connection = connect_database(&fixture.root, &config).expect("connect");
    let error = traverse(&connection, "____", 1, "both", None)
        .expect_err("underscores must not be wildcards");
    assert!(error.to_string().contains("No node matches"));
    let error = traverse(&connection, "%", 1, "both", None).expect_err("percent is literal");
    assert!(error.to_string().contains("No node matches"));
}

#[test]
fn alias_and_edge_ids_cannot_shadow_node_ids() {
    let fixture = Fixture::new();
    write_jsonl(
        &fixture.root.join(".project-graph/assertions/shadow.jsonl"),
        &[
            json!({"kind":"alias","alias":"symbol:send","node":"interface:serve"}),
            json!({
                "kind":"edge","id":"interface:serve","source":"symbol:send",
                "predicate":"SENDS","target":"interface:serve",
                "evidence":[{"path":"client.js","start_anchor":"function send() {"}]
            }),
        ],
    );
    let (config, records) = fixture.load();
    let errors = validate_records(&fixture.root, &config, &records, false);
    assert!(errors
        .iter()
        .any(|error| error.contains("alias \"symbol:send\" collides")));
    assert!(errors
        .iter()
        .any(|error| error.contains("edge id \"interface:serve\" collides")));
}

#[test]
fn newer_lock_versions_are_rejected() {
    let fixture = Fixture::new();
    fs::write(
        fixture.root.join(".project-graph/evidence-lock.json"),
        "{\"version\": 99, \"evidence\": {}}\n",
    )
    .expect("future lock");
    let error = load_lock(&fixture.root).expect_err("future lock version must be refused");
    assert!(error.to_string().contains("newer than this binary"));
}

#[test]
fn blocked_paths_match_case_insensitively() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("OVH.TXT"), "do not read\n").expect("blocked fixture");
    let (config, records) = fixture.load();
    let mut raw: Vec<Value> = records
        .iter()
        .map(|loaded| serde_json::to_value(&loaded.record).expect("serialize"))
        .collect();
    raw[2]["evidence"][0]["path"] = json!("OVH.TXT");
    write_jsonl(
        &fixture.root.join(".project-graph/assertions/core.jsonl"),
        &raw,
    );
    let records = load_records(&fixture.root, &config).expect("reload");
    assert!(validate_records(&fixture.root, &config, &records, true)
        .iter()
        .any(|error| error.contains("blocked")));
}

#[cfg(unix)]
#[test]
fn symlink_cannot_alias_a_blocked_source() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("ovh.txt"), "do not read\n").expect("blocked fixture");
    std::os::unix::fs::symlink(
        fixture.root.join("ovh.txt"),
        fixture.root.join("innocent.txt"),
    )
    .expect("symlink");
    let (config, records) = fixture.load();
    let mut raw: Vec<Value> = records
        .iter()
        .map(|loaded| serde_json::to_value(&loaded.record).expect("serialize"))
        .collect();
    raw[2]["evidence"][0]["path"] = json!("innocent.txt");
    write_jsonl(
        &fixture.root.join(".project-graph/assertions/core.jsonl"),
        &raw,
    );
    let records = load_records(&fixture.root, &config).expect("reload");
    assert!(validate_records(&fixture.root, &config, &records, true)
        .iter()
        .any(|error| error.contains("blocked")));
}

#[test]
fn evidence_selector_typos_are_rejected() {
    let fixture = Fixture::new();
    let (config, records) = fixture.load();
    let mut raw: Vec<Value> = records
        .iter()
        .map(|loaded| serde_json::to_value(&loaded.record).expect("serialize"))
        .collect();
    raw[2]["evidence"][0]["start_occurence"] = json!(2);
    write_jsonl(
        &fixture.root.join(".project-graph/assertions/core.jsonl"),
        &raw,
    );
    let records = load_records(&fixture.root, &config).expect("reload");
    assert!(validate_records(&fixture.root, &config, &records, false)
        .iter()
        .any(|error| error.contains("resembles a selector field")));
}

#[test]
fn undirected_neighborhoods_return_the_induced_edge_set() {
    let fixture = Fixture::new();
    write_jsonl(
        &fixture.root.join(".project-graph/assertions/core.jsonl"),
        &[
            json!({"kind":"node","id":"node:a","type":"Component","name":"a"}),
            json!({"kind":"node","id":"node:b","type":"Component","name":"b"}),
            json!({"kind":"node","id":"node:c","type":"Component","name":"c"}),
            json!({"kind":"edge","id":"edge:ab","source":"node:a","predicate":"DEPENDS_ON","target":"node:b",
                "evidence":[{"path":"client.js","start_anchor":"function send() {"}]}),
            json!({"kind":"edge","id":"edge:ac","source":"node:a","predicate":"DEPENDS_ON","target":"node:c",
                "evidence":[{"path":"client.js","start_anchor":"function send() {"}]}),
            json!({"kind":"edge","id":"edge:bc","source":"node:b","predicate":"DEPENDS_ON","target":"node:c",
                "evidence":[{"path":"client.js","start_anchor":"function send() {"}]}),
        ],
    );
    let (config, records) = fixture.load();
    build_database(&fixture.root, &config, &records).expect("build");
    let connection = connect_database(&fixture.root, &config).expect("connect");
    // b and c are both discovered at depth 1 from a; edge b->c joins two
    // final-level nodes and must still be present in the undirected result.
    let neighborhood = traverse(&connection, "node:a", 1, "both", None).expect("neighborhood");
    assert_eq!(neighborhood["nodes"].as_array().expect("nodes").len(), 3);
    assert_eq!(neighborhood["edges"].as_array().expect("edges").len(), 3);
    // A directed traversal keeps discovery edges only.
    let directed = traverse(&connection, "node:a", 1, "out", None).expect("directed");
    assert_eq!(directed["edges"].as_array().expect("edges").len(), 2);
}
