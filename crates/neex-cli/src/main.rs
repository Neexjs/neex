//! Neex CLI - fast, polyglot monorepo task runner
//!
//!   neex build                 build every project (or the one you are in)
//!   neex run build test        several tasks
//!   neex build -F web...       web and everything it depends on
//!   neex test --affected       only projects changed since main
//!   neex why build             explain cache hits and misses
//!   neex build --dry=json      the plan, without running anything
//!   neex ls | graph | init | migrate | daemon

mod output;
mod tui;

use anyhow::{anyhow, bail, Context, Result};
use clap::{CommandFactory, Parser, ValueEnum};
use dialoguer::{theme::ColorfulTheme, Input, Password, Select};
use neex_core::artifacts::ArtifactStore;
use neex_core::config::{self, ConfigSource, RootConfig};
use neex_core::executor::{self, ContinueMode, Event, RunOptions, TaskStatus};
use neex_core::project::normalize_path;
use neex_core::{
    hash_ast, is_parseable, load_config, save_config, CloudConfig, RemoteCache, S3Config,
    TaskGraph, Workspace,
};
use output::{fmt_ms, LogOrder, OutputLogs, Printer};
use std::collections::{BTreeSet, HashSet};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[derive(Parser)]
#[command(
    name = "neex",
    version,
    about = "Fast, polyglot monorepo task runner",
    after_help = "EXAMPLES:\n  neex build                  Build every project (or the current one)\n  neex run lint test          Run several tasks\n  neex build -F web...        web and everything it depends on\n  neex test --affected        Only projects changed since main\n  neex why build              Explain why tasks hit or miss the cache\n  neex build --dry=json       Show the plan without running\n  neex ls                     List projects\n  neex init                   Add neex to this repo"
)]
struct Cli {
    /// Tasks to run, or a command: run, why, ls, graph, init, migrate, info, prune, daemon
    args: Vec<String>,

    /// Arguments after `--` are passed to the requested tasks
    #[arg(last = true)]
    pass_args: Vec<String>,

    /// Select projects: name, path (./apps/web), glob (@acme/*), `name...`
    /// (with deps), `...name` (with dependents), `!name` (exclude)
    #[arg(long, short = 'F', value_name = "PATTERN")]
    filter: Vec<String>,

    /// Run in every project, even when inside a project directory
    #[arg(long, short = 'a')]
    all: bool,

    /// Only projects changed since the base ref, and their dependents
    #[arg(long)]
    affected: bool,

    /// Base ref for --affected (default: GITHUB_BASE_REF, origin/main, main)
    #[arg(long, value_name = "REF")]
    base: Option<String>,

    /// Max tasks at once: a number, or a percentage of CPUs like `50%`
    #[arg(long, short = 'c', value_parser = parse_concurrency)]
    concurrency: Option<usize>,

    /// Ignore cached results (still refreshes the cache)
    #[arg(long)]
    force: bool,

    /// Do not read or write the cache
    #[arg(long)]
    no_cache: bool,

    /// Keep going after a failure: `dependencies-successful` or `always`
    #[arg(long = "continue", value_enum, num_args = 0..=1, default_missing_value = "always")]
    continue_mode: Option<ContinueArg>,

    /// Print the plan (`--dry` or `--dry=json`) without running anything
    #[arg(long, value_enum, num_args = 0..=1, default_missing_value = "text", require_equals = true)]
    dry: Option<DryMode>,

    /// Write a JSON run summary to .neex/runs/
    #[arg(long)]
    summarize: bool,

    /// `grouped` prints each task's output together (default on GitHub Actions)
    #[arg(long, value_enum, default_value = "auto")]
    log_order: LogOrder,

    /// Which output to show
    #[arg(long, value_enum, default_value = "full")]
    output_logs: OutputLogs,

    /// Interactive terminal UI
    #[arg(long)]
    tui: bool,

    /// Show the project graph
    #[arg(long)]
    graph: bool,

    /// List projects
    #[arg(long)]
    list: bool,

    /// Workspace information
    #[arg(long)]
    info: bool,

    /// Print a file's content hash
    #[arg(long, value_name = "FILE")]
    hash: Option<PathBuf>,

    /// Configure the S3/R2 remote cache
    #[arg(long)]
    login: bool,

    /// Disable the remote cache and forget its secret
    #[arg(long)]
    logout: bool,

    /// Delete the local cache
    #[arg(long)]
    prune: bool,

    // Deprecated aliases, kept so existing scripts keep working
    #[arg(long, short = 'r', hide = true)]
    raw: bool,
    #[arg(long, hide = true)]
    changed: bool,
    #[arg(long, hide = true)]
    prune_all: bool,
    #[arg(long, hide = true)]
    daemon_start: bool,
    #[arg(long, hide = true)]
    daemon_stop: bool,
    #[arg(long, hide = true)]
    symbols: bool,
    #[arg(long = "why", hide = true)]
    why_flag: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ContinueArg {
    Never,
    DependenciesSuccessful,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DryMode {
    Text,
    Json,
}

fn parse_concurrency(s: &str) -> Result<usize, String> {
    let cpus = executor::default_concurrency();
    let n = if let Some(p) = s.strip_suffix('%') {
        let pct: f64 = p
            .parse()
            .map_err(|_| format!("invalid percentage `{}`", s))?;
        if pct <= 0.0 {
            return Err("concurrency must be above 0%".into());
        }
        ((cpus as f64 * pct / 100.0).floor() as usize).max(1)
    } else {
        s.parse::<usize>()
            .map_err(|_| format!("invalid concurrency `{}`", s))?
    };
    if n == 0 {
        return Err("concurrency must be at least 1".into());
    }
    Ok(n)
}

fn main() {
    let code = match real_main() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("\x1b[31merror:\x1b[0m {:#}", e);
            1
        }
    };
    std::process::exit(code);
}

#[tokio::main]
async fn real_main() -> Result<i32> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;

    // Flag-style commands (kept for compatibility)
    if cli.graph {
        return cmd_graph(&cwd, &[]);
    }
    if cli.list {
        return cmd_ls(&cwd);
    }
    if cli.info {
        return cmd_info(&cwd).await;
    }
    if let Some(file) = &cli.hash {
        return hash_file(file);
    }
    if cli.login {
        return cloud_login().await;
    }
    if cli.logout {
        return cloud_logout();
    }
    if cli.prune || cli.prune_all {
        return cmd_prune(&cwd);
    }
    if cli.daemon_start {
        return cmd_daemon(&cwd, "start").await;
    }
    if cli.daemon_stop {
        return cmd_daemon(&cwd, "stop").await;
    }
    if let Some(p) = &cli.why_flag {
        return cmd_dependents(&cwd, p);
    }
    if cli.symbols {
        bail!(
            "--symbols is disabled in this release: symbol-level tracking can miss changes \
             (namespace imports, `export {{ a }}`, private helpers). Regular task caching is \
             safe and on by default."
        );
    }

    let Some(first) = cli.args.first().cloned() else {
        Cli::command().print_help()?;
        println!();
        return Ok(0);
    };
    let rest: Vec<String> = cli.args[1..].to_vec();

    match first.as_str() {
        "run" => {
            if rest.is_empty() {
                bail!("`neex run` needs at least one task, e.g. `neex run build`");
            }
            cmd_run(&cli, &cwd, rest).await
        }
        "why" => {
            if rest.is_empty() {
                bail!("`neex why` needs a task, e.g. `neex why build`");
            }
            cmd_why(&cli, &cwd, rest)
        }
        "ls" | "list" => cmd_ls(&cwd),
        "graph" => cmd_graph(&cwd, &rest),
        "init" => cmd_init(&cwd, cli.force),
        "migrate" => cmd_migrate(&cwd, cli.force),
        "info" => cmd_info(&cwd).await,
        "prune" | "clean" => cmd_prune(&cwd),
        "login" => cloud_login().await,
        "logout" => cloud_logout(),
        "daemon" => cmd_daemon(&cwd, rest.first().map(|s| s.as_str()).unwrap_or("start")).await,
        _ => cmd_run(&cli, &cwd, cli.args.clone()).await,
    }
}

// ═══════════════════════════════════════
// Project selection
// ═══════════════════════════════════════

fn is_glob(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[') || s.contains('{')
}

/// Projects matching one filter pattern (without `!` / `...`)
fn match_pattern(ws: &Workspace, pat: &str) -> Result<Vec<usize>> {
    // turbo-style `{./apps/*}` path globs
    let (pat, path_mode) = match pat.strip_prefix('{').and_then(|p| p.strip_suffix('}')) {
        Some(inner) => (inner, true),
        None => (pat, pat.starts_with("./") || pat.starts_with("../")),
    };
    if path_mode || (pat.contains('/') && !pat.starts_with('@')) {
        let norm = normalize_path(Path::new(pat.trim_start_matches("./")));
        if is_glob(&norm) {
            let g = neex_core::inputs::compile_glob(&norm)?.compile_matcher();
            return Ok((0..ws.projects.len())
                .filter(|&i| g.is_match(ws.projects[i].root_str()))
                .collect());
        }
        if let Some(i) = ws.find_project(pat) {
            return Ok(vec![i]);
        }
        // A path inside a project selects that project
        return Ok(ws
            .project_for_path(&ws.root.join(&norm))
            .into_iter()
            .collect());
    }
    if is_glob(pat) {
        let g = neex_core::inputs::compile_glob(pat)?.compile_matcher();
        return Ok((0..ws.projects.len())
            .filter(|&i| g.is_match(&ws.projects[i].name) || g.is_match(&ws.projects[i].ident))
            .collect());
    }
    Ok(ws.find_project(pat).into_iter().collect())
}

fn apply_filters(ws: &Workspace, filters: &[String]) -> Result<Vec<usize>> {
    let mut include: BTreeSet<usize> = BTreeSet::new();
    let mut exclude: BTreeSet<usize> = BTreeSet::new();
    let mut any_positive = false;
    for raw in filters {
        let (neg, body) = match raw.strip_prefix('!') {
            Some(b) => (true, b),
            None => (false, raw.as_str()),
        };
        let (dependents, body) = match body.strip_prefix("...") {
            Some(b) => (true, b),
            None => (false, body),
        };
        let (deps, body) = match body.strip_suffix("...") {
            Some(b) => (true, b),
            None => (false, body),
        };
        let matched = match_pattern(ws, body)?;
        if matched.is_empty() {
            bail!(
                "--filter `{}` matched no project (run `neex ls` to see project names)",
                raw
            );
        }
        let mut set: BTreeSet<usize> = matched.iter().copied().collect();
        for &m in &matched {
            if deps {
                set.extend(ws.transitive_deps(m));
            }
            if dependents {
                set.extend(ws.affected_by(m));
            }
        }
        if neg {
            exclude.extend(set);
        } else {
            any_positive = true;
            include.extend(set);
        }
    }
    if !any_positive {
        include = (0..ws.projects.len()).collect();
    }
    Ok(include.difference(&exclude).copied().collect())
}

fn affected_projects(ws: &Workspace, files: &[String]) -> Result<BTreeSet<usize>> {
    let globals: HashSet<String> = ws
        .global_input_files()
        .iter()
        .map(|p| normalize_path(p))
        .collect();
    let mut global_globs = globset::GlobSetBuilder::new();
    for g in &ws.config.global_inputs {
        global_globs.add(neex_core::inputs::compile_glob(g)?);
    }
    // Root-relative task inputs affect the project even when the changed
    // file lives outside every project directory.
    for task in ws.config.tasks.values().chain(
        ws.project_configs
            .iter()
            .filter_map(|c| c.as_ref())
            .flat_map(|c| c.tasks.values()),
    ) {
        for g in neex_core::inputs::InputFilter::root_globs(task.inputs.as_deref()) {
            global_globs.add(neex_core::inputs::compile_glob(&g)?);
        }
    }
    let global_globs = global_globs.build()?;
    if files
        .iter()
        .any(|f| globals.contains(f) || global_globs.is_match(f))
    {
        return Ok((0..ws.projects.len()).collect());
    }
    let mut out = BTreeSet::new();
    for f in files {
        if let Some(p) = ws.project_for_path(&ws.root.join(f)) {
            out.extend(ws.affected_by(p));
        }
    }
    Ok(out)
}

fn select_projects(ws: &Workspace, cwd: &Path, cli: &Cli) -> Result<Vec<usize>> {
    let all: Vec<usize> = (0..ws.projects.len()).collect();
    let affected_mode = cli.affected || cli.changed;
    let mut selected: Vec<usize> = if cli.all {
        all.clone()
    } else if !cli.filter.is_empty() {
        apply_filters(ws, &cli.filter)?
    } else if affected_mode {
        all.clone()
    } else {
        // Inside a project directory: just that project (plus its deps via ^)
        let here = if cwd != ws.root {
            ws.project_for_path(cwd)
                .filter(|&p| !ws.projects[p].root.as_os_str().is_empty())
        } else {
            None
        };
        match here {
            Some(p) => vec![p],
            None => all.clone(),
        }
    };
    if affected_mode {
        let changed = neex_core::scm::changed_files(&ws.root, cli.base.as_deref())?;
        let affected = affected_projects(ws, &changed.files)?;
        eprintln!(
            "neex: {} changed file(s) since {} → {} affected project(s)",
            changed.files.len(),
            changed.base,
            affected.len()
        );
        selected.retain(|p| affected.contains(p));
    }
    Ok(selected)
}

fn load_workspace(cwd: &Path) -> Result<Workspace> {
    let ws = Workspace::load(cwd)?;
    for w in &ws.warnings {
        eprintln!("\x1b[33mwarning:\x1b[0m {}", w);
    }
    if ws.has_cycle() {
        bail!("the project graph has a cycle; run `neex graph` to see it");
    }
    Ok(ws)
}

/// Build the task graph for a run, with the checks every command needs
fn prepare(cli: &Cli, cwd: &Path, tasks: &[String]) -> Result<(Workspace, TaskGraph)> {
    let ws = load_workspace(cwd)?;
    if ws.projects.is_empty() {
        bail!(
            "no projects found under {}.\nneex reads package.json `workspaces`, \
             pnpm-workspace.yaml, Cargo `[workspace]`, go.work, uv workspaces, and neex.json \
             `projects`. Run `neex init` to set one up.",
            ws.root.display()
        );
    }
    let selected = select_projects(&ws, cwd, cli)?;
    let seeds: Vec<usize> = selected
        .into_iter()
        .filter(|&p| tasks.iter().any(|t| ws.task_command(p, t).is_some()))
        .collect();
    if seeds.is_empty() {
        if cli.affected || cli.changed {
            return Ok((ws, TaskGraph::default()));
        }
        bail!(
            "no selected project has a `{}` task (run `neex ls` to see projects)",
            tasks.join("`, `")
        );
    }
    let graph = TaskGraph::build(&ws, tasks, &seeds)?;

    // A script that calls neex for the task it is part of would loop forever
    if let Ok(current) = std::env::var("NEEX_TASK") {
        if graph.get(&current).is_some() {
            bail!(
                "recursive invocation: `{}` runs neex, which would run `{}` again. Remove the \
                 neex call from that script (root scripts like `neex build` are fine; package \
                 scripts should run the real command).",
                current,
                current
            );
        }
    }
    Ok((ws, graph))
}

fn run_options(cli: &Cli, tasks: &[String], with_remote: bool) -> RunOptions {
    let remote = if with_remote && !cli.no_cache {
        match RemoteCache::from_user_config() {
            Ok(r) => r.map(Arc::new),
            Err(e) => {
                eprintln!("\x1b[33mwarning:\x1b[0m remote cache disabled: {:#}", e);
                None
            }
        }
    } else {
        None
    };
    RunOptions {
        concurrency: cli
            .concurrency
            .unwrap_or_else(executor::default_concurrency),
        force: cli.force,
        no_cache: cli.no_cache,
        continue_mode: match cli.continue_mode {
            None | Some(ContinueArg::Never) => ContinueMode::Never,
            Some(ContinueArg::DependenciesSuccessful) => ContinueMode::DependenciesSuccessful,
            Some(ContinueArg::Always) => ContinueMode::Always,
        },
        pass_args: cli.pass_args.clone(),
        requested_tasks: tasks.to_vec(),
        remote,
    }
}

// ═══════════════════════════════════════
// run
// ═══════════════════════════════════════

// Handle termination before returning so runner guards can reap task trees.
async fn shutdown_signal() -> Result<i32> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut terminate = signal(SignalKind::terminate())?;
        let mut hangup = signal(SignalKind::hangup())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => { result?; Ok(130) },
            _ = terminate.recv() => Ok(143),
            _ = hangup.recv() => Ok(129),
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await?;
        Ok(130)
    }
}

async fn cmd_run(cli: &Cli, cwd: &Path, tasks: Vec<String>) -> Result<i32> {
    let (ws, graph) = prepare(cli, cwd, &tasks)?;
    if graph.is_empty() {
        println!("neex: nothing to run");
        return Ok(0);
    }

    if let Some(mode) = cli.dry {
        return dry_run(&ws, &graph, &run_options(cli, &tasks, false), mode);
    }

    let opts = run_options(cli, &tasks, true);
    let task_ids: Vec<String> = graph
        .nodes
        .iter()
        .filter(|n| n.command.is_some())
        .map(|n| n.id.clone())
        .collect();
    let root = ws.root.clone();
    let ws = Arc::new(ws);
    let graph = Arc::new(graph);
    let (tx, rx) = mpsc::unbounded_channel();
    let handle = tokio::spawn(executor::run(Arc::clone(&ws), Arc::clone(&graph), opts, tx));

    let use_tui = cli.tui && std::io::stdout().is_terminal();
    let summary = if use_tui {
        match run_with_tui(handle, rx, &task_ids).await? {
            Some(s) => s,
            None => return Ok(130),
        }
    } else {
        let mut printer = Printer::new(cli.log_order, cli.output_logs, &task_ids);
        let mut rx = rx;
        let shutdown = shutdown_signal();
        tokio::pin!(shutdown);
        let summary = loop {
            tokio::select! {
                ev = rx.recv() => match ev {
                    Some(e) => printer.handle(&e),
                    None => break None,
                },
                signal = &mut shutdown => {
                    handle.abort();
                    let _ = handle.await;
                    eprintln!("\nneex: interrupted");
                    return signal;
                }
            }
        };
        let _: Option<()> = summary;
        let summary = handle.await.context("executor stopped unexpectedly")??;
        printer.summary(&summary);
        summary
    };

    if cli.summarize {
        let dir = root.join(".neex").join("runs");
        std::fs::create_dir_all(&dir)?;
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let path = dir.join(format!("{}.json", ts));
        std::fs::write(&path, serde_json::to_vec_pretty(&summary)?)?;
        println!("  Summary: {}", path.display());
    }
    Ok(summary.exit_code)
}

async fn run_with_tui(
    handle: tokio::task::JoinHandle<Result<executor::RunSummary>>,
    mut rx: mpsc::UnboundedReceiver<Event>,
    task_ids: &[String],
) -> Result<Option<executor::RunSummary>> {
    use tui::{TaskStatus as TuiStatus, TuiState};
    let state = Arc::new(Mutex::new(TuiState::default()));
    {
        let mut s = state.lock().unwrap();
        for id in task_ids {
            s.add_task(id);
        }
    }
    let tui_state = Arc::clone(&state);
    let tui_thread = std::thread::spawn(move || tui::run_tui(tui_state));
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(100));
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            signal = &mut shutdown => {
                handle.abort();
                let _ = handle.await;
                state.lock().unwrap_or_else(|p| p.into_inner()).should_quit = true;
                let _ = tui_thread.join();
                signal?;
                eprintln!("neex: interrupted");
                return Ok(None);
            }
            ev = rx.recv() => {
                let Some(ev) = ev else { break };
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                match ev {
                    Event::Started { id, command } => {
                        s.update_task(&id, TuiStatus::Running);
                        s.add_log(&id, &format!("$ {}", command));
                    }
                    Event::Line { id, line } => s.add_log(&id, &line.text),
                    Event::Replay { id, logs } => {
                        for l in logs {
                            s.add_log(&id, &l.text);
                        }
                    }
                    Event::Finished { id, summary } => {
                        let st = match summary.status {
                            TaskStatus::Success => TuiStatus::Completed(summary.duration_ms),
                            TaskStatus::CacheHit { .. } => TuiStatus::Cached(summary.duration_ms),
                            TaskStatus::Failed { exit_code } => TuiStatus::Failed(format!("exit {}", exit_code)),
                            TaskStatus::Skipped { reason } => TuiStatus::Failed(reason),
                            TaskStatus::NoCommand => TuiStatus::Completed(0),
                        };
                        s.update_task(&id, st);
                    }
                    Event::Warning(w) => {
                        if let Some(first) = task_ids.first() {
                            s.add_log(first, &format!("warning: {}", w));
                        }
                    }
                }
            }
            _ = tick.tick() => {
                let cancel = state.lock().map(|s| s.cancel_requested).unwrap_or(true);
                if cancel {
                    handle.abort();
                    let _ = handle.await;
                    let _ = tui_thread.join();
                    eprintln!("neex: cancelled");
                    return Ok(None);
                }
            }
        }
    }

    let summary = handle.await.context("executor stopped unexpectedly")??;
    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.should_quit = true;
    }
    let _ = tui_thread.join();
    // The TUI hid the logs; show failures and the summary on the normal screen
    let printer = Printer::new(LogOrder::Stream, OutputLogs::None, task_ids);
    printer.summary(&summary);
    Ok(Some(summary))
}

fn dry_run(ws: &Workspace, graph: &TaskGraph, opts: &RunOptions, mode: DryMode) -> Result<i32> {
    let planned = executor::plan(ws, graph, opts)?;
    let store = ArtifactStore::open(&ws.root)?;
    let status = |p: &executor::PlannedTask| -> &'static str {
        if p.command.is_none() {
            "NO_COMMAND"
        } else if p.persistent {
            "PERSISTENT"
        } else if !p.cacheable {
            "NOT_CACHED"
        } else if p
            .key
            .as_ref()
            .and_then(|k| store.get(k).ok().flatten())
            .is_some()
        {
            "HIT"
        } else {
            "MISS"
        }
    };
    match mode {
        DryMode::Json => {
            let items: Vec<serde_json::Value> = planned
                .iter()
                .zip(graph.nodes.iter())
                .map(|(p, n)| {
                    serde_json::json!({
                        "taskId": p.id,
                        "project": n.project_name,
                        "task": n.task,
                        "directory": ws.projects[n.project].root_str(),
                        "hash": p.key,
                        "cache": status(p),
                        "command": p.command,
                        "dependencies": p.deps,
                        "outputs": n.config.outputs.clone().unwrap_or_default(),
                        "inputs": p.inputs,
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "root": ws.root,
                    "config": ws.config.source,
                    "tasks": items,
                }))?
            );
        }
        DryMode::Text => {
            let w = planned.iter().map(|p| p.id.len()).max().unwrap_or(4);
            println!(
                "{:<w$}  {:<10}  {:<12}  COMMAND",
                "TASK",
                "CACHE",
                "HASH",
                w = w
            );
            for p in &planned {
                println!(
                    "{:<w$}  {:<10}  {:<12}  {}",
                    p.id,
                    status(p),
                    p.key.as_deref().map(|k| &k[..12]).unwrap_or("-"),
                    p.command.as_deref().unwrap_or("-"),
                    w = w
                );
            }
        }
    }
    Ok(0)
}

// ═══════════════════════════════════════
// why
// ═══════════════════════════════════════

fn cmd_why(cli: &Cli, cwd: &Path, tasks: Vec<String>) -> Result<i32> {
    let (ws, graph) = prepare(cli, cwd, &tasks)?;
    let opts = run_options(cli, &tasks, false);
    let planned = executor::plan(&ws, &graph, &opts)?;
    let store = ArtifactStore::open(&ws.root)?;
    for (p, n) in planned.iter().zip(graph.nodes.iter()) {
        if p.command.is_none() {
            continue;
        }
        print!("\x1b[1m{}\x1b[0m  ", p.id);
        if p.persistent {
            println!("runs every time (persistent task)");
            continue;
        }
        if !p.cacheable {
            println!(
                "runs every time (not cached; add `\"{}\": {{}}` to neex.json tasks to cache it)",
                n.task
            );
            continue;
        }
        let key = p.key.clone().unwrap_or_default();
        if store.get(&key)?.is_some() {
            println!("\x1b[36m⚡ cache hit\x1b[0m ({})", &key[..12]);
            continue;
        }
        println!("\x1b[33m✗ cache miss\x1b[0m ({})", &key[..12]);
        match (store.load_last(&p.id)?, &p.inputs) {
            (Some(last), Some(now)) => {
                let diff = now.diff(&last);
                if diff.is_empty() {
                    println!("    inputs match the last run, but its result is not in the cache (pruned, or it failed)");
                }
                for d in diff.iter().take(20) {
                    println!("    {}", d);
                }
                if diff.len() > 20 {
                    println!("    … and {} more", diff.len() - 20);
                }
            }
            _ => println!("    no previous run recorded"),
        }
    }
    Ok(0)
}

// ═══════════════════════════════════════
// ls / graph / info
// ═══════════════════════════════════════

fn cmd_ls(cwd: &Path) -> Result<i32> {
    let ws = load_workspace(cwd)?;
    if ws.projects.is_empty() {
        println!("no projects found under {}", ws.root.display());
        return Ok(0);
    }
    let nw = ws.projects.iter().map(|p| p.name.len()).max().unwrap_or(4);
    let pw = ws
        .projects
        .iter()
        .map(|p| p.root_str().len().max(1))
        .max()
        .unwrap_or(4);
    println!(
        "{:<nw$}  {:<7}  {:<pw$}  DEPENDS ON",
        "PROJECT",
        "LANG",
        "PATH",
        nw = nw,
        pw = pw
    );
    for (i, p) in ws.projects.iter().enumerate() {
        let deps: Vec<&str> = ws.deps[i]
            .iter()
            .map(|&d| ws.projects[d].name.as_str())
            .collect();
        let path = if p.root_str().is_empty() {
            ".".to_string()
        } else {
            p.root_str()
        };
        println!(
            "{:<nw$}  {:<7}  {:<pw$}  {}",
            p.name,
            p.language.as_str(),
            path,
            deps.join(", "),
            nw = nw,
            pw = pw
        );
    }
    println!(
        "\n{} projects · config: {}",
        ws.projects.len(),
        config_label(&ws.config)
    );
    Ok(0)
}

fn config_label(cfg: &RootConfig) -> &'static str {
    match cfg.source {
        ConfigSource::Neex => "neex.json",
        ConfigSource::Turbo => "turbo.json (compat)",
        ConfigSource::Default => "defaults (no neex.json)",
    }
}

fn cmd_graph(cwd: &Path, tasks: &[String]) -> Result<i32> {
    let ws = load_workspace(cwd)?;
    if !tasks.is_empty() {
        let all: Vec<usize> = (0..ws.projects.len())
            .filter(|&p| tasks.iter().any(|t| ws.task_command(p, t).is_some()))
            .collect();
        let g = TaskGraph::build(&ws, tasks, &all)?;
        for n in &g.nodes {
            if n.deps.is_empty() {
                println!("{}", n.id);
            } else {
                println!("{} ← {}", n.id, n.deps.join(", "));
            }
        }
        return Ok(0);
    }
    let order = ws.build_order()?;
    let edges: usize = ws.deps.iter().map(|d| d.len()).sum();
    println!(
        "{} projects, {} dependencies (build order):\n",
        ws.projects.len(),
        edges
    );
    for (n, &i) in order.iter().enumerate() {
        let deps: Vec<&str> = ws.deps[i]
            .iter()
            .map(|&d| ws.projects[d].name.as_str())
            .collect();
        if deps.is_empty() {
            println!("  {:>3}. {}", n + 1, ws.projects[i].name);
        } else {
            println!(
                "  {:>3}. {}  ← {}",
                n + 1,
                ws.projects[i].name,
                deps.join(", ")
            );
        }
    }
    Ok(0)
}

fn cmd_dependents(cwd: &Path, name: &str) -> Result<i32> {
    let ws = load_workspace(cwd)?;
    let Some(i) = ws.find_project(name) else {
        bail!("project `{}` not found", name);
    };
    println!("{} affects:", ws.projects[i].name);
    for a in ws.affected_by(i) {
        if a != i {
            println!("  → {}", ws.projects[a].name);
        }
    }
    Ok(0)
}

async fn cmd_info(cwd: &Path) -> Result<i32> {
    let ws = load_workspace(cwd)?;
    println!("📁 root      {}", ws.root.display());
    println!("⚙️  config    {}", config_label(&ws.config));
    let mut langs: std::collections::BTreeMap<&str, usize> = Default::default();
    for p in &ws.projects {
        *langs.entry(p.language.as_str()).or_default() += 1;
    }
    let langs: Vec<String> = langs.iter().map(|(l, n)| format!("{} {}", n, l)).collect();
    println!("📦 projects  {} ({})", ws.projects.len(), langs.join(", "));
    let store = ArtifactStore::open(&ws.root)?;
    let (records, bytes) = store.stats()?;
    println!(
        "💾 cache     {} entries, {:.1} MB",
        records,
        bytes as f64 / 1_048_576.0
    );
    match load_config()?.s3 {
        Some(s3) if s3.enabled => {
            let write = neex_core::remote::WritePolicy::from_env().allows_write();
            println!(
                "☁️  remote    {} ({})",
                s3.bucket,
                if write {
                    "read/write"
                } else {
                    "read-only here"
                }
            );
        }
        _ => println!("☁️  remote    not configured (neex login)"),
    }
    let socket = ws.root.join(".neex").join("daemon.sock");
    let daemon = send_request(&socket, neex_daemon::DaemonRequest::Stats)
        .await
        .is_ok();
    println!(
        "🔁 daemon    {}",
        if daemon {
            "running"
        } else {
            "off (not needed)"
        }
    );
    Ok(0)
}

fn hash_file(file: &Path) -> Result<i32> {
    if !file.exists() {
        bail!("{} not found", file.display());
    }
    let content = std::fs::read_to_string(file)?;
    let hash = if is_parseable(file) {
        hash_ast(file, &content)?
    } else {
        neex_core::ast_hasher::hash_raw(&content)?
    };
    println!("{}", hash);
    Ok(0)
}

fn cmd_prune(cwd: &Path) -> Result<i32> {
    let root = Workspace::find_root(cwd);
    let dir = root.join(".neex").join("cache");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
    }
    println!("✅ cache cleared ({})", dir.display());
    Ok(0)
}

// ═══════════════════════════════════════
// init / migrate
// ═══════════════════════════════════════

fn has_manifest(dir: &Path) -> bool {
    [
        "package.json",
        "Cargo.toml",
        "go.mod",
        "go.work",
        "pyproject.toml",
        "pnpm-workspace.yaml",
    ]
    .iter()
    .any(|f| dir.join(f).is_file())
}

fn ensure_gitignore(root: &Path) -> Result<bool> {
    let path = root.join(".gitignore");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing
        .lines()
        .any(|l| l.trim() == ".neex" || l.trim() == ".neex/")
    {
        return Ok(false);
    }
    let mut text = existing;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str("\n# neex cache\n.neex/\n");
    std::fs::write(&path, text)?;
    Ok(true)
}

fn cmd_init(cwd: &Path, force: bool) -> Result<i32> {
    if !has_manifest(cwd) {
        // Empty directory: scaffold a new project with create-neex
        println!("✨ No project here, creating a new one with create-neex…");
        let status = js_tool("pnpm")
            .args(["create", "neex"])
            .status()
            .or_else(|_| js_tool("npx").arg("create-neex@latest").status())
            .context("neither pnpm nor npx is available to run create-neex")?;
        return Ok(if status.success() {
            0
        } else {
            status.code().unwrap_or(1)
        });
    }

    let root = Workspace::find_root(cwd);
    let cfg_path = config::config_path(&root);
    if cfg_path.exists() && !force {
        println!("neex.json already exists at {}", cfg_path.display());
    } else if root.join("turbo.json").exists() && !force {
        println!(
            "Found turbo.json. neex already reads it as-is; run `neex migrate` to convert it."
        );
    } else {
        std::fs::write(&cfg_path, config::starter_config())?;
        println!("✅ wrote {}", cfg_path.display());
    }
    if ensure_gitignore(&root)? {
        println!("✅ added .neex/ to .gitignore");
    }
    let ws = load_workspace(&root)?;
    println!(
        "\nFound {} project(s). Next:\n  neex ls        see them\n  neex build     build everything (second run is cached)\n  neex why build explain cache hits and misses",
        ws.projects.len()
    );
    Ok(0)
}

fn cmd_migrate(cwd: &Path, force: bool) -> Result<i32> {
    let root = Workspace::find_root(cwd);
    let turbo = config::TURBO_CONFIG_FILES
        .iter()
        .map(|f| root.join(f))
        .find(|p| p.exists())
        .ok_or_else(|| anyhow!("no turbo.json found in {}", root.display()))?;
    let cfg = RootConfig::from_turbo_file(&turbo)?;
    for w in &cfg.warnings {
        eprintln!("\x1b[33mwarning:\x1b[0m {}", w);
    }
    let out = config::config_path(&root);
    if out.exists() && !force {
        bail!(
            "{} already exists (use --force to overwrite)",
            out.display()
        );
    }
    let mut obj = serde_json::Map::new();
    obj.insert(
        "$schema".into(),
        "https://raw.githubusercontent.com/Neexjs/neex/main/schema.json".into(),
    );
    if !cfg.global_env.is_empty() {
        obj.insert("globalEnv".into(), serde_json::to_value(&cfg.global_env)?);
    }
    if !cfg.global_inputs.is_empty() {
        obj.insert(
            "globalInputs".into(),
            serde_json::to_value(&cfg.global_inputs)?,
        );
    }
    obj.insert("tasks".into(), serde_json::to_value(&cfg.tasks)?);
    let text = serde_json::to_string_pretty(&serde_json::Value::Object(obj))? + "\n";
    // Round-trip check: what we write must parse as a neex.json
    RootConfig::from_neex_str(&text).context("generated neex.json does not validate")?;
    std::fs::write(&out, text)?;
    println!(
        "✅ wrote {} from {} ({} tasks). neex.json takes precedence; you can delete {} when ready.",
        out.display(),
        turbo.display(),
        cfg.tasks.len(),
        turbo.file_name().unwrap().to_string_lossy()
    );
    Ok(0)
}

/// `pnpm` / `npx` are `.cmd` shims on Windows and must go through cmd.exe
fn js_tool(name: &str) -> std::process::Command {
    if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", name]);
        c
    } else {
        std::process::Command::new(name)
    }
}

// ═══════════════════════════════════════
// Remote cache login
// ═══════════════════════════════════════

async fn cloud_login() -> Result<i32> {
    println!("☁️  Remote cache setup (S3-compatible: AWS S3, Cloudflare R2, MinIO)");
    let theme = ColorfulTheme::default();

    let providers = &["Cloudflare R2", "AWS S3", "MinIO", "Other"];
    let provider = Select::with_theme(&theme)
        .with_prompt("Provider")
        .items(providers)
        .default(0)
        .interact()?;
    let hint = match provider {
        0 => "https://<account-id>.r2.cloudflarestorage.com",
        1 => "https://s3.<region>.amazonaws.com",
        2 => "http://localhost:9000",
        _ => "https://",
    };

    let endpoint: String = Input::with_theme(&theme)
        .with_prompt("Endpoint")
        .with_initial_text(hint)
        .interact_text()?;
    let bucket: String = Input::with_theme(&theme)
        .with_prompt("Bucket")
        .with_initial_text("neex-cache")
        .interact_text()?;
    let region: String = Input::with_theme(&theme)
        .with_prompt("Region")
        .with_initial_text(if provider == 1 { "us-east-1" } else { "auto" })
        .interact_text()?;
    let access: String = Input::with_theme(&theme)
        .with_prompt("Access key ID")
        .interact_text()?;
    let secret: String = Password::with_theme(&theme)
        .with_prompt("Secret access key")
        .interact()?;

    let s3 = S3Config {
        endpoint,
        bucket: bucket.clone(),
        region,
        access_key: access,
        secret_key: secret,
        enabled: true,
    };
    save_config(&CloudConfig {
        s3: Some(s3.clone()),
    })?;
    println!(
        "✅ saved to {} (mode 600)",
        neex_core::get_config_path().display()
    );

    print!("Testing connection… ");
    let remote = RemoteCache::from_s3(&s3, neex_core::remote::WritePolicy::Never)?;
    match remote.ping().await {
        Ok(true) => println!("✓ {}", bucket),
        Ok(false) => println!("✗ the bucket answered with an error (check credentials)"),
        Err(e) => println!("✗ {}", e),
    }
    println!(
        "Note: only trusted CI runs upload by default. Set NEEX_REMOTE_CACHE_WRITE=always to \
         upload from this machine."
    );
    Ok(0)
}

fn cloud_logout() -> Result<i32> {
    let mut c = load_config()?;
    if let Some(ref mut s3) = c.s3 {
        s3.enabled = false;
        s3.secret_key.clear();
        s3.access_key.clear();
    }
    save_config(&c)?;
    println!("✅ remote cache disabled and credentials removed");
    Ok(0)
}

// ═══════════════════════════════════════
// Daemon (optional; never needed for correct results)
// ═══════════════════════════════════════

async fn cmd_daemon(cwd: &Path, action: &str) -> Result<i32> {
    let root = Workspace::find_root(cwd);
    let socket = root.join(".neex").join("daemon.sock");
    match action {
        "start" => {
            #[cfg(unix)]
            {
                println!(
                    "🔁 neex daemon watching {} (Ctrl-C to stop)",
                    root.display()
                );
                let mut server = neex_daemon::DaemonServer::new(&root)?;
                server.start().await?;
                Ok(0)
            }
            #[cfg(not(unix))]
            {
                bail!("the daemon is not supported on this platform (it is optional)")
            }
        }
        "stop" => {
            send_request(&socket, neex_daemon::DaemonRequest::Shutdown)
                .await
                .map(|_| ())
                .or_else(|e| {
                    // The daemon exits while answering; a closed socket means it stopped
                    if socket.exists() {
                        Err(e)
                    } else {
                        Ok(())
                    }
                })?;
            println!("✅ daemon stopped");
            Ok(0)
        }
        "status" => {
            let up = send_request(&socket, neex_daemon::DaemonRequest::Stats)
                .await
                .is_ok();
            println!("daemon {}", if up { "running" } else { "not running" });
            Ok(if up { 0 } else { 1 })
        }
        other => bail!("unknown daemon action `{}` (start, stop, status)", other),
    }
}

#[cfg(unix)]
async fn send_request(
    socket: &Path,
    req: neex_daemon::DaemonRequest,
) -> Result<neex_daemon::DaemonResponse> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let mut stream = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        tokio::net::UnixStream::connect(socket),
    )
    .await
    .map_err(|_| anyhow!("daemon did not answer"))??;
    stream
        .write_all(serde_json::to_string(&req)?.as_bytes())
        .await?;
    stream.write_all(b"\n").await?;
    let (r, _) = stream.into_split();
    let mut reader = BufReader::new(r);
    let mut line = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        reader.read_line(&mut line),
    )
    .await
    .map_err(|_| anyhow!("daemon did not answer"))??;
    Ok(serde_json::from_str(&line)?)
}

#[cfg(not(unix))]
async fn send_request(
    _socket: &Path,
    _req: neex_daemon::DaemonRequest,
) -> Result<neex_daemon::DaemonResponse> {
    bail!("the daemon is not supported on this platform")
}

#[allow(dead_code)]
fn _assert_fmt() -> String {
    fmt_ms(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrency_parsing() {
        assert_eq!(parse_concurrency("4").unwrap(), 4);
        assert!(parse_concurrency("0").is_err());
        assert!(parse_concurrency("abc").is_err());
        assert!(parse_concurrency("50%").unwrap() >= 1);
        assert!(parse_concurrency("0%").is_err());
    }

    #[test]
    fn affected_includes_root_relative_task_inputs() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("package.json"), r#"{"workspaces":["web"]}"#).unwrap();
        std::fs::create_dir(tmp.path().join("web")).unwrap();
        std::fs::write(tmp.path().join("web/package.json"), r#"{"name":"web"}"#).unwrap();
        std::fs::write(
            tmp.path().join("web/neex.json"),
            r#"{"tasks":{"build":{"inputs":["//configs/*.json"]}}}"#,
        )
        .unwrap();
        let ws = Workspace::load(tmp.path()).unwrap();
        assert_eq!(
            affected_projects(&ws, &["configs/base.json".into()]).unwrap(),
            BTreeSet::from([ws.find_project("web").unwrap()])
        );
    }

    #[test]
    fn cli_parses_tasks_filters_and_passthrough() {
        let cli = Cli::parse_from([
            "neex",
            "run",
            "build",
            "test",
            "-F",
            "web...",
            "--dry=json",
            "--",
            "--watch",
        ]);
        assert_eq!(cli.args, vec!["run", "build", "test"]);
        assert_eq!(cli.filter, vec!["web..."]);
        assert_eq!(cli.dry, Some(DryMode::Json));
        assert_eq!(cli.pass_args, vec!["--watch"]);
        let cli = Cli::parse_from(["neex", "build", "--continue"]);
        assert_eq!(cli.continue_mode, Some(ContinueArg::Always));
        let cli = Cli::parse_from([
            "neex",
            "build",
            "--continue=dependencies-successful",
            "--dry",
        ]);
        assert_eq!(cli.continue_mode, Some(ContinueArg::DependenciesSuccessful));
        assert_eq!(cli.dry, Some(DryMode::Text));
    }
}
