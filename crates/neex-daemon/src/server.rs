//! Daemon Server - Unix Socket IPC
//!
//! CLI connects to daemon via Unix socket for instant responses

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{debug, error, info};

use crate::state::DaemonState;
use crate::watcher::FileWatcher;

/// Request from CLI to daemon
#[derive(Debug, Serialize, Deserialize)]
pub enum DaemonRequest {
    /// Get file hash
    GetHash { path: String },
    /// Get global hash
    GlobalHash,
    /// Get changed files
    GetChanged {
        hashes: std::collections::HashMap<String, String>,
    },
    /// Force rescan
    Rescan,
    /// Get stats
    Stats,
    /// Shutdown
    Shutdown,
}

/// Response from daemon to CLI
#[derive(Debug, Serialize, Deserialize)]
pub enum DaemonResponse {
    Hash(Option<String>),
    GlobalHash(String),
    Changed(Vec<String>),
    Stats { cached_files: usize, db_size: u64 },
    Ok,
    Error(String),
}

/// Daemon server
pub struct DaemonServer {
    socket_path: std::path::PathBuf,
    state: DaemonState,
    watcher: FileWatcher,
}

impl DaemonServer {
    /// Create new daemon server
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        let socket_path = root.join(".neex").join("daemon.sock");

        // Create directory
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Remove old socket if exists
        let _ = std::fs::remove_file(&socket_path);

        let state = DaemonState::new(root)?;
        let watcher = FileWatcher::new(root)?;

        Ok(Self {
            socket_path,
            state,
            watcher,
        })
    }

    /// Start daemon server
    pub async fn start(&mut self) -> Result<()> {
        // Initial scan
        info!("Performing initial file scan...");
        let count = self.state.full_scan()?;
        info!("Scanned {} files", count);

        // Start file watcher
        self.watcher.start()?;

        // Create Unix socket
        let listener = UnixListener::bind(&self.socket_path)?;
        info!("Daemon listening on: {:?}", self.socket_path);

        loop {
            tokio::select! {
                // Handle new connections
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _)) => {
                            if let Err(e) = self.handle_connection(stream).await {
                                error!("Connection error: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("Accept error: {}", e);
                        }
                    }
                }

                // Poll for file changes periodically
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                    self.process_file_changes();
                }
            }
        }
    }

    /// Process pending file changes from watcher
    fn process_file_changes(&mut self) {
        let changes = self.watcher.poll();

        for change in changes {
            match change.kind {
                crate::watcher::ChangeKind::Create | crate::watcher::ChangeKind::Modify => {
                    if let Err(e) = self.state.update_file(&change.path) {
                        debug!("Failed to update {}: {}", change.path.display(), e);
                    }
                }
                crate::watcher::ChangeKind::Delete => {
                    if let Err(e) = self.state.remove_file(&change.path) {
                        debug!("Failed to remove {}: {}", change.path.display(), e);
                    }
                }
            }
        }
    }

    /// Handle a single connection
    async fn handle_connection(&self, mut stream: UnixStream) -> Result<()> {
        let (reader, mut writer) = stream.split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        reader.read_line(&mut line).await?;

        let request: DaemonRequest = serde_json::from_str(&line)?;
        debug!("Request: {:?}", request);

        let response = match request {
            DaemonRequest::GetHash { path } => {
                let hash = self.state.get_hash(std::path::Path::new(&path));
                DaemonResponse::Hash(hash)
            }
            DaemonRequest::GlobalHash => match self.state.global_hash() {
                Ok(hash) => DaemonResponse::GlobalHash(hash),
                Err(e) => DaemonResponse::Error(e.to_string()),
            },
            DaemonRequest::GetChanged { hashes } => {
                let old: std::collections::HashMap<std::path::PathBuf, String> = hashes
                    .into_iter()
                    .map(|(k, v)| (std::path::PathBuf::from(k), v))
                    .collect();
                let changed = self.state.get_changed(&old);
                DaemonResponse::Changed(
                    changed
                        .into_iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect(),
                )
            }
            DaemonRequest::Stats => {
                let stats = self.state.stats();
                DaemonResponse::Stats {
                    cached_files: stats.cached_files,
                    db_size: stats.db_size,
                }
            }
            DaemonRequest::Rescan => {
                // Would need mutable access, simplified for now
                DaemonResponse::Ok
            }
            DaemonRequest::Shutdown => {
                info!("Shutdown requested");
                std::process::exit(0);
            }
        };

        let response_json = serde_json::to_string(&response)?;
        writer.write_all(response_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;

        Ok(())
    }
}
