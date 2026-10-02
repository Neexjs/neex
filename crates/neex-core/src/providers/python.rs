//! Python provider (uv workspaces)
//!
//! Members come from `[tool.uv.workspace] members` / `exclude` in the root
//! `pyproject.toml`. Dependencies are `[project].dependencies` names whose
//! `[tool.uv.sources]` entry is `workspace = true` or a `path`.

use super::{expand_project_globs, read_optional, DiscoveryCtx, WorkspaceProvider};
use crate::project::{normalize_path, DepRef, Language, Project, TaskCommand};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct PythonProvider;

#[derive(Debug, Deserialize, Default)]
struct PyProject {
    #[serde(default)]
    project: Option<PyProjectMeta>,
    #[serde(default)]
    tool: Option<Tool>,
}

#[derive(Debug, Deserialize, Default)]
struct PyProjectMeta {
    name: Option<String>,
    #[serde(default)]
    dependencies: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct Tool {
    #[serde(default)]
    uv: Option<Uv>,
}

#[derive(Debug, Deserialize, Default)]
struct Uv {
    #[serde(default)]
    workspace: Option<UvWorkspace>,
    #[serde(default)]
    sources: BTreeMap<String, UvSource>,
}

#[derive(Debug, Deserialize, Default)]
struct UvWorkspace {
    #[serde(default)]
    members: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum UvSource {
    Table {
        #[serde(default)]
        workspace: Option<bool>,
        #[serde(default)]
        path: Option<String>,
    },
    Other(#[allow(dead_code)] toml::Value),
}

/// PEP 503 normalisation: lowercase, runs of `-_.` become `-`
pub fn normalize_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_sep = false;
    for c in name.chars() {
        if c == '-' || c == '_' || c == '.' {
            if !last_sep {
                out.push('-');
            }
            last_sep = true;
        } else {
            out.push(c.to_ascii_lowercase());
            last_sep = false;
        }
    }
    out
}

/// `requests>=2 ; python_version > '3'` → `requests`
fn requirement_name(spec: &str) -> String {
    let end = spec
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'))
        .unwrap_or(spec.len());
    normalize_name(&spec[..end])
}

impl WorkspaceProvider for PythonProvider {
    fn language(&self) -> Language {
        Language::Python
    }

    fn discover(&self, ctx: &DiscoveryCtx) -> Result<Vec<Project>> {
        let root = ctx.root;
        let Some(text) = read_optional(&root.join("pyproject.toml"))? else {
            return Ok(vec![]);
        };
        let root_py: PyProject = toml::from_str(&text).context("parsing pyproject.toml")?;
        let ws = root_py
            .tool
            .as_ref()
            .and_then(|t| t.uv.as_ref())
            .and_then(|u| u.workspace.as_ref());

        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(ws) = ws {
            let mut globs = ws.members.clone();
            globs.extend(ws.exclude.iter().map(|e| format!("!{}", e)));
            dirs = expand_project_globs(root, &globs)?;
        }
        if root_py
            .project
            .as_ref()
            .and_then(|p| p.name.as_ref())
            .is_some()
        {
            dirs.insert(0, PathBuf::new());
        }
        for dir in expand_project_globs(root, ctx.extra_globs)? {
            if root.join(&dir).join("pyproject.toml").is_file() && !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }

        let mut projects = Vec::new();
        for dir in dirs {
            let path = root.join(&dir).join("pyproject.toml");
            let Some(text) = read_optional(&path)? else {
                continue;
            };
            let py: PyProject =
                toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
            let Some(meta) = py.project else {
                continue;
            };
            let Some(name) = meta.name else {
                continue;
            };
            // Member sources override root sources
            let mut sources: BTreeMap<String, &UvSource> = BTreeMap::new();
            if let Some(uv) = root_py.tool.as_ref().and_then(|t| t.uv.as_ref()) {
                for (k, v) in &uv.sources {
                    sources.insert(normalize_name(k), v);
                }
            }
            let member_uv = py.tool.as_ref().and_then(|t| t.uv.as_ref());
            if let Some(uv) = member_uv {
                for (k, v) in &uv.sources {
                    sources.insert(normalize_name(k), v);
                }
            }
            let mut deps = Vec::new();
            for spec in &meta.dependencies {
                let dep = requirement_name(spec);
                match sources.get(&dep) {
                    Some(UvSource::Table {
                        workspace: Some(true),
                        ..
                    }) => deps.push(DepRef::Ident(dep)),
                    Some(UvSource::Table { path: Some(p), .. }) => {
                        let abs = root.join(&dir).join(p);
                        if let Ok(rel) = abs.strip_prefix(root) {
                            deps.push(DepRef::Path(PathBuf::from(normalize_path(rel))));
                        }
                    }
                    _ => {}
                }
            }
            projects.push(Project {
                name: name.clone(),
                ident: normalize_name(&name),
                root: dir,
                language: Language::Python,
                manifest: PathBuf::from("pyproject.toml"),
                tasks: BTreeMap::new(),
                deps,
                lock_group: None,
            });
        }
        Ok(projects)
    }

    fn default_command(&self, _project: &Project, task: &str) -> Option<TaskCommand> {
        let cmd = match task {
            "build" => "uv build",
            "test" => "uv run pytest",
            "lint" => "uv run ruff check .",
            "typecheck" | "check" => "uv run mypy .",
            _ => return None,
        };
        Some(TaskCommand::Shell(cmd.to_string()))
    }

    fn global_inputs(&self, root: &Path) -> Vec<PathBuf> {
        ["pyproject.toml", "uv.lock", ".python-version"]
            .iter()
            .map(PathBuf::from)
            .filter(|p| root.join(p).is_file())
            .collect()
    }

    fn implicit_env(&self) -> &'static [&'static str] {
        &["PYTHONOPTIMIZE", "PYTHONHASHSEED", "UV_PYTHON"]
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

    #[test]
    fn discovers_uv_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "pyproject.toml",
            r#"[project]
name = "root-app"
dependencies = ["my_lib>=0.1", "requests"]
[tool.uv.workspace]
members = ["packages/*"]
[tool.uv.sources]
my-lib = { workspace = true }
"#,
        );
        write(
            root,
            "packages/my_lib/pyproject.toml",
            "[project]\nname = \"my_lib\"\ndependencies = []\n",
        );
        let ctx = DiscoveryCtx {
            root,
            extra_globs: &[],
        };
        let projects = PythonProvider.discover(&ctx).unwrap();
        assert_eq!(projects.len(), 2);
        let app = &projects[0];
        assert_eq!(app.ident, "root-app");
        assert_eq!(app.deps, vec![DepRef::Ident("my-lib".into())]);
        assert_eq!(projects[1].ident, "my-lib");
    }

    #[test]
    fn normalizes_names() {
        assert_eq!(normalize_name("My_Lib.X"), "my-lib-x");
        assert_eq!(requirement_name("Foo_Bar[extra]>=1 ; x"), "foo-bar");
    }
}
