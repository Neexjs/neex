//! Neex CLI - Ultra-fast Monorepo Build Tool
//!
//! Commands:
//! - neex daemon start    - Start background daemon
//! - neex run <script>    - Run script with caching
//! - neex build           - Alias for neex run build
//! - neex run-all <script> - Run script in all workspaces (parallel)
//! - neex graph           - Show dependency graph and build order
//! - neex hash <file>     - Hash a file (AST-aware)

use anyhow::Result;
use clap::{Parser, Subcommand};
use neex_core::{hash_ast, is_parseable, TaskRunner, Hasher, DepGraph, Scheduler, SchedulerTask};
use neex_daemon::{DaemonRequest, DaemonResponse};
use std::path::PathBuf;
use std::sync::Arc;
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
    /// Run a script in all workspaces (parallel, respects dependencies)
    RunAll {
        /// Script name to run in all workspaces
        script: String,
        /// Max concurrent tasks (default: CPU cores)
        #[arg(short, long)]
        concurrency: Option<usize>,
    },
    /// Hash a file (AST-aware for JS/TS)
    Hash {
        /// File to hash
        file: PathBuf,
    },
    /// Show workspace dependency graph and build order
    Graph,
    /// Show packages affected by a change
    Affected {
        /// Package name that changed
        package: String,
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

        Commands::RunAll { script, concurrency } => {
            run_all_parallel(&cwd, &script, concurrency).await?;
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

        Commands::Graph => {
            show_graph(&cwd)?;
        }

        Commands::Affected { package } => {
            show_affected(&cwd, &package)?;
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

/// Run a script in all workspaces in parallel
async fn run_all_parallel(cwd: &PathBuf, script: &str, concurrency: Option<usize>) -> Result<()> {
    println!("🚀 neex run-all {} (parallel)", script);
    let start = Instant::now();

    // Build dependency graph
    let graph = DepGraph::from_root(cwd)?;
    
    if graph.package_count() == 0 {
        println!("❌ No workspaces found");
        return Ok(());
    }

    // Get build order
    let build_order = graph.get_build_order()?;
    println!("📦 Found {} workspaces", build_order.len());
    println!();

    // Build dependency map (package name -> list of dependencies)
    let mut dep_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for node in graph.packages() {
        let pkg_path = cwd.join(&node.path).join("package.json");
        if let Ok(content) = std::fs::read_to_string(&pkg_path) {
            if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                let mut deps = Vec::new();
                if let Some(dep_obj) = pkg.get("dependencies").and_then(|d| d.as_object()) {
                    for dep_name in dep_obj.keys() {
                        // Only include workspace dependencies
                        if graph.get_package(dep_name).is_some() {
                            deps.push(dep_name.clone());
                        }
                    }
                }
                dep_map.insert(node.name.clone(), deps);
            }
        }
    }

    // Create scheduler tasks
    let root = Arc::new(cwd.clone());
    let script_arc = Arc::new(script.to_string());
    
    let tasks: Vec<SchedulerTask> = build_order.iter().map(|node| {
        let pkg_name = node.name.clone();
        let pkg_path = node.path.clone();
        let deps = dep_map.get(&pkg_name).cloned().unwrap_or_default();
        let root_clone = Arc::clone(&root);
        let script_clone = Arc::clone(&script_arc);
        
        SchedulerTask::new(pkg_name.clone(), deps, move || {
            let full_path = root_clone.join(&pkg_path);
            
            // Check if script exists
            let pkg_json_path = full_path.join("package.json");
            let content = std::fs::read_to_string(&pkg_json_path)?;
            let pkg: serde_json::Value = serde_json::from_str(&content)?;
            
            let cmd = pkg.get("scripts")
                .and_then(|s| s.get(script_clone.as_str()))
                .and_then(|c| c.as_str());
            
            if let Some(command) = cmd {
                println!("▶ {} → {}", pkg_name, command);
                
                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .current_dir(&full_path)
                    .output()?;
                
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(anyhow::anyhow!("{} failed: {}", pkg_name, stderr));
                }
                
                println!("✓ {}", pkg_name);
            } else {
                println!("⏭ {} (no {} script)", pkg_name, script_clone);
            }
            
            Ok(())
        })
    }).collect();

    // Run with scheduler
    let concurrency = concurrency.unwrap_or_else(|| {
        std::thread::available_parallelism().map(|p| p.get()).unwrap_or(4)
    });
    
    let scheduler = Scheduler::new(concurrency);
    let results = scheduler.execute(tasks).await?;

    let elapsed = start.elapsed();
    
    // Summary
    println!();
    let succeeded = results.iter().filter(|r| r.status == neex_core::TaskStatus::Completed).count();
    let failed = results.iter().filter(|r| r.status == neex_core::TaskStatus::Failed).count();
    
    if failed == 0 {
        println!("✅ All {} packages completed in {:?}", succeeded, elapsed);
    } else {
        println!("❌ {} succeeded, {} failed in {:?}", succeeded, failed, elapsed);
        for r in results.iter().filter(|r| r.status == neex_core::TaskStatus::Failed) {
            if let Some(err) = &r.error {
                println!("   • {} failed: {}", r.name, err);
            }
        }
    }

    Ok(())
}

/// Show dependency graph
fn show_graph(cwd: &PathBuf) -> Result<()> {
    println!("🕸️  Building Dependency Graph...");
    let start = Instant::now();
    
    let graph = DepGraph::from_root(cwd)?;
    let elapsed = start.elapsed();
    
    println!();
    println!("📦 Packages: {}", graph.package_count());
    println!("🔗 Dependencies: {}", graph.edge_count());
    println!();

    if graph.has_cycle() {
        println!("⚠️  Warning: Circular dependency detected!");
        return Ok(());
    }

    println!("📋 Workspaces:");
    for pkg in graph.packages() {
        println!("   • {} ({})", pkg.name, pkg.path.display());
    }
    println!();

    match graph.get_build_order() {
        Ok(order) => {
            println!("🔨 Build Order (dependencies first):");
            for (i, pkg) in order.iter().enumerate() {
                println!("   {}. {}", i + 1, pkg.name);
            }
        }
        Err(e) => {
            println!("❌ Error: {}", e);
        }
    }
    
    println!();
    println!("✅ Graph built in {:?}", elapsed);
    
    Ok(())
}

/// Show affected packages
fn show_affected(cwd: &PathBuf, package: &str) -> Result<()> {
    println!("🔍 Finding packages affected by {}...", package);
    
    let graph = DepGraph::from_root(cwd)?;
    let affected = graph.get_affected(package);
    
    if affected.is_empty() {
        println!("❌ Package '{}' not found", package);
        return Ok(());
    }
    
    println!();
    println!("📦 Affected packages ({}):", affected.len());
    for pkg in &affected {
        println!("   • {}", pkg.name);
    }
    
    Ok(())
}

/// Run a script with caching
async fn run_script(cwd: &PathBuf, script: &str) -> Result<()> {
    let start = Instant::now();
    
    let runner = TaskRunner::new(cwd)?;

    let command = match runner.get_script(script)? {
        Some(cmd) => cmd,
        None => {
            println!("❌ Script '{}' not found in package.json", script);
            return Ok(());
        }
    };

    println!("🔨 neex run {} ", script);
    println!("   Command: {}", command);

    let hasher = Hasher::new(cwd);
    let project_hash = hasher.global_hash()?;
    let cache_key = format!("{}:{}", script, &project_hash[..16]);

    println!("   Hash: {}...", &project_hash[..16]);

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

    println!();
    println!("─────────────────────────────────");
    let output = runner.execute(&command).await?;
    println!("─────────────────────────────────");

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
