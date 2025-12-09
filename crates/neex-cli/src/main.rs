//! Neex CLI - Ultra-fast Monorepo Build Tool
//!
//! Inspired by Turbo/Nx - Short, clean commands
//!
//! Quick commands: neex dev, neex build, neex test
//! Filters: --filter=pkg, --changed, --all

use anyhow::Result;
use clap::{Parser, Subcommand, Args};
use dialoguer::{Input, Password, Select, Confirm, theme::ColorfulTheme};
use neex_core::{
    hash_ast, is_parseable, TaskRunner, Hasher, DepGraph, 
    Scheduler, SchedulerTask, CloudCache, CloudConfig, S3Config,
    load_config, save_config, get_config_path,
};
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
    command: Option<Commands>,

    /// Run any task directly: neex <task>
    #[arg(trailing_var_arg = true)]
    task: Vec<String>,
}

#[derive(Args, Clone)]
struct RunFlags {
    /// Filter by package name
    #[arg(long, short = 'f')]
    filter: Option<String>,
    
    /// Only run on changed packages (affected)
    #[arg(long)]
    changed: bool,
    
    /// Run on all packages
    #[arg(long)]
    all: bool,
    
    /// Concurrency limit
    #[arg(long, short = 'c')]
    concurrency: Option<usize>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run dev script
    Dev(RunFlags),
    /// Run build script
    Build(RunFlags),
    /// Run test script
    Test(RunFlags),
    /// Run lint script
    Lint(RunFlags),
    /// Run any script
    Run {
        /// Script name
        script: String,
        #[command(flatten)]
        flags: RunFlags,
    },
    
    /// Show dependency graph
    Graph,
    /// Show why a package is included in the build
    Why {
        /// Package name
        package: String,
    },
    /// List all packages
    List,
    
    /// Login to cloud cache (S3/R2)
    Login,
    /// Logout from cloud cache
    Logout,
    /// Clean cache
    Prune {
        /// Clean everything (local + cloud)
        #[arg(long)]
        all: bool,
    },
    
    /// Show project info (packages, cache, config)
    Info,
    
    /// Daemon management
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    
    /// Hash a file (AST-aware for JS/TS)
    Hash {
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum DaemonAction {
    Start,
    Stop,
    Status,
}

fn get_socket_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".neex")
        .join("daemon.sock")
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;

    // Handle direct task execution: neex <task>
    if cli.command.is_none() && !cli.task.is_empty() {
        let task = cli.task.join(" ");
        return run_task(&cwd, &task, &RunFlags { 
            filter: None, changed: false, all: false, concurrency: None 
        }).await;
    }

    let Some(command) = cli.command else {
        println!("Usage: neex <command>");
        println!();
        println!("Commands:");
        println!("  dev      Run dev script");
        println!("  build    Run build script");
        println!("  test     Run test script");
        println!("  graph    Show dependency graph");
        println!("  login    Setup cloud cache");
        println!("  info     Show project info");
        println!();
        println!("Run 'neex --help' for more options");
        return Ok(());
    };

    match command {
        Commands::Dev(flags) => run_task(&cwd, "dev", &flags).await?,
        Commands::Build(flags) => run_task(&cwd, "build", &flags).await?,
        Commands::Test(flags) => run_task(&cwd, "test", &flags).await?,
        Commands::Lint(flags) => run_task(&cwd, "lint", &flags).await?,
        Commands::Run { script, flags } => run_task(&cwd, &script, &flags).await?,
        
        Commands::Graph => show_graph(&cwd)?,
        Commands::Why { package } => show_why(&cwd, &package)?,
        Commands::List => list_packages(&cwd)?,
        
        Commands::Login => cloud_login().await?,
        Commands::Logout => cloud_logout()?,
        Commands::Prune { all } => prune_cache(&cwd, all).await?,
        
        Commands::Info => show_info(&cwd).await?,
        
        Commands::Daemon { action } => match action {
            DaemonAction::Start => {
                println!("🚀 Starting daemon...");
                println!("   Run: cargo run -p neex-daemon");
            }
            DaemonAction::Stop => {
                send_request(&get_socket_path(), DaemonRequest::Shutdown).await?;
                println!("✅ Daemon stopped");
            }
            DaemonAction::Status => daemon_status().await?,
        },
        
        Commands::Hash { file } => {
            if !file.exists() {
                println!("❌ File not found: {}", file.display());
                return Ok(());
            }
            let content = std::fs::read_to_string(&file)?;
            let hash = if is_parseable(&file) {
                hash_ast(&file, &content)?
            } else {
                neex_core::ast_hasher::hash_raw(&content)?
            };
            println!("{}", hash);
        }
    }

    Ok(())
}

/// Run a task with optional flags
async fn run_task(cwd: &PathBuf, script: &str, flags: &RunFlags) -> Result<()> {
    let start = Instant::now();

    // --all: run on all packages in parallel
    if flags.all {
        return run_all_packages(cwd, script, flags.concurrency).await;
    }

    // --changed: run only on affected packages
    if flags.changed {
        return run_changed_packages(cwd, script, flags.concurrency).await;
    }

    // --filter: run on specific package
    if let Some(ref pkg) = flags.filter {
        return run_filtered_package(cwd, script, pkg).await;
    }

    // Default: run in current directory with tiered caching
    run_with_tiered_cache(cwd, script).await
}

/// Run with tiered caching (L1→L2→L3→L4)
async fn run_with_tiered_cache(cwd: &PathBuf, script: &str) -> Result<()> {
    let start = Instant::now();
    let runner = TaskRunner::new(cwd)?;

    let command = match runner.get_script(script)? {
        Some(cmd) => cmd,
        None => {
            println!("❌ Script '{}' not found", script);
            return Ok(());
        }
    };

    println!("▶ neex {}", script);

    let hasher = Hasher::new(cwd);
    let project_hash = hasher.global_hash()?;
    let cache_key = format!("{}:{}", script, &project_hash[..16]);

    // L1: Local cache
    if let Some(cached) = runner.get_cached(&cache_key)? {
        println!("⚡ cached ({}ms)", start.elapsed().as_millis());
        runner.replay_output(&cached);
        return Ok(());
    }

    // L3: Cloud cache
    if let Ok(Some(cloud)) = CloudCache::try_new() {
        if let Ok(Some(data)) = cloud.download(&cache_key).await {
            if let Ok(output) = serde_json::from_slice::<neex_core::TaskOutput>(&data) {
                let _ = runner.store_cached(&cache_key, &output);
                println!("☁️ cloud cached");
                runner.replay_output(&output);
                return Ok(());
            }
        }
    }

    // L4: Execute
    let output = runner.execute(&command).await?;
    
    let mut output_with_hash = output.clone();
    output_with_hash.hash = cache_key.clone();
    runner.store_cached(&cache_key, &output_with_hash)?;

    // Background cloud upload
    let json_data = serde_json::to_vec(&output_with_hash)?;
    CloudCache::upload_background(cache_key, json_data);

    println!("✓ {} ({}ms)", script, start.elapsed().as_millis());
    Ok(())
}

/// Run on all packages in parallel
async fn run_all_packages(cwd: &PathBuf, script: &str, concurrency: Option<usize>) -> Result<()> {
    let start = Instant::now();
    let graph = DepGraph::from_root(cwd)?;
    
    if graph.package_count() == 0 {
        println!("❌ No packages found");
        return Ok(());
    }

    println!("▶ neex {} --all ({} packages)", script, graph.package_count());

    let build_order = graph.get_build_order()?;
    let tasks = create_parallel_tasks(cwd, &build_order, script, &graph);

    let concurrency = concurrency.unwrap_or_else(|| {
        std::thread::available_parallelism().map(|p| p.get()).unwrap_or(4)
    });
    
    let scheduler = Scheduler::new(concurrency);
    let results = scheduler.execute(tasks).await?;

    let ok = results.iter().filter(|r| r.status == neex_core::TaskStatus::Completed).count();
    let fail = results.iter().filter(|r| r.status == neex_core::TaskStatus::Failed).count();
    
    if fail == 0 {
        println!("✓ {} packages ({}ms)", ok, start.elapsed().as_millis());
    } else {
        println!("✗ {} ok, {} failed ({}ms)", ok, fail, start.elapsed().as_millis());
    }

    Ok(())
}

/// Run only on changed/affected packages
async fn run_changed_packages(cwd: &PathBuf, script: &str, concurrency: Option<usize>) -> Result<()> {
    let start = Instant::now();
    let graph = DepGraph::from_root(cwd)?;
    
    // For now, run all (TODO: integrate with git diff)
    println!("▶ neex {} --changed", script);
    println!("   (Running all packages - git integration coming soon)");
    
    run_all_packages(cwd, script, concurrency).await
}

/// Run on a specific package
async fn run_filtered_package(cwd: &PathBuf, script: &str, pkg_name: &str) -> Result<()> {
    let start = Instant::now();
    let graph = DepGraph::from_root(cwd)?;
    
    let pkg = graph.get_package(pkg_name);
    if pkg.is_none() {
        println!("❌ Package '{}' not found", pkg_name);
        return Ok(());
    }
    let pkg = pkg.unwrap();

    println!("▶ neex {} --filter={}", script, pkg_name);

    let pkg_path = cwd.join(&pkg.path);
    run_with_tiered_cache(&pkg_path, script).await
}

fn create_parallel_tasks(
    cwd: &PathBuf,
    build_order: &[&neex_core::WorkspaceNode],
    script: &str,
    graph: &DepGraph,
) -> Vec<SchedulerTask> {
    let mut dep_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    
    for node in graph.packages() {
        let pkg_path = cwd.join(&node.path).join("package.json");
        if let Ok(content) = std::fs::read_to_string(&pkg_path) {
            if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                let mut deps = Vec::new();
                if let Some(dep_obj) = pkg.get("dependencies").and_then(|d| d.as_object()) {
                    for dep_name in dep_obj.keys() {
                        if graph.get_package(dep_name).is_some() {
                            deps.push(dep_name.clone());
                        }
                    }
                }
                dep_map.insert(node.name.clone(), deps);
            }
        }
    }

    let root = Arc::new(cwd.clone());
    let script_arc = Arc::new(script.to_string());
    
    build_order.iter().map(|node| {
        let pkg_name = node.name.clone();
        let pkg_path = node.path.clone();
        let deps = dep_map.get(&pkg_name).cloned().unwrap_or_default();
        let root_clone = Arc::clone(&root);
        let script_clone = Arc::clone(&script_arc);
        
        SchedulerTask::new(pkg_name.clone(), deps, move || {
            let full_path = root_clone.join(&pkg_path);
            let pkg_json_path = full_path.join("package.json");
            let content = std::fs::read_to_string(&pkg_json_path)?;
            let pkg: serde_json::Value = serde_json::from_str(&content)?;
            
            if let Some(command) = pkg.get("scripts")
                .and_then(|s| s.get(script_clone.as_str()))
                .and_then(|c| c.as_str())
            {
                print!("  {} ", pkg_name);
                let output = std::process::Command::new("sh")
                    .arg("-c").arg(command)
                    .current_dir(&full_path)
                    .output()?;
                
                if !output.status.success() {
                    println!("✗");
                    return Err(anyhow::anyhow!("failed"));
                }
                println!("✓");
            }
            Ok(())
        })
    }).collect()
}

/// Show dependency graph
fn show_graph(cwd: &PathBuf) -> Result<()> {
    let graph = DepGraph::from_root(cwd)?;
    
    println!("📦 {} packages, {} deps", graph.package_count(), graph.edge_count());
    println!();

    if graph.has_cycle() {
        println!("⚠️ Circular dependency!");
        return Ok(());
    }

    match graph.get_build_order() {
        Ok(order) => {
            println!("Build order:");
            for (i, pkg) in order.iter().enumerate() {
                println!("  {}. {}", i + 1, pkg.name);
            }
        }
        Err(e) => println!("❌ {}", e),
    }
    
    Ok(())
}

/// Show why a package is included
fn show_why(cwd: &PathBuf, package: &str) -> Result<()> {
    let graph = DepGraph::from_root(cwd)?;
    let affected = graph.get_affected(package);
    
    if affected.is_empty() {
        println!("❌ Package '{}' not found", package);
        return Ok(());
    }
    
    println!("📦 {} affects:", package);
    for pkg in &affected {
        if pkg.name != package {
            println!("  → {}", pkg.name);
        }
    }
    Ok(())
}

/// List all packages
fn list_packages(cwd: &PathBuf) -> Result<()> {
    let graph = DepGraph::from_root(cwd)?;
    
    println!("Packages:");
    for pkg in graph.packages() {
        println!("  {} ({})", pkg.name, pkg.path.display());
    }
    
    Ok(())
}

/// Cloud login wizard
async fn cloud_login() -> Result<()> {
    println!("☁️ Cloud Cache Setup");
    println!();

    let theme = ColorfulTheme::default();

    let options = &["Cloudflare R2", "AWS S3", "MinIO", "Other"];
    let selection = Select::with_theme(&theme)
        .with_prompt("Provider")
        .items(options)
        .default(0)
        .interact()?;

    let endpoint: String = Input::with_theme(&theme)
        .with_prompt("Endpoint")
        .interact_text()?;

    let bucket: String = Input::with_theme(&theme)
        .with_prompt("Bucket")
        .with_initial_text("neex-cache")
        .interact_text()?;

    let region: String = Input::with_theme(&theme)
        .with_prompt("Region")
        .with_initial_text("auto")
        .interact_text()?;

    let access_key: String = Input::with_theme(&theme)
        .with_prompt("Access Key")
        .interact_text()?;

    let secret_key: String = Password::with_theme(&theme)
        .with_prompt("Secret Key")
        .interact()?;

    let config = CloudConfig {
        s3: Some(S3Config {
            endpoint, bucket: bucket.clone(), region,
            access_key, secret_key, enabled: true,
        }),
    };

    save_config(&config)?;
    println!();
    println!("✅ Saved to ~/.neex/config.json");

    // Test connection
    print!("Testing... ");
    match CloudCache::try_new() {
        Ok(Some(cloud)) => match cloud.ping().await {
            Ok(true) => println!("✓ Connected to {}", bucket),
            _ => println!("✗ Connection failed"),
        },
        _ => println!("✗ Config error"),
    }

    Ok(())
}

/// Cloud logout
fn cloud_logout() -> Result<()> {
    let mut config = load_config()?;
    if let Some(ref mut s3) = config.s3 {
        s3.enabled = false;
    }
    save_config(&config)?;
    println!("✅ Logged out");
    Ok(())
}

/// Prune cache
async fn prune_cache(cwd: &PathBuf, all: bool) -> Result<()> {
    if all {
        let theme = ColorfulTheme::default();
        let confirm = Confirm::with_theme(&theme)
            .with_prompt("Delete ALL cache (local + cloud)?")
            .default(false)
            .interact()?;
        if !confirm {
            return Ok(());
        }
    }

    let runner = TaskRunner::new(cwd)?;
    runner.clear_cache()?;
    println!("✅ Local cache cleared");

    if all {
        println!("⚠️ Cloud cleanup requires manual bucket management");
    }

    Ok(())
}

/// Show project info
async fn show_info(cwd: &PathBuf) -> Result<()> {
    println!("Neex Info");
    println!();

    // Packages
    let graph = DepGraph::from_root(cwd);
    match graph {
        Ok(g) => println!("📦 Packages: {}", g.package_count()),
        Err(_) => println!("📦 Packages: (not a monorepo)"),
    }

    // Cache
    let cache_dir = cwd.join(".neex").join("cache");
    if cache_dir.exists() {
        let size = dir_size(&cache_dir)?;
        println!("💾 Cache: {} KB", size / 1024);
    } else {
        println!("💾 Cache: empty");
    }

    // Cloud
    let config = load_config()?;
    match config.s3 {
        Some(s3) if s3.enabled => println!("☁️ Cloud: {} ✓", s3.bucket),
        Some(_) => println!("☁️ Cloud: disabled"),
        None => println!("☁️ Cloud: not configured"),
    }

    // Daemon
    print!("🚀 Daemon: ");
    match send_request(&get_socket_path(), DaemonRequest::Stats).await {
        Ok(_) => println!("running"),
        Err(_) => println!("stopped"),
    }

    Ok(())
}

async fn daemon_status() -> Result<()> {
    match send_request(&get_socket_path(), DaemonRequest::Stats).await {
        Ok(DaemonResponse::Stats { cached_files, db_size }) => {
            println!("Daemon running");
            println!("  Files: {}", cached_files);
            println!("  DB: {} bytes", db_size);
        }
        Err(_) => println!("Daemon not running"),
        _ => {}
    }
    Ok(())
}

fn dir_size(path: &PathBuf) -> Result<u64> {
    let mut size = 0;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            if meta.is_file() {
                size += meta.len();
            } else if meta.is_dir() {
                size += dir_size(&entry.path())?;
            }
        }
    }
    Ok(size)
}

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
