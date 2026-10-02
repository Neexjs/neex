//! JavaScript / TypeScript provider
//!
//! Discovery sources, in order:
//! - `pnpm-workspace.yaml` `packages` (with `!` negation)
//! - root `package.json` `workspaces` (array or `{ packages: [...] }`)
//!
//! Dependencies come from `dependencies`, `devDependencies`,
//! `peerDependencies` and `optionalDependencies` that name another
//! workspace package (including `workspace:` protocol versions).

use super::{expand_project_globs, read_optional, DiscoveryCtx, WorkspaceProvider};
use crate::project::{DepRef, Language, PackageManager, Project, TaskCommand};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct JsProvider;

#[derive(Debug, Deserialize)]
struct PackageJson {
    name: Option<String>,
    #[serde(default)]
    workspaces: Option<Workspaces>,
    #[serde(default)]
    scripts: BTreeMap<String, String>,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
    #[serde(default, rename = "devDependencies")]
    dev_dependencies: BTreeMap<String, String>,
    #[serde(default, rename = "peerDependencies")]
    peer_dependencies: BTreeMap<String, String>,
    #[serde(default, rename = "optionalDependencies")]
    optional_dependencies: BTreeMap<String, String>,
    #[serde(default, rename = "packageManager")]
    package_manager: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Workspaces {
    List(Vec<String>),
    Object {
        #[serde(default)]
        packages: Vec<String>,
    },
}

#[derive(Debug, Deserialize)]
struct PnpmWorkspace {
    #[serde(default)]
    packages: Vec<String>,
}

/// Workspace globs declared by the JS ecosystem at `root`
pub fn workspace_globs(root: &Path) -> Result<Vec<String>> {
    if let Some(yaml) = read_optional(&root.join("pnpm-workspace.yaml"))? {
        let ws: PnpmWorkspace =
            serde_yaml_ng::from_str(&yaml).context("parsing pnpm-workspace.yaml")?;
        if !ws.packages.is_empty() {
            return Ok(ws.packages);
        }
    }
    if let Some(text) = read_optional(&root.join("package.json"))? {
        let pkg: PackageJson = serde_json::from_str(&text).context("parsing package.json")?;
        return Ok(match pkg.workspaces {
            Some(Workspaces::List(l)) => l,
            Some(Workspaces::Object { packages }) => packages,
            None => vec![],
        });
    }
    Ok(vec![])
}

/// Detect the package manager from `packageManager` or the lockfile
pub fn detect_package_manager(root: &Path) -> PackageManager {
    if let Ok(Some(text)) = read_optional(&root.join("package.json")) {
        if let Ok(pkg) = serde_json::from_str::<PackageJson>(&text) {
            if let Some(pm) = pkg.package_manager {
                let name = pm.split('@').next().unwrap_or("");
                match name {
                    "pnpm" => return PackageManager::Pnpm,
                    "yarn" => return PackageManager::Yarn,
                    "bun" => return PackageManager::Bun,
                    "npm" => return PackageManager::Npm,
                    _ => {}
                }
            }
        }
    }
    if root.join("pnpm-lock.yaml").exists() {
        PackageManager::Pnpm
    } else if root.join("bun.lock").exists() || root.join("bun.lockb").exists() {
        PackageManager::Bun
    } else if root.join("yarn.lock").exists() {
        PackageManager::Yarn
    } else {
        PackageManager::Npm
    }
}

impl WorkspaceProvider for JsProvider {
    fn language(&self) -> Language {
        Language::Js
    }

    fn discover(&self, ctx: &DiscoveryCtx) -> Result<Vec<Project>> {
        let mut globs = workspace_globs(ctx.root)?;
        globs.extend(ctx.extra_globs.iter().cloned());
        let standalone = globs.is_empty();
        let pm = detect_package_manager(ctx.root);
        let mut projects = Vec::new();
        let dirs = if standalone {
            vec![PathBuf::new()]
        } else {
            expand_project_globs(ctx.root, &globs)?
        };
        for dir in dirs {
            let manifest = ctx.root.join(&dir).join("package.json");
            let Some(text) = read_optional(&manifest)? else {
                continue;
            };
            let pkg: PackageJson = serde_json::from_str(&text)
                .with_context(|| format!("parsing {}", manifest.display()))?;
            let Some(name) = pkg.name.clone().or_else(|| {
                standalone.then(|| {
                    ctx.root
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                })
            }) else {
                tracing::warn!("{}: package has no name, skipped", manifest.display());
                continue;
            };
            let tasks = pkg
                .scripts
                .iter()
                .map(|(k, body)| {
                    (
                        k.clone(),
                        TaskCommand::Script {
                            pm,
                            name: k.clone(),
                            body: body.clone(),
                        },
                    )
                })
                .collect();
            let deps = pkg
                .dependencies
                .keys()
                .chain(pkg.dev_dependencies.keys())
                .chain(pkg.peer_dependencies.keys())
                .chain(pkg.optional_dependencies.keys())
                .map(|d| DepRef::Ident(d.clone()))
                .collect();
            projects.push(Project {
                name: name.clone(),
                ident: name,
                root: dir,
                language: Language::Js,
                manifest: PathBuf::from("package.json"),
                tasks,
                deps,
                lock_group: None,
            });
        }
        Ok(projects)
    }

    fn default_command(&self, _project: &Project, _task: &str) -> Option<TaskCommand> {
        // JS tasks are always scripts; a package without the script is a no-op
        None
    }

    fn global_inputs(&self, root: &Path) -> Vec<PathBuf> {
        [
            "package.json",
            "pnpm-workspace.yaml",
            "pnpm-lock.yaml",
            "package-lock.json",
            "yarn.lock",
            "bun.lock",
            "bun.lockb",
            ".npmrc",
            ".nvmrc",
            ".node-version",
        ]
        .iter()
        .map(PathBuf::from)
        .filter(|p| root.join(p).is_file())
        .collect()
    }

    fn implicit_env(&self) -> &'static [&'static str] {
        &["NODE_ENV"]
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
    fn discovers_pnpm_workspace_with_negation() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "package.json", r#"{"name":"root","private":true}"#);
        write(root, "pnpm-lock.yaml", "");
        write(
            root,
            "pnpm-workspace.yaml",
            "packages:\n  - 'packages/*'\n  - '!packages/skip'\n",
        );
        write(
            root,
            "packages/ui/package.json",
            r#"{"name":"@acme/ui","scripts":{"build":"tsc"},"dependencies":{"@acme/utils":"workspace:*","react":"^19"}}"#,
        );
        write(
            root,
            "packages/utils/package.json",
            r#"{"name":"@acme/utils"}"#,
        );
        write(
            root,
            "packages/skip/package.json",
            r#"{"name":"@acme/skip"}"#,
        );

        let ctx = DiscoveryCtx {
            root,
            extra_globs: &[],
        };
        let projects = JsProvider.discover(&ctx).unwrap();
        let names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["@acme/ui", "@acme/utils"]);
        let ui = &projects[0];
        assert_eq!(
            ui.tasks["build"],
            TaskCommand::Script {
                pm: PackageManager::Pnpm,
                name: "build".into(),
                body: "tsc".into()
            }
        );
        assert!(ui.deps.contains(&DepRef::Ident("@acme/utils".into())));
        assert_eq!(detect_package_manager(root), PackageManager::Pnpm);
    }

    #[test]
    fn discovers_package_json_object_form() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "package.json",
            r#"{"name":"root","workspaces":{"packages":["apps/*"]},"packageManager":"yarn@4.0.0"}"#,
        );
        write(root, "apps/web/package.json", r#"{"name":"web"}"#);
        let ctx = DiscoveryCtx {
            root,
            extra_globs: &[],
        };
        let projects = JsProvider.discover(&ctx).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].root, PathBuf::from("apps/web"));
        assert_eq!(detect_package_manager(root), PackageManager::Yarn);
    }
}
