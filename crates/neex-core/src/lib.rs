//! Neex Core - High-Performance Monorepo Engine
//!
//! Features:
//! - BLAKE3 hashing (10x faster than SHA)
//! - AST-based hashing (ignores comments/whitespace) - KILLER FEATURE
//! - Workspace dependency graph with topological sort - MONOREPO FEATURE
//! - Parallel task scheduler with dependency awareness - PARALLEL FEATURE
//! - Task execution with output caching - TURBO FEATURE
//! - Cloud cache (S3/R2) for remote teams - CLOUD FEATURE
//! - Parallel file processing with Rayon
//! - Respects .gitignore patterns
//! - Content-addressable storage

pub mod hasher;
pub mod ast_hasher;
pub mod graph;
pub mod cache;
pub mod runner;
pub mod scheduler;
pub mod cloud;

pub use hasher::Hasher;
pub use ast_hasher::{hash_ast, is_parseable};
pub use graph::{DepGraph, WorkspaceNode, DependencyGraph};
pub use runner::{TaskRunner, TaskOutput};
pub use scheduler::{Scheduler, SchedulerTask, TaskResult, TaskStatus};
pub use cloud::{CloudCache, CloudConfig, S3Config, load_config, save_config, get_config_path};
