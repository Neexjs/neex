//! Content-Addressable Cache for build outputs

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Cache entry metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub hash: String,
    pub timestamp: u64,
    pub duration_ms: u64,
    pub exit_code: i32,
    pub outputs: Vec<String>,
}

/// Content-addressable cache
pub struct Cache {
    root: PathBuf,
    entries: HashMap<String, CacheEntry>,
}

impl Cache {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let cache_dir = root.as_ref().join(".neex").join("cache");
        fs::create_dir_all(&cache_dir)?;

        let entries = Self::load_entries(&cache_dir)?;

        Ok(Self {
            root: cache_dir,
            entries,
        })
    }

    fn load_entries(cache_dir: &Path) -> Result<HashMap<String, CacheEntry>> {
        let index_path = cache_dir.join("index.json");

        if index_path.exists() {
            let content = fs::read_to_string(&index_path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(HashMap::new())
        }
    }

    fn save_entries(&self) -> Result<()> {
        let index_path = self.root.join("index.json");
        let content = serde_json::to_string_pretty(&self.entries)?;
        fs::write(index_path, content)?;
        Ok(())
    }

    /// Check if cache exists for hash
    pub fn has(&self, hash: &str) -> bool {
        self.entries.contains_key(hash)
    }

    /// Get cache entry
    pub fn get(&self, hash: &str) -> Option<&CacheEntry> {
        self.entries.get(hash)
    }

    /// Store cache entry
    pub fn put(&mut self, hash: String, entry: CacheEntry) -> Result<()> {
        self.entries.insert(hash, entry);
        self.save_entries()?;
        Ok(())
    }

    /// Get cache stats
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.entries.len(),
            cache_dir: self.root.clone(),
        }
    }

    /// Clear cache
    pub fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        self.save_entries()?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct CacheStats {
    pub entries: usize,
    pub cache_dir: PathBuf,
}
