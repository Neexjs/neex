//! Workspace providers - one per ecosystem
//!
//! A provider reads the ecosystem's own manifests (package.json workspaces,
//! Cargo [workspace], go.work, uv workspaces) and turns them into
//! [`Project`]s. Nothing here needs neex-specific config.

pub mod generic;
pub mod go;
pub mod js;
pub mod python;
pub mod rust;

use crate::project::{Language, Project, TaskCommand};
use anyhow::Result;
use globset::{GlobSet, GlobSetBuilder};
use std::path::{Path, PathBuf};

/// Directories never descended into when expanding project globs
pub const SKIP_DIRS: &[&str] = &[
    ".git",
    ".neex",
    "node_modules",
    "target",
    ".venv",
    "venv",
    "__pycache__",
    "dist",
    "build",
    ".next",
    ".turbo",
    ".cache",
];

/// Everything a provider may look at
pub struct DiscoveryCtx<'a> {
    pub root: &'a Path,
    /// Extra project globs from `neex.json` `projects`
    pub extra_globs: &'a [String],
}

pub trait WorkspaceProvider: Send + Sync {
    fn language(&self) -> Language;

    /// Find projects of this ecosystem. Return an empty list when the
    /// workspace has none; return an error only for a malformed manifest.
    fn discover(&self, ctx: &DiscoveryCtx) -> Result<Vec<Project>>;

    /// Default command for a task the project does not declare itself
    fn default_command(&self, project: &Project, task: &str) -> Option<TaskCommand>;

    /// Root-relative files that affect every project of this ecosystem
    /// (lockfiles, toolchain pins)
    fn global_inputs(&self, root: &Path) -> Vec<PathBuf>;

    /// Env vars that change build output for this ecosystem
    fn implicit_env(&self) -> &'static [&'static str];
}

/// All providers, in precedence order for merging
pub fn all_providers() -> Vec<Box<dyn WorkspaceProvider>> {
    vec![
        Box::new(generic::GenericProvider),
        Box::new(js::JsProvider),
        Box::new(rust::RustProvider),
        Box::new(go::GoProvider),
        Box::new(python::PythonProvider),
    ]
}

pub fn provider_for(language: Language) -> Box<dyn WorkspaceProvider> {
    match language {
        Language::Generic => Box::new(generic::GenericProvider),
        Language::Js => Box::new(js::JsProvider),
        Language::Rust => Box::new(rust::RustProvider),
        Language::Go => Box::new(go::GoProvider),
        Language::Python => Box::new(python::PythonProvider),
    }
}

/// Expand workspace globs (`packages/*`, `apps/**`, `!packages/internal`)
/// into directories under `root`. Patterns may also point at files
/// (`crates/*/Cargo.toml`), in which case the file's directory is returned.
///
/// Results are root-relative, deduplicated and sorted.
pub fn expand_project_globs(root: &Path, patterns: &[String]) -> Result<Vec<PathBuf>> {
    let mut include = GlobSetBuilder::new();
    let mut exclude = GlobSetBuilder::new();
    let mut max_depth = 0usize;
    let mut has_include = false;

    for pat in patterns {
        let pat = pat.trim().trim_end_matches('/');
        if pat.is_empty() {
            continue;
        }
        let (neg, body) = match pat.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, pat),
        };
        let body = body.trim_start_matches("./");
        let glob = crate::inputs::compile_glob(body)?;
        if neg {
            exclude.add(glob);
        } else {
            include.add(glob);
            has_include = true;
            max_depth = max_depth.max(glob_depth(body));
        }
    }
    if !has_include {
        return Ok(vec![]);
    }
    let include: GlobSet = include.build()?;
    let exclude: GlobSet = exclude.build()?;

    let mut out: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = vec![(PathBuf::new(), 0)];
    while let Some((rel, depth)) = stack.pop() {
        let abs = root.join(&rel);
        let Ok(entries) = std::fs::read_dir(&abs) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            let child_rel = if rel.as_os_str().is_empty() {
                PathBuf::from(name_str.as_ref())
            } else {
                rel.join(name_str.as_ref())
            };
            let child_str = crate::project::normalize_path(&child_rel);
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

            if is_dir {
                if SKIP_DIRS.contains(&name_str.as_ref()) {
                    continue;
                }
                if include.is_match(&child_str) && !exclude.is_match(&child_str) {
                    out.push(child_rel.clone());
                }
                if depth + 1 < max_depth {
                    stack.push((child_rel, depth + 1));
                }
            } else if include.is_match(&child_str) && !exclude.is_match(&child_str) {
                if let Some(parent) = child_rel.parent() {
                    if !exclude.is_match(crate::project::normalize_path(parent)) {
                        out.push(parent.to_path_buf());
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// How many directory levels a glob can reach (`**` = unbounded)
fn glob_depth(pattern: &str) -> usize {
    if pattern.contains("**") {
        return 64;
    }
    pattern.split('/').filter(|s| !s.is_empty()).count()
}

/// Read a file to a string, or `None` if it does not exist
pub(crate) fn read_optional(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }

    #[test]
    fn expands_globs_with_negation_and_file_patterns() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        touch(&root.join("packages/a/package.json"));
        touch(&root.join("packages/b/package.json"));
        touch(&root.join("packages/internal/package.json"));
        touch(&root.join("packages/a/node_modules/x/package.json"));
        touch(&root.join("crates/core/Cargo.toml"));
        touch(&root.join("apps/deep/web/package.json"));

        let dirs = expand_project_globs(
            root,
            &[
                "packages/*".into(),
                "!packages/internal".into(),
                "crates/*/Cargo.toml".into(),
                "apps/**".into(),
            ],
        )
        .unwrap();
        let names: Vec<String> = dirs
            .iter()
            .map(|p| crate::project::normalize_path(p))
            .collect();
        assert_eq!(
            names,
            vec![
                "apps/deep",
                "apps/deep/web",
                "crates/core",
                "packages/a",
                "packages/b"
            ]
        );
    }
}
