//! File Watcher - Real-time file system monitoring
//!
//! Uses notify crate (FSEvents on macOS, inotify on Linux)
//! Updates hash cache in real-time

use anyhow::Result;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tracing::{debug, info, warn};

/// File change event
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: PathBuf,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, Copy)]
pub enum ChangeKind {
    Create,
    Modify,
    Delete,
}

/// File watcher with debouncing
pub struct FileWatcher {
    root: PathBuf,
    watcher: Option<RecommendedWatcher>,
    receiver: Option<Receiver<Result<Event, notify::Error>>>,
    pending_changes: Arc<RwLock<HashSet<PathBuf>>>,
    ignore_patterns: Vec<String>,
}

impl FileWatcher {
    /// Create new file watcher
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();

        let ignore_patterns = vec![
            "node_modules".to_string(),
            ".git".to_string(),
            ".neex".to_string(),
            "dist".to_string(),
            ".next".to_string(),
            "target".to_string(),
        ];

        Ok(Self {
            root,
            watcher: None,
            receiver: None,
            pending_changes: Arc::new(RwLock::new(HashSet::new())),
            ignore_patterns,
        })
    }

    /// Check if path should be ignored
    fn should_ignore(&self, path: &Path) -> bool {
        for component in path.components() {
            if let std::path::Component::Normal(name) = component {
                let name_str = name.to_string_lossy();
                for pattern in &self.ignore_patterns {
                    if name_str.contains(pattern) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Start watching files
    pub fn start(&mut self) -> Result<()> {
        let (tx, rx) = channel();

        let config = Config::default().with_poll_interval(Duration::from_millis(100));

        let mut watcher = RecommendedWatcher::new(tx, config)?;
        watcher.watch(&self.root, RecursiveMode::Recursive)?;

        self.watcher = Some(watcher);
        self.receiver = Some(rx);

        info!("File watcher started for: {:?}", self.root);
        Ok(())
    }

    /// Stop watching
    pub fn stop(&mut self) {
        self.watcher = None;
        self.receiver = None;
        info!("File watcher stopped");
    }

    /// Poll for changes (non-blocking)
    pub fn poll(&self) -> Vec<FileChange> {
        let mut changes = Vec::new();

        if let Some(ref rx) = self.receiver {
            // Drain all pending events
            while let Ok(result) = rx.try_recv() {
                match result {
                    Ok(event) => {
                        for path in event.paths {
                            if self.should_ignore(&path) {
                                continue;
                            }

                            let kind = match event.kind {
                                notify::EventKind::Create(_) => ChangeKind::Create,
                                notify::EventKind::Modify(_) => ChangeKind::Modify,
                                notify::EventKind::Remove(_) => ChangeKind::Delete,
                                _ => continue,
                            };

                            debug!("File change: {:?} ({:?})", path, kind);
                            changes.push(FileChange { path, kind });
                        }
                    }
                    Err(e) => {
                        warn!("Watch error: {:?}", e);
                    }
                }
            }
        }

        changes
    }

    /// Get pending changes count
    pub fn pending_count(&self) -> usize {
        self.pending_changes.read().unwrap().len()
    }
}

impl Drop for FileWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}
