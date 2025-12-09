//! Neex CLI - Ultra-fast Monorepo Build Tool
//!
//! Commands:
//! - neex run <script>       - Run script with tiered caching
//! - neex build              - Alias for neex run build
//! - neex run-all <script>   - Parallel workspace execution
//! - neex graph              - Show dependency graph
//! - neex cache remote login - Setup cloud cache (S3/R2)
//! - neex cache clean        - Clean local/cloud cache

use anyhow::Result;
use clap::{Parser, Subcommand};
use dialoguer::{Input, Password, Select, Confirm, theme::ColorfulTheme};
use neex_core::{
    hash_ast, is_parseable, TaskRunner, Hasher, DepGraph, 
    Scheduler, SchedulerTask, CloudCache, CloudConfig, S3Config,
    load_config, save_config, get_config_path,
};
use neex_daemon::{DaemonRequest, DaemonResponse, PeerManager};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Neex - Ultra-fast Monorepo Build Tool
#[derive(Parser)]
#[command(name = "neex", version, about = "Ultra-fast monorepo build tool with tiered caching")]
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
    /// Run a script with tiered caching (Local → P2P → Cloud → Execute)
    Run {
        /// Script name from package.json
        script: String,
    },
    /// Build the project (alias for: neex run build)
    Build,
    /// Run a script in all workspaces (parallel)
    RunAll {
        /// Script name
        script: String,
        /// Max concurrent tasks
        #[arg(short, long)]
        concurrency: Option<usize>,
    },
    /// Hash a file (AST-aware for JS/TS)
    Hash {
        /// File to hash
        file: PathBuf,
    },
    /// Show dependency graph and build order
    Graph,
    /// Show affected packages
    Affected {
        /// Package name that changed
        package: String,
    },
    /// Cache management (local, cloud, P2P)
    Cache {
        #[command(subcommand)]
        action: CacheAction,
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

#[derive(Subcommand)]
enum CacheAction {
    /// Remote cloud cache management
    Remote {
        #[command(subcommand)]
        action: RemoteAction,
    },
    /// Clean cache
    Clean {
        /// Clean all (local + cloud)
        #[arg(long)]
        all: bool,
    },
    /// Show cache statistics
    Stats,
}

#[derive(Subcommand)]
enum RemoteAction {
    /// Setup cloud cache (interactive wizard)
    Login {
        /// Use S3-compatible provider
        #[arg(long)]
        s3: bool,
        /// Use Cloudflare R2
        #[arg(long)]
        r2: bool,
    },
    /// Test cloud connection
    Status,
    /// Disable cloud cache
    Disable,
    /// Enable cloud cache
    Enable,
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

        Commands::Build => run_script_tiered(&cwd, "build").await?,
        Commands::Run { script } => run_script_tiered(&cwd, &script).await?,
        Commands::RunAll { script, concurrency } => run_all_parallel(&cwd, &script, concurrency).await?,

        Commands::Hash { file } => {
            if !file.exists() {
                println!("❌ File not found: {}", file.display());
                return Ok(());
            }
            let content = std::fs::read_to_string(&file)?;
            let hash = if is_parseable(&file) {
                println!("🧠 AST hash (ignores comments)");
                hash_ast(&file, &content)?
            } else {
                println!("📄 Raw hash");
                neex_core::ast_hasher::hash_raw(&content)?
            };
            println!("🔑 {}", hash);
        }

        Commands::Graph => show_graph(&cwd)?,
        Commands::Affected { package } => show_affected(&cwd, &package)?,

        Commands::Cache { action } => match action {
            CacheAction::Remote { action } => match action {
                RemoteAction::Login { s3, r2 } => remote_login(s3, r2).await?,
                RemoteAction::Status => remote_status().await?,
                RemoteAction::Disable => remote_toggle(false)?,
                RemoteAction::Enable => remote_toggle(true)?,
            },
            CacheAction::Clean { all } => cache_clean(&cwd, all).await?,
            CacheAction::Stats => cache_stats(&cwd)?,
        },

        Commands::Status => {
            let socket_path = get_socket_path();
            match send_request(&socket_path, DaemonRequest::Stats).await {
                Ok(DaemonResponse::Stats { cached_files, db_size }) => {
                    println!("📊 Daemon Status:");
                    println!("   Cached files: {}", cached_files);
                    println!("   DB size: {} bytes", db_size);
                }
                Err(_) => println!("❌ Daemon not running"),
                _ => {}
            }
        }
    }

    Ok(())
}

/// Interactive cloud login wizard
async fn remote_login(s3: bool, r2: bool) -> Result<()> {
    println!("☁️  Cloud Cache Setup");
    println!();

    let theme = ColorfulTheme::default();

    // Select provider if not specified
    let provider = if r2 {
        "Cloudflare R2"
    } else if s3 {
        "S3-Compatible"
    } else {
        let options = &["Cloudflare R2 (recommended)", "AWS S3", "MinIO", "Other S3-Compatible"];
        let selection = Select::with_theme(&theme)
            .with_prompt("Select your cloud provider")
            .items(options)
            .default(0)
            .interact()?;
        options[selection]
    };

    println!("   Provider: {}", provider);
    println!();

    // Get endpoint
    let default_endpoint = if provider.contains("R2") {
        "https://<account-id>.r2.cloudflarestorage.com"
    } else if provider.contains("AWS") {
        "https://s3.us-east-1.amazonaws.com"
    } else {
        "https://your-s3-endpoint.com"
    };

    let endpoint: String = Input::with_theme(&theme)
        .with_prompt("Endpoint URL")
        .with_initial_text(default_endpoint)
        .interact_text()?;

    let bucket: String = Input::with_theme(&theme)
        .with_prompt("Bucket name")
        .with_initial_text("neex-cache")
        .interact_text()?;

    let region: String = Input::with_theme(&theme)
        .with_prompt("Region")
        .with_initial_text("auto")
        .interact_text()?;

    let access_key: String = Input::with_theme(&theme)
        .with_prompt("Access Key ID")
        .interact_text()?;

    let secret_key: String = Password::with_theme(&theme)
        .with_prompt("Secret Access Key")
        .interact()?;

    // Save config
    let config = CloudConfig {
        s3: Some(S3Config {
            endpoint,
            bucket: bucket.clone(),
            region,
            access_key,
            secret_key,
            enabled: true,
        }),
    };

    save_config(&config)?;
    
    println!();
    println!("✅ Config saved to {:?}", get_config_path());
    println!();

    // Test connection
    println!("🔗 Testing connection...");
    match CloudCache::try_new() {
        Ok(Some(cloud)) => {
            match cloud.ping().await {
                Ok(true) => println!("✅ Connected to bucket: {}", bucket),
                Ok(false) => println!("⚠️  Bucket accessible but may have permission issues"),
                Err(e) => println!("❌ Connection failed: {}", e),
            }
        }
        Ok(None) => println!("⚠️  Cloud not configured"),
        Err(e) => println!("❌ Config error: {}", e),
    }

    Ok(())
}

/// Check cloud connection status
async fn remote_status() -> Result<()> {
    println!("☁️  Cloud Cache Status");
    println!();

    let config = load_config()?;
    
    match config.s3 {
        Some(s3) if !s3.endpoint.is_empty() => {
            println!("   Endpoint: {}", s3.endpoint);
            println!("   Bucket:   {}", s3.bucket);
            println!("   Region:   {}", s3.region);
            println!("   Enabled:  {}", if s3.enabled { "✅" } else { "❌" });
            println!();

            if s3.enabled {
                print!("   Testing connection... ");
                match CloudCache::try_new() {
                    Ok(Some(cloud)) => match cloud.ping().await {
                        Ok(true) => println!("✅ Connected"),
                        _ => println!("❌ Failed"),
                    },
                    _ => println!("❌ Not configured"),
                }
            }
        }
        _ => {
            println!("   ❌ Cloud cache not configured");
            println!();
            println!("   Run: neex cache remote login");
        }
    }

    Ok(())
}

/// Toggle cloud cache enabled/disabled
fn remote_toggle(enable: bool) -> Result<()> {
    let mut config = load_config()?;
    
    if let Some(ref mut s3) = config.s3 {
        s3.enabled = enable;
        save_config(&config)?;
        println!("{} Cloud cache {}", 
            if enable { "✅" } else { "❌" },
            if enable { "enabled" } else { "disabled" }
        );
    } else {
        println!("⚠️  Cloud cache not configured. Run: neex cache remote login");
    }

    Ok(())
}

/// Clean cache
async fn cache_clean(cwd: &PathBuf, all: bool) -> Result<()> {
    let theme = ColorfulTheme::default();

    if all {
        let confirm = Confirm::with_theme(&theme)
            .with_prompt("⚠️  Delete ALL cache (local + cloud)? This cannot be undone!")
            .default(false)
            .interact()?;

        if !confirm {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Clean local cache
    println!("🗑️  Cleaning local cache...");
    let runner = TaskRunner::new(cwd)?;
    runner.clear_cache()?;
    println!("   ✅ Local cache cleared");

    if all {
        // Clean cloud cache
        println!("🗑️  Cleaning cloud cache...");
        match CloudCache::try_new() {
            Ok(Some(cloud)) => {
                // Note: clear_all not implemented in rusty-s3 simple version
                println!("   ⚠️  Cloud cleanup requires manual bucket management");
            }
            _ => println!("   ⏭️  Cloud not configured"),
        }
    }

    println!();
    println!("✅ Cache cleaned!");
    Ok(())
}

/// Show cache statistics
fn cache_stats(cwd: &PathBuf) -> Result<()> {
    println!("📊 Cache Statistics");
    println!();

    // Local cache
    let cache_dir = cwd.join(".neex").join("cache");
    if cache_dir.exists() {
        let size = dir_size(&cache_dir)?;
        println!("   Local (sled): {} KB", size / 1024);
    } else {
        println!("   Local: Empty");
    }

    // Cloud config
    let config = load_config()?;
    match config.s3 {
        Some(s3) if s3.enabled => {
            println!("   Cloud: {} ({})", s3.bucket, if s3.enabled { "enabled" } else { "disabled" });
        }
        _ => println!("   Cloud: Not configured"),
    }

    println!();
    println!("   Config: {:?}", get_config_path());

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

/// Run script with tiered caching (L1→L2→L3→L4)
async fn run_script_tiered(cwd: &PathBuf, script: &str) -> Result<()> {
    let start = Instant::now();
    let runner = TaskRunner::new(cwd)?;

    let command = match runner.get_script(script)? {
        Some(cmd) => cmd,
        None => {
            println!("❌ Script '{}' not found in package.json", script);
            return Ok(());
        }
    };

    println!("🔨 neex run {}", script);
    println!("   Command: {}", command);

    // Calculate project hash
    let hasher = Hasher::new(cwd);
    let project_hash = hasher.global_hash()?;
    let cache_key = format!("{}:{}", script, &project_hash[..16]);
    println!("   Hash: {}...", &project_hash[..16]);

    // L1: Check local cache
    if let Some(cached) = runner.get_cached(&cache_key)? {
        let elapsed = start.elapsed();
        println!();
        println!("⚡ L1 HIT (local)");
        println!("─────────────────────────────────");
        runner.replay_output(&cached);
        println!("─────────────────────────────────");
        println!("✅ {} in {:?} (cached)", script, elapsed);
        return Ok(());
    }

    // L2: P2P (if daemon running with peers)
    // TODO: Integrate with PeerManager when daemon is running

    // L3: Cloud cache
    if let Ok(Some(cloud)) = CloudCache::try_new() {
        if let Ok(Some(data)) = cloud.download(&cache_key).await {
            println!();
            println!("☁️ L3 HIT (cloud)");
            // Store in local for next time
            if let Ok(output) = serde_json::from_slice::<neex_core::TaskOutput>(&data) {
                let _ = runner.store_cached(&cache_key, &output);
                println!("─────────────────────────────────");
                runner.replay_output(&output);
                println!("─────────────────────────────────");
                println!("✅ {} in {:?} (cloud cached)", script, start.elapsed());
                return Ok(());
            }
        }
    }

    // L4: Execute
    println!();
    println!("🔨 L4 MISS - Executing...");
    println!("─────────────────────────────────");
    let output = runner.execute(&command).await?;
    println!("─────────────────────────────────");

    // Store in local
    let mut output_with_hash = output.clone();
    output_with_hash.hash = cache_key.clone();
    runner.store_cached(&cache_key, &output_with_hash)?;

    // Upload to cloud in background (fire and forget)
    let json_data = serde_json::to_vec(&output_with_hash)?;
    CloudCache::upload_background(cache_key.clone(), json_data);

    let elapsed = start.elapsed();
    println!("✅ {} in {:?}", script, elapsed);

    Ok(())
}

/// Show dependency graph
fn show_graph(cwd: &PathBuf) -> Result<()> {
    let start = Instant::now();
    let graph = DepGraph::from_root(cwd)?;
    
    println!("🕸️  Dependency Graph");
    println!("📦 {} packages, 🔗 {} deps", graph.package_count(), graph.edge_count());
    println!();

    if graph.has_cycle() {
        println!("⚠️  Circular dependency detected!");
        return Ok(());
    }

    println!("📋 Workspaces:");
    for pkg in graph.packages() {
        println!("   • {} ({})", pkg.name, pkg.path.display());
    }
    println!();

    match graph.get_build_order() {
        Ok(order) => {
            println!("🔨 Build Order:");
            for (i, pkg) in order.iter().enumerate() {
                println!("   {}. {}", i + 1, pkg.name);
            }
        }
        Err(e) => println!("❌ {}", e),
    }
    
    println!();
    println!("✅ Built in {:?}", start.elapsed());
    Ok(())
}

fn show_affected(cwd: &PathBuf, package: &str) -> Result<()> {
    let graph = DepGraph::from_root(cwd)?;
    let affected = graph.get_affected(package);
    
    if affected.is_empty() {
        println!("❌ Package '{}' not found", package);
        return Ok(());
    }
    
    println!("📦 Affected by {}:", package);
    for pkg in &affected {
        println!("   • {}", pkg.name);
    }
    Ok(())
}

/// Parallel workspace execution
async fn run_all_parallel(cwd: &PathBuf, script: &str, concurrency: Option<usize>) -> Result<()> {
    println!("🚀 neex run-all {}", script);
    let start = Instant::now();

    let graph = DepGraph::from_root(cwd)?;
    if graph.package_count() == 0 {
        println!("❌ No workspaces found");
        return Ok(());
    }

    let build_order = graph.get_build_order()?;
    println!("📦 {} workspaces", build_order.len());
    println!();

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
    
    let tasks: Vec<SchedulerTask> = build_order.iter().map(|node| {
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
                println!("▶ {} → {}", pkg_name, command);
                let output = std::process::Command::new("sh")
                    .arg("-c").arg(command)
                    .current_dir(&full_path)
                    .output()?;
                
                if !output.status.success() {
                    return Err(anyhow::anyhow!("{} failed", pkg_name));
                }
                println!("✓ {}", pkg_name);
            } else {
                println!("⏭ {} (no {} script)", pkg_name, script_clone);
            }
            Ok(())
        })
    }).collect();

    let concurrency = concurrency.unwrap_or_else(|| {
        std::thread::available_parallelism().map(|p| p.get()).unwrap_or(4)
    });
    
    let scheduler = Scheduler::new(concurrency);
    let results = scheduler.execute(tasks).await?;

    let succeeded = results.iter().filter(|r| r.status == neex_core::TaskStatus::Completed).count();
    let failed = results.iter().filter(|r| r.status == neex_core::TaskStatus::Failed).count();
    
    println!();
    if failed == 0 {
        println!("✅ {} packages in {:?}", succeeded, start.elapsed());
    } else {
        println!("❌ {} ok, {} failed in {:?}", succeeded, failed, start.elapsed());
    }

    Ok(())
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
