//! Neex Hasher - BLAKE3-based Content Hashing
//!
//! Features:
//! - 10x faster than SHA-256
//! - Parallel file hashing with Rayon
//! - Respects .gitignore patterns
//! - Incremental updates

use anyhow::Result;
use blake3::Hasher as Blake3Hasher;
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// File hash result
#[derive(Debug, Clone)]
pub struct FileHash {
    pub path: PathBuf,
    pub hash: String,
    pub size: u64,
}

/// Main hasher struct
pub struct Hasher {
    root: PathBuf,
    cache: Mutex<HashMap<PathBuf, FileHash>>,
}

impl Hasher {
    /// Create a new hasher for the given root directory
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Hash a single file using BLAKE3
    pub fn hash_file(&self, path: impl AsRef<Path>) -> Result<String> {
        let content = fs::read(path.as_ref())?;
        let hash = blake3::hash(&content);
        Ok(hash.to_hex().to_string())
    }

    /// Hash all files in directory (parallel with Rayon)
    /// Target: 10,000 files < 100ms
    pub fn hash_all(&self) -> Result<Vec<FileHash>> {
        let files: Vec<PathBuf> = WalkBuilder::new(&self.root)
            .hidden(false)
            .ignore(true)        // Respect .gitignore
            .git_ignore(true)
            .git_global(true)
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .map(|e| e.path().to_path_buf())
            .collect();

        // Parallel hashing with Rayon
        let results: Vec<FileHash> = files
            .par_iter()
            .filter_map(|path| {
                let content = fs::read(path).ok()?;
                let hash = blake3::hash(&content);
                let size = content.len() as u64;
                
                Some(FileHash {
                    path: path.clone(),
                    hash: hash.to_hex().to_string(),
                    size,
                })
            })
            .collect();

        // Update cache
        {
            let mut cache = self.cache.lock().unwrap();
            for result in &results {
                cache.insert(result.path.clone(), result.clone());
            }
        }

        Ok(results)
    }

    /// Get global hash of all files (for cache key)
    pub fn global_hash(&self) -> Result<String> {
        let files = self.hash_all()?;
        
        let mut hasher = Blake3Hasher::new();
        
        // Sort for deterministic hash
        let mut sorted: Vec<_> = files.iter().collect();
        sorted.sort_by(|a, b| a.path.cmp(&b.path));
        
        for file in sorted {
            hasher.update(file.hash.as_bytes());
        }
        
        Ok(hasher.finalize().to_hex().to_string())
    }

    /// Get files that changed since last hash
    pub fn get_changed(&self, old_hashes: &HashMap<PathBuf, String>) -> Result<Vec<PathBuf>> {
        let current = self.hash_all()?;
        
        let changed: Vec<PathBuf> = current
            .iter()
            .filter(|file| {
                match old_hashes.get(&file.path) {
                    Some(old_hash) => old_hash != &file.hash,
                    None => true, // New file
                }
            })
            .map(|f| f.path.clone())
            .collect();
        
        Ok(changed)
    }

    /// Get stats
    pub fn stats(&self) -> HasherStats {
        let cache = self.cache.lock().unwrap();
        let total_size: u64 = cache.values().map(|f| f.size).sum();
        
        HasherStats {
            total_files: cache.len(),
            total_size,
        }
    }
}

#[derive(Debug)]
pub struct HasherStats {
    pub total_files: usize,
    pub total_size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_hash_performance() {
        let hasher = Hasher::new(".");
        
        let start = Instant::now();
        let files = hasher.hash_all().unwrap();
        let elapsed = start.elapsed();
        
        println!("Hashed {} files in {:?}", files.len(), elapsed);
        
        // Target: should be fast
        assert!(elapsed.as_millis() < 5000, "Hashing took too long!");
    }
}
