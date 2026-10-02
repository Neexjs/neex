//! Workspace - root detection, project discovery across all providers,
//! dependency resolution and the project graph

use crate::config::{ProjectConfig, RootConfig, TaskConfig};
use crate::project::{normalize_path, DepRef, Language, Project, TaskCommand};
use crate::providers::{self, DiscoveryCtx, WorkspaceProvider};
use anyhow::{anyhow, Result};
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Name of the synthetic project that owns `//#task` root tasks
pub const ROOT_PROJECT: &str = "//";

pub struct Workspace {
    pub root: PathBuf,
    pub config: RootConfig,
    /// Sorted by name; index is the project id used everywhere else
    pub projects: Vec<Project>,
    /// Per-project `neex.json`, if present
    pub project_configs: Vec<Option<ProjectConfig>>,
    /// Resolved local dependencies (project ids), per project
    pub deps: Vec<Vec<usize>>,
    pub warnings: Vec<String>,
    graph: DiGraph<usize, ()>,
    node_of: Vec<NodeIndex>,
    by_name: HashMap<String, usize>,
}

/// Files whose presence marks a workspace root
fn is_root_marker(dir: &Path) -> bool {
    if dir.join(crate::config::ROOT_CONFIG_FILE).is_file()
        || dir.join("turbo.json").is_file()
        || dir.join("turbo.jsonc").is_file()
        || dir.join("pnpm-workspace.yaml").is_file()
        || dir.join("go.work").is_file()
    {
        return true;
    }
    if let Ok(text) = std::fs::read_to_string(dir.join("package.json")) {
        if text.contains("\"workspaces\"") {
            return true;
        }
    }
    if let Ok(text) = std::fs::read_to_string(dir.join("Cargo.toml")) {
        if text.contains("[workspace]") {
            return true;
        }
    }
    if let Ok(text) = std::fs::read_to_string(dir.join("pyproject.toml")) {
        if text.contains("[tool.uv.workspace]") {
            return true;
        }
    }
    false
}

impl Workspace {
    /// Walk up from `cwd` and pick the highest directory that looks like a
    /// workspace root, never crossing above a `.git` directory. Falls back
    /// to `cwd` itself.
    pub fn find_root(cwd: &Path) -> PathBuf {
        let mut best: Option<PathBuf> = None;
        let mut standalone: Option<PathBuf> = None;
        let mut dir = Some(cwd.to_path_buf());
        while let Some(d) = dir {
            if is_root_marker(&d) {
                best = Some(d.clone());
            }
            if standalone.is_none()
                && ["package.json", "Cargo.toml", "go.mod", "pyproject.toml"]
                    .iter()
                    .any(|name| d.join(name).is_file())
            {
                standalone = Some(d.clone());
            }
            if d.join(".git").exists() {
                break;
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
        best.or(standalone).unwrap_or_else(|| cwd.to_path_buf())
    }

    pub fn load(cwd: &Path) -> Result<Self> {
        let root = Self::find_root(cwd);
        let config = RootConfig::load(&root)?;
        let warnings = config.warnings.clone();

        let ctx = DiscoveryCtx {
            root: &root,
            extra_globs: &config.projects,
        };

        // Discover with every provider; merge projects that share a root
        let mut by_root: BTreeMap<String, Project> = BTreeMap::new();
        for provider in providers::all_providers() {
            let found = match provider.discover(&ctx) {
                Ok(p) => p,
                Err(e) => {
                    return Err(anyhow!(
                        "{} discovery: {:#}",
                        provider.language().as_str(),
                        e
                    ))
                }
            };
            for p in found {
                let key = p.root_str();
                match by_root.get_mut(&key) {
                    Some(existing) => {
                        // Generic configuration augments the native package;
                        // retain its identity, defaults, lockfiles and env.
                        if existing.language == Language::Generic && p.language != Language::Generic
                        {
                            existing.name = p.name.clone();
                            existing.ident = p.ident.clone();
                            existing.language = p.language;
                            existing.manifest = p.manifest.clone();
                        }
                        for (k, v) in p.tasks {
                            existing.tasks.entry(k).or_insert(v);
                        }
                        for d in p.deps {
                            if !existing.deps.contains(&d) {
                                existing.deps.push(d);
                            }
                        }
                        if existing.lock_group.is_none() {
                            existing.lock_group = p.lock_group;
                        }
                    }
                    None => {
                        by_root.insert(key, p);
                    }
                }
            }
        }

        let mut projects: Vec<Project> = by_root.into_values().collect();

        // Synthetic root project for `//#task`
        let root_tasks: BTreeMap<String, TaskCommand> = config
            .root_tasks()
            .filter_map(|(name, t)| {
                t.command
                    .as_ref()
                    .map(|c| (name.to_string(), TaskCommand::Shell(c.clone())))
            })
            .collect();
        if !root_tasks.is_empty() && !projects.iter().any(|p| p.root.as_os_str().is_empty()) {
            projects.push(Project {
                name: ROOT_PROJECT.into(),
                ident: ROOT_PROJECT.into(),
                root: PathBuf::new(),
                language: Language::Generic,
                manifest: PathBuf::from(crate::config::ROOT_CONFIG_FILE),
                tasks: root_tasks,
                deps: vec![],
                lock_group: None,
            });
        } else if let Some(p) = projects.iter_mut().find(|p| p.root.as_os_str().is_empty()) {
            for (k, v) in root_tasks {
                p.tasks.insert(k, v);
            }
        }

        // Qualify colliding names
        let mut counts: HashMap<String, usize> = HashMap::new();
        for p in &projects {
            *counts.entry(p.name.clone()).or_default() += 1;
        }
        for p in projects.iter_mut() {
            if counts[&p.name] > 1 {
                p.name = format!("{}:{}", p.language.as_str(), p.name);
            }
        }
        projects.sort_by(|a, b| a.name.cmp(&b.name));

        // Project-level config: name override, extra deps, tasks
        let mut project_configs = Vec::with_capacity(projects.len());
        for p in projects.iter_mut() {
            // The root neex.json is the workspace config, not a project file
            let cfg = if p.root.as_os_str().is_empty() {
                None
            } else {
                ProjectConfig::load(&root.join(&p.root))?
            };
            if let Some(cfg) = &cfg {
                if let Some(name) = &cfg.name {
                    p.name = name.clone();
                }
                for d in &cfg.deps {
                    let r = if !d.starts_with('@') && (d.contains('/') || d.starts_with('.')) {
                        DepRef::Path(PathBuf::from(d.trim_start_matches("./")))
                    } else {
                        DepRef::Ident(d.clone())
                    };
                    if !p.deps.contains(&r) {
                        p.deps.push(r);
                    }
                }
                for (k, t) in &cfg.tasks {
                    if let Some(c) = &t.command {
                        p.tasks.insert(k.clone(), TaskCommand::Shell(c.clone()));
                    }
                }
            }
            project_configs.push(cfg);
        }

        // Index
        let mut by_name = HashMap::new();
        let mut by_ident: HashMap<(Language, String), usize> = HashMap::new();
        let mut any_ident: HashMap<String, usize> = HashMap::new();
        let mut by_path: HashMap<String, usize> = HashMap::new();
        for (i, p) in projects.iter().enumerate() {
            if by_name.insert(p.name.clone(), i).is_some() {
                return Err(anyhow!(
                    "duplicate project name `{}`; give each project a unique name",
                    p.name
                ));
            }
            by_ident.insert((p.language, p.ident.clone()), i);
            any_ident.entry(p.ident.clone()).or_insert(i);
            by_path.insert(p.root_str(), i);
        }

        // Resolve deps
        let mut deps: Vec<Vec<usize>> = vec![vec![]; projects.len()];
        for (i, p) in projects.iter().enumerate() {
            for d in &p.deps {
                let target = match d {
                    DepRef::Path(path) => by_path.get(&normalize_path(path)).copied(),
                    DepRef::Ident(id) => by_ident
                        .get(&(p.language, id.clone()))
                        .copied()
                        .or_else(|| by_name.get(id).copied())
                        .or_else(|| any_ident.get(id).copied()),
                };
                if let Some(t) = target {
                    if t != i && !deps[i].contains(&t) {
                        deps[i].push(t);
                    }
                }
            }
            deps[i].sort_unstable();
        }

        // Graph: edge dependent -> dependency
        let mut graph = DiGraph::new();
        let node_of: Vec<NodeIndex> = (0..projects.len()).map(|i| graph.add_node(i)).collect();
        for (i, ds) in deps.iter().enumerate() {
            for &d in ds {
                graph.add_edge(node_of[i], node_of[d], ());
            }
        }

        Ok(Workspace {
            root,
            config,
            projects,
            project_configs,
            deps,
            warnings,
            graph,
            node_of,
            by_name,
        })
    }

    pub fn project(&self, idx: usize) -> &Project {
        &self.projects[idx]
    }

    /// Look a project up by name, ident, or root path (`apps/web`, `./apps/web`)
    pub fn find_project(&self, spec: &str) -> Option<usize> {
        if spec == ROOT_PROJECT {
            return self
                .projects
                .iter()
                .position(|p| p.root.as_os_str().is_empty());
        }
        if let Some(&i) = self.by_name.get(spec) {
            return Some(i);
        }
        let path = normalize_path(Path::new(spec.trim_start_matches("./")));
        self.projects
            .iter()
            .position(|p| p.ident == spec || (!path.is_empty() && p.root_str() == path))
    }

    /// Project whose root contains `abs_path` (deepest match)
    pub fn project_for_path(&self, abs_path: &Path) -> Option<usize> {
        let rel = abs_path.strip_prefix(&self.root).ok()?;
        let rel = normalize_path(rel);
        let mut best: Option<(usize, usize)> = None;
        for (i, p) in self.projects.iter().enumerate() {
            let r = p.root_str();
            let matches = r.is_empty() || rel == r || rel.starts_with(&format!("{}/", r));
            if matches {
                let len = r.len();
                if best.map(|(_, l)| len > l).unwrap_or(true) {
                    best = Some((i, len));
                }
            }
        }
        best.map(|(i, _)| i)
    }

    pub fn dependents_of(&self, idx: usize) -> Vec<usize> {
        self.graph
            .neighbors_directed(self.node_of[idx], petgraph::Direction::Incoming)
            .map(|n| self.graph[n])
            .collect()
    }

    /// All projects reachable through dependency edges (excluding `idx`)
    pub fn transitive_deps(&self, idx: usize) -> Vec<usize> {
        let mut seen = HashSet::new();
        let mut stack = self.deps[idx].clone();
        while let Some(d) = stack.pop() {
            if seen.insert(d) {
                stack.extend(self.deps[d].iter().copied());
            }
        }
        let mut v: Vec<usize> = seen.into_iter().collect();
        v.sort_unstable();
        v
    }

    /// All projects that (transitively) depend on `idx`, plus `idx`
    pub fn affected_by(&self, idx: usize) -> Vec<usize> {
        let mut seen = HashSet::new();
        let mut stack = vec![idx];
        while let Some(d) = stack.pop() {
            if seen.insert(d) {
                stack.extend(self.dependents_of(d));
            }
        }
        let mut v: Vec<usize> = seen.into_iter().collect();
        v.sort_unstable();
        v
    }

    pub fn has_cycle(&self) -> bool {
        petgraph::algo::is_cyclic_directed(&self.graph)
    }

    /// Dependencies first
    pub fn build_order(&self) -> Result<Vec<usize>> {
        let sorted = toposort(&self.graph, None).map_err(|c| {
            anyhow!(
                "circular dependency involving `{}`",
                self.projects[self.graph[c.node_id()]].name
            )
        })?;
        Ok(sorted.into_iter().rev().map(|n| self.graph[n]).collect())
    }

    /// Effective task config for a project: root config layers, then the
    /// project's own `neex.json`
    pub fn task_config(&self, idx: usize, task: &str) -> TaskConfig {
        let p = &self.projects[idx];
        let mut cfg = self.config.task_config(&p.name, task);
        if p.root.as_os_str().is_empty() && p.name != ROOT_PROJECT {
            if let Some(root_task) = self.config.tasks.get(&format!("//#{}", task)) {
                cfg = cfg.merge(root_task);
            }
        }
        if let Some(pc) = &self.project_configs[idx] {
            if let Some(t) = pc.tasks.get(task) {
                cfg = cfg.merge(t);
                if t.cache.is_none() && cfg.persistent != Some(true) {
                    cfg.cache = Some(true);
                }
            }
        }
        if cfg.persistent == Some(true) {
            cfg.cache = Some(false);
        }
        cfg
    }

    /// The command that runs `task` in project `idx`, if any
    pub fn task_command(&self, idx: usize, task: &str) -> Option<TaskCommand> {
        let p = &self.projects[idx];
        let cfg = self.task_config(idx, task);
        if let Some(c) = cfg.command {
            return Some(TaskCommand::Shell(c));
        }
        if let Some(c) = p.tasks.get(task) {
            return Some(c.clone());
        }
        providers::provider_for(p.language).default_command(p, task)
    }

    /// Root-relative files that feed every task's hash
    pub fn global_input_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = Vec::new();
        let langs: HashSet<Language> = self.projects.iter().map(|p| p.language).collect();
        for lang in langs {
            for f in providers::provider_for(lang).global_inputs(&self.root) {
                if !files.contains(&f) {
                    files.push(f);
                }
            }
        }
        for name in [crate::config::ROOT_CONFIG_FILE, "turbo.json", "turbo.jsonc"] {
            let p = PathBuf::from(name);
            if self.root.join(&p).is_file() && !files.contains(&p) {
                files.push(p);
            }
        }
        files.sort();
        files
    }

    /// Env var names every task of these projects should hash
    pub fn implicit_env(&self, idx: usize) -> &'static [&'static str] {
        providers::provider_for(self.projects[idx].language).implicit_env()
    }

    pub fn provider(&self, idx: usize) -> Box<dyn WorkspaceProvider> {
        providers::provider_for(self.projects[idx].language)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    /// A JS + Rust + Go + generic workspace with a cross-language edge
    fn polyglot() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, ".git/HEAD", "ref: refs/heads/main");
        write(
            root,
            "neex.json",
            r#"{"projects":["services/*"],"tasks":{"build":{"outputs":["dist/**"]},"//#format":{"command":"prettier --check ."}}}"#,
        );
        write(
            root,
            "pnpm-workspace.yaml",
            "packages:\n  - 'apps/*'\n  - 'packages/*'\n",
        );
        write(root, "package.json", r#"{"name":"root","private":true}"#);
        write(root, "pnpm-lock.yaml", "");
        write(
            root,
            "apps/web/package.json",
            r#"{"name":"web","scripts":{"build":"next build"},"dependencies":{"ui":"workspace:*"}}"#,
        );
        write(
            root,
            "packages/ui/package.json",
            r#"{"name":"ui","scripts":{"build":"tsc"}}"#,
        );
        write(
            root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/*\"]\n",
        );
        write(
            root,
            "crates/core/Cargo.toml",
            "[package]\nname = \"core\"\n",
        );
        write(
            root,
            "crates/api/Cargo.toml",
            "[package]\nname = \"api\"\n[dependencies]\ncore = { path = \"../core\" }\n",
        );
        write(root, "go.work", "use ./services/gateway\n");
        write(
            root,
            "services/gateway/go.mod",
            "module example.com/gateway\n",
        );
        write(
            root,
            "services/gateway/neex.json",
            r#"{"deps":["api"],"tasks":{"build":"go build -o bin/gw ."}}"#,
        );
        write(
            root,
            "services/proto/neex.json",
            r#"{"name":"proto","tasks":{"generate":"buf generate"}}"#,
        );
        tmp
    }

    #[test]
    fn loads_polyglot_workspace() {
        let tmp = polyglot();
        let ws = Workspace::load(&tmp.path().join("apps/web")).unwrap();
        assert_eq!(ws.root, tmp.path());
        let names: Vec<&str> = ws.projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["//", "api", "core", "gateway", "proto", "ui", "web"]
        );

        let web = ws.find_project("web").unwrap();
        let ui = ws.find_project("ui").unwrap();
        assert_eq!(ws.deps[web], vec![ui]);

        let api = ws.find_project("api").unwrap();
        let core = ws.find_project("core").unwrap();
        assert_eq!(ws.deps[api], vec![core]);

        // cross-language edge declared in the project file
        let gw = ws.find_project("gateway").unwrap();
        assert_eq!(ws.deps[gw], vec![api]);
        assert_eq!(
            ws.task_command(gw, "build").unwrap().shell_line(),
            "go build -o bin/gw ."
        );
        assert_eq!(
            ws.task_command(core, "build").unwrap().shell_line(),
            "cargo build -p core"
        );
        assert_eq!(
            ws.task_command(web, "build").unwrap().shell_line(),
            "pnpm run build"
        );
        assert!(ws.task_command(ui, "test").is_none());

        let order = ws.build_order().unwrap();
        let pos = |n: &str| {
            order
                .iter()
                .position(|&i| ws.projects[i].name == n)
                .unwrap()
        };
        assert!(pos("core") < pos("api"));
        assert!(pos("api") < pos("gateway"));
        assert!(pos("ui") < pos("web"));

        assert_eq!(ws.affected_by(core), vec![api, core, gw]);
        assert_eq!(
            ws.project_for_path(&tmp.path().join("apps/web/src/x.ts")),
            Some(web)
        );
        assert_eq!(ws.find_project("./apps/web"), Some(web));
        assert!(ws
            .global_input_files()
            .contains(&PathBuf::from("pnpm-lock.yaml")));
        assert!(ws
            .global_input_files()
            .contains(&PathBuf::from("neex.json")));
    }

    #[test]
    fn root_task_and_project_config_override() {
        let tmp = polyglot();
        let ws = Workspace::load(tmp.path()).unwrap();
        let root = ws.find_project("//").unwrap();
        assert_eq!(
            ws.task_command(root, "format").unwrap().shell_line(),
            "prettier --check ."
        );
        let web = ws.find_project("web").unwrap();
        let cfg = ws.task_config(web, "build");
        assert_eq!(cfg.outputs.unwrap(), vec!["dist/**"]);
        assert_eq!(cfg.depends_on.unwrap(), vec!["^build"]);
    }

    #[test]
    fn find_root_stops_at_git() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "pnpm-workspace.yaml", "packages: []\n");
        write(root, "inner/.git/HEAD", "x");
        write(root, "inner/package.json", r#"{"workspaces":["a"]}"#);
        write(root, "inner/a/package.json", r#"{"name":"a"}"#);
        assert_eq!(
            Workspace::find_root(&root.join("inner/a")),
            root.join("inner")
        );
    }

    #[test]
    fn standalone_js_package_loads_from_a_source_subdirectory() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "package.json",
            r#"{"name":"single","scripts":{"build":"tsc"}}"#,
        );
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        let ws = Workspace::load(&tmp.path().join("src")).unwrap();
        assert_eq!(ws.root, tmp.path());
        assert_eq!(ws.projects.len(), 1);
        assert_eq!(
            ws.task_command(0, "build").unwrap().shell_line(),
            "npm run build"
        );
        write(
            tmp.path(),
            "neex.json",
            r#"{"tasks":{"//#custom":{"command":"echo custom","cache":false}}}"#,
        );
        let ws = Workspace::load(tmp.path()).unwrap();
        assert_eq!(ws.find_project("//"), Some(0));
        assert_eq!(
            ws.task_command(0, "custom").unwrap().shell_line(),
            "echo custom"
        );
    }

    #[test]
    fn name_collisions_are_qualified() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "package.json", r#"{"workspaces":["js/*"]}"#);
        write(root, "js/utils/package.json", r#"{"name":"utils"}"#);
        write(root, "Cargo.toml", "[workspace]\nmembers = [\"rs/*\"]\n");
        write(root, "rs/utils/Cargo.toml", "[package]\nname = \"utils\"\n");
        let ws = Workspace::load(root).unwrap();
        let names: Vec<&str> = ws.projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["js:utils", "rust:utils"]);
    }

    #[test]
    fn project_config_preserves_native_identity_and_scoped_dependencies() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "neex.json", r#"{"projects":["p/*"]}"#);
        write(root, "package.json", r#"{"workspaces":["p/*"]}"#);
        write(root, "pnpm-lock.yaml", "");
        write(
            root,
            "p/web/package.json",
            r#"{"name":"@acme/web","scripts":{"build":"tsc"}}"#,
        );
        write(
            root,
            "p/web/neex.json",
            r#"{"name":"web","deps":["@acme/ui"],"tasks":{"build":{"outputs":["dist/**"]}}}"#,
        );
        write(root, "p/ui/package.json", r#"{"name":"@acme/ui"}"#);
        let ws = Workspace::load(root).unwrap();
        let web = ws.find_project("web").unwrap();
        let ui = ws.find_project("@acme/ui").unwrap();
        assert_eq!(ws.projects[web].language, Language::Js);
        assert_eq!(ws.projects[web].ident, "@acme/web");
        assert_eq!(ws.deps[web], vec![ui]);
        assert!(ws
            .global_input_files()
            .contains(&PathBuf::from("pnpm-lock.yaml")));
        assert_eq!(ws.implicit_env(web), &["NODE_ENV"]);
    }

    #[test]
    fn duplicate_project_names_and_malformed_manifests_fail_loading() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "package.json", r#"{"workspaces":["p/*"]}"#);
        write(root, "p/a/package.json", r#"{"name":"a"}"#);
        write(root, "p/b/package.json", r#"{"name":"b"}"#);
        write(root, "p/b/neex.json", r#"{"name":"a"}"#);
        assert!(Workspace::load(root)
            .err()
            .unwrap()
            .to_string()
            .contains("duplicate"));
        std::fs::remove_file(root.join("p/b/neex.json")).unwrap();
        write(root, "p/b/package.json", "invalid json");
        assert!(Workspace::load(root)
            .err()
            .unwrap()
            .to_string()
            .contains("js discovery"));
    }
}
