//! Project model - the language-neutral unit of work in a workspace
//!
//! A `Project` is any directory the workspace knows how to build: an npm
//! package, a Cargo crate, a Go module, a Python package, or a plain
//! directory with a `neex.json` that declares shell commands.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Ecosystem a project belongs to
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Js,
    Rust,
    Go,
    Python,
    Generic,
}

impl Language {
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::Js => "js",
            Language::Rust => "rust",
            Language::Go => "go",
            Language::Python => "python",
            Language::Generic => "generic",
        }
    }
}

/// JS package manager, detected from the lockfile or `packageManager`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl PackageManager {
    pub fn as_str(&self) -> &'static str {
        match self {
            PackageManager::Npm => "npm",
            PackageManager::Pnpm => "pnpm",
            PackageManager::Yarn => "yarn",
            PackageManager::Bun => "bun",
        }
    }

    /// Shell command that runs a package.json script with the package's
    /// own `node_modules/.bin` on PATH
    pub fn run_script(&self, script: &str) -> String {
        format!("{} run {}", self.as_str(), script)
    }
}

/// How a task is executed
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TaskCommand {
    /// A package.json script, run through the package manager
    Script {
        pm: PackageManager,
        name: String,
        /// The script body, hashed so editing the script busts the cache
        body: String,
    },
    /// A plain shell command
    Shell(String),
}

impl TaskCommand {
    /// The command line handed to the shell
    pub fn shell_line(&self) -> String {
        match self {
            TaskCommand::Script { pm, name, .. } => pm.run_script(name),
            TaskCommand::Shell(cmd) => cmd.clone(),
        }
    }

    /// Bytes that identify this command for hashing
    pub fn hash_material(&self) -> String {
        match self {
            TaskCommand::Script { pm, name, body } => {
                format!("script:{}:{}:{}", pm.as_str(), name, body)
            }
            TaskCommand::Shell(cmd) => format!("shell:{}", cmd),
        }
    }
}

/// A reference to another project, resolved by the workspace
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DepRef {
    /// Ecosystem identifier: npm name, crate name, Go module path, PEP 503 name
    Ident(String),
    /// Workspace-relative path to the dependency's root
    Path(PathBuf),
}

/// A discovered project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// Unique display name; qualified (`rust:utils`) on collision
    pub name: String,
    /// Ecosystem identifier used for dependency resolution
    pub ident: String,
    /// Root directory, relative to the workspace root, `/`-separated
    pub root: PathBuf,
    pub language: Language,
    /// The manifest this project was discovered from, relative to root
    pub manifest: PathBuf,
    /// Tasks the project declares itself (scripts, neex.json commands)
    pub tasks: BTreeMap<String, TaskCommand>,
    /// Local dependencies, resolved later by the workspace
    pub deps: Vec<DepRef>,
    /// Projects sharing a lock group are never run in parallel
    /// (e.g. all crates in one Cargo workspace share a target dir lock)
    pub lock_group: Option<String>,
}

impl Project {
    /// Project root as an absolute path
    pub fn abs_root(&self, workspace_root: &Path) -> PathBuf {
        workspace_root.join(&self.root)
    }

    /// Root as a `/`-separated string for hashing and display
    pub fn root_str(&self) -> String {
        normalize_path(&self.root)
    }
}

/// Convert a path to a `/`-separated string without a trailing slash.
/// `.` (the workspace root itself) becomes an empty string.
pub fn normalize_path(path: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for c in path.components() {
        match c {
            std::path::Component::Normal(s) => parts.push(s.to_string_lossy().to_string()),
            // `a/b/../c` → `a/c`, resolved lexically
            std::path::Component::ParentDir => {
                if parts.last().map(|p| p != "..").unwrap_or(false) {
                    parts.pop();
                } else {
                    parts.push("..".to_string());
                }
            }
            _ => {}
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_curdir_and_uses_slashes() {
        assert_eq!(normalize_path(Path::new("./apps/web/")), "apps/web");
        assert_eq!(normalize_path(Path::new(".")), "");
        assert_eq!(normalize_path(Path::new("a/./b")), "a/b");
        assert_eq!(
            normalize_path(Path::new("services/api/../../libs/x")),
            "libs/x"
        );
        assert_eq!(normalize_path(Path::new("../outside")), "../outside");
    }

    #[test]
    fn script_command_shell_line() {
        let cmd = TaskCommand::Script {
            pm: PackageManager::Pnpm,
            name: "build".into(),
            body: "tsc".into(),
        };
        assert_eq!(cmd.shell_line(), "pnpm run build");
        assert!(cmd.hash_material().contains("tsc"));
    }
}
