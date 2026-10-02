//! Executor - runs a task graph with caching
//!
//! One code path for every mode (stream, grouped CI logs, TUI): the
//! executor emits [`Event`]s and the CLI decides how to show them.
//!
//! Guarantees:
//! - a task starts only after all its dependencies succeeded (or hit)
//! - a failed task is never cached; its dependents are skipped
//! - the run's exit code is the highest task exit code (never 0 on failure)
//! - projects in the same lock group never build concurrently
//! - remote uploads finish (with a timeout) before `run` returns

use crate::artifacts::{ArtifactStore, LogLine, Stream, TaskRecord};
use crate::remote::RemoteCache;
use crate::runner::{self, LineSink};
use crate::task_graph::{TaskGraph, TaskNode};
use crate::task_hash::{HashInputs, TaskHasher};
use crate::workspace::Workspace;
use anyhow::Result;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex as AsyncMutex, Semaphore};
use tokio::task::JoinSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContinueMode {
    /// Stop starting new tasks after the first failure (default)
    Never,
    /// Keep running tasks whose dependencies all succeeded
    DependenciesSuccessful,
    /// Run everything, even tasks whose dependencies failed
    Always,
}

#[derive(Clone)]
pub struct RunOptions {
    pub concurrency: usize,
    /// Ignore cache reads (still writes)
    pub force: bool,
    /// Neither read nor write the cache
    pub no_cache: bool,
    pub continue_mode: ContinueMode,
    /// Extra args appended to the commands of the requested tasks
    pub pass_args: Vec<String>,
    /// Task names the user asked for (pass_args only go to these)
    pub requested_tasks: Vec<String>,
    pub remote: Option<Arc<RemoteCache>>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            concurrency: default_concurrency(),
            force: false,
            no_cache: false,
            continue_mode: ContinueMode::Never,
            pass_args: vec![],
            requested_tasks: vec![],
            remote: None,
        }
    }
}

pub fn default_concurrency() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4)
        .max(1)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum TaskStatus {
    Success,
    CacheHit {
        source: CacheSource,
    },
    Failed {
        exit_code: i32,
    },
    Skipped {
        reason: String,
    },
    /// The project has no command for this task
    NoCommand,
}

impl TaskStatus {
    pub fn ok(&self) -> bool {
        matches!(
            self,
            TaskStatus::Success | TaskStatus::CacheHit { .. } | TaskStatus::NoCommand
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheSource {
    Local,
    Remote,
}

#[derive(Debug, Clone)]
pub enum Event {
    Started {
        id: String,
        command: String,
    },
    /// A live output line
    Line {
        id: String,
        line: LogLine,
    },
    /// Output replayed from the cache
    Replay {
        id: String,
        logs: Vec<LogLine>,
    },
    Finished {
        id: String,
        summary: TaskSummary,
    },
    Warning(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskSummary {
    pub id: String,
    pub project: String,
    pub task: String,
    #[serde(flatten)]
    pub status: TaskStatus,
    pub duration_ms: u64,
    pub key: Option<String>,
    pub command: Option<String>,
    /// Why the cache missed, compared with the last recorded run
    pub miss_reasons: Vec<String>,
    /// Output lines (only kept for failed tasks)
    #[serde(skip)]
    pub failure_logs: Vec<LogLine>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub tasks: Vec<TaskSummary>,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub cached: usize,
    pub executed: usize,
    pub failed: usize,
}

/// Planned key and inputs for every node, computed before anything runs
#[derive(Debug, Clone, Serialize)]
pub struct PlannedTask {
    pub id: String,
    pub key: Option<String>,
    #[serde(skip)]
    pub inputs: Option<HashInputs>,
    pub command: Option<String>,
    pub cacheable: bool,
    pub persistent: bool,
    pub deps: Vec<String>,
}

/// Compute cache keys for the whole graph (dependencies first)
pub fn plan(ws: &Workspace, graph: &TaskGraph, opts: &RunOptions) -> Result<Vec<PlannedTask>> {
    let hasher = TaskHasher::new(ws)?;
    let mut keys: BTreeMap<String, String> = BTreeMap::new();
    let mut out = Vec::with_capacity(graph.len());
    for node in &graph.nodes {
        let command = command_line(node, opts);
        let (key, inputs) = if node.persistent() {
            (None, None)
        } else if node.command.is_none() {
            // A no-op still forwards its dependencies' keys to dependents
            let mut h = blake3::Hasher::new();
            h.update(b"noop");
            for d in &node.deps {
                if let Some(k) = keys.get(d) {
                    h.update(k.as_bytes());
                }
            }
            (Some(h.finalize().to_hex().to_string()), None)
        } else {
            let mut inputs = hasher.inputs(node, &keys)?;
            if let Some(c) = &command {
                if !opts.pass_args.is_empty() && opts.requested_tasks.contains(&node.task) {
                    inputs.command = format!("{} {}", inputs.command, c);
                }
            }
            (Some(inputs.key()), Some(inputs))
        };
        if let Some(k) = &key {
            keys.insert(node.id.clone(), k.clone());
        }
        out.push(PlannedTask {
            id: node.id.clone(),
            key,
            inputs,
            command,
            cacheable: node.cacheable(),
            persistent: node.persistent(),
            deps: node.deps.clone(),
        });
    }
    Ok(out)
}

fn command_line(node: &TaskNode, opts: &RunOptions) -> Option<String> {
    let base = node.command.as_ref()?.shell_line();
    if !opts.pass_args.is_empty() && opts.requested_tasks.contains(&node.task) {
        let args: Vec<String> = opts.pass_args.iter().map(|a| shell_quote(a)).collect();
        let sep = match &node.command {
            Some(crate::project::TaskCommand::Script {
                pm: crate::project::PackageManager::Npm,
                ..
            }) => " -- ",
            _ => " ",
        };
        Some(format!("{}{}{}", base, sep, args.join(" ")))
    } else {
        Some(base)
    }
}

fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:@,+".contains(c))
    {
        s.to_string()
    } else if cfg!(windows) {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

#[derive(Clone)]
enum State {
    Pending,
    Running,
    Done(TaskStatus),
}

/// Run the graph. Events are sent to `events`; the returned summary has one
/// entry per node in graph order.
pub async fn run(
    ws: Arc<Workspace>,
    graph: Arc<TaskGraph>,
    opts: RunOptions,
    events: mpsc::UnboundedSender<Event>,
) -> Result<RunSummary> {
    let start = Instant::now();
    let planned = {
        let ws = Arc::clone(&ws);
        let graph = Arc::clone(&graph);
        let opts = opts.clone();
        tokio::task::spawn_blocking(move || plan(&ws, &graph, &opts)).await??
    };
    let planned = Arc::new(planned);
    let store = Arc::new(ArtifactStore::open(&ws.root)?);

    let semaphore = Arc::new(Semaphore::new(opts.concurrency.max(1)));
    let mut lock_groups: HashMap<String, Arc<AsyncMutex<()>>> = HashMap::new();
    for node in &graph.nodes {
        if let Some(g) = &ws.projects[node.project].lock_group {
            lock_groups
                .entry(g.clone())
                .or_insert_with(|| Arc::new(AsyncMutex::new(())));
        }
    }

    let n = graph.len();
    let mut state: Vec<State> = vec![State::Pending; n];
    let mut summaries: Vec<Option<TaskSummary>> = vec![None; n];
    let index: HashMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, nd)| (nd.id.as_str(), i))
        .collect();
    let mut joins: JoinSet<(usize, TaskSummary)> = JoinSet::new();
    let uploads: Arc<AsyncMutex<JoinSet<()>>> = Arc::new(AsyncMutex::new(JoinSet::new()));
    let mut stop_starting = false;

    loop {
        // Start or skip everything that is ready
        let mut progressed = true;
        while progressed {
            progressed = false;
            for i in 0..n {
                if !matches!(state[i], State::Pending) {
                    continue;
                }
                let node = &graph.nodes[i];
                let dep_states: Vec<&State> = node
                    .deps
                    .iter()
                    .filter_map(|d| index.get(d.as_str()).map(|&j| &state[j]))
                    .collect();
                if dep_states
                    .iter()
                    .any(|s| matches!(s, State::Pending | State::Running))
                {
                    continue;
                }
                let failed_dep = node.deps.iter().find(|d| {
                    index
                        .get(d.as_str())
                        .map(|&j| matches!(&state[j], State::Done(s) if !s.ok()))
                        .unwrap_or(false)
                });
                let skip_reason = if stop_starting {
                    Some("cancelled after an earlier failure".to_string())
                } else if let (Some(d), false) =
                    (failed_dep, opts.continue_mode == ContinueMode::Always)
                {
                    Some(format!("dependency `{}` did not succeed", d))
                } else {
                    None
                };
                if let Some(reason) = skip_reason {
                    let status = TaskStatus::Skipped { reason };
                    let summary = TaskSummary {
                        id: node.id.clone(),
                        project: node.project_name.clone(),
                        task: node.task.clone(),
                        status: status.clone(),
                        duration_ms: 0,
                        key: planned[i].key.clone(),
                        command: planned[i].command.clone(),
                        miss_reasons: vec![],
                        failure_logs: vec![],
                    };
                    let _ = events.send(Event::Finished {
                        id: node.id.clone(),
                        summary: summary.clone(),
                    });
                    summaries[i] = Some(summary);
                    state[i] = State::Done(status);
                    progressed = true;
                    continue;
                }

                // Leave queued tasks Pending so a failure can cancel them.
                // Spawning all ready nodes at once hides semaphore waiters
                // from the scheduler and lets them run after fail-fast.
                if !node.persistent()
                    && state
                        .iter()
                        .enumerate()
                        .filter(|(j, s)| {
                            matches!(s, State::Running) && !graph.nodes[*j].persistent()
                        })
                        .count()
                        >= opts.concurrency.max(1)
                {
                    continue;
                }

                state[i] = State::Running;
                progressed = true;
                let ctx = TaskCtx {
                    ws: Arc::clone(&ws),
                    graph: Arc::clone(&graph),
                    planned: Arc::clone(&planned),
                    store: Arc::clone(&store),
                    semaphore: Arc::clone(&semaphore),
                    lock: ws.projects[node.project]
                        .lock_group
                        .as_ref()
                        .and_then(|g| lock_groups.get(g).cloned()),
                    opts: opts.clone(),
                    events: events.clone(),
                    uploads: Arc::clone(&uploads),
                };
                joins.spawn(async move {
                    let summary = run_one(ctx, i).await;
                    (i, summary)
                });
            }
        }

        if joins.is_empty() {
            break;
        }
        match joins.join_next().await {
            Some(Ok((i, summary))) => {
                if !summary.status.ok() && opts.continue_mode == ContinueMode::Never {
                    stop_starting = true;
                }
                state[i] = State::Done(summary.status.clone());
                let _ = events.send(Event::Finished {
                    id: summary.id.clone(),
                    summary: summary.clone(),
                });
                summaries[i] = Some(summary);
            }
            Some(Err(e)) => {
                return Err(anyhow::anyhow!("task panicked: {}", e));
            }
            None => break,
        }
    }

    // Let remote uploads finish, but never hang the run on them
    {
        let mut ups = uploads.lock().await;
        let deadline = tokio::time::sleep(Duration::from_secs(60));
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                r = ups.join_next() => { if r.is_none() { break; } }
                _ = &mut deadline => {
                    let _ = events.send(Event::Warning("remote cache upload timed out".into()));
                    ups.abort_all();
                    break;
                }
            }
        }
    }

    let tasks: Vec<TaskSummary> = summaries.into_iter().flatten().collect();
    let exit_code = tasks
        .iter()
        .filter_map(|t| match &t.status {
            TaskStatus::Failed { exit_code } => Some(if *exit_code > 0 { *exit_code } else { 1 }),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let cached = tasks
        .iter()
        .filter(|t| matches!(t.status, TaskStatus::CacheHit { .. }))
        .count();
    let executed = tasks
        .iter()
        .filter(|t| matches!(t.status, TaskStatus::Success | TaskStatus::Failed { .. }))
        .count();
    let failed = tasks
        .iter()
        .filter(|t| matches!(t.status, TaskStatus::Failed { .. }))
        .count();
    Ok(RunSummary {
        tasks,
        exit_code,
        duration_ms: start.elapsed().as_millis() as u64,
        cached,
        executed,
        failed,
    })
}

struct TaskCtx {
    ws: Arc<Workspace>,
    graph: Arc<TaskGraph>,
    planned: Arc<Vec<PlannedTask>>,
    store: Arc<ArtifactStore>,
    semaphore: Arc<Semaphore>,
    lock: Option<Arc<AsyncMutex<()>>>,
    opts: RunOptions,
    events: mpsc::UnboundedSender<Event>,
    uploads: Arc<AsyncMutex<JoinSet<()>>>,
}

async fn run_one(ctx: TaskCtx, i: usize) -> TaskSummary {
    let node = &ctx.graph.nodes[i];
    let plan = &ctx.planned[i];
    let started = Instant::now();
    let mut summary = TaskSummary {
        id: node.id.clone(),
        project: node.project_name.clone(),
        task: node.task.clone(),
        status: TaskStatus::NoCommand,
        duration_ms: 0,
        key: plan.key.clone(),
        command: plan.command.clone(),
        miss_reasons: vec![],
        failure_logs: vec![],
    };
    let Some(command) = plan.command.clone() else {
        return summary;
    };
    let project_root = ctx.ws.projects[node.project].abs_root(&ctx.ws.root);
    let use_cache = plan.cacheable && !ctx.opts.no_cache;

    // Cache lookup
    if use_cache && !ctx.opts.force {
        if let Some(key) = &plan.key {
            match lookup(&ctx, key).await {
                Ok(Some((record, source))) => {
                    let store = Arc::clone(&ctx.store);
                    let root = project_root.clone();
                    let rec = record.clone();
                    let restored = tokio::task::spawn_blocking(move || {
                        rec.validate_inputs()?;
                        store.restore(&rec, &root)
                    })
                    .await;
                    match restored {
                        Ok(Ok(_)) => {
                            let _ = ctx.events.send(Event::Replay {
                                id: node.id.clone(),
                                logs: record.logs.clone(),
                            });
                            let _ = ctx.store.save_last(&node.id, &record.inputs);
                            summary.status = TaskStatus::CacheHit { source };
                            summary.duration_ms = started.elapsed().as_millis() as u64;
                            return summary;
                        }
                        Ok(Err(e)) => {
                            let _ = ctx.events.send(Event::Warning(format!(
                                "{}: cache restore failed, running instead: {:#}",
                                node.id, e
                            )));
                        }
                        Err(e) => {
                            let _ = ctx.events.send(Event::Warning(format!(
                                "{}: cache restore failed, running instead: {}",
                                node.id, e
                            )));
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    let _ = ctx.events.send(Event::Warning(format!(
                        "{}: cache read failed: {:#}",
                        node.id, e
                    )));
                }
            }
        }
    }

    // Explain the miss against the last recorded run
    if let (Some(inputs), true) = (&plan.inputs, use_cache) {
        if let Ok(Some(last)) = ctx.store.load_last(&node.id) {
            summary.miss_reasons = inputs.diff(&last);
        }
    }

    // Concurrency: persistent tasks don't hold a slot, so a dev server can
    // never starve the other tasks
    let _permit = if node.persistent() {
        None
    } else {
        Some(
            ctx.semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed"),
        )
    };
    let _group = match &ctx.lock {
        Some(l) => Some(l.clone().lock_owned().await),
        None => None,
    };

    let _ = ctx.events.send(Event::Started {
        id: node.id.clone(),
        command: command.clone(),
    });

    let mut env: HashMap<String, String> = HashMap::new();
    env.insert("NEEX_TASK".into(), node.id.clone());
    env.insert("NEEX_PROJECT".into(), node.project_name.clone());
    let sink_events = ctx.events.clone();
    let sink_id = node.id.clone();
    let sink: LineSink = Arc::new(move |line: &LogLine| {
        let _ = sink_events.send(Event::Line {
            id: sink_id.clone(),
            line: line.clone(),
        });
    });

    let result = runner::execute(&command, &project_root, &env, Some(sink)).await;
    summary.duration_ms = started.elapsed().as_millis() as u64;

    let result = match result {
        Ok(r) => r,
        Err(e) => {
            summary.status = TaskStatus::Failed { exit_code: 1 };
            summary.failure_logs = vec![LogLine {
                stream: Stream::Stderr,
                text: format!("failed to start `{}`: {}", command, e),
            }];
            return summary;
        }
    };

    if !result.success() {
        summary.status = TaskStatus::Failed {
            exit_code: result.exit_code,
        };
        summary.failure_logs = result.logs;
        return summary;
    }
    summary.status = TaskStatus::Success;

    // Store (success only)
    if use_cache {
        if let (Some(key), Some(inputs)) = (plan.key.clone(), plan.inputs.clone()) {
            let store = Arc::clone(&ctx.store);
            let outputs = node.config.outputs.clone().unwrap_or_default();
            let id = node.id.clone();
            let root = project_root.clone();
            let logs = result.logs.clone();
            let duration = result.duration_ms;
            let stored = tokio::task::spawn_blocking(move || {
                store.store(&key, &id, &root, &outputs, logs, duration, inputs)
            })
            .await;
            match stored {
                Ok(Ok(record)) => {
                    let _ = ctx.store.save_last(&node.id, &record.inputs);
                    if record.files.is_empty()
                        && !node.config.outputs.clone().unwrap_or_default().is_empty()
                    {
                        let _ = ctx.events.send(Event::Warning(format!(
                            "{}: declared outputs matched no files",
                            node.id
                        )));
                    }
                    if let Some(remote) = &ctx.opts.remote {
                        if remote.can_write() {
                            let remote = Arc::clone(remote);
                            let store = Arc::clone(&ctx.store);
                            let events = ctx.events.clone();
                            ctx.uploads.lock().await.spawn(async move {
                                if let Err(e) = remote.upload(&store, &record).await {
                                    let _ = events.send(Event::Warning(format!(
                                        "remote cache upload failed for {}: {:#}",
                                        record.task_id, e
                                    )));
                                }
                            });
                        }
                    }
                }
                Ok(Err(e)) => {
                    let _ = ctx.events.send(Event::Warning(format!(
                        "{}: could not write cache: {:#}",
                        node.id, e
                    )));
                }
                Err(e) => {
                    let _ = ctx.events.send(Event::Warning(format!(
                        "{}: could not write cache: {}",
                        node.id, e
                    )));
                }
            }
        }
    }
    summary
}

async fn lookup(ctx: &TaskCtx, key: &str) -> Result<Option<(TaskRecord, CacheSource)>> {
    if let Some(rec) = ctx.store.get(key)? {
        return Ok(Some((rec, CacheSource::Local)));
    }
    if let Some(remote) = &ctx.opts.remote {
        if remote.download(&ctx.store, key).await? {
            if let Some(rec) = ctx.store.get(key)? {
                return Ok(Some((rec, CacheSource::Remote)));
            }
        }
    }
    Ok(None)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::Path;

    fn write(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    fn setup() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "neex.json",
            r#"{"tasks":{
                "build":{"dependsOn":["^build"],"outputs":["dist/**"]},
                "lib#build":{"command":"mkdir -p dist && echo built > dist/out.txt && echo ran >> ../runs.log"},
                "app#build":{"command":"echo app >> ../runs.log && cat ../lib/dist/out.txt"},
                "fail#build":{"command":"echo boom 1>&2; exit 7"},
                "after#build":{"command":"echo should-not-run >> ../runs.log"}
            }}"#,
        );
        write(root, "package.json", r#"{"workspaces":["*"]}"#);
        write(root, "lib/package.json", r#"{"name":"lib"}"#);
        write(
            root,
            "app/package.json",
            r#"{"name":"app","dependencies":{"lib":"*"}}"#,
        );
        write(root, "fail/package.json", r#"{"name":"fail"}"#);
        write(
            root,
            "after/package.json",
            r#"{"name":"after","dependencies":{"fail":"*"}}"#,
        );
        write(root, "lib/src.txt", "v1");
        tmp
    }

    async fn run_tasks(root: &Path, projects: &[&str], force: bool) -> RunSummary {
        let ws = Workspace::load(root).unwrap();
        let idx: Vec<usize> = projects
            .iter()
            .map(|p| ws.find_project(p).unwrap())
            .collect();
        let graph = TaskGraph::build(&ws, &["build".into()], &idx).unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let opts = RunOptions {
            force,
            ..Default::default()
        };
        let summary = run(Arc::new(ws), Arc::new(graph), opts, tx).await.unwrap();
        while rx.try_recv().is_ok() {}
        summary
    }

    fn runs(root: &Path) -> String {
        std::fs::read_to_string(root.join("runs.log")).unwrap_or_default()
    }

    #[tokio::test]
    async fn second_run_hits_cache_and_restores_outputs() {
        let tmp = setup();
        let root = tmp.path();
        let s1 = run_tasks(root, &["app"], false).await;
        assert_eq!(s1.exit_code, 0);
        assert_eq!(s1.executed, 2);
        assert_eq!(runs(root).lines().count(), 2);

        std::fs::remove_dir_all(root.join("lib/dist")).unwrap();
        let s2 = run_tasks(root, &["app"], false).await;
        assert_eq!(s2.cached, 2, "{:?}", s2.tasks);
        assert_eq!(runs(root).lines().count(), 2, "nothing re-ran");
        assert_eq!(
            std::fs::read_to_string(root.join("lib/dist/out.txt")).unwrap(),
            "built\n"
        );

        // change lib → both rebuild, and the miss is explained
        write(root, "lib/src.txt", "v2");
        let s3 = run_tasks(root, &["app"], false).await;
        assert_eq!(s3.executed, 2);
        let lib = s3.tasks.iter().find(|t| t.id == "lib#build").unwrap();
        assert!(
            lib.miss_reasons.iter().any(|r| r.contains("src.txt")),
            "{:?}",
            lib.miss_reasons
        );

        // --force re-runs
        let s4 = run_tasks(root, &["app"], true).await;
        assert_eq!(s4.executed, 2);
    }

    #[tokio::test]
    async fn failure_is_not_cached_and_dependents_are_skipped() {
        let tmp = setup();
        let root = tmp.path();
        let s1 = run_tasks(root, &["after"], false).await;
        assert_eq!(s1.exit_code, 7);
        assert_eq!(s1.failed, 1);
        let after = s1.tasks.iter().find(|t| t.id == "after#build").unwrap();
        assert!(matches!(after.status, TaskStatus::Skipped { .. }));
        assert!(!runs(root).contains("should-not-run"));
        let fail = s1.tasks.iter().find(|t| t.id == "fail#build").unwrap();
        assert!(fail.failure_logs.iter().any(|l| l.text == "boom"));

        // runs again (not served from cache)
        let s2 = run_tasks(root, &["after"], false).await;
        assert_eq!(s2.exit_code, 7);
        assert_eq!(s2.cached, 0);
    }

    #[tokio::test]
    async fn fail_fast_cancels_queued_independent_tasks() {
        for mode in [
            ContinueMode::Never,
            ContinueMode::DependenciesSuccessful,
            ContinueMode::Always,
        ] {
            let tmp = setup();
            write(
                tmp.path(),
                "z/package.json",
                r#"{"name":"z","scripts":{"build":"unused"}}"#,
            );
            write(
                tmp.path(),
                "z/neex.json",
                r#"{"tasks":{"build":{"command":"echo ran > marker","cache":false}}}"#,
            );
            let ws = Workspace::load(tmp.path()).unwrap();
            let projects = [
                ws.find_project("fail").unwrap(),
                ws.find_project("z").unwrap(),
            ];
            let graph = TaskGraph::build(&ws, &["build".into()], &projects).unwrap();
            let (tx, _rx) = mpsc::unbounded_channel();
            let summary = run(
                Arc::new(ws),
                Arc::new(graph),
                RunOptions {
                    concurrency: 1,
                    continue_mode: mode,
                    ..Default::default()
                },
                tx,
            )
            .await
            .unwrap();
            assert_eq!(summary.exit_code, 7);
            assert_eq!(
                tmp.path().join("z/marker").exists(),
                mode != ContinueMode::Never
            );
            assert_eq!(summary.tasks.len(), 2);
        }
    }

    #[test]
    fn empty_passthrough_argument_is_preserved() {
        assert_eq!(shell_quote(""), "''");
    }
}
