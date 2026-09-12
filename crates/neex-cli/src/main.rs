//! Neex CLI - Ultra-fast Monorepo Build Tool
//!
//! Task-First Design: Any task runs directly
//!
//! Usage:
//!   neex build              # Run build task (with TUI)
//!   neex dev --filter=web   # Run dev on web package
//!   neex test --all         # Run test on all packages
//!   neex build --raw        # Simple line output (CI-friendly)
//!   neex --graph            # Show dependency graph
//!   neex --login            # Setup cloud cache

mod tui;

use anyhow::Result;
use clap::Parser;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Password, Select};
use neex_core::{
    hash_ast, is_parseable, load_config, save_config, CloudCache, CloudConfig, DepGraph, Hasher,
    S3Config, Scheduler, SchedulerTask, SymbolCache, SymbolGraph, TaskRunner,
};
use neex_daemon::{DaemonRequest, DaemonResponse};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(unix)]
use tokio::net::UnixStream;
use tui::{TaskStatus as TuiTaskStatus, TuiState};

/// Neex - Ultra-fast monorepo build tool
#[derive(Parser)]
#[command(name = "neex", version, about = "Ultra-fast monorepo build tool")]
struct Cli {
    /// Task to run (build, dev, test, lint, etc.)
    task: Option<String>,

    // ═══════════════════════════════════════
    // Execution Flags
    // ═══════════════════════════════════════
    /// Filter by package name
    #[arg(long, short = 'f')]
    filter: Option<String>,

    /// Run on all packages (parallel)
    #[arg(long, short = 'a')]
    all: bool,

    /// Only changed packages
    #[arg(long)]
    changed: bool,

    /// Use symbol-level tracking (smart rebuild)
    #[arg(long)]
    symbols: bool,

    /// Concurrency limit
    #[arg(long, short = 'c')]
    concurrency: Option<usize>,

    // ═══════════════════════════════════════
    // Special Commands (Flags)
    // ═══════════════════════════════════════
    /// Show dependency graph
    #[arg(long)]
    graph: bool,

    /// List all packages
    #[arg(long)]
    list: bool,

    /// Show project info
    #[arg(long)]
    info: bool,

    /// Show why package is built
    #[arg(long)]
    why: Option<String>,

    /// Hash a file
    #[arg(long)]
    hash: Option<PathBuf>,

    // ═══════════════════════════════════════
    // Cache Commands
    // ═══════════════════════════════════════
    /// Login to cloud cache
    #[arg(long)]
    login: bool,

    /// Logout from cloud cache
    #[arg(long)]
    logout: bool,

    /// Clean cache (--prune-all for everything)
    #[arg(long)]
    prune: bool,

    /// Clean all cache (local + cloud)
    #[arg(long)]
    prune_all: bool,

    // ═══════════════════════════════════════
    // Daemon
    // ═══════════════════════════════════════
    /// Start daemon
    #[arg(long)]
    daemon_start: bool,

    /// Stop daemon
    #[arg(long)]
    daemon_stop: bool,

    // ═══════════════════════════════════════
    // Output Mode
    // ═══════════════════════════════════════
    /// Use simple line output instead of TUI (CI-friendly)
    #[arg(long, short = 'r')]
    raw: bool,
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

    // ═══════════════════════════════════════
    // Special Commands (priority order)
    // ═══════════════════════════════════════

    if cli.graph {
        return show_graph(&cwd);
    }

    if cli.list {
        return list_packages(&cwd);
    }

    if cli.info {
        return show_info(&cwd).await;
    }

    if let Some(pkg) = cli.why {
        return show_why(&cwd, &pkg);
    }

    if let Some(file) = cli.hash {
        return hash_file(&file);
    }

    if cli.login {
        return cloud_login().await;
    }

    if cli.logout {
        return cloud_logout();
    }

    if cli.prune || cli.prune_all {
        return prune_cache(&cwd, cli.prune_all).await;
    }

    if cli.daemon_start {
        println!("🚀 Start daemon with: cargo run -p neex-daemon");
        return Ok(());
    }

    if cli.daemon_stop {
        send_request(&get_socket_path(), DaemonRequest::Shutdown).await?;
        println!("✅ Daemon stopped");
        return Ok(());
    }

    // ═══════════════════════════════════════
    // Task Execution
    // ═══════════════════════════════════════

    let Some(task) = cli.task else {
        print_usage();
        return Ok(());
    };

    // Run task with flags
    let use_raw = cli.raw || !atty::is(atty::Stream::Stdout);

    if cli.symbols {
        run_symbols(&cwd, &task).await
    } else if cli.all {
        run_all(&cwd, &task, cli.concurrency, use_raw).await
    } else if cli.changed {
        run_changed(&cwd, &task, cli.concurrency, use_raw).await
    } else if let Some(pkg) = cli.filter {
        run_filtered(&cwd, &task, &pkg).await
    } else if task == "init" {
        run_init().await
    } else {
        run_task(&cwd, &task).await
    }
}

fn print_usage() {
    println!("neex - Ultra-fast monorepo build tool");
    println!();
    println!("USAGE:");
    println!("  neex <task>              Run a task (build, dev, test, etc.)");
    println!("  neex <task> --all        Run on all packages");
    println!("  neex <task> --filter=pkg Run on specific package");
    println!();
    println!("COMMANDS:");
    println!("  --graph      Show dependency graph");
    println!("  --list       List packages");
    println!("  --info       Project info");
    println!("  --login      Setup cloud cache");
    println!("  --prune      Clean cache");
    println!();
    println!("EXAMPLES:");
    println!("  neex build");
    println!("  neex dev --filter=web");
    println!("  neex test --all -c 4");
}

// ═══════════════════════════════════════
// Task Execution
// ═══════════════════════════════════════

async fn run_task(cwd: &PathBuf, task: &str) -> Result<()> {
    let start = Instant::now();
    let runner = TaskRunner::new(cwd)?;

    let command = match runner.get_script(task)? {
        Some(cmd) => cmd,
        None => {
            println!("❌ Task '{}' not found in package.json", task);
            return Ok(());
        }
    };

    print!("▶ {}", task);

    let hasher = Hasher::new(cwd);
    let hash = hasher.global_hash()?;
    let key = format!("{}:{}", task, &hash[..16]);

    // L1: Local
    if let Some(cached) = runner.get_cached(&key)? {
        println!(" ⚡ {}ms", start.elapsed().as_millis());
        runner.replay_output(&cached);
        return Ok(());
    }

    // L3: Cloud
    if let Ok(Some(cloud)) = CloudCache::try_new() {
        if let Ok(Some(data)) = cloud.download(&key).await {
            if let Ok(output) = serde_json::from_slice::<neex_core::TaskOutput>(&data) {
                let _ = runner.store_cached(&key, &output);
                println!(" ☁️ cloud");
                runner.replay_output(&output);
                return Ok(());
            }
        }
    }

    // L4: Execute
    println!();
    let output = runner.execute(&command).await?;

    let mut out = output.clone();
    out.hash = key.clone();
    runner.store_cached(&key, &out)?;

    // Background upload
    if let Ok(json) = serde_json::to_vec(&out) {
        CloudCache::upload_background(key, json);
    }

    println!("✓ {} {}ms", task, start.elapsed().as_millis());
    Ok(())
}

/// Smart rebuild using symbol-level tracking
async fn run_symbols(cwd: &PathBuf, task: &str) -> Result<()> {
    let start = Instant::now();

    println!("▶ {} --symbols", task);
    println!("  Building symbol graph...");

    // Build symbol graph
    let graph = match SymbolGraph::build(cwd) {
        Ok(g) => g,
        Err(e) => {
            println!("⚠️ Symbol graph failed: {}", e);
            println!("  Falling back to normal build...");
            return run_all(cwd, task, None, true).await; // fallback to raw mode
        }
    };

    let (pkgs, symbols, consumers) = graph.stats();
    println!(
        "  📦 {} packages, 🔣 {} symbols, 🔗 {} links",
        pkgs, symbols, consumers
    );

    // Load previous cache
    let cache_path = cwd.join(".neex").join("symbols.json");
    let old_cache = SymbolCache::load(&cache_path).unwrap_or_default();

    // Find changed symbols
    let changed = graph.get_changed_symbols(&old_cache);

    if changed.is_empty() {
        println!();
        println!(
            "⚡ No symbol changes detected ({} ms)",
            start.elapsed().as_millis()
        );
        return Ok(());
    }

    println!("  ⚠️ {} symbols changed", changed.len());

    // Get affected files
    let affected = graph.get_affected_files(&changed);

    if affected.is_empty() {
        println!("  No consumers affected");

        // Still save new cache
        let _ = graph.to_cache().save(&cache_path);

        println!();
        println!("✓ Symbol check ({} ms)", start.elapsed().as_millis());
        return Ok(());
    }

    println!("  🔨 {} files to rebuild", affected.len());

    for file in &affected {
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        println!("    → {}", name);
    }

    // Run task for affected packages
    let dep_graph = DepGraph::from_root(cwd)?;
    let mut rebuilt = 0;

    for file in &affected {
        // Find which package this file belongs to
        for pkg in dep_graph.packages() {
            let pkg_path = cwd.join(&pkg.path);
            if file.starts_with(&pkg_path) {
                run_task(&pkg_path, task).await?;
                rebuilt += 1;
                break;
            }
        }
    }

    // Save new cache
    let _ = graph.to_cache().save(&cache_path);

    println!();
    println!(
        "✓ {} packages rebuilt ({} ms)",
        rebuilt,
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn run_all(
    cwd: &PathBuf,
    task: &str,
    concurrency: Option<usize>,
    use_raw: bool,
) -> Result<()> {
    let start = Instant::now();
    let graph = DepGraph::from_root(cwd)?;

    if graph.package_count() == 0 {
        println!("❌ No packages");
        return Ok(());
    }

    let order = graph.get_build_order()?;
    let c = concurrency.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4)
    });

    if use_raw {
        // === Raw Mode: Simple line output ===
        println!("▶ {} --all ({} packages)", task, graph.package_count());

        let tasks = create_tasks(cwd, &order, task, &graph).await;
        let results = Scheduler::new(c).execute(tasks).await?;

        let ok = results
            .iter()
            .filter(|r| r.status == neex_core::TaskStatus::Completed)
            .count();
        let fail = results
            .iter()
            .filter(|r| r.status == neex_core::TaskStatus::Failed)
            .count();

        if fail == 0 {
            println!("✓ {} packages {}ms", ok, start.elapsed().as_millis());
        } else {
            println!("✗ {}ok {}fail {}ms", ok, fail, start.elapsed().as_millis());
        }
    } else {
        // === TUI Mode: Interactive UI with parallel execution ===
        let tui_state = Arc::new(Mutex::new(TuiState::default()));

        // Add all tasks to TUI
        {
            let mut state = tui_state.lock().unwrap();
            for node in &order {
                state.add_task(&node.name);
            }
        }

        // Clone for TUI thread
        let tui_state_clone = Arc::clone(&tui_state);

        // Start TUI in background thread
        let tui_handle = std::thread::spawn(move || {
            let _ = tui::run_tui(tui_state_clone);
        });

        // Create tasks with TUI updates
        let mut tui_tasks: Vec<SchedulerTask> = Vec::new();
        for node in order.iter() {
            let name = node.name.clone();
            let path = node.path.clone();
            let root = cwd.clone();
            let task_str = task.to_string();
            let tui_state_ref = Arc::clone(&tui_state);

            // Get dependencies
            let pkg_path = root.join(&path).join("package.json");
            let deps = if let Ok(content) = tokio::fs::read_to_string(&pkg_path).await {
                if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                    pkg.get("dependencies")
                        .and_then(|d| d.as_object())
                        .map(|d| {
                            d.keys()
                                .filter(|k| order.iter().any(|n| &n.name == *k))
                                .cloned()
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                } else {
                    vec![]
                }
            } else {
                vec![]
            };

            tui_tasks.push(SchedulerTask::new(name.clone(), deps, move || async move {
                let task_start = Instant::now();

                // Update TUI: Task started
                {
                    let mut state = tui_state_ref.lock().unwrap();
                    state.update_task(&name, TuiTaskStatus::Running);
                    state.add_log(&name, &format!("▶ Starting {}...", name));
                }

                let full_path = root.join(&path);
                let pkg_path = full_path.join("package.json");

                let result = if let Ok(content) = tokio::fs::read_to_string(&pkg_path).await {
                    let pkg: serde_json::Value = serde_json::from_str(&content)?;

                    if let Some(cmd) = pkg
                        .get("scripts")
                        .and_then(|s| s.get(&task_str))
                        .and_then(|c| c.as_str())
                    {
                        // Add log
                        {
                            let mut state = tui_state_ref.lock().unwrap();
                            state.add_log(&name, &format!("$ {}", cmd));
                        }

                        let output = tokio::process::Command::new("sh")
                            .arg("-c")
                            .arg(cmd)
                            .current_dir(&full_path)
                            .output()
                            .await;

                        match output {
                            Ok(out) => {
                                let stdout = String::from_utf8_lossy(&out.stdout);
                                let stderr = String::from_utf8_lossy(&out.stderr);

                                // Add output logs
                                {
                                    let mut state = tui_state_ref.lock().unwrap();
                                    for line in stdout.lines().take(20) {
                                        state.add_log(&name, line);
                                    }
                                    for line in stderr.lines().take(10) {
                                        state.add_log(&name, &format!("⚠ {}", line));
                                    }
                                }

                                out.status.success()
                            }
                            Err(e) => {
                                let mut state = tui_state_ref.lock().unwrap();
                                state.add_log(&name, &format!("Error: {}", e));
                                false
                            }
                        }
                    } else {
                        // No script found
                        let mut state = tui_state_ref.lock().unwrap();
                        state.add_log(&name, "⚡ Skipped (no script)");
                        true
                    }
                } else {
                    true
                };

                let elapsed = task_start.elapsed().as_millis() as u64;

                // Update TUI: Task completed
                {
                    let mut state = tui_state_ref.lock().unwrap();
                    if result {
                        state.update_task(&name, TuiTaskStatus::Completed(elapsed));
                        state.add_log(&name, &format!("✓ Done in {}ms", elapsed));
                    } else {
                        state.update_task(&name, TuiTaskStatus::Failed("Build failed".to_string()));
                        state.add_log(&name, "✗ Failed");
                        return Err(anyhow::anyhow!("Task {} failed", name));
                    }
                }

                Ok(())
            }));
        }

        // Execute tasks in parallel using Scheduler
        let results = Scheduler::new(c).execute(tui_tasks).await?;

        let ok = results
            .iter()
            .filter(|r| r.status == neex_core::TaskStatus::Completed)
            .count();
        let fail = results
            .iter()
            .filter(|r| r.status == neex_core::TaskStatus::Failed)
            .count();

        // Wait a moment for user to see results
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Signal TUI to quit
        {
            let mut state = tui_state.lock().unwrap();
            state.should_quit = true;
        }

        // Wait for TUI thread
        let _ = tui_handle.join();

        if fail == 0 {
            println!("✓ {} packages {}ms", ok, start.elapsed().as_millis());
        } else {
            println!("✗ {}ok {}fail {}ms", ok, fail, start.elapsed().as_millis());
        }
    }

    Ok(())
}

async fn run_changed(
    cwd: &PathBuf,
    task: &str,
    concurrency: Option<usize>,
    use_raw: bool,
) -> Result<()> {
    if use_raw {
        println!("▶ {} --changed (TODO: git integration)", task);
    }
    run_all(cwd, task, concurrency, use_raw).await
}

async fn run_filtered(cwd: &PathBuf, task: &str, pkg: &str) -> Result<()> {
    let graph = DepGraph::from_root(cwd)?;

    if let Some(p) = graph.get_package(pkg) {
        println!("▶ {} --filter={}", task, pkg);
        let path = cwd.join(&p.path);
        run_task(&path, task).await
    } else {
        println!("❌ Package '{}' not found", pkg);
        Ok(())
    }
}

async fn run_init() -> Result<()> {
    println!("✨ Initializing new Neex project...");

    let mut cmd = std::process::Command::new("pnpm");
    cmd.arg("create")
        .arg("neex")
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

    let status = cmd.status();

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Err(anyhow::anyhow!("Initialization failed")),
        Err(e) => {
            // Fallback to npx if pnpm is not found, though we prefer pnpm
            println!("⚠️ pnpm not found (error: {}), trying npx...", e);
            let mut cmd = std::process::Command::new("npx");
            cmd.arg("create-neex@latest")
                .stdin(std::process::Stdio::inherit())
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit());

            let status = cmd.status()?;
            if status.success() {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Initialization failed"))
            }
        }
    }
}

async fn create_tasks(
    cwd: &Path,
    order: &[&neex_core::WorkspaceNode],
    task: &str,
    graph: &DepGraph,
) -> Vec<SchedulerTask> {
    let mut deps: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

    for node in graph.packages() {
        let path = cwd.join(&node.path).join("package.json");
        if let Ok(content) = tokio::fs::read_to_string(&path).await {
            if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                let mut d = Vec::new();
                if let Some(obj) = pkg.get("dependencies").and_then(|x| x.as_object()) {
                    for name in obj.keys() {
                        if graph.get_package(name).is_some() {
                            d.push(name.clone());
                        }
                    }
                }
                deps.insert(node.name.clone(), d);
            }
        }
    }

    let root: Arc<PathBuf> = Arc::new(cwd.to_path_buf());
    let task_arc = Arc::new(task.to_string());

    order
        .iter()
        .map(|node| {
            let name = node.name.clone();
            let path = node.path.clone();
            let d = deps.get(&name).cloned().unwrap_or_default();
            let r = Arc::clone(&root);
            let t = Arc::clone(&task_arc);

            SchedulerTask::new(name.clone(), d, move || async move {
                let full = r.join(&path);
                let pkg_path = full.join("package.json");
                let content = tokio::fs::read_to_string(&pkg_path).await?;
                let pkg: serde_json::Value = serde_json::from_str(&content)?;

                if let Some(cmd) = pkg
                    .get("scripts")
                    .and_then(|s| s.get(t.as_str()))
                    .and_then(|c| c.as_str())
                {
                    print!("  {} ", name);
                    let out = tokio::process::Command::new("sh")
                        .arg("-c")
                        .arg(cmd)
                        .current_dir(&full)
                        .output()
                        .await?;

                    if out.status.success() {
                        println!("✓");
                    } else {
                        println!("✗");
                        return Err(anyhow::anyhow!("failed"));
                    }
                }
                Ok(())
            })
        })
        .collect()
}

// ═══════════════════════════════════════
// Special Commands
// ═══════════════════════════════════════

fn show_graph(cwd: &PathBuf) -> Result<()> {
    let g = DepGraph::from_root(cwd)?;

    println!("📦 {} packages, {} deps", g.package_count(), g.edge_count());

    if g.has_cycle() {
        println!("⚠️ Cycle detected!");
        return Ok(());
    }

    if let Ok(order) = g.get_build_order() {
        println!();
        for (i, p) in order.iter().enumerate() {
            println!("  {}. {}", i + 1, p.name);
        }
    }
    Ok(())
}

fn list_packages(cwd: &PathBuf) -> Result<()> {
    let g = DepGraph::from_root(cwd)?;
    for p in g.packages() {
        println!("{} ({})", p.name, p.path.display());
    }
    Ok(())
}

fn show_why(cwd: &PathBuf, pkg: &str) -> Result<()> {
    let g = DepGraph::from_root(cwd)?;
    let affected = g.get_affected(pkg);

    if affected.is_empty() {
        println!("❌ '{}' not found", pkg);
        return Ok(());
    }

    println!("{} affects:", pkg);
    for p in &affected {
        if p.name != pkg {
            println!("  → {}", p.name);
        }
    }
    Ok(())
}

async fn show_info(cwd: &PathBuf) -> Result<()> {
    // Packages
    let g = DepGraph::from_root(cwd);
    match g {
        Ok(g) => println!("📦 {} packages", g.package_count()),
        Err(_) => println!("📦 not a monorepo"),
    }

    // Cache
    let cache = cwd.join(".neex").join("cache");
    if cache.exists() {
        let size = dir_size(&cache)?;
        println!("💾 {} KB cache", size / 1024);
    } else {
        println!("💾 no cache");
    }

    // Cloud
    let config = load_config()?;
    match config.s3 {
        Some(s3) if s3.enabled => println!("☁️ {} ✓", s3.bucket),
        Some(_) => println!("☁️ disabled"),
        None => println!("☁️ not configured"),
    }

    // Daemon
    print!("🚀 ");
    match send_request(&get_socket_path(), DaemonRequest::Stats).await {
        Ok(_) => println!("daemon running"),
        Err(_) => println!("daemon stopped"),
    }

    Ok(())
}

fn hash_file(file: &PathBuf) -> Result<()> {
    if !file.exists() {
        println!("❌ not found");
        return Ok(());
    }
    let content = std::fs::read_to_string(file)?;
    let hash = if is_parseable(file) {
        hash_ast(file, &content)?
    } else {
        neex_core::ast_hasher::hash_raw(&content)?
    };
    println!("{}", hash);
    Ok(())
}

// ═══════════════════════════════════════
// Cache Commands
// ═══════════════════════════════════════

async fn cloud_login() -> Result<()> {
    println!("☁️ Cloud Setup");
    let theme = ColorfulTheme::default();

    let providers = &["Cloudflare R2", "AWS S3", "MinIO", "Other"];
    let _ = Select::with_theme(&theme)
        .with_prompt("Provider")
        .items(providers)
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
    let access: String = Input::with_theme(&theme)
        .with_prompt("Access Key")
        .interact_text()?;
    let secret: String = Password::with_theme(&theme)
        .with_prompt("Secret Key")
        .interact()?;

    save_config(&CloudConfig {
        s3: Some(S3Config {
            endpoint,
            bucket: bucket.clone(),
            region,
            access_key: access,
            secret_key: secret,
            enabled: true,
        }),
    })?;

    println!("✅ Saved");

    print!("Testing... ");
    match CloudCache::try_new() {
        Ok(Some(c)) => match c.ping().await {
            Ok(true) => println!("✓ {}", bucket),
            _ => println!("✗"),
        },
        _ => println!("✗"),
    }
    Ok(())
}

fn cloud_logout() -> Result<()> {
    let mut c = load_config()?;
    if let Some(ref mut s3) = c.s3 {
        s3.enabled = false;
    }
    save_config(&c)?;
    println!("✅ Logged out");
    Ok(())
}

async fn prune_cache(cwd: &PathBuf, all: bool) -> Result<()> {
    if all {
        let theme = ColorfulTheme::default();
        if !Confirm::with_theme(&theme)
            .with_prompt("Delete ALL?")
            .default(false)
            .interact()?
        {
            return Ok(());
        }
    }

    TaskRunner::new(cwd)?.clear_cache()?;
    println!("✅ Cache cleared");
    Ok(())
}

// ═══════════════════════════════════════
// Helpers
// ═══════════════════════════════════════

fn dir_size(path: &PathBuf) -> Result<u64> {
    let mut size = 0;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let e = entry?;
            let m = e.metadata()?;
            if m.is_file() {
                size += m.len();
            } else if m.is_dir() {
                size += dir_size(&e.path())?;
            }
        }
    }
    Ok(size)
}

#[cfg(unix)]
async fn send_request(socket: &PathBuf, req: DaemonRequest) -> Result<DaemonResponse> {
    let mut stream = UnixStream::connect(socket).await?;
    stream
        .write_all(serde_json::to_string(&req)?.as_bytes())
        .await?;
    stream.write_all(b"\n").await?;

    let (r, _) = stream.into_split();
    let mut reader = BufReader::new(r);
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    Ok(serde_json::from_str(&line)?)
}

#[cfg(windows)]
async fn send_request(_socket: &PathBuf, _req: DaemonRequest) -> Result<DaemonResponse> {
    anyhow::bail!("Daemon mode is not supported on Windows")
}
