use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use project_knowledge_graph::core::{
    audit_evidence, build_database, connect_database, diagnostics, evaluate_gold, evidence_for,
    git_lock_status, init_project, load_config, load_lock, load_records, mermaid, project_root,
    project_status, search, shortest_path, traverse, validate_records, verify_lock, write_lock,
};
use project_knowledge_graph::{GraphError, Result};
use serde_json::{json, Value};

#[derive(Debug, Parser)]
#[command(
    name = "project-graph",
    version,
    about = "Build and query a provenance-first engineering knowledge graph."
)]
struct Cli {
    /// Project root containing .project-graph/config.json.
    #[arg(long, global = true, default_value = ".")]
    project: PathBuf,
    /// Emit compact machine-readable JSON.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize graph files in a project.
    Init {
        /// Human-readable project name.
        #[arg(long)]
        name: Option<String>,
    },
    /// Validate assertions and evidence paths.
    Validate,
    /// Accept current evidence spans and capture Git context when available.
    Lock {
        /// Refuse to lock unless this Git worktree is clean.
        #[arg(long)]
        require_clean_worktree: bool,
    },
    /// Reuse an unchanged SQLite index or atomically rebuild it.
    Build,
    /// Full-text search reviewed nodes, aliases, and edges.
    Query {
        term: String,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Traverse a node neighborhood.
    Neighbors {
        node: String,
        #[arg(long, default_value_t = 1)]
        depth: usize,
        #[arg(long, value_enum, default_value_t = Direction::Both)]
        direction: Direction,
        #[arg(long = "predicate")]
        predicates: Vec<String>,
    },
    /// Find a shortest path between two nodes.
    Path {
        source: String,
        target: String,
        #[arg(long, default_value_t = 8)]
        max_depth: usize,
        #[arg(long, value_enum, default_value_t = Direction::Both)]
        direction: Direction,
    },
    /// Conservatively traverse configured impact predicates.
    Impact {
        node: String,
        #[arg(long, default_value_t = 2)]
        depth: usize,
    },
    /// Show evidence for a node or edge.
    Evidence { subject: String },
    /// List non-fresh evidence and its Git lock context.
    Stale,
    /// Report index availability, evidence freshness, and Git lock context.
    Status,
    /// Report or enforce evidence and Git lock integrity requirements.
    Verify {
        /// Require every cited evidence span to match the lock.
        #[arg(long)]
        require_fresh: bool,
        /// Require the current Git HEAD to match the Git context captured by lock.
        #[arg(long)]
        require_same_commit: bool,
        /// Require the current Git worktree to be clean.
        #[arg(long)]
        require_clean_worktree: bool,
    },
    /// Report detailed graph health signals from the current SQLite index.
    Diagnostics,
    /// Run deterministic gold-question graph coverage checks.
    Eval,
    /// Export Mermaid graph source.
    Export {
        #[arg(long, value_enum, default_value_t = ExportFormat::Mermaid)]
        format: ExportFormat,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        center: Option<String>,
        #[arg(long, default_value_t = 2)]
        depth: usize,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Direction {
    In,
    Out,
    Both,
}

impl Direction {
    const fn as_str(self) -> &'static str {
        match self {
            Self::In => "in",
            Self::Out => "out",
            Self::Both => "both",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ExportFormat {
    Mermaid,
}

fn emit(value: &Value, compact: bool) -> Result<()> {
    let output = if compact {
        serde_json::to_string(value)?
    } else {
        serde_json::to_string_pretty(value)?
    };
    println!("{output}");
    Ok(())
}

fn execute(cli: &Cli) -> Result<i32> {
    if let Command::Init { name } = &cli.command {
        let initialized = init_project(&cli.project, name.as_deref())?;
        let created: Vec<_> = initialized
            .created
            .iter()
            .filter_map(|path| path.strip_prefix(&initialized.root).ok())
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect();
        emit(
            &json!({"project": initialized.root, "created": created}),
            cli.json,
        )?;
        return Ok(0);
    }

    let root = project_root(&cli.project)?;
    let config = load_config(&root)?;
    let records = load_records(&root, &config)?;

    match &cli.command {
        Command::Validate => {
            let errors = validate_records(&root, &config, &records, true);
            let valid = errors.is_empty();
            emit(
                &json!({"valid": valid, "records": records.len(), "errors": errors}),
                cli.json,
            )?;
            Ok(i32::from(!valid))
        }
        Command::Lock {
            require_clean_worktree,
        } => {
            let errors = validate_records(&root, &config, &records, true);
            if !errors.is_empty() {
                return Err(GraphError::msg(errors.join("\n")));
            }
            let locked_evidence = write_lock(&root, &config, &records, *require_clean_worktree)?;
            emit(
                &json!({
                    "locked_evidence": locked_evidence,
                    "git": git_lock_status(&root, &load_lock(&root)?),
                }),
                cli.json,
            )?;
            Ok(0)
        }
        Command::Build => {
            let (database, counts) = build_database(&root, &config, &records)?;
            emit(
                &json!({
                    "database": database,
                    "nodes": counts.nodes,
                    "edges": counts.edges,
                    "aliases": counts.aliases,
                    "evidence": counts.evidence,
                    "reused": counts.reused,
                }),
                cli.json,
            )?;
            Ok(0)
        }
        Command::Stale => {
            let rows: Vec<_> = audit_evidence(&root, &config, &records)?
                .into_iter()
                .filter(|row| row["lock_status"] != "fresh")
                .collect();
            let exit = i32::from(!rows.is_empty());
            let git = git_lock_status(&root, &load_lock(&root)?);
            emit(
                &json!({"count": rows.len(), "evidence": rows, "git": git}),
                cli.json,
            )?;
            Ok(exit)
        }
        Command::Status => {
            emit(&project_status(&root, &config, &records)?, cli.json)?;
            Ok(0)
        }
        Command::Verify {
            require_fresh,
            require_same_commit,
            require_clean_worktree,
        } => {
            let report = verify_lock(
                &root,
                &config,
                &records,
                *require_fresh,
                *require_same_commit,
                *require_clean_worktree,
            )?;
            let exit = i32::from(!report["valid"].as_bool().unwrap_or(false));
            emit(&report, cli.json)?;
            Ok(exit)
        }
        // Index-backed commands connect per arm; the match stays exhaustive so
        // adding a Command variant is a compile error until it is handled here.
        Command::Query { term, limit } => {
            let connection = connect_database(&root, &config)?;
            emit(&search(&connection, term, *limit)?, cli.json)?;
            Ok(0)
        }
        Command::Neighbors {
            node,
            depth,
            direction,
            predicates,
        } => {
            let connection = connect_database(&root, &config)?;
            let predicates = (!predicates.is_empty())
                .then(|| predicates.iter().cloned().collect::<HashSet<_>>());
            emit(
                &traverse(
                    &connection,
                    node,
                    *depth,
                    direction.as_str(),
                    predicates.as_ref(),
                )?,
                cli.json,
            )?;
            Ok(0)
        }
        Command::Path {
            source,
            target,
            max_depth,
            direction,
        } => {
            let connection = connect_database(&root, &config)?;
            emit(
                &shortest_path(&connection, source, target, *max_depth, direction.as_str())?,
                cli.json,
            )?;
            Ok(0)
        }
        Command::Impact { node, depth } => {
            let connection = connect_database(&root, &config)?;
            let predicates = config
                .impact_predicates
                .iter()
                .cloned()
                .collect::<HashSet<_>>();
            emit(
                &traverse(&connection, node, *depth, "both", Some(&predicates))?,
                cli.json,
            )?;
            Ok(0)
        }
        Command::Evidence { subject } => {
            let connection = connect_database(&root, &config)?;
            emit(&evidence_for(&connection, subject)?, cli.json)?;
            Ok(0)
        }
        Command::Diagnostics => {
            let connection = connect_database(&root, &config)?;
            let mut report = diagnostics(&connection)?;
            let nonfresh = audit_evidence(&root, &config, &records)?
                .iter()
                .filter(|row| row["lock_status"] != "fresh")
                .count();
            report["nonfresh_evidence"] = json!(nonfresh);
            report["git"] = git_lock_status(&root, &load_lock(&root)?);
            emit(&report, cli.json)?;
            Ok(0)
        }
        Command::Eval => {
            let connection = connect_database(&root, &config)?;
            let report = evaluate_gold(&root, &connection)?;
            let exit = i32::from(report["failed"].as_u64().unwrap_or_default() > 0);
            emit(&report, cli.json)?;
            Ok(exit)
        }
        Command::Export {
            format: _,
            output,
            center,
            depth,
        } => {
            let connection = connect_database(&root, &config)?;
            let selected = if let Some(center) = center {
                let neighborhood = traverse(&connection, center, *depth, "both", None)?;
                Some(
                    neighborhood["nodes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|node| node["id"].as_str().map(str::to_owned))
                        .collect::<HashSet<_>>(),
                )
            } else {
                None
            };
            let text = mermaid(&connection, selected.as_ref())?;
            if let Some(output) = output {
                if let Some(parent) = output.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(output, text)?;
                emit(&json!({"output": output, "format": "mermaid"}), cli.json)?;
            } else {
                print!("{text}");
            }
            Ok(0)
        }
        // Handled before configuration loads, at the top of this function.
        Command::Init { .. } => Ok(0),
    }
}

fn main() {
    let cli = Cli::parse();
    match execute(&cli) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("project-graph: {error}");
            std::process::exit(2);
        }
    }
}
