//! Task Runner - Execute and cache task outputs
//!
//! The real Turbo-like feature:
//! - Read package.json scripts
//! - Execute with tokio::process
//! - Persist cache to disk (sled DB)
//! - Replay cached output instantly on cache hit

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Cached task output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskOutput {
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub hash: String,
    pub cached_at: u64,
}

/// Package.json structure (minimal)
#[derive(Debug, Deserialize)]
pub struct PackageJson {
    pub name: Option<String>,
    pub scripts: Option<HashMap<String, String>>,
}

/// Task Runner with persistent caching (sled DB)
pub struct TaskRunner {
    root: PathBuf,
    db: sled::Db,
}

impl TaskRunner {
    /// Create new task runner with persistent cache
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let cache_dir = root.join(".neex").join("cache");
        std::fs::create_dir_all(&cache_dir)?;

        let db = sled::open(&cache_dir)?;

        Ok(Self { root, db })
    }

    /// Load package.json and get script command
    pub fn get_script(&self, script_name: &str) -> Result<Option<String>> {
        let pkg_path = self.root.join("package.json");
        if !pkg_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&pkg_path)?;
        let pkg: PackageJson = serde_json::from_str(&content)?;

        Ok(pkg.scripts.and_then(|s| s.get(script_name).cloned()))
    }

    /// Check if task is cached (persistent)
    pub fn get_cached(&self, hash: &str) -> Result<Option<TaskOutput>> {
        match self.db.get(hash.as_bytes())? {
            Some(data) => {
                let output: TaskOutput = serde_json::from_slice(&data)?;
                Ok(Some(output))
            }
            None => Ok(None),
        }
    }

    /// Store task output in persistent cache
    pub fn store_cached(&self, hash: &str, output: &TaskOutput) -> Result<()> {
        let data = serde_json::to_vec(output)?;
        self.db.insert(hash.as_bytes(), data)?;
        self.db.flush()?;
        Ok(())
    }

    /// Execute a task and capture output
    pub async fn execute(&self, command: &str) -> Result<TaskOutput> {
        let start = Instant::now();

        // Use sh on Unix, cmd on Windows
        let (shell, shell_arg) = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };

        let mut child = Command::new(shell)
            .arg(shell_arg)
            .arg(command)
            .current_dir(&self.root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let mut stdout_lines = Vec::new();
        let mut stderr_lines = Vec::new();

        // Capture stdout
        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Some(line) = lines.next_line().await? {
                println!("{}", line); // Stream to console
                stdout_lines.push(line);
            }
        }

        // Capture stderr
        if let Some(stderr) = child.stderr.take() {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Some(line) = lines.next_line().await? {
                eprintln!("{}", line); // Stream to console
                stderr_lines.push(line);
            }
        }

        let status = child.wait().await?;
        let duration = start.elapsed();

        Ok(TaskOutput {
            stdout: stdout_lines,
            stderr: stderr_lines,
            exit_code: status.code().unwrap_or(-1),
            duration_ms: duration.as_millis() as u64,
            hash: String::new(),
            cached_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        })
    }

    /// Replay cached output (print to console)
    pub fn replay_output(&self, output: &TaskOutput) {
        for line in &output.stdout {
            println!("{}", line);
        }
        for line in &output.stderr {
            eprintln!("{}", line);
        }
    }

    /// Get cache stats
    pub fn cache_stats(&self) -> Result<(usize, u64)> {
        let count = self.db.len();
        let size = self.db.size_on_disk()?;
        Ok((count, size))
    }

    /// Clear cache
    pub fn clear_cache(&self) -> Result<()> {
        self.db.clear()?;
        self.db.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_simple_command() {
        let runner = TaskRunner::new(".").unwrap();
        let output = runner.execute("echo hello").await.unwrap();

        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.iter().any(|l| l.contains("hello")));
    }
}
