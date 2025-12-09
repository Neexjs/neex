//! Neex CLI - Ultra-fast Monorepo Build Tool
//!
//! Commands:
//! - neex daemon start  - Start background daemon
//! - neex daemon stop   - Stop daemon
//! - neex build         - Build project
//! - neex hash <file>   - Hash a file (AST-aware)

use anyhow::Result;
use clap::{Parser, Subcommand};
use neex_core::{hash_ast, is_parseable};
use neex_daemon::{DaemonRequest, DaemonResponse};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tracing::info;

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
    /// Build the project
    Build,
    /// Hash a file (AST-aware for JS/TS)
    Hash {
        /// File to hash
        file: PathBuf,
    },
    /// Get daemon status
    Status,
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
    // Setup logging
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let socket_path = get_socket_path();

    match cli.command {
        Commands::Daemon { action } => match action {
            DaemonAction::Start => {
                println!("🚀 Starting neex daemon...");
                println!("   Run: cargo run -p neex-daemon");
                // TODO: Actually spawn daemon as background process
            }
            DaemonAction::Stop => {
                send_request(&socket_path, DaemonRequest::Shutdown).await?;
                println!("✅ Daemon stopped");
            }
        },

        Commands::Build => {
            println!("🔨 Building...");
            
            // Connect to daemon and get global hash
            match send_request(&socket_path, DaemonRequest::GlobalHash).await {
                Ok(DaemonResponse::GlobalHash(hash)) => {
                    println!("📦 Global hash: {}", &hash[..16]);
                    // TODO: Check cache, run build if needed
                }
                Ok(DaemonResponse::Error(e)) => {
                    println!("❌ Error: {}", e);
                }
                Err(e) => {
                    println!("❌ Daemon not running: {}", e);
                    println!("   Run: neex daemon start");
                }
                _ => {}
            }
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
    }

    Ok(())
}

/// Send request to daemon via Unix socket
async fn send_request(socket_path: &PathBuf, request: DaemonRequest) -> Result<DaemonResponse> {
    let mut stream = UnixStream::connect(socket_path).await?;
    
    // Send JSON request
    let request_json = serde_json::to_string(&request)?;
    stream.write_all(request_json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    
    // Read response
    let (reader, _) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut response_line = String::new();
    reader.read_line(&mut response_line).await?;
    
    let response: DaemonResponse = serde_json::from_str(&response_line)?;
    Ok(response)
}
