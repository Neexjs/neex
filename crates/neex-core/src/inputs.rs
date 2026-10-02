//! Input hashing - which files feed a task's cache key, and their hashes
//!
//! Rules:
//! - Paths are workspace-relative and `/`-separated; absolute paths never
//!   enter a hash, so keys are portable across machines.
//! - `.gitignore` files inside the workspace are honoured whether or not the
//!   checkout is a git repo; ignore files *above* the workspace root are not
//!   read (a `~/.gitignore` with `*` must never hide sources). Leaving a file
//!   out of a key causes wrong cache hits, while an extra file only costs a
//!   miss, so every rule errs towards including.
//! - Only directories that never hold sources are skipped: `.git`, `.neex`,
//!   `node_modules`, `.turbo`, `.venv`, `__pycache__`, and `target` next to a
//!   `Cargo.toml`. Roots of other projects nested inside this one are skipped.
//! - Declared `outputs` are excluded from inputs.
//! - The executable bit is part of a file's hash.

use crate::project::normalize_path;
use anyhow::{anyhow, Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Per-run memo of file hashes, keyed by absolute path
#[derive(Default)]
pub struct FileHashCache {
    inner: Mutex<HashMap<PathBuf, String>>,
}

impl FileHashCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// blake3(content) with an `x` suffix when the file is executable
    pub fn hash_file(&self, abs: &Path) -> Result<String> {
        if let Some(h) = self.inner.lock().unwrap().get(abs) {
            return Ok(h.clone());
        }
        let h = hash_file_uncached(abs)?;
        self.inner
            .lock()
            .unwrap()
            .insert(abs.to_path_buf(), h.clone());
        Ok(h)
    }
}

pub fn hash_file_uncached(abs: &Path) -> Result<String> {
    let content = std::fs::read(abs)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(&content);
    let mut hex = hasher.finalize().to_hex().to_string();
    if is_executable(abs) {
        hex.push('x');
    }
    Ok(hex)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    false
}

/// Compiled include/exclude globs
pub struct InputFilter {
    include: Option<GlobSet>,
    exclude: GlobSet,
}

impl InputFilter {
    /// `inputs` are project-relative globs; a leading `!` excludes. When
    /// there are no positive globs, every file is included. `outputs` are
    /// always excluded.
    pub fn new(inputs: Option<&[String]>, outputs: &[String]) -> Result<Self> {
        let mut inc = GlobSetBuilder::new();
        let mut exc = GlobSetBuilder::new();
        let mut has_inc = false;
        for pat in inputs.unwrap_or(&[]) {
            if pat.starts_with("//") {
                continue; // root-relative, handled by root_globs()
            }
            if let Some(rest) = pat.strip_prefix('!') {
                exc.add(glob_for(rest)?);
            } else {
                inc.add(glob_for(pat)?);
                has_inc = true;
            }
        }
        for pat in outputs {
            if let Some(rest) = pat.strip_prefix('!') {
                // an output negation re-includes; ignore for input purposes
                let _ = rest;
            } else {
                exc.add(glob_for(pat)?);
            }
        }
        Ok(Self {
            include: if has_inc { Some(inc.build()?) } else { None },
            exclude: exc.build()?,
        })
    }

    pub fn matches(&self, rel: &str) -> bool {
        if self.exclude.is_match(rel) {
            return false;
        }
        match &self.include {
            Some(inc) => inc.is_match(rel),
            None => true,
        }
    }

    /// Root-relative input globs (`//tsconfig.base.json`)
    pub fn root_globs(inputs: Option<&[String]>) -> Vec<String> {
        inputs
            .unwrap_or(&[])
            .iter()
            .filter_map(|p| p.strip_prefix("//").map(|s| s.to_string()))
            .collect()
    }
}

/// Compile a workspace glob. `*` stays within one path segment and `**`
/// spans directories, as in gitignore, pnpm and turbo globs.
pub fn compile_glob(pattern: &str) -> Result<Glob> {
    let pat = pattern.trim_start_matches("./");
    globset::GlobBuilder::new(pat)
        .literal_separator(true)
        .build()
        .map_err(|e| anyhow!("bad glob `{}`: {}", pat, e))
}

fn glob_for(pat: &str) -> Result<Glob> {
    compile_glob(pat)
}

/// Directories that never contain source inputs, in any ecosystem
const INPUT_SKIP_DIRS: &[&str] = &[
    ".git",
    ".neex",
    "node_modules",
    ".turbo",
    ".venv",
    "__pycache__",
];

/// `target/` is cargo's build directory only when it sits next to a
/// `Cargo.toml`; anywhere else it may hold real sources
fn skip_input_dir(dir: &Path, name: &str) -> bool {
    INPUT_SKIP_DIRS.contains(&name)
        || (name == "target"
            && dir
                .parent()
                .map(|p| p.join("Cargo.toml").is_file())
                .unwrap_or(false))
}

/// A gitignore-aware walk rooted at the workspace root that never reads
/// ignore files above it
fn workspace_walker(workspace_root: &Path) -> WalkBuilder {
    let mut w = WalkBuilder::new(workspace_root);
    w.hidden(false)
        .parents(false)
        .require_git(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .ignore(true)
        .follow_links(false);
    w
}

/// Hash every input file of a project directory.
///
/// Returns project-relative path → hash, sorted.
pub fn hash_project_inputs(
    workspace_root: &Path,
    project_root_rel: &Path,
    nested_project_roots: &[String],
    filter: &InputFilter,
    cache: &FileHashCache,
) -> Result<BTreeMap<String, String>> {
    if !workspace_root.join(project_root_rel).is_dir() {
        return Err(anyhow!(
            "input project directory is missing: {}",
            workspace_root.join(project_root_rel).display()
        ));
    }
    let project_rel_str = normalize_path(project_root_rel);
    let prefix = if project_rel_str.is_empty() {
        String::new()
    } else {
        format!("{}/", project_rel_str)
    };
    let nested: Vec<String> = nested_project_roots
        .iter()
        .filter(|r| {
            !r.is_empty() && **r != project_rel_str && (prefix.is_empty() || r.starts_with(&prefix))
        })
        .cloned()
        .collect();

    // The walk starts at the workspace root so the root .gitignore applies
    // to every project, and only descends through the project's ancestors
    let filter_root = workspace_root.to_path_buf();
    let project = project_rel_str.clone();
    let mut walker = workspace_walker(workspace_root);
    walker.filter_entry(move |entry| {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            return true;
        }
        let Ok(rel) = entry.path().strip_prefix(&filter_root) else {
            return true;
        };
        let rel = normalize_path(rel);
        if rel.is_empty() {
            return true;
        }
        let inside =
            project.is_empty() || rel == project || rel.starts_with(&format!("{}/", project));
        if !inside {
            return project.starts_with(&format!("{}/", rel));
        }
        if skip_input_dir(entry.path(), &entry.file_name().to_string_lossy()) {
            return false;
        }
        !nested.contains(&rel)
    });

    let entries = walker.build().collect::<std::result::Result<Vec<_>, _>>()?;
    let files: Vec<PathBuf> = entries
        .into_iter()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.into_path())
        .collect();

    let hashed: Vec<(String, String)> = files
        .par_iter()
        .map(|abs| -> Result<Option<(String, String)>> {
            let ws_rel = normalize_path(abs.strip_prefix(workspace_root)?);
            let rel = if prefix.is_empty() {
                ws_rel
            } else {
                let Some(rel) = ws_rel.strip_prefix(&prefix) else {
                    return Ok(None);
                };
                rel.to_string()
            };
            if !filter.matches(&rel) {
                return Ok(None);
            }
            let hash = cache
                .hash_file(abs)
                .with_context(|| format!("hashing input {}", abs.display()))?;
            Ok(Some((rel, hash)))
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();

    Ok(hashed.into_iter().collect())
}

/// Hash root-relative files matched by globs (for `globalInputs` and
/// `//`-prefixed task inputs). Literal paths that do not exist are recorded
/// with the hash `missing` so adding the file later changes the key.
pub fn hash_root_globs(
    workspace_root: &Path,
    globs: &[String],
    cache: &FileHashCache,
) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    if globs.is_empty() {
        return Ok(out);
    }
    let mut builder = GlobSetBuilder::new();
    let mut literal: Vec<String> = Vec::new();
    for g in globs {
        let g = g.trim_start_matches("./");
        if g.contains('*') || g.contains('?') || g.contains('[') || g.contains('{') {
            builder.add(glob_for(g)?);
        } else {
            literal.push(g.to_string());
        }
    }
    for l in &literal {
        let abs = workspace_root.join(l);
        if abs.is_file() {
            out.insert(l.clone(), cache.hash_file(&abs)?);
        } else {
            out.insert(l.clone(), "missing".into());
        }
    }
    let set = builder.build()?;
    if set.is_empty() {
        return Ok(out);
    }
    let walker = workspace_walker(workspace_root)
        .filter_entry(|e| {
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            !(is_dir && skip_input_dir(e.path(), &e.file_name().to_string_lossy()))
        })
        .build();
    let entries = walker.collect::<std::result::Result<Vec<_>, _>>()?;
    let files: Vec<PathBuf> = entries
        .into_iter()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.into_path())
        .collect();
    let hashed: Vec<(String, String)> = files
        .par_iter()
        .map(|abs| -> Result<Option<(String, String)>> {
            let rel = normalize_path(abs.strip_prefix(workspace_root)?);
            if !set.is_match(&rel) {
                return Ok(None);
            }
            let hash = cache
                .hash_file(abs)
                .with_context(|| format!("hashing global input {}", abs.display()))?;
            Ok(Some((rel, hash)))
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    out.extend(hashed);
    Ok(out)
}

/// Combine a sorted path→hash map into one digest
pub fn digest_map(map: &BTreeMap<String, String>) -> String {
    let mut hasher = blake3::Hasher::new();
    for (k, v) in map {
        hasher.update(k.as_bytes());
        hasher.update(&[0]);
        hasher.update(v.as_bytes());
        hasher.update(&[0]);
    }
    hasher.finalize().to_hex().to_string()
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
    fn respects_gitignore_skip_dirs_outputs_and_nested_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, ".gitignore", "*.log\n");
        write(root, "apps/web/src/a.ts", "a");
        write(root, "apps/web/debug.log", "x");
        write(root, "apps/web/node_modules/x/index.js", "x");
        write(root, "apps/web/dist/out.js", "x");
        write(root, "apps/web/.git/HEAD", "x");
        write(root, "apps/web/.neex/cache/x", "x");
        write(root, "apps/web/nested/package.json", "{}");
        write(root, "apps/web/README.md", "r");

        let cache = FileHashCache::new();
        let filter = InputFilter::new(Some(&["!README.md".into()]), &["dist/**".into()]).unwrap();
        let files = hash_project_inputs(
            root,
            Path::new("apps/web"),
            &["apps/web/nested".into()],
            &filter,
            &cache,
        )
        .unwrap();
        let keys: Vec<&str> = files.keys().map(|s| s.as_str()).collect();
        assert_eq!(keys, vec!["src/a.ts"]);

        // positive inputs restrict the set
        let filter = InputFilter::new(Some(&["src/**".into(), "README.md".into()]), &[]).unwrap();
        let files = hash_project_inputs(root, Path::new("apps/web"), &[], &filter, &cache).unwrap();
        let keys: Vec<&str> = files.keys().map(|s| s.as_str()).collect();
        assert_eq!(keys, vec!["README.md", "src/a.ts"]);
    }

    #[test]
    fn ignore_files_above_the_workspace_do_not_hide_sources() {
        let tmp = tempfile::tempdir().unwrap();
        let outer = tmp.path();
        // A dotfiles-style home .gitignore that ignores everything
        write(outer, ".gitignore", "*\n");
        let root = outer.join("ws");
        write(&root, ".gitignore", "*.log\n");
        write(&root, "tools/build/gen/src/main.go", "package main");
        write(&root, "tools/build/gen/target/keep.txt", "not a cargo dir");
        write(&root, "tools/build/gen/run.log", "ignored by the workspace");
        write(&root, "crates/c/Cargo.toml", "[package]\nname = \"c\"\n");
        write(&root, "crates/c/target/debug/out", "cargo output");
        write(&root, "crates/c/src/lib.rs", "");

        let cache = FileHashCache::new();
        let f = InputFilter::new(None, &[]).unwrap();
        let files =
            hash_project_inputs(&root, Path::new("tools/build/gen"), &[], &f, &cache).unwrap();
        let keys: Vec<&str> = files.keys().map(|s| s.as_str()).collect();
        assert_eq!(keys, vec!["src/main.go", "target/keep.txt"]);

        let files = hash_project_inputs(&root, Path::new("crates/c"), &[], &f, &cache).unwrap();
        let keys: Vec<&str> = files.keys().map(|s| s.as_str()).collect();
        assert_eq!(keys, vec!["Cargo.toml", "src/lib.rs"]);
    }

    #[test]
    fn root_globs_literal_and_pattern() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "tsconfig.base.json", "{}");
        write(root, "configs/a.json", "{}");
        let cache = FileHashCache::new();
        let m = hash_root_globs(
            root,
            &[
                "tsconfig.base.json".into(),
                "missing.json".into(),
                "configs/*.json".into(),
            ],
            &cache,
        )
        .unwrap();
        assert_eq!(m["missing.json"], "missing");
        assert!(m.contains_key("tsconfig.base.json"));
        assert!(m.contains_key("configs/a.json"));
    }

    #[test]
    fn missing_project_is_an_error_instead_of_an_empty_fingerprint() {
        let tmp = tempfile::tempdir().unwrap();
        let filter = InputFilter::new(None, &[]).unwrap();
        assert!(hash_project_inputs(
            tmp.path(),
            Path::new("missing"),
            &[],
            &filter,
            &FileHashCache::new()
        )
        .is_err());
        assert!(hash_project_inputs(
            &tmp.path().join("missing"),
            Path::new(""),
            &[],
            &filter,
            &FileHashCache::new()
        )
        .is_err());
        assert!(hash_root_globs(
            &tmp.path().join("missing"),
            &["**/*.rs".into()],
            &FileHashCache::new()
        )
        .is_err());
    }

    #[test]
    fn content_change_changes_digest_and_rename_too() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "p/a.txt", "1");
        let cache = FileHashCache::new();
        let f = InputFilter::new(None, &[]).unwrap();
        let d1 = digest_map(&hash_project_inputs(root, Path::new("p"), &[], &f, &cache).unwrap());
        write(root, "p/a.txt", "2");
        let cache = FileHashCache::new();
        let d2 = digest_map(&hash_project_inputs(root, Path::new("p"), &[], &f, &cache).unwrap());
        assert_ne!(d1, d2);
        std::fs::rename(root.join("p/a.txt"), root.join("p/b.txt")).unwrap();
        let d3 = digest_map(&hash_project_inputs(root, Path::new("p"), &[], &f, &cache).unwrap());
        assert_ne!(d2, d3);
    }
}
