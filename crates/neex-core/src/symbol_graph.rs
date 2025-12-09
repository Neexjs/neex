//! Symbol Graph - Phase 8.2
//!
//! Build a dependency graph at the Symbol level (not file level).
//! This enables 10x reduction in rebuilds by tracking which files
//! use which specific exports.
//!
//! Example:
//!   formatDate() changed → only rebuild files that import formatDate
//!   (NOT all files that import the package)

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::symbols::{extract_from_file, Symbol};

/// Unique identifier for a symbol: "package:symbol_name"
pub type SymbolId = String;

/// Symbol Graph - tracks symbol dependencies across files
#[derive(Debug, Default)]
pub struct SymbolGraph {
    /// Symbol -> files that import it
    pub consumers: HashMap<SymbolId, HashSet<PathBuf>>,
    
    /// File -> its exported symbols with hashes
    pub exports: HashMap<PathBuf, Vec<Symbol>>,
    
    /// Package name -> file path (for resolving imports)
    pub packages: HashMap<String, PathBuf>,
}

/// Stored symbol hashes for change detection
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SymbolCache {
    /// Symbol ID -> hash
    pub hashes: HashMap<SymbolId, String>,
}

impl SymbolGraph {
    /// Build graph from workspace root
    pub fn build(root: &Path) -> Result<Self> {
        let mut graph = SymbolGraph::default();
        
        // Find all packages
        graph.discover_packages(root)?;
        
        // Extract symbols from all JS/TS files
        graph.extract_all_symbols(root)?;
        
        // Build consumer map
        graph.build_consumers(root)?;
        
        Ok(graph)
    }

    /// Discover packages in workspace
    fn discover_packages(&mut self, root: &Path) -> Result<()> {
        let pkg_json = root.join("package.json");
        if !pkg_json.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&pkg_json)?;
        let pkg: serde_json::Value = serde_json::from_str(&content)?;

        // Get workspaces
        let workspaces = pkg.get("workspaces")
            .and_then(|w| w.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
            .unwrap_or_default();

        for pattern in workspaces {
            let pattern_path = root.join(pattern);
            let _base = pattern_path.parent().unwrap_or(root);
            
            if let Ok(entries) = glob::glob(&pattern_path.to_string_lossy()) {
                for entry in entries.flatten() {
                    if entry.is_dir() {
                        let pkg_json = entry.join("package.json");
                        if pkg_json.exists() {
                            if let Ok(content) = std::fs::read_to_string(&pkg_json) {
                                if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                                    if let Some(name) = pkg.get("name").and_then(|n| n.as_str()) {
                                        self.packages.insert(name.to_string(), entry.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Extract symbols from all JS/TS files
    fn extract_all_symbols(&mut self, root: &Path) -> Result<()> {
        for (_, pkg_path) in &self.packages.clone() {
            self.extract_package_symbols(pkg_path)?;
        }
        
        // Also scan root src if exists
        let src_dir = root.join("src");
        if src_dir.exists() {
            self.scan_directory(&src_dir)?;
        }
        
        Ok(())
    }

    /// Extract symbols from a package
    fn extract_package_symbols(&mut self, pkg_path: &Path) -> Result<()> {
        let src = pkg_path.join("src");
        if src.exists() {
            self.scan_directory(&src)?;
        }
        
        // Check index files
        for index in &["index.ts", "index.tsx", "index.js", "index.jsx"] {
            let path = pkg_path.join(index);
            if path.exists() {
                self.extract_file(&path)?;
            }
        }
        
        Ok(())
    }

    /// Scan directory for JS/TS files
    fn scan_directory(&mut self, dir: &Path) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }

        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mts" | "mjs") {
                    self.extract_file(path)?;
                }
            }
        }
        
        Ok(())
    }

    /// Extract symbols from a single file
    fn extract_file(&mut self, path: &Path) -> Result<()> {
        match extract_from_file(path) {
            Ok(symbols) => {
                self.exports.insert(path.to_path_buf(), symbols.exports);
            }
            Err(_) => {} // Skip unparseable files
        }
        Ok(())
    }

    /// Build consumer map from imports
    fn build_consumers(&mut self, root: &Path) -> Result<()> {
        // Scan all files again for imports
        for (_, pkg_path) in &self.packages.clone() {
            self.scan_imports(pkg_path, root)?;
        }
        
        let src = root.join("src");
        if src.exists() {
            self.scan_imports(&src, root)?;
        }
        
        Ok(())
    }

    /// Scan imports in a directory
    fn scan_imports(&mut self, dir: &Path, _root: &Path) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }

        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mts" | "mjs") {
                    self.process_imports(path)?;
                }
            }
        }
        
        Ok(())
    }

    /// Process imports in a file
    fn process_imports(&mut self, file: &Path) -> Result<()> {
        let symbols = extract_from_file(file)?;
        
        for import in symbols.imports {
            // Check if import is from a known package
            if let Some(_pkg_path) = self.packages.get(&import.from) {
                for symbol_name in &import.symbols {
                    let id = format!("{}:{}", import.from, symbol_name);
                    self.consumers
                        .entry(id)
                        .or_default()
                        .insert(file.to_path_buf());
                }
            }
        }
        
        Ok(())
    }

    /// Get files that import a specific symbol
    pub fn get_consumers(&self, package: &str, symbol: &str) -> Vec<PathBuf> {
        let id = format!("{}:{}", package, symbol);
        self.consumers
            .get(&id)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all exported symbols from all packages
    pub fn get_all_symbols(&self) -> Vec<(SymbolId, String)> {
        let mut result = Vec::new();
        
        for (path, symbols) in &self.exports {
            // Find package name for this path
            let pkg_name = self.packages.iter()
                .find(|(_, p)| path.starts_with(p))
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            
            for symbol in symbols {
                let id = format!("{}:{}", pkg_name, symbol.name);
                result.push((id, symbol.hash.clone()));
            }
        }
        
        result
    }

    /// Get changed symbols by comparing with previous cache
    pub fn get_changed_symbols(&self, cache: &SymbolCache) -> Vec<SymbolId> {
        let mut changed = Vec::new();
        
        for (id, hash) in self.get_all_symbols() {
            match cache.hashes.get(&id) {
                Some(old_hash) if old_hash == &hash => {} // Same
                _ => changed.push(id), // New or changed
            }
        }
        
        changed
    }

    /// Get all files affected by changed symbols
    pub fn get_affected_files(&self, changed: &[SymbolId]) -> Vec<PathBuf> {
        let mut affected = HashSet::new();
        
        for id in changed {
            if let Some(consumers) = self.consumers.get(id) {
                affected.extend(consumers.iter().cloned());
            }
        }
        
        affected.into_iter().collect()
    }

    /// Create cache from current state
    pub fn to_cache(&self) -> SymbolCache {
        let mut cache = SymbolCache::default();
        
        for (id, hash) in self.get_all_symbols() {
            cache.hashes.insert(id, hash);
        }
        
        cache
    }

    /// Summary stats
    pub fn stats(&self) -> (usize, usize, usize) {
        (
            self.packages.len(),
            self.exports.values().map(|v| v.len()).sum(),
            self.consumers.len(),
        )
    }
}

impl SymbolCache {
    /// Load from disk
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Save to disk
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn create_test_monorepo() -> tempfile::TempDir {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Root package.json
        fs::write(root.join("package.json"), r#"
            {"workspaces": ["packages/*"]}
        "#).unwrap();

        // Utils package
        let utils = root.join("packages/utils");
        fs::create_dir_all(&utils).unwrap();
        fs::write(utils.join("package.json"), r#"{"name": "@my/utils"}"#).unwrap();
        fs::write(utils.join("index.ts"), r#"
            export function formatDate() { return "date"; }
            export function formatNumber() { return 123; }
        "#).unwrap();

        // Web package
        let web = root.join("packages/web");
        fs::create_dir_all(&web).unwrap();
        fs::write(web.join("package.json"), r#"{"name": "@my/web"}"#).unwrap();
        fs::write(web.join("index.ts"), r#"
            import { formatDate } from "@my/utils";
            export function App() { return formatDate(); }
        "#).unwrap();

        dir
    }

    #[test]
    fn test_build_graph() {
        let dir = create_test_monorepo();
        let graph = SymbolGraph::build(dir.path()).unwrap();
        
        assert_eq!(graph.packages.len(), 2);
        assert!(graph.packages.contains_key("@my/utils"));
        assert!(graph.packages.contains_key("@my/web"));
    }

    #[test]
    fn test_get_consumers() {
        let dir = create_test_monorepo();
        let graph = SymbolGraph::build(dir.path()).unwrap();
        
        let consumers = graph.get_consumers("@my/utils", "formatDate");
        assert_eq!(consumers.len(), 1);
        
        let consumers = graph.get_consumers("@my/utils", "formatNumber");
        assert_eq!(consumers.len(), 0); // Not imported anywhere
    }

    #[test]
    fn test_symbol_cache() {
        let dir = create_test_monorepo();
        let graph = SymbolGraph::build(dir.path()).unwrap();
        let cache = graph.to_cache();
        
        // Should have symbols from utils
        assert!(cache.hashes.keys().any(|k| k.contains("formatDate")));
    }

    #[test]
    fn test_changed_detection() {
        let dir = create_test_monorepo();
        let graph = SymbolGraph::build(dir.path()).unwrap();
        
        // First run - everything is "changed"
        let cache = SymbolCache::default();
        let changed = graph.get_changed_symbols(&cache);
        assert!(!changed.is_empty());
        
        // Second run with same cache - nothing changed
        let cache = graph.to_cache();
        let changed = graph.get_changed_symbols(&cache);
        assert!(changed.is_empty());
    }
}
