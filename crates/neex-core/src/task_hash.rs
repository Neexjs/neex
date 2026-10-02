//! Task cache key - and the record that explains it (`neex why`)
//!
//! key = blake3 of:
//!   neex version, OS, arch,
//!   project, task, resolved command,
//!   this project's input files (path + content hash),
//!   input fingerprint of every transitive local dependency,
//!   cache keys of the tasks this one depends on,
//!   values of declared env vars (unset recorded as unset),
//!   global input files (lockfiles, toolchain pins, neex.json),
//!   root-relative task inputs

use crate::config::TaskConfig;
use crate::inputs::{digest_map, hash_project_inputs, hash_root_globs, FileHashCache, InputFilter};
use crate::task_graph::TaskNode;
use crate::workspace::Workspace;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Mutex;

pub const KEY_VERSION: &str = "neex-key-v3";

/// Everything that went into a key, in a form that can be diffed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HashInputs {
    pub key_version: String,
    pub neex_version: String,
    pub platform: String,
    pub project: String,
    pub task: String,
    pub command: String,
    #[serde(default)]
    pub config: TaskConfig,
    /// project-relative path → hash
    pub files: BTreeMap<String, String>,
    /// dependency project → digest of its default inputs
    pub dep_projects: BTreeMap<String, String>,
    /// dependency task id → its cache key
    pub dep_tasks: BTreeMap<String, String>,
    /// env var → value digest, `None` when unset. Raw values must never be
    /// serialized into local records, remote records or run explanations.
    pub env: BTreeMap<String, Option<String>>,
    /// root-relative path → hash
    pub global_files: BTreeMap<String, String>,
}

impl HashInputs {
    pub fn key(&self) -> String {
        let mut h = blake3::Hasher::new();
        let mut feed = |label: &str, v: &str| {
            h.update(label.as_bytes());
            h.update(&[0]);
            h.update(v.as_bytes());
            h.update(&[0]);
        };
        feed("kv", &self.key_version);
        feed("ver", &self.neex_version);
        feed("plat", &self.platform);
        feed("proj", &self.project);
        feed("task", &self.task);
        feed("cmd", &self.command);
        feed(
            "config",
            &serde_json::to_string(&self.config).expect("task config is serializable"),
        );
        feed("files", &digest_map(&self.files));
        feed("deps", &digest_map(&self.dep_projects));
        feed("tasks", &digest_map(&self.dep_tasks));
        let env: BTreeMap<String, String> = self
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone().unwrap_or_else(|| "\u{0}unset".into())))
            .collect();
        feed("env", &digest_map(&env));
        feed("global", &digest_map(&self.global_files));
        h.finalize().to_hex().to_string()
    }

    /// Human-readable differences between two input records
    pub fn diff(&self, other: &HashInputs) -> Vec<String> {
        let mut out = Vec::new();
        if self.key_version != other.key_version {
            out.push("cache key format changed".into());
        }
        if self.config != other.config {
            out.push("task configuration changed".into());
        }
        if self.neex_version != other.neex_version {
            out.push(format!(
                "neex version {} → {}",
                other.neex_version, self.neex_version
            ));
        }
        if self.platform != other.platform {
            out.push(format!("platform {} → {}", other.platform, self.platform));
        }
        if self.command != other.command {
            out.push(format!(
                "command changed: `{}` → `{}`",
                other.command, self.command
            ));
        }
        diff_maps("file", &other.files, &self.files, &mut out);
        diff_maps(
            "dependency project",
            &other.dep_projects,
            &self.dep_projects,
            &mut out,
        );
        diff_maps(
            "dependency task",
            &other.dep_tasks,
            &self.dep_tasks,
            &mut out,
        );
        diff_maps(
            "global input",
            &other.global_files,
            &self.global_files,
            &mut out,
        );
        let a: BTreeMap<String, String> = other
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone().unwrap_or_else(|| "<unset>".into())))
            .collect();
        let b: BTreeMap<String, String> = self
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone().unwrap_or_else(|| "<unset>".into())))
            .collect();
        diff_maps("env", &a, &b, &mut out);
        out
    }
}

fn diff_maps(
    label: &str,
    old: &BTreeMap<String, String>,
    new: &BTreeMap<String, String>,
    out: &mut Vec<String>,
) {
    for (k, v) in new {
        match old.get(k) {
            None => out.push(format!("{} added: {}", label, k)),
            // env values are never printed, only their names
            Some(o) if o != v => out.push(format!("{} changed: {}", label, k)),
            _ => {}
        }
    }
    for k in old.keys() {
        if !new.contains_key(k) {
            out.push(format!("{} removed: {}", label, k));
        }
    }
}

/// Computes keys for one run; memoises per-project fingerprints
pub struct TaskHasher<'a> {
    ws: &'a Workspace,
    cache: FileHashCache,
    project_fingerprints: Mutex<HashMap<usize, String>>,
    global_files: BTreeMap<String, String>,
    nested_roots: Vec<String>,
    env: HashMap<String, String>,
}

impl<'a> TaskHasher<'a> {
    pub fn new(ws: &'a Workspace) -> Result<Self> {
        let cache = FileHashCache::new();
        let mut globs: Vec<String> = ws
            .global_input_files()
            .iter()
            .map(|p| crate::project::normalize_path(p))
            .collect();
        globs.extend(ws.config.global_inputs.iter().cloned());
        let global_files = hash_root_globs(&ws.root, &globs, &cache)?;
        let nested_roots = ws.projects.iter().map(|p| p.root_str()).collect();
        Ok(Self {
            ws,
            cache,
            project_fingerprints: Mutex::new(HashMap::new()),
            global_files,
            nested_roots,
            env: std::env::vars().collect(),
        })
    }

    /// Digest of a project's default inputs (all files, no task filter)
    pub fn project_fingerprint(&self, idx: usize) -> Result<String> {
        if let Some(f) = self.project_fingerprints.lock().unwrap().get(&idx) {
            return Ok(f.clone());
        }
        // Build products must not become source inputs for dependents on
        // the next run, even when they are not in .gitignore.
        let mut tasks: BTreeSet<String> = self.ws.projects[idx].tasks.keys().cloned().collect();
        tasks.extend(
            self.ws
                .config
                .tasks
                .keys()
                .map(|k| k.rsplit('#').next().unwrap().to_string()),
        );
        if let Some(config) = &self.ws.project_configs[idx] {
            tasks.extend(config.tasks.keys().cloned());
        }
        let outputs: Vec<String> = tasks
            .iter()
            .flat_map(|task| self.ws.task_config(idx, task).outputs.unwrap_or_default())
            .collect();
        let filter = InputFilter::new(None, &outputs)?;
        let files = hash_project_inputs(
            &self.ws.root,
            &self.ws.projects[idx].root,
            &self.nested_roots,
            &filter,
            &self.cache,
        )?;
        let d = digest_map(&files);
        self.project_fingerprints
            .lock()
            .unwrap()
            .insert(idx, d.clone());
        Ok(d)
    }

    /// Full input record for a task node. `dep_task_keys` must contain the
    /// key of every task in `node.deps` that produced one.
    pub fn inputs(
        &self,
        node: &TaskNode,
        dep_task_keys: &BTreeMap<String, String>,
    ) -> Result<HashInputs> {
        let ws = self.ws;
        let project = &ws.projects[node.project];
        let cfg = &node.config;
        let outputs = cfg.outputs.clone().unwrap_or_default();
        let filter = InputFilter::new(cfg.inputs.as_deref(), &outputs)?;
        let files = hash_project_inputs(
            &ws.root,
            &project.root,
            &self.nested_roots,
            &filter,
            &self.cache,
        )?;

        let mut dep_projects = BTreeMap::new();
        for d in ws.transitive_deps(node.project) {
            dep_projects.insert(ws.projects[d].name.clone(), self.project_fingerprint(d)?);
        }

        let mut env: BTreeMap<String, Option<String>> = BTreeMap::new();
        let mut names: Vec<String> = ws.config.global_env.clone();
        names.extend(cfg.env.clone().unwrap_or_default());
        names.extend(ws.implicit_env(node.project).iter().map(|s| s.to_string()));
        for name in names {
            if let Some(prefix) = name.strip_suffix('*') {
                for (k, v) in &self.env {
                    if k.starts_with(prefix) {
                        env.insert(
                            k.clone(),
                            Some(blake3::hash(v.as_bytes()).to_hex().to_string()),
                        );
                    }
                }
            } else {
                env.insert(
                    name.clone(),
                    self.env
                        .get(&name)
                        .map(|v| blake3::hash(v.as_bytes()).to_hex().to_string()),
                );
            }
        }

        let mut global_files = self.global_files.clone();
        let root_globs = InputFilter::root_globs(cfg.inputs.as_deref());
        if !root_globs.is_empty() {
            global_files.extend(hash_root_globs(&ws.root, &root_globs, &self.cache)?);
        }

        let dep_tasks = node
            .deps
            .iter()
            .filter_map(|d| dep_task_keys.get(d).map(|k| (d.clone(), k.clone())))
            .collect();

        Ok(HashInputs {
            key_version: KEY_VERSION.into(),
            neex_version: env!("CARGO_PKG_VERSION").into(),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            project: project.name.clone(),
            task: node.task.clone(),
            command: node
                .command
                .as_ref()
                .map(|c| c.hash_material())
                .unwrap_or_default(),
            config: cfg.clone(),
            files,
            dep_projects,
            dep_tasks,
            env,
            global_files,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_graph::TaskGraph;
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
            r#"{"tasks":{"build":{"dependsOn":["^build"],"outputs":["dist/**"],"env":["API_URL"]}}}"#,
        );
        write(root, "package.json", r#"{"workspaces":["p/*"]}"#);
        write(root, "pnpm-lock.yaml", "lock1");
        write(
            root,
            "p/web/package.json",
            r#"{"name":"web","scripts":{"build":"b"},"dependencies":{"ui":"*"}}"#,
        );
        write(root, "p/web/src/a.ts", "a");
        write(
            root,
            "p/ui/package.json",
            r#"{"name":"ui","scripts":{"build":"b"}}"#,
        );
        write(root, "p/ui/src/b.ts", "b");
        tmp
    }

    fn keys(root: &Path) -> (String, String, HashInputs) {
        let ws = Workspace::load(root).unwrap();
        let web = ws.find_project("web").unwrap();
        let g = TaskGraph::build(&ws, &["build".into()], &[web]).unwrap();
        let h = TaskHasher::new(&ws).unwrap();
        let mut dep_keys = BTreeMap::new();
        let ui_in = h.inputs(g.get("ui#build").unwrap(), &dep_keys).unwrap();
        let ui_key = ui_in.key();
        dep_keys.insert("ui#build".into(), ui_key.clone());
        let web_in = h.inputs(g.get("web#build").unwrap(), &dep_keys).unwrap();
        (ui_key, web_in.key(), web_in)
    }

    #[test]
    fn key_is_stable_and_sensitive_to_the_right_things() {
        std::env::remove_var("API_URL");
        let tmp = setup();
        let root = tmp.path();
        let (ui1, web1, _) = keys(root);
        let (ui2, web2, _) = keys(root);
        assert_eq!(ui1, ui2);
        assert_eq!(web1, web2);

        // editing web leaves ui alone
        write(root, "p/web/src/a.ts", "a2");
        let (ui3, web3, _) = keys(root);
        assert_eq!(ui1, ui3);
        assert_ne!(web1, web3);

        // editing ui changes both (dep fingerprint + dep task key)
        write(root, "p/ui/src/b.ts", "b2");
        let (ui4, web4, _) = keys(root);
        assert_ne!(ui3, ui4);
        assert_ne!(web3, web4);

        // outputs are not inputs
        write(root, "p/web/dist/out.js", "x");
        let (_, web5, _) = keys(root);
        assert_eq!(web4, web5);

        // lockfile is a global input
        write(root, "pnpm-lock.yaml", "lock2");
        let (_, web6, _) = keys(root);
        assert_ne!(web5, web6);

        // declared env changes the key, undeclared does not
        std::env::set_var("API_URL", "https://x");
        let (_, web7, inputs7) = keys(root);
        assert_ne!(web6, web7);
        std::env::set_var("UNRELATED_VAR", "1");
        let (_, web8, _) = keys(root);
        assert_eq!(web7, web8);
        std::env::remove_var("API_URL");
        std::env::remove_var("UNRELATED_VAR");

        assert!(inputs7.env.contains_key("API_URL"));
        assert!(inputs7.dep_tasks.contains_key("ui#build"));
        assert!(inputs7.dep_projects.contains_key("ui"));
        assert!(inputs7.global_files.contains_key("pnpm-lock.yaml"));
        assert!(inputs7.files.keys().all(|k| !k.starts_with('/')));
    }

    #[test]
    fn diff_explains_changes() {
        let tmp = setup();
        let root = tmp.path();
        let (_, _, a) = keys(root);
        write(root, "p/web/src/a.ts", "changed");
        write(root, "p/web/src/new.ts", "n");
        let (_, _, b) = keys(root);
        let d = b.diff(&a);
        assert!(d.iter().any(|l| l == "file changed: src/a.ts"), "{:?}", d);
        assert!(d.iter().any(|l| l == "file added: src/new.ts"), "{:?}", d);
    }

    #[test]
    fn dependency_outputs_do_not_invalidate_dependent_keys() {
        let tmp = setup();
        let (_, before, _) = keys(tmp.path());
        write(tmp.path(), "p/ui/dist/out.js", "built output");
        let (_, after, _) = keys(tmp.path());
        assert_eq!(before, after);
    }

    #[test]
    fn serialized_inputs_do_not_expose_environment_values() {
        let tmp = setup();
        let ws = Workspace::load(tmp.path()).unwrap();
        let web = ws.find_project("web").unwrap();
        let graph = TaskGraph::build(&ws, &["build".into()], &[web]).unwrap();
        let mut hasher = TaskHasher::new(&ws).unwrap();
        hasher
            .env
            .insert("API_URL".into(), "secret-environment-value".into());
        let before = hasher
            .inputs(graph.get("web#build").unwrap(), &BTreeMap::new())
            .unwrap();
        assert!(!serde_json::to_string(&before)
            .unwrap()
            .contains("secret-environment-value"));
        assert_eq!(before.env["API_URL"].as_ref().unwrap().len(), 64);
        hasher
            .env
            .insert("API_URL".into(), "changed-secret-value".into());
        let after = hasher
            .inputs(graph.get("web#build").unwrap(), &BTreeMap::new())
            .unwrap();
        assert_ne!(before.key(), after.key());
        assert!(after.diff(&before).contains(&"env changed: API_URL".into()));
    }

    #[test]
    fn project_task_config_is_hashed_even_with_restricted_inputs() {
        let tmp = setup();
        write(
            tmp.path(),
            "p/web/neex.json",
            r#"{"tasks":{"build":{"inputs":["src/**"],"outputs":["dist/**"]}}}"#,
        );
        let (_, before, _) = keys(tmp.path());
        write(
            tmp.path(),
            "p/web/neex.json",
            r#"{"tasks":{"build":{"inputs":["src/**"],"outputs":["out/**"]}}}"#,
        );
        let (_, after, inputs) = keys(tmp.path());
        assert_ne!(before, after);
        assert!(!inputs.files.contains_key("neex.json"));
    }
}
