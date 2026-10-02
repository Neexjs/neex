//! Source control - which files changed, for `--affected`
//!
//! Fails closed: if no base can be resolved the caller gets an error, never
//! a silent "everything changed" (or worse, "nothing changed").

use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::process::Command;

pub struct ChangedFiles {
    /// The ref the comparison was made against (after merge-base)
    pub base: String,
    /// Root-relative, `/`-separated
    pub files: Vec<String>,
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .context("running git (is it installed?)")?;
    if !out.status.success() {
        return Err(anyhow!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn ref_exists(root: &Path, r: &str) -> bool {
    git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{}^{{commit}}", r),
        ],
    )
    .is_ok()
}

/// Base ref candidates, most specific first:
/// `--base`, `NEEX_SCM_BASE`, `GITHUB_BASE_REF` (as `origin/<ref>` too),
/// then `origin/main`, `main`, `origin/master`, `master`
pub fn base_candidates(explicit: Option<&str>) -> Vec<String> {
    let mut c = Vec::new();
    if let Some(b) = explicit {
        c.push(b.to_string());
        return c;
    }
    if let Ok(b) = std::env::var("NEEX_SCM_BASE") {
        if !b.is_empty() {
            c.push(b);
            return c;
        }
    }
    if let Ok(b) = std::env::var("GITHUB_BASE_REF") {
        if !b.is_empty() {
            c.push(format!("origin/{}", b));
            c.push(b);
        }
    }
    for b in ["origin/main", "main", "origin/master", "master"] {
        c.push(b.to_string());
    }
    c
}

/// Files changed between the merge-base of `base` and HEAD, plus
/// uncommitted and untracked (non-ignored) files
pub fn changed_files(workspace_root: &Path, explicit_base: Option<&str>) -> Result<ChangedFiles> {
    let top = git(workspace_root, &["rev-parse", "--show-toplevel"])?;
    let top = Path::new(top.trim());

    let candidates = base_candidates(explicit_base);
    let base = candidates
        .iter()
        .find(|c| ref_exists(workspace_root, c))
        .cloned()
        .ok_or_else(|| {
            anyhow!(
                "--affected: could not resolve a base ref (tried {}). In CI, fetch the base \
                 branch (e.g. `git fetch origin main`) or pass --base <ref>. neex refuses to \
                 guess, because guessing either rebuilds everything or skips real changes.",
                candidates.join(", ")
            )
        })?;
    let merge_base = git(workspace_root, &["merge-base", &base, "HEAD"])
        .with_context(|| format!("finding merge-base of {} and HEAD (shallow clone?)", base))?;
    let merge_base = merge_base.trim().to_string();

    let mut files: Vec<String> = Vec::new();
    // Disable rename detection so both the old and new project are affected.
    for line in git(
        workspace_root,
        &[
            "diff",
            "--no-renames",
            "--name-only",
            "-z",
            &merge_base,
            "--",
        ],
    )?
    .split('\0')
    {
        if !line.is_empty() {
            files.push(line.to_string());
        }
    }
    for line in git(
        workspace_root,
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
            "--full-name",
        ],
    )?
    .split('\0')
    {
        if !line.is_empty() {
            files.push(line.to_string());
        }
    }

    // git paths are relative to the repo top; make them workspace-relative
    let prefix = workspace_root
        .canonicalize()
        .ok()
        .and_then(|r| {
            top.canonicalize()
                .ok()
                .and_then(|t| r.strip_prefix(&t).ok().map(crate::project::normalize_path))
        })
        .unwrap_or_default();
    let mut rel: Vec<String> = files
        .into_iter()
        .filter_map(|f| {
            if prefix.is_empty() {
                Some(f)
            } else {
                f.strip_prefix(&format!("{}/", prefix))
                    .map(|s| s.to_string())
            }
        })
        .collect();
    rel.sort();
    rel.dedup();
    Ok(ChangedFiles {
        base: format!("{} ({})", base, &merge_base[..merge_base.len().min(12)]),
        files: rel,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, cmd: &str) {
        let st = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(dir)
            .status()
            .unwrap();
        assert!(st.success(), "{}", cmd);
    }

    #[test]
    fn explicit_base_wins() {
        assert_eq!(base_candidates(Some("dev")), vec!["dev"]);
    }

    #[test]
    #[cfg(unix)]
    fn lists_committed_uncommitted_and_untracked_changes() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        sh(
            d,
            "git init -q -b main && git config user.email t@t && git config user.name t",
        );
        sh(
            d,
            "mkdir -p a b && echo 1 > a/x && echo 1 > b/y && git add . && git commit -qm init",
        );
        sh(
            d,
            "git checkout -qb feature && echo 2 > a/x && git commit -qam change",
        );
        sh(d, "echo 3 > b/y && echo new > b/z");
        let c = changed_files(d, Some("main")).unwrap();
        assert_eq!(c.files, vec!["a/x", "b/y", "b/z"]);
        assert!(changed_files(d, Some("does-not-exist")).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn renames_affect_both_source_and_destination_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        sh(
            d,
            "git init -q -b main && git config user.email t@t && git config user.name t",
        );
        sh(
            d,
            "mkdir -p a b && echo source > a/file && git add . && git commit -qm init",
        );
        sh(d, "git checkout -qb feature && git mv a/file b/file");
        let changed = changed_files(d, Some("main")).unwrap();
        assert_eq!(changed.files, vec!["a/file", "b/file"]);
    }
}
