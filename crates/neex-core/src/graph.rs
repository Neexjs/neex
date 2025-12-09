//! Dependency Graph - petgraph-based package dependency tracking

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::algo::toposort;
use petgraph::Direction;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Package node in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageNode {
    pub name: String,
    pub path: String,
    pub version: String,
}

/// Dependency graph for monorepo packages
pub struct DependencyGraph {
    graph: DiGraph<PackageNode, ()>,
    name_to_index: HashMap<String, NodeIndex>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            name_to_index: HashMap::new(),
        }
    }

    /// Add a package to the graph
    pub fn add_package(&mut self, package: PackageNode) -> NodeIndex {
        let name = package.name.clone();
        let idx = self.graph.add_node(package);
        self.name_to_index.insert(name, idx);
        idx
    }

    /// Add a dependency edge (from depends on to)
    pub fn add_dependency(&mut self, from: &str, to: &str) -> bool {
        let from_idx = self.name_to_index.get(from);
        let to_idx = self.name_to_index.get(to);

        match (from_idx, to_idx) {
            (Some(&f), Some(&t)) => {
                self.graph.add_edge(f, t, ());
                true
            }
            _ => false,
        }
    }

    /// Get topological order (build order)
    pub fn build_order(&self) -> Vec<String> {
        match toposort(&self.graph, None) {
            Ok(sorted) => sorted
                .iter()
                .rev()
                .filter_map(|idx| self.graph.node_weight(*idx))
                .map(|node| node.name.clone())
                .collect(),
            Err(_) => vec![], // Cycle detected
        }
    }

    /// Get all packages affected by changes to a package
    pub fn affected(&self, changed: &[String]) -> Vec<String> {
        let mut affected = std::collections::HashSet::new();
        let mut queue: std::collections::VecDeque<_> = changed.iter().cloned().collect();

        while let Some(pkg) = queue.pop_front() {
            if affected.contains(&pkg) {
                continue;
            }
            affected.insert(pkg.clone());

            // Get dependents (packages that depend on this one)
            if let Some(&idx) = self.name_to_index.get(&pkg) {
                for neighbor in self.graph.neighbors_directed(idx, Direction::Incoming) {
                    if let Some(node) = self.graph.node_weight(neighbor) {
                        if !affected.contains(&node.name) {
                            queue.push_back(node.name.clone());
                        }
                    }
                }
            }
        }

        affected.into_iter().collect()
    }

    /// Get direct dependencies of a package
    pub fn dependencies(&self, name: &str) -> Vec<String> {
        self.name_to_index
            .get(name)
            .map(|&idx| {
                self.graph
                    .neighbors_directed(idx, Direction::Outgoing)
                    .filter_map(|n| self.graph.node_weight(n))
                    .map(|node| node.name.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all packages
    pub fn packages(&self) -> Vec<&PackageNode> {
        self.graph.node_weights().collect()
    }

    /// Package count
    pub fn len(&self) -> usize {
        self.graph.node_count()
    }

    pub fn is_empty(&self) -> bool {
        self.graph.node_count() == 0
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}
