//! Generic provider - any directory with a project `neex.json`
//!
//! This is how a language neex has no provider for joins the graph: the
//! project file declares tasks as shell commands and `deps` by name or path.
//! Directories are found through the root config's `projects` globs.

use super::{expand_project_globs, DiscoveryCtx, WorkspaceProvider};
use crate::config::ProjectConfig;
use crate::project::{DepRef, Language, Project, TaskCommand};
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct GenericProvider;

impl WorkspaceProvider for GenericProvider {
    fn language(&self) -> Language {
        Language::Generic
    }

    fn discover(&self, ctx: &DiscoveryCtx) -> Result<Vec<Project>> {
        let mut projects = Vec::new();
        for dir in expand_project_globs(ctx.root, ctx.extra_globs)? {
            let abs = ctx.root.join(&dir);
            let Some(cfg) = ProjectConfig::load(&abs)? else {
                continue;
            };
            let fallback = dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "root".to_string());
            let name = cfg.name.clone().unwrap_or(fallback);
            let tasks: BTreeMap<String, TaskCommand> = cfg
                .tasks
                .iter()
                .filter_map(|(k, t)| {
                    t.command
                        .as_ref()
                        .map(|c| (k.clone(), TaskCommand::Shell(c.clone())))
                })
                .collect();
            let deps = cfg
                .deps
                .iter()
                .map(|d| {
                    if !d.starts_with('@') && (d.contains('/') || d.starts_with('.')) {
                        DepRef::Path(PathBuf::from(d.trim_start_matches("./")))
                    } else {
                        DepRef::Ident(d.clone())
                    }
                })
                .collect();
            projects.push(Project {
                name: name.clone(),
                ident: name,
                root: dir,
                language: Language::Generic,
                manifest: PathBuf::from("neex.json"),
                tasks,
                deps,
                lock_group: None,
            });
        }
        Ok(projects)
    }

    fn default_command(&self, _project: &Project, _task: &str) -> Option<TaskCommand> {
        None
    }

    fn global_inputs(&self, _root: &Path) -> Vec<PathBuf> {
        vec![]
    }

    fn implicit_env(&self) -> &'static [&'static str] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_generic_project() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let dir = root.join("services/proto");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("neex.json"),
            r#"{"name":"proto","deps":["schemas"],"tasks":{"generate":"buf generate","build":{"command":"make","outputs":["out/**"]}}}"#,
        )
        .unwrap();
        let globs = vec!["services/*".to_string()];
        let ctx = DiscoveryCtx {
            root,
            extra_globs: &globs,
        };
        let projects = GenericProvider.discover(&ctx).unwrap();
        assert_eq!(projects.len(), 1);
        let p = &projects[0];
        assert_eq!(p.name, "proto");
        assert_eq!(
            p.tasks["generate"],
            TaskCommand::Shell("buf generate".into())
        );
        assert_eq!(p.tasks["build"], TaskCommand::Shell("make".into()));
        assert_eq!(p.deps, vec![DepRef::Ident("schemas".into())]);
    }
}
