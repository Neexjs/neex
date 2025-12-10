//! Daemon State - RAM-cached file hashes
//!
//! Stores file hashes in memory for instant access
//! Persists to sled DB for crash recovery

use anyhow::Result;
use neex_core::hasher::Hasher;
use sled::Db;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tracing::{debug, info};

/// Daemon state with cached hashes
#[allow(dead_code)]
pub struct DaemonState {
    root: PathBuf,
    hashes: Arc<RwLock<HashMap<PathBuf, String>>>,
    db: Db,
    hasher: Hasher,
    last_scan: Option<Instant>,
}

impl DaemonState {
    /// Create new daemon state
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let db_path = root.join(".neex").join("daemon.db");

        // Create parent directory
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db = sled::open(&db_path)?;
        let hasher = Hasher::new(&root);

        Ok(Self {
            root,
            hashes: Arc::new(RwLock::new(HashMap::new())),
            db,
            hasher,
            last_scan: None,
        })
    }

    /// Load cached hashes from sled DB
    pub fn load_from_db(&self) -> Result<usize> {
        let mut count = 0;
        let mut hashes = self.hashes.write().unwrap();

        for item in self.db.iter() {
            let (key, value) = item?;
            let path = PathBuf::from(String::from_utf8_lossy(&key).to_string());
            let hash = String::from_utf8_lossy(&value).to_string();
            hashes.insert(path, hash);
            count += 1;
        }

        info!("Loaded {} cached hashes from DB", count);
        Ok(count)
    }

    /// Full scan and cache all hashes
    pub fn full_scan(&mut self) -> Result<usize> {
        let start = Instant::now();
        let files = self.hasher.hash_all()?;

        {
            let mut hashes = self.hashes.write().unwrap();
            hashes.clear();

            // Batch write to DB
            let mut batch = sled::Batch::default();

            for file in &files {
                hashes.insert(file.path.clone(), file.hash.clone());
                batch.insert(file.path.to_string_lossy().as_bytes(), file.hash.as_bytes());
            }

            self.db.apply_batch(batch)?;
        }

        self.last_scan = Some(start);
        let elapsed = start.elapsed();

        info!("Full scan: {} files in {:?}", files.len(), elapsed);
        Ok(files.len())
    }

    /// Update hash for a single file
    pub fn update_file(&self, path: &Path) -> Result<Option<String>> {
        let hash = self.hasher.hash_file(path)?;

        {
            let mut hashes = self.hashes.write().unwrap();
            hashes.insert(path.to_path_buf(), hash.clone());
        }

        // Persist to DB
        self.db.insert(path.to_string_lossy().as_bytes(), hash.as_bytes())?;

        debug!("Updated hash: {:?}", path);
        Ok(Some(hash))
    }

    /// Remove file from cache
    pub fn remove_file(&self, path: &Path) -> Result<()> {
        {
            let mut hashes = self.hashes.write().unwrap();
            hashes.remove(path);
        }

        self.db.remove(path.to_string_lossy().as_bytes())?;
        debug!("Removed: {:?}", path);
        Ok(())
    }

    /// Get hash for a file (from RAM cache)
    pub fn get_hash(&self, path: &Path) -> Option<String> {
        self.hashes.read().unwrap().get(path).cloned()
    }

    /// Get global hash (all files combined)
    pub fn global_hash(&self) -> Result<String> {
        self.hasher.global_hash()
    }

    /// Get changed files since provided hashes
    pub fn get_changed(&self, old_hashes: &HashMap<PathBuf, String>) -> Vec<PathBuf> {
        let current = self.hashes.read().unwrap();

        current
            .iter()
            .filter(|(path, hash)| old_hashes.get(*path).map(|h| h != *hash).unwrap_or(true))
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// Get stats
    pub fn stats(&self) -> DaemonStats {
        let hashes = self.hashes.read().unwrap();
        DaemonStats {
            cached_files: hashes.len(),
            db_size: self.db.size_on_disk().unwrap_or(0),
            last_scan: self.last_scan,
        }
    }
}

#[derive(Debug)]
pub struct DaemonStats {
    pub cached_files: usize,
    pub db_size: u64,
    pub last_scan: Option<Instant>,
}
