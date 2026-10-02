//! Task graph - expands `dependsOn` into concrete `project#task` nodes
//!
//! `^build`  → `build` in each direct dependency project
//! `test`    → `test` in the same project
//! `api#gen` → exactly that task

use crate::config::TaskConfig;
use crate::project::TaskCommand;
use crate::workspace::Workspace;
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap, VecDeque};

#[derive(Debug, Clone, Serialize)]
pub struct TaskNode {
    /// `project#task`
    pub id: String,
    pub project: usize,
    pub project_name: String,
    pub task: String,
    /// `None` = the project has no such task; a no-op that still orders deps
    pub command: Option<TaskCommand>,
    pub config: TaskConfig,
    /// Ids of tasks that must finish first
    pub deps: Vec<String>,
}

impl TaskNode {
    pub fn cacheable(&self) -> bool {
        self.command.is_some() && self.config.cache.unwrap_or(false)
            // A build hit without declared outputs cannot restore a deleted
            // build directory. An explicit [] opts into log-only caching.
            && (!matches!(self.task.as_str(), "build" | "compile") || self.config.outputs.is_some())
    }
    pub fn persistent(&self) -> bool {
        self.config.persistent.unwrap_or(false)
    }
}

#[derive(Debug, Default, Serialize)]
pub struct TaskGraph {
    /// Topological order: dependencies before dependents
    pub nodes: Vec<TaskNode>,
    #[serde(skip)]
    index: HashMap<String, usize>,
}

pub fn task_id(project_name: &str, task: &str) -> String {
    format!("{}#{}", project_name, task)
}

impl TaskGraph {
    /// Build the graph for running `tasks` in `projects`, pulling in every
    /// task they depend on (in any project)
    pub fn build(ws: &Workspace, tasks: &[String], projects: &[usize]) -> Result<TaskGraph> {
        let mut nodes: HashMap<String, TaskNode> = HashMap::new();
        let mut queue: VecDeque<(usize, String)> = VecDeque::new();
        for &p in projects {
            for t in tasks {
                queue.push_back((p, t.clone()));
            }
        }

        while let Some((p, task)) = queue.pop_front() {
            let name = ws.projects[p].name.clone();
            let id = task_id(&name, &task);
            if nodes.contains_key(&id) {
                continue;
            }
            let config = ws.task_config(p, &task);
            let command = ws.task_command(p, &task);
            let mut deps: BTreeSet<String> = BTreeSet::new();
            for dep in config.depends_on.clone().unwrap_or_default() {
                if let Some(up) = dep.strip_prefix('^') {
                    for &d in &ws.deps[p] {
                        deps.insert(task_id(&ws.projects[d].name, up));
                        queue.push_back((d, up.to_string()));
                    }
                } else if let Some((proj, t)) = dep.split_once('#') {
                    let target = ws.find_project(proj).ok_or_else(|| {
                        anyhow!(
                            "task `{}` depends on `{}` but project `{}` does not exist",
                            id,
                            dep,
                            proj
                        )
                    })?;
                    deps.insert(task_id(&ws.projects[target].name, t));
                    queue.push_back((target, t.to_string()));
                } else {
                    deps.insert(task_id(&name, &dep));
                    queue.push_back((p, dep));
                }
            }
            nodes.insert(
                id.clone(),
                TaskNode {
                    id,
                    project: p,
                    project_name: name,
                    task,
                    command,
                    config,
                    deps: deps.into_iter().collect(),
                },
            );
        }

        // Validate: nothing may depend on a persistent task
        for n in nodes.values() {
            for d in &n.deps {
                if let Some(dep) = nodes.get(d) {
                    if dep.persistent() {
                        return Err(anyhow!(
                            "`{}` depends on `{}`, which is persistent and never finishes",
                            n.id,
                            d
                        ));
                    }
                }
            }
        }

        // Topological sort (Kahn), deterministic by id
        let mut indegree: HashMap<&str, usize> = HashMap::new();
        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
        for n in nodes.values() {
            indegree.entry(n.id.as_str()).or_insert(0);
            for d in &n.deps {
                *indegree.entry(n.id.as_str()).or_insert(0) += 1;
                dependents
                    .entry(d.as_str())
                    .or_default()
                    .push(n.id.as_str());
            }
        }
        let mut ready: BTreeSet<&str> = indegree
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(&id, _)| id)
            .collect();
        let mut order: Vec<String> = Vec::with_capacity(nodes.len());
        while let Some(&id) = ready.iter().next() {
            ready.remove(id);
            order.push(id.to_string());
            if let Some(ds) = dependents.get(id) {
                for &d in ds {
                    let e = indegree.get_mut(d).unwrap();
                    *e -= 1;
                    if *e == 0 {
                        ready.insert(d);
                    }
                }
            }
        }
        if order.len() != nodes.len() {
            let stuck: Vec<&str> = nodes
                .keys()
                .filter(|k| !order.contains(k))
                .map(|s| s.as_str())
                .collect();
            return Err(anyhow!(
                "circular task dependency among: {}",
                stuck.join(", ")
            ));
        }

        let mut graph = TaskGraph::default();
        for id in order {
            let node = nodes.remove(&id).unwrap();
            graph.index.insert(id, graph.nodes.len());
            graph.nodes.push(node);
        }
        Ok(graph)
    }

    pub fn get(&self, id: &str) -> Option<&TaskNode> {
        self.index.get(id).map(|&i| &self.nodes[i])
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Ids of every task that transitively depends on `id`
    pub fn dependents_of(&self, id: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut changed = true;
        let mut set: BTreeSet<&str> = BTreeSet::new();
        set.insert(id);
        while changed {
            changed = false;
            for n in &self.nodes {
                if !set.contains(n.id.as_str()) && n.deps.iter().any(|d| set.contains(d.as_str())) {
                    set.insert(n.id.as_str());
                    out.push(n.id.clone());
                    changed = true;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn write(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    fn ws() -> (tempfile::TempDir, Workspace) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "neex.json",
            r#"{"tasks":{
                "build":{"dependsOn":["^build"]},
                "test":{"dependsOn":["build"]},
                "web#build":{"dependsOn":["^build","proto#generate"]},
                "dev":{"persistent":true},
                "bad":{"dependsOn":["dev"]}
            }}"#,
        );
        write(root, "package.json", r#"{"workspaces":["p/*"]}"#);
        write(
            root,
            "p/web/package.json",
            r#"{"name":"web","scripts":{"build":"b","test":"t","dev":"d","bad":"x"},"dependencies":{"ui":"*"}}"#,
        );
        write(
            root,
            "p/ui/package.json",
            r#"{"name":"ui","scripts":{"build":"b"},"dependencies":{"utils":"*"}}"#,
        );
        write(
            root,
            "p/utils/package.json",
            r#"{"name":"utils","scripts":{"build":"b"}}"#,
        );
        write(
            root,
            "p/proto/package.json",
            r#"{"name":"proto","scripts":{"generate":"g"}}"#,
        );
        let ws = Workspace::load(root).unwrap();
        (tmp, ws)
    }

    #[test]
    fn expands_caret_same_project_and_explicit_deps() {
        let (_tmp, ws) = ws();
        let web = ws.find_project("web").unwrap();
        let g = TaskGraph::build(&ws, &["test".into()], &[web]).unwrap();
        let ids: Vec<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "proto#generate",
                "utils#build",
                "ui#build",
                "web#build",
                "web#test"
            ]
        );
        assert_eq!(
            g.get("web#build").unwrap().deps,
            vec!["proto#generate", "ui#build"]
        );
        assert_eq!(g.get("web#test").unwrap().deps, vec!["web#build"]);
        assert!(g.get("utils#build").unwrap().command.is_some());
        assert_eq!(g.dependents_of("utils#build").len(), 3);
    }

    #[test]
    fn missing_task_is_a_noop_node() {
        let (_tmp, ws) = ws();
        let ui = ws.find_project("ui").unwrap();
        let g = TaskGraph::build(&ws, &["test".into()], &[ui]).unwrap();
        assert!(g.get("ui#test").unwrap().command.is_none());
        assert!(!g.get("ui#test").unwrap().cacheable());
    }

    #[test]
    fn builds_need_declared_outputs_to_be_cacheable() {
        let (_tmp, ws) = ws();
        let ui = ws.find_project("ui").unwrap();
        let g = TaskGraph::build(&ws, &["build".into()], &[ui]).unwrap();
        let mut node = g.get("ui#build").unwrap().clone();
        assert!(!node.cacheable());
        node.config.outputs = Some(vec!["dist/**".into()]);
        assert!(node.cacheable());
        node.config.outputs = Some(vec![]);
        assert!(node.cacheable());
    }

    #[test]
    fn depending_on_persistent_is_rejected() {
        let (_tmp, ws) = ws();
        let web = ws.find_project("web").unwrap();
        let err = TaskGraph::build(&ws, &["bad".into()], &[web]).unwrap_err();
        assert!(err.to_string().contains("persistent"));
    }
}
