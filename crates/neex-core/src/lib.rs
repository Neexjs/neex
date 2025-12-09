//! Neex Core - High-Performance Monorepo Engine
//!
//! Features:
//! - BLAKE3 hashing (10x faster than SHA)
//! - AST-based hashing (ignores comments/whitespace) - KILLER FEATURE
//! - Task execution with output caching - TURBO FEATURE
//! - Parallel file processing with Rayon
//! - Respects .gitignore patterns
//! - Content-addressable storage

pub mod hasher;
pub mod ast_hasher;
pub mod graph;
pub mod cache;
pub mod runner;

pub use hasher::Hasher;
pub use ast_hasher::{hash_ast, is_parseable};
pub use graph::DependencyGraph;
pub use runner::{TaskRunner, TaskOutput};
