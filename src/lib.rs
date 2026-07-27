//! Provenance-first engineering knowledge graphs.
//!
//! Assertions remain portable, reviewable JSONL. `SQLite` is a generated query
//! index and can always be rebuilt.

pub mod core;
pub mod error;
pub mod model;

pub use error::{GraphError, Result};
