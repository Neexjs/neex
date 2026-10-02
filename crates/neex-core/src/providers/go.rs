//! Go provider
//!
//! Modules come from `go.work` `use` directives; without a `go.work`, the
//! root `go.mod` is the single module. A `require` of another workspace
//! module becomes an `Ident` dep; a `replace X => ./p` becomes a `Path` dep.

use super::{expand_project_globs, read_optional, DiscoveryCtx, WorkspaceProvider};
use crate::project::{normalize_path, DepRef, Language, Project, TaskCommand};
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct GoProvider;

#[derive(Debug, Default)]
struct GoMod {
    module: Option<String>,
    requires: Vec<String>,
    /// (from, to-path) for local replaces
    replaces: Vec<(String, String)>,
}

/// Minimal go.mod / go.work parser (handles single-line and block forms)
fn parse_go_file(text: &str) -> (GoMod, Vec<String>) {
    let mut m = GoMod::default();
    let mut uses = Vec::new();
    let mut block: Option<&str> = None;

    for raw in text.lines() {
        let line = raw.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(kind) = block {
            if line == ")" {
                block = None;
                continue;
            }
            handle_directive(kind, line, &mut m, &mut uses);
            continue;
        }
        let mut parts = line.splitn(2, ' ');
        let kw = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("").trim();
        match kw {
            "module" => m.module = Some(rest.trim_matches('"').to_string()),
            "require" | "replace" | "use" => {
                if rest == "(" {
                    block = Some(kw);
                } else {
                    handle_directive(kw, rest, &mut m, &mut uses);
                }
            }
            _ => {}
        }
    }
    (m, uses)
}

fn handle_directive(kind: &str, line: &str, m: &mut GoMod, uses: &mut Vec<String>) {
    match kind {
        "require" => {
            if let Some(path) = line.split_whitespace().next() {
                m.requires.push(path.to_string());
            }
        }
        "replace" => {
            if let Some((from, to)) = line.split_once("=>") {
                let from = from.split_whitespace().next().unwrap_or("").to_string();
                let to = to.split_whitespace().next().unwrap_or("").trim_matches('"');
                if to.starts_with('.') || to.starts_with('/') {
                    m.replaces.push((from, to.to_string()));
                }
            }
        }
        "use" => uses.push(line.trim_matches('"').to_string()),
        _ => {}
    }
}

impl WorkspaceProvider for GoProvider {
    fn language(&self) -> Language {
        Language::Go
    }

    fn discover(&self, ctx: &DiscoveryCtx) -> Result<Vec<Project>> {
        let root = ctx.root;
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(work) = read_optional(&root.join("go.work"))? {
            let (_, uses) = parse_go_file(&work);
            for u in uses {
                let rel = root.join(&u);
                if let Ok(r) = rel.strip_prefix(root) {
                    dirs.push(PathBuf::from(normalize_path(r)));
                } else if u == "." {
                    dirs.push(PathBuf::new());
                }
            }
        } else if root.join("go.mod").is_file() {
            dirs.push(PathBuf::new());
        }
        for dir in expand_project_globs(root, ctx.extra_globs)? {
            if root.join(&dir).join("go.mod").is_file() && !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }

        // First pass: module paths so `require` can be matched to local modules
        let mut mods: Vec<(PathBuf, GoMod)> = Vec::new();
        for dir in dirs {
            let Some(text) = read_optional(&root.join(&dir).join("go.mod"))? else {
                continue;
            };
            let (m, _) = parse_go_file(&text);
            mods.push((dir, m));
        }
        let local: BTreeMap<String, PathBuf> = mods
            .iter()
            .filter_map(|(d, m)| m.module.clone().map(|name| (name, d.clone())))
            .collect();

        let mut projects = Vec::new();
        for (dir, m) in mods {
            let Some(module) = m.module.clone() else {
                continue;
            };
            let mut deps: Vec<DepRef> = Vec::new();
            for (from, to) in &m.replaces {
                let abs = root.join(&dir).join(to);
                if let Ok(rel) = abs.strip_prefix(root) {
                    deps.push(DepRef::Path(PathBuf::from(normalize_path(rel))));
                } else {
                    deps.push(DepRef::Ident(from.clone()));
                }
            }
            for r in &m.requires {
                if local.contains_key(r) && !m.replaces.iter().any(|(f, _)| f == r) {
                    deps.push(DepRef::Ident(r.clone()));
                }
            }
            let name = module.rsplit('/').next().unwrap_or(&module).to_string();
            projects.push(Project {
                name,
                ident: module,
                root: dir,
                language: Language::Go,
                manifest: PathBuf::from("go.mod"),
                tasks: BTreeMap::new(),
                deps,
                lock_group: None,
            });
        }
        Ok(projects)
    }

    fn default_command(&self, _project: &Project, task: &str) -> Option<TaskCommand> {
        let cmd = match task {
            "build" => "go build ./...",
            "test" => "go test ./...",
            "lint" => "go vet ./...",
            "dev" | "start" => "go run .",
            _ => return None,
        };
        Some(TaskCommand::Shell(cmd.to_string()))
    }

    fn global_inputs(&self, root: &Path) -> Vec<PathBuf> {
        ["go.work", "go.work.sum", "go.mod", "go.sum"]
            .iter()
            .map(PathBuf::from)
            .filter(|p| root.join(p).is_file())
            .collect()
    }

    fn implicit_env(&self) -> &'static [&'static str] {
        &["GOOS", "GOARCH", "CGO_ENABLED", "GOFLAGS"]
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
    fn discovers_go_work_modules() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "go.work",
            "go 1.22\n\nuse (\n\t./services/api\n\t./libs/shared\n)\n",
        );
        write(
            root,
            "services/api/go.mod",
            "module example.com/api\n\ngo 1.22\n\nrequire (\n\texample.com/shared v0.0.0\n\tgithub.com/x/y v1.0.0\n)\n\nreplace example.com/shared => ../../libs/shared\n",
        );
        write(root, "libs/shared/go.mod", "module example.com/shared\n");

        let ctx = DiscoveryCtx {
            root,
            extra_globs: &[],
        };
        let projects = GoProvider.discover(&ctx).unwrap();
        assert_eq!(projects.len(), 2);
        let api = projects.iter().find(|p| p.name == "api").unwrap();
        assert_eq!(api.ident, "example.com/api");
        assert_eq!(api.deps, vec![DepRef::Path(PathBuf::from("libs/shared"))]);
    }

    #[test]
    fn single_module_without_go_work() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "go.mod", "module example.com/app\n");
        let ctx = DiscoveryCtx {
            root,
            extra_globs: &[],
        };
        let projects = GoProvider.discover(&ctx).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].root, PathBuf::new());
    }
}
