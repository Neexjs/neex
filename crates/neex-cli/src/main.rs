//! Neex CLI - Ultra-fast Monorepo Build Tool
//!
//! Commands:
//! - neex daemon start  - Start background daemon
//! - neex run <script>  - Run script with caching
//! - neex build         - Alias for neex run build
//! - neex hash <file>   - Hash a file (AST-aware)

use anyhow::Result;
use clap::{Parser, Subcommand};
use neex_core::{hash_ast, is_parseable, TaskRunner, Hasher};
use neex_daemon::{DaemonRequest, DaemonResponse};
use std::path::PathBuf;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Neex - Ultra-fast Monorepo Build Tool
#[derive(Parser)]
#[command(name = "neex", version, about = "Ultra-fast monorepo build tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Daemon management
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Run a script with caching (e.g., neex run build)
    Run {
        /// Script name from package.json
        script: String,
    },
    /// Build the project (alias for: neex run build)
    Build,
    /// Hash a file (AST-aware for JS/TS)
    Hash {
        /// File to hash
        file: PathBuf,
    },
    /// Get daemon status
    Status,
    /// Clear task cache
    ClearCache,
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Start the daemon
    Start,
    /// Stop the daemon
    Stop,
}

fn get_socket_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".neex").join("daemon.sock")
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;

    match cli.command {
        Commands::Daemon { action } => match action {
            DaemonAction::Start => {
                println!("🚀 Starting neex daemon...");
                println!("   Run: cargo run -p neex-daemon");
            }
            DaemonAction::Stop => {
                let socket_path = get_socket_path();
                send_request(&socket_path, DaemonRequest::Shutdown).await?;
                println!("✅ Daemon stopped");
            }
        },

        Commands::Build => {
            run_script(&cwd, "build").await?;
        }

        Commands::Run { script } => {
            run_script(&cwd, &script).await?;
        }

        Commands::Hash { file } => {
            if !file.exists() {
                println!("❌ File not found: {}", file.display());
                return Ok(());
            }

            let content = std::fs::read_to_string(&file)?;
            
            let hash = if is_parseable(&file) {
                println!("🧠 Using AST hash (ignores comments)");
                hash_ast(&file, &content)?
            } else {
                println!("📄 Using raw hash");
                neex_core::ast_hasher::hash_raw(&content)?
            };

            println!("🔑 Hash: {}", hash);
        }

        Commands::Status => {
            let socket_path = get_socket_path();
            match send_request(&socket_path, DaemonRequest::Stats).await {
                Ok(DaemonResponse::Stats { cached_files, db_size }) => {
                    println!("📊 Daemon Status:");
                    println!("   Cached files: {}", cached_files);
                    println!("   DB size: {} bytes", db_size);
                }
                Err(_) => {
                    println!("❌ Daemon not running");
                }
                _ => {}
            }
        }

        Commands::ClearCache => {
            let runner = TaskRunner::new(&cwd)?;
            runner.clear_cache()?;
            println!("🗑️  Cache cleared!");
        }
    }

    Ok(())
}

/// Run a script with caching
async fn run_script(cwd: &PathBuf, script: &str) -> Result<()> {
    let start = Instant::now();
    
    // Create task runner with persistent cache
    let runner = TaskRunner::new(cwd)?;

    // Check if script exists
    let command = match runner.get_script(script)? {
        Some(cmd) => cmd,
        None => {
            println!("❌ Script '{}' not found in package.json", script);
            return Ok(());
        }
    };

    println!("🔨 neex run {} ", script);
    println!("   Command: {}", command);

    // Calculate project hash
    let hasher = Hasher::new(cwd);
    let project_hash = hasher.global_hash()?;
    let cache_key = format!("{}:{}", script, &project_hash[..16]);

    println!("   Hash: {}...", &project_hash[..16]);

    // Check cache (persistent)
    if let Some(cached) = runner.get_cached(&cache_key)? {
        let elapsed = start.elapsed();
        println!();
        println!("⚡ CACHED! Replaying output...");
        println!("─────────────────────────────────");
        runner.replay_output(&cached);
        println!("─────────────────────────────────");
        println!("✅ {} (cached) in {:?}", script, elapsed);
        println!("   Original run took {}ms", cached.duration_ms);
        return Ok(());
    }

    // Execute task
    println!();
    println!("─────────────────────────────────");
    let output = runner.execute(&command).await?;
    println!("─────────────────────────────────");

    // Store in persistent cache
    let mut output_with_hash = output.clone();
    output_with_hash.hash = cache_key.clone();
    runner.store_cached(&cache_key, &output_with_hash)?;

    let elapsed = start.elapsed();

    if output.exit_code == 0 {
        println!("✅ {} completed in {:?}", script, elapsed);
    } else {
        println!("❌ {} failed with exit code {} in {:?}", script, output.exit_code, elapsed);
    }

    Ok(())
}

/// Send request to daemon via Unix socket
async fn send_request(socket_path: &PathBuf, request: DaemonRequest) -> Result<DaemonResponse> {
    let mut stream = UnixStream::connect(socket_path).await?;
    
    let request_json = serde_json::to_string(&request)?;
    stream.write_all(request_json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    
    let (reader, _) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut response_line = String::new();
    reader.read_line(&mut response_line).await?;
    
    let response: DaemonResponse = serde_json::from_str(&response_line)?;
    Ok(response)
}
