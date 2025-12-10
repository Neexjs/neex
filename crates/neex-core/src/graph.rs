//! Workspace Dependency Graph - Monorepo Support
//!
//! Features:
//! - Workspace discovery from package.json globs
//! - Directed dependency graph (petgraph)
//! - Topological sort for build order
//! - Cycle detection to prevent infinite builds
//! - Living graph updated by daemon

use anyhow::{anyhow, Result};
use petgraph::algo::{is_cyclic_directed, toposort};
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A workspace package node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceNode {
    /// Package name from package.json
    pub name: String,
    /// Relative path from root
    pub path: PathBuf,
    /// Full path to package.json
    pub package_json_path: PathBuf,
    /// Package version
    pub version: Option<String>,
    /// Scripts available
    pub scripts: Vec<String>,
}

/// Root package.json structure
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RootPackageJson {
    name: Option<String>,
    workspaces: Option<WorkspacesConfig>,
}

/// Workspaces can be array or object with packages key
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum WorkspacesConfig {
    Array(Vec<String>),
    Object { packages: Vec<String> },
}

/// Package-level package.json structure
#[derive(Debug, Deserialize)]
struct PackageJson {
    name: Option<String>,
    version: Option<String>,
    scripts: Option<HashMap<String, String>>,
    dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "devDependencies")]
    dev_dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "peerDependencies")]
    peer_dependencies: Option<HashMap<String, String>>,
}

/// Dependency Graph for workspace packages
pub struct DepGraph {
    /// The directed graph
    pub graph: DiGraph<WorkspaceNode, ()>,
    /// Map from package name to node index
    name_to_idx: HashMap<String, NodeIndex>,
    /// Root directory
    root: PathBuf,
}

impl DepGraph {
    /// Create a new empty dependency graph
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            name_to_idx: HashMap::new(),
            root: PathBuf::new(),
        }
    }

    /// Build graph from a monorepo root directory
    pub fn from_root(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let mut graph = Self {
            graph: DiGraph::new(),
            name_to_idx: HashMap::new(),
            root: root.clone(),
        };

        // Step 1: Discover workspaces
        let workspaces = graph.discover_workspaces()?;

        // Step 2: Add nodes for each workspace
        for ws_path in &workspaces {
            if let Err(e) = graph.add_workspace_node(ws_path) {
                tracing::warn!("Failed to add workspace {:?}: {}", ws_path, e);
            }
        }

        // Step 3: Build dependency edges
        graph.build_edges()?;

        Ok(graph)
    }

    /// Discover workspace directories from root package.json
    fn discover_workspaces(&self) -> Result<Vec<PathBuf>> {
        let root_pkg_path = self.root.join("package.json");
        if !root_pkg_path.exists() {
            return Err(anyhow!("No package.json found in {:?}", self.root));
        }

        let content = std::fs::read_to_string(&root_pkg_path)?;
        let root_pkg: RootPackageJson = serde_json::from_str(&content)?;

        let patterns = match root_pkg.workspaces {
            Some(WorkspacesConfig::Array(arr)) => arr,
            Some(WorkspacesConfig::Object { packages }) => packages,
            None => return Ok(vec![]), // Not a monorepo
        };

        let mut workspaces = Vec::new();

        for pattern in patterns {
            // Handle glob patterns like "packages/*"
            let glob_pattern = self.root.join(&pattern).to_string_lossy().to_string();

            for entry in glob::glob(&glob_pattern)? {
                if let Ok(path) = entry {
                    if path.is_dir() && path.join("package.json").exists() {
                        workspaces.push(path);
                    }
                }
            }
        }

        Ok(workspaces)
    }

    /// Add a workspace node to the graph
    fn add_workspace_node(&mut self, ws_path: &Path) -> Result<NodeIndex> {
        let pkg_json_path = ws_path.join("package.json");
        let content = std::fs::read_to_string(&pkg_json_path)?;
        let pkg: PackageJson = serde_json::from_str(&content)?;

        let name = pkg.name.ok_or_else(|| anyhow!("Package has no name: {:?}", ws_path))?;
        let relative_path = ws_path.strip_prefix(&self.root).unwrap_or(ws_path).to_path_buf();

        let scripts: Vec<String> =
            pkg.scripts.map(|s| s.keys().cloned().collect()).unwrap_or_default();

        let node = WorkspaceNode {
            name: name.clone(),
            path: relative_path,
            package_json_path: pkg_json_path,
            version: pkg.version,
            scripts,
        };

        let idx = self.graph.add_node(node);
        self.name_to_idx.insert(name, idx);

        Ok(idx)
    }

    /// Build dependency edges between nodes
    fn build_edges(&mut self) -> Result<()> {
        // Collect all dependencies first (to avoid borrow issues)
        let mut edges: Vec<(String, String)> = Vec::new();

        for idx in self.graph.node_indices() {
            let node = &self.graph[idx];
            let pkg_content = std::fs::read_to_string(&node.package_json_path)?;
            let pkg: PackageJson = serde_json::from_str(&pkg_content)?;

            let source_name = node.name.clone();

            // Collect all dependency types
            let all_deps: Vec<String> = [
                pkg.dependencies,
                pkg.dev_dependencies,
                pkg.peer_dependencies,
            ]
            .into_iter()
            .flatten()
            .flat_map(|m| m.into_keys())
            .collect();

            for dep_name in all_deps {
                // Only add edge if dependency is in our workspace
                if self.name_to_idx.contains_key(&dep_name) {
                    edges.push((source_name.clone(), dep_name));
                }
            }
        }

        // Add edges (dependent -> dependency, so dependency comes first in topo sort)
        for (from_name, to_name) in edges {
            if let (Some(&from_idx), Some(&to_idx)) = (
                self.name_to_idx.get(&from_name),
                self.name_to_idx.get(&to_name),
            ) {
                // Edge direction: dependent -> dependency
                // This ensures dependencies come FIRST in topological order
                self.graph.add_edge(from_idx, to_idx, ());
            }
        }

        Ok(())
    }

    /// Check if graph has a cycle (circular dependency)
    pub fn has_cycle(&self) -> bool {
        is_cyclic_directed(&self.graph)
    }

    /// Get build order (topological sort)
    /// Returns packages in order they should be built (dependencies first)
    pub fn get_build_order(&self) -> Result<Vec<&WorkspaceNode>> {
        if self.has_cycle() {
            return Err(anyhow!("Circular dependency detected!"));
        }

        // toposort returns nodes in dependency order (dependencies last)
        // We need to reverse because we want dependencies first
        let sorted = toposort(&self.graph, None)
            .map_err(|_| anyhow!("Cycle detected during topological sort"))?;

        // Reverse: dependencies should come first
        let build_order: Vec<&WorkspaceNode> =
            sorted.into_iter().rev().map(|idx| &self.graph[idx]).collect();

        Ok(build_order)
    }

    /// Get packages affected by a change in the given package
    pub fn get_affected(&self, package_name: &str) -> Vec<&WorkspaceNode> {
        let Some(&start_idx) = self.name_to_idx.get(package_name) else {
            return vec![];
        };

        // BFS to find all packages that depend on this one
        let mut affected = vec![&self.graph[start_idx]];
        let mut visited = std::collections::HashSet::new();
        let mut queue = vec![start_idx];

        visited.insert(start_idx);

        while let Some(idx) = queue.pop() {
            // Find all packages that have an edge TO this node (dependents)
            for neighbor in self.graph.neighbors_directed(idx, petgraph::Direction::Incoming) {
                if visited.insert(neighbor) {
                    affected.push(&self.graph[neighbor]);
                    queue.push(neighbor);
                }
            }
        }

        affected
    }

    /// Get package by name
    pub fn get_package(&self, name: &str) -> Option<&WorkspaceNode> {
        self.name_to_idx.get(name).map(|&idx| &self.graph[idx])
    }

    /// Get all packages
    pub fn packages(&self) -> Vec<&WorkspaceNode> {
        self.graph.node_indices().map(|idx| &self.graph[idx]).collect()
    }

    /// Get package count
    pub fn package_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Get edge count
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }
}

impl Default for DepGraph {
    fn default() -> Self {
        Self::new()
    }
}

// Re-export old DependencyGraph for compatibility
pub type DependencyGraph = DepGraph;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cycle_detection() {
        let mut dep_graph = DepGraph::new();

        let idx_a = dep_graph.graph.add_node(WorkspaceNode {
            name: "A".into(),
            path: PathBuf::new(),
            package_json_path: PathBuf::new(),
            version: None,
            scripts: vec![],
        });
        let idx_b = dep_graph.graph.add_node(WorkspaceNode {
            name: "B".into(),
            path: PathBuf::new(),
            package_json_path: PathBuf::new(),
            version: None,
            scripts: vec![],
        });

        // A -> B
        dep_graph.graph.add_edge(idx_a, idx_b, ());
        assert!(!dep_graph.has_cycle(), "Should not detect cycle yet");

        // B -> A (creates cycle!)
        dep_graph.graph.add_edge(idx_b, idx_a, ());
        assert!(dep_graph.has_cycle(), "Should detect cycle!");
    }

    #[test]
    fn test_topological_sort() {
        let mut dep_graph = DepGraph::new();

        // utils <- ui <- web
        // utils has no deps, ui depends on utils, web depends on ui and utils

        let utils_idx = dep_graph.graph.add_node(WorkspaceNode {
            name: "@my/utils".into(),
            path: PathBuf::from("packages/utils"),
            package_json_path: PathBuf::new(),
            version: None,
            scripts: vec![],
        });
        dep_graph.name_to_idx.insert("@my/utils".into(), utils_idx);

        let ui_idx = dep_graph.graph.add_node(WorkspaceNode {
            name: "@my/ui".into(),
            path: PathBuf::from("packages/ui"),
            package_json_path: PathBuf::new(),
            version: None,
            scripts: vec![],
        });
        dep_graph.name_to_idx.insert("@my/ui".into(), ui_idx);

        let web_idx = dep_graph.graph.add_node(WorkspaceNode {
            name: "@my/web".into(),
            path: PathBuf::from("packages/web"),
            package_json_path: PathBuf::new(),
            version: None,
            scripts: vec![],
        });
        dep_graph.name_to_idx.insert("@my/web".into(), web_idx);

        // ui -> utils (ui depends on utils)
        dep_graph.graph.add_edge(ui_idx, utils_idx, ());
        // web -> ui (web depends on ui)
        dep_graph.graph.add_edge(web_idx, ui_idx, ());
        // web -> utils (web depends on utils)
        dep_graph.graph.add_edge(web_idx, utils_idx, ());

        let order = dep_graph.get_build_order().unwrap();
        let names: Vec<&str> = order.iter().map(|n| n.name.as_str()).collect();

        // utils must come before ui, ui must come before web
        let utils_pos = names.iter().position(|&n| n == "@my/utils").unwrap();
        let ui_pos = names.iter().position(|&n| n == "@my/ui").unwrap();
        let web_pos = names.iter().position(|&n| n == "@my/web").unwrap();

        assert!(utils_pos < ui_pos, "utils should come before ui");
        assert!(ui_pos < web_pos, "ui should come before web");
    }

    #[test]
    fn test_affected_packages() {
        let mut dep_graph = DepGraph::new();

        let utils_idx = dep_graph.graph.add_node(WorkspaceNode {
            name: "@my/utils".into(),
            path: PathBuf::from("packages/utils"),
            package_json_path: PathBuf::new(),
            version: None,
            scripts: vec![],
        });
        dep_graph.name_to_idx.insert("@my/utils".into(), utils_idx);

        let ui_idx = dep_graph.graph.add_node(WorkspaceNode {
            name: "@my/ui".into(),
            path: PathBuf::from("packages/ui"),
            package_json_path: PathBuf::new(),
            version: None,
            scripts: vec![],
        });
        dep_graph.name_to_idx.insert("@my/ui".into(), ui_idx);

        let web_idx = dep_graph.graph.add_node(WorkspaceNode {
            name: "@my/web".into(),
            path: PathBuf::from("packages/web"),
            package_json_path: PathBuf::new(),
            version: None,
            scripts: vec![],
        });
        dep_graph.name_to_idx.insert("@my/web".into(), web_idx);

        // ui -> utils
        dep_graph.graph.add_edge(ui_idx, utils_idx, ());
        // web -> ui
        dep_graph.graph.add_edge(web_idx, ui_idx, ());

        // If utils changes, both ui and web are affected
        let affected = dep_graph.get_affected("@my/utils");
        let names: Vec<&str> = affected.iter().map(|n| n.name.as_str()).collect();

        assert!(names.contains(&"@my/utils"));
        assert!(names.contains(&"@my/ui"));
        assert!(names.contains(&"@my/web"));
    }
}
