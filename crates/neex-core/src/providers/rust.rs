//! Rust provider
//!
//! Reads `[workspace] members` / `exclude` from the root `Cargo.toml`
//! (no `cargo metadata` call: it is slow and may touch the network).
//! Dependencies are `path = "..."` entries, and `x.workspace = true`
//! entries that resolve to a path in `[workspace.dependencies]`.

use super::{expand_project_globs, read_optional, DiscoveryCtx, WorkspaceProvider};
use crate::project::{normalize_path, DepRef, Language, Project, TaskCommand};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct RustProvider;

#[derive(Debug, Deserialize, Default)]
struct CargoToml {
    #[serde(default)]
    workspace: Option<CargoWorkspace>,
    #[serde(default)]
    package: Option<CargoPackage>,
    #[serde(default)]
    dependencies: BTreeMap<String, DepSpec>,
    #[serde(default, rename = "dev-dependencies")]
    dev_dependencies: BTreeMap<String, DepSpec>,
    #[serde(default, rename = "build-dependencies")]
    build_dependencies: BTreeMap<String, DepSpec>,
    #[serde(default)]
    target: BTreeMap<String, CargoTarget>,
}

#[derive(Debug, Deserialize, Default)]
struct CargoWorkspace {
    #[serde(default)]
    members: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default)]
    dependencies: BTreeMap<String, DepSpec>,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    name: String,
}

#[derive(Debug, Deserialize, Default)]
struct CargoTarget {
    #[serde(default)]
    dependencies: BTreeMap<String, DepSpec>,
    #[serde(default, rename = "dev-dependencies")]
    dev_dependencies: BTreeMap<String, DepSpec>,
    #[serde(default, rename = "build-dependencies")]
    build_dependencies: BTreeMap<String, DepSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DepSpec {
    Version(#[allow(dead_code)] String),
    Table {
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        workspace: Option<bool>,
        #[serde(default)]
        package: Option<String>,
    },
}

fn local_dep(
    name: &str,
    spec: &DepSpec,
    crate_dir: &Path,
    ws_root: &Path,
    ws_deps: &BTreeMap<String, DepSpec>,
) -> Option<DepRef> {
    match spec {
        DepSpec::Version(_) => None,
        DepSpec::Table {
            path: Some(p),
            package,
            ..
        } => {
            // path deps: resolve relative to this crate, keep as workspace-relative path
            let abs = crate_dir.join(p);
            let rel = abs
                .strip_prefix(ws_root)
                .ok()
                .map(|r| PathBuf::from(normalize_path(r)));
            let _ = package;
            rel.map(DepRef::Path)
        }
        DepSpec::Table {
            workspace: Some(true),
            ..
        } => match ws_deps.get(name) {
            Some(DepSpec::Table { path: Some(p), .. }) => {
                let rel = ws_root.join(p);
                rel.strip_prefix(ws_root)
                    .ok()
                    .map(|r| DepRef::Path(PathBuf::from(normalize_path(r))))
            }
            _ => None,
        },
        DepSpec::Table { .. } => None,
    }
}

impl WorkspaceProvider for RustProvider {
    fn language(&self) -> Language {
        Language::Rust
    }

    fn discover(&self, ctx: &DiscoveryCtx) -> Result<Vec<Project>> {
        let root = ctx.root;
        let Some(text) = read_optional(&root.join("Cargo.toml"))? else {
            return Ok(vec![]);
        };
        let manifest: CargoToml = toml::from_str(&text).context("parsing Cargo.toml")?;

        let mut globs: Vec<String> = Vec::new();
        let mut ws_deps = BTreeMap::new();
        if let Some(ws) = manifest.workspace {
            globs.extend(ws.members.iter().cloned());
            globs.extend(ws.exclude.iter().map(|e| format!("!{}", e)));
            ws_deps = ws.dependencies;
        }
        // A root Cargo.toml with [package] is itself a crate
        let mut dirs: Vec<PathBuf> = if globs.is_empty() {
            vec![]
        } else {
            expand_project_globs(root, &globs)?
        };
        if manifest.package.is_some() {
            dirs.insert(0, PathBuf::new());
        }
        // Extra globs from neex.json may name crate dirs too
        for dir in expand_project_globs(root, ctx.extra_globs)? {
            if root.join(&dir).join("Cargo.toml").is_file() && !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }

        let lock_group = Some(format!("cargo:{}", normalize_path(Path::new(""))));
        let mut projects = Vec::new();
        for dir in dirs {
            let crate_dir = root.join(&dir);
            let path = crate_dir.join("Cargo.toml");
            let Some(text) = read_optional(&path)? else {
                continue;
            };
            let m: CargoToml =
                toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
            let Some(pkg) = m.package else {
                continue; // a virtual manifest inside members
            };
            let mut deps = Vec::new();
            let mut tables: Vec<&BTreeMap<String, DepSpec>> =
                vec![&m.dependencies, &m.dev_dependencies, &m.build_dependencies];
            for t in m.target.values() {
                tables.push(&t.dependencies);
                tables.push(&t.dev_dependencies);
                tables.push(&t.build_dependencies);
            }
            for table in tables {
                for (name, spec) in table {
                    if let Some(d) = local_dep(name, spec, &crate_dir, root, &ws_deps) {
                        if !deps.contains(&d) {
                            deps.push(d);
                        }
                    }
                }
            }
            projects.push(Project {
                name: pkg.name.clone(),
                ident: pkg.name,
                root: dir,
                language: Language::Rust,
                manifest: PathBuf::from("Cargo.toml"),
                tasks: BTreeMap::new(),
                deps,
                lock_group: lock_group.clone(),
            });
        }
        Ok(projects)
    }

    fn default_command(&self, project: &Project, task: &str) -> Option<TaskCommand> {
        let p = &project.ident;
        let cmd = match task {
            "build" => format!("cargo build -p {}", p),
            "test" => format!("cargo test -p {}", p),
            "lint" => format!("cargo clippy -p {} -- -D warnings", p),
            "check" | "typecheck" => format!("cargo check -p {}", p),
            "format:check" => "cargo fmt --check".to_string(),
            "dev" | "start" => format!("cargo run -p {}", p),
            _ => return None,
        };
        Some(TaskCommand::Shell(cmd))
    }

    fn global_inputs(&self, root: &Path) -> Vec<PathBuf> {
        [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            "rust-toolchain",
        ]
        .iter()
        .map(PathBuf::from)
        .filter(|p| root.join(p).is_file())
        .collect()
    }

    fn implicit_env(&self) -> &'static [&'static str] {
        &[
            "RUSTFLAGS",
            "CARGO_BUILD_TARGET",
            "CARGO_PROFILE_RELEASE_LTO",
        ]
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
    fn discovers_cargo_workspace_with_path_and_workspace_deps() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "Cargo.toml",
            r#"[workspace]
members = ["crates/*"]
exclude = ["crates/old"]
[workspace.dependencies]
core = { path = "crates/core" }
"#,
        );
        write(
            root,
            "crates/core/Cargo.toml",
            "[package]\nname = \"core\"\n",
        );
        write(
            root,
            "crates/cli/Cargo.toml",
            "[package]\nname = \"cli\"\n[dependencies]\ncore = { workspace = true }\ndaemon = { path = \"../daemon\" }\nserde = \"1\"\n",
        );
        write(
            root,
            "crates/daemon/Cargo.toml",
            "[package]\nname = \"daemon\"\n",
        );
        write(root, "crates/old/Cargo.toml", "[package]\nname = \"old\"\n");

        let ctx = DiscoveryCtx {
            root,
            extra_globs: &[],
        };
        let projects = RustProvider.discover(&ctx).unwrap();
        let names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["cli", "core", "daemon"]);
        let cli = &projects[0];
        assert!(cli
            .deps
            .contains(&DepRef::Path(PathBuf::from("crates/core"))));
        assert!(cli
            .deps
            .contains(&DepRef::Path(PathBuf::from("crates/daemon"))));
        assert_eq!(cli.deps.len(), 2);
        assert!(cli.lock_group.is_some());
        assert_eq!(
            RustProvider
                .default_command(cli, "build")
                .unwrap()
                .shell_line(),
            "cargo build -p cli"
        );
    }
}
