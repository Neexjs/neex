//! Neex Core - High-Performance Monorepo Engine
//!
//! Features:
//! - BLAKE3 hashing (10x faster than SHA)
//! - Parallel file processing with Rayon
//! - Respects .gitignore patterns
//! - Content-addressable storage

pub mod hasher;
pub mod graph;
pub mod cache;

pub use hasher::Hasher;
pub use graph::DependencyGraph;
