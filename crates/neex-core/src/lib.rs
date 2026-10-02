//! Neex Core - polyglot monorepo task engine
//!
//! - Workspace discovery for JS, Rust, Go, Python and generic projects
//! - `neex.json` config (reads `turbo.json` too)
//! - Task graph with `^task`, `task` and `project#task` dependencies
//! - Per-task cache keys that can explain every miss
//! - Content-addressed output cache with safe restore
//! - S3/R2 remote cache with trust-scoped writes

pub mod artifacts;
pub mod ast_hasher;
pub mod cloud;
pub mod config;
pub mod executor;
pub mod hasher;
pub mod inputs;
pub mod project;
pub mod providers;
pub mod remote;
pub mod runner;
pub mod scm;
pub mod symbol_graph;
pub mod symbols;
pub mod task_graph;
pub mod task_hash;
pub mod workspace;

pub use ast_hasher::{hash_ast, is_parseable};
pub use cloud::{get_config_path, load_config, save_config, CloudConfig, S3Config};
pub use config::{RootConfig, TaskConfig};
pub use executor::{run, ContinueMode, Event, RunOptions, RunSummary, TaskStatus, TaskSummary};
pub use hasher::Hasher;
pub use project::{Language, Project, TaskCommand};
pub use remote::RemoteCache;
pub use symbol_graph::{SymbolCache, SymbolGraph};
pub use symbols::{extract_from_file, extract_symbols, FileSymbols, Import, Symbol, SymbolKind};
pub use task_graph::{TaskGraph, TaskNode};
pub use workspace::Workspace;
