//! Artifact store - task records and content-addressed output files
//!
//! Layout under `<root>/.neex/cache/v1/`:
//!   tasks/<key>.json   one [`TaskRecord`] per cache key
//!   blobs/<hash>       file contents, deduplicated
//!
//! Only successful runs are ever stored. Restores never write outside the
//! project root, never follow symlinks, and skip files already in place.

use crate::inputs::compile_glob;
use crate::project::normalize_path;
use crate::task_hash::HashInputs;
use anyhow::{anyhow, Context, Result};
use globset::GlobSetBuilder;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLine {
    pub stream: Stream,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputFile {
    /// project-relative, `/`-separated
    pub path: String,
    pub hash: String,
    pub executable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub key: String,
    pub task_id: String,
    pub duration_ms: u64,
    pub created_at: u64,
    pub logs: Vec<LogLine>,
    pub files: Vec<OutputFile>,
    /// Only these globs are owned by this task. Negations protect files
    /// from capture AND from stale-output cleanup on restore.
    #[serde(default)]
    pub outputs: Vec<String>,
    pub inputs: HashInputs,
}

pub struct ArtifactStore {
    dir: PathBuf,
}

impl ArtifactStore {
    pub fn open(workspace_root: &Path) -> Result<Self> {
        let dir = workspace_root.join(".neex").join("cache").join("v1");
        std::fs::create_dir_all(dir.join("tasks"))?;
        std::fs::create_dir_all(dir.join("blobs"))?;
        Ok(Self { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn record_path(&self, key: &str) -> PathBuf {
        self.dir.join("tasks").join(format!("{}.json", key))
    }

    fn blob_path(&self, hash: &str) -> PathBuf {
        self.dir.join("blobs").join(hash)
    }

    pub fn get(&self, key: &str) -> Result<Option<TaskRecord>> {
        safe_cache_name(key)?;
        let path = self.record_path(key);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let rec: TaskRecord = serde_json::from_slice(&bytes)
                    .with_context(|| format!("corrupt cache record {}", path.display()))?;
                rec.validate()?;
                if rec.key != key {
                    return Err(anyhow!("cache record key does not match {}", key));
                }
                // A record whose blobs went missing is a miss, not an error
                for f in &rec.files {
                    if !self.blob_path(&f.hash).is_file() {
                        return Ok(None);
                    }
                }
                Ok(Some(rec))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Collect `outputs` under `project_root` into the blob store and write
    /// the record. Returns the files that were captured.
    #[allow(clippy::too_many_arguments)]
    pub fn store(
        &self,
        key: &str,
        task_id: &str,
        project_root: &Path,
        outputs: &[String],
        logs: Vec<LogLine>,
        duration_ms: u64,
        inputs: HashInputs,
    ) -> Result<TaskRecord> {
        safe_cache_name(key)?;
        let files = self.collect_outputs(project_root, outputs)?;
        let record = TaskRecord {
            key: key.to_string(),
            task_id: task_id.to_string(),
            duration_ms,
            created_at: now(),
            logs,
            files,
            outputs: outputs.to_vec(),
            inputs,
        };
        atomic_write(&self.record_path(key), &serde_json::to_vec(&record)?)?;
        Ok(record)
    }

    fn collect_outputs(&self, project_root: &Path, outputs: &[String]) -> Result<Vec<OutputFile>> {
        if outputs.is_empty() {
            return Ok(vec![]);
        }
        let spec = OutputSpec::new(outputs)?;

        let mut files = Vec::new();
        for entry in WalkDir::new(project_root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !(e.file_type().is_dir()
                    && (name == "node_modules" || name == ".git" || name == ".neex"))
            })
        {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = normalize_path(entry.path().strip_prefix(project_root)?);
            if !spec.matches(&rel) {
                continue;
            }
            let content = std::fs::read(entry.path())?;
            let hash = blake3::hash(&content).to_hex().to_string();
            let blob = self.blob_path(&hash);
            // A previous interrupted write or disk corruption must not make
            // every future rebuild reuse a permanently damaged blob.
            if self.read_blob(&hash).is_err() {
                atomic_write(&blob, &content)?;
            }
            files.push(OutputFile {
                path: rel,
                hash,
                executable: is_executable(entry.path()),
            });
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }

    /// Write a record's outputs back under `project_root`. Files whose
    /// content already matches are left untouched. Returns the number of
    /// files written.
    pub fn restore(&self, record: &TaskRecord, project_root: &Path) -> Result<usize> {
        record.validate()?;
        let project_root = project_root.canonicalize()?;
        // Validate every blob before deleting stale outputs or modifying any
        // target. A damaged cache must leave the current build intact.
        for file in &record.files {
            self.read_blob(&file.hash)?;
        }
        self.remove_stale_outputs(record, &project_root)?;
        let mut written = 0;
        for f in &record.files {
            let rel = safe_relative(&f.path)?;
            // Check every ancestor before inspecting the target: even an
            // existing regular file may be reached through a directory symlink.
            if let Some(parent) = rel.parent() {
                ensure_real_dir(&project_root, parent)?;
            }
            let target = project_root.join(&rel);
            // Never write through a symlink; replace it with a real file/dir
            if let Ok(meta) = std::fs::symlink_metadata(&target) {
                if meta.file_type().is_symlink() {
                    remove_symlink(&target)?;
                } else if meta.is_dir() {
                    // Only empty directories can be replaced; never erase
                    // undeclared files hidden inside a file-shaped output.
                    std::fs::remove_dir(&target)?;
                } else if meta.is_file() {
                    if let Ok(existing) = std::fs::read(&target) {
                        if blake3::hash(&existing).to_hex().to_string() == f.hash
                            && is_executable(&target) == f.executable
                        {
                            continue;
                        }
                    }
                }
            }
            let bytes = self.read_blob(&f.hash)?;
            atomic_write(&target, &bytes)
                .with_context(|| format!("restoring {}", target.display()))?;
            set_executable(&target, f.executable)?;
            written += 1;
        }
        Ok(written)
    }

    fn remove_stale_outputs(&self, record: &TaskRecord, root: &Path) -> Result<()> {
        let spec = OutputSpec::new(&record.outputs)?;
        let retained: HashSet<&str> = record.files.iter().map(|f| f.path.as_str()).collect();
        let mut directories = Vec::new();
        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                !(e.file_type().is_dir() && protected_name(&e.file_name().to_string_lossy()))
            })
        {
            let entry = entry?;
            if entry.path() == root {
                continue;
            }
            let relative = normalize_path(entry.path().strip_prefix(root)?);
            if !spec.matches(&relative) || retained.contains(relative.as_str()) {
                continue;
            }
            if entry.file_type().is_symlink() {
                remove_symlink(entry.path())?;
            } else if entry.file_type().is_file() {
                std::fs::remove_file(entry.path())?;
            } else if entry.file_type().is_dir() {
                directories.push(entry.into_path());
            }
        }
        for dir in directories.into_iter().rev() {
            match std::fs::remove_dir(dir) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    pub fn has_blob(&self, hash: &str) -> bool {
        valid_hash(hash) && self.blob_path(hash).is_file()
    }

    pub fn read_blob(&self, hash: &str) -> Result<Vec<u8>> {
        if !valid_hash(hash) {
            return Err(anyhow!("invalid blob hash"));
        }
        let bytes = std::fs::read(self.blob_path(hash))?;
        if blake3::hash(&bytes).to_hex().as_str() != hash {
            return Err(anyhow!("corrupt cache blob {}", hash));
        }
        Ok(bytes)
    }

    /// Store blob bytes after checking they match `hash`
    pub fn put_blob(&self, hash: &str, bytes: &[u8]) -> Result<()> {
        let actual = blake3::hash(bytes).to_hex().to_string();
        if actual != hash {
            return Err(anyhow!(
                "blob {} failed verification (got {})",
                hash,
                actual
            ));
        }
        atomic_write(&self.blob_path(hash), bytes)?;
        Ok(())
    }

    /// Write a record fetched from elsewhere (blobs must already be present)
    pub fn put_record(&self, record: &TaskRecord) -> Result<()> {
        record.validate()?;
        atomic_write(&self.record_path(&record.key), &serde_json::to_vec(record)?)?;
        Ok(())
    }

    fn last_path(&self, task_id: &str) -> PathBuf {
        let name = blake3::hash(task_id.as_bytes()).to_hex().to_string();
        self.dir.join("last").join(format!("{}.json", &name[..32]))
    }

    /// Remember the inputs of the most recent run of `task_id` (for `why`)
    pub fn save_last(&self, task_id: &str, inputs: &HashInputs) -> Result<()> {
        let path = self.last_path(task_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(&path, &serde_json::to_vec(inputs)?)?;
        Ok(())
    }

    pub fn load_last(&self, task_id: &str) -> Result<Option<HashInputs>> {
        match std::fs::read(self.last_path(task_id)) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Remove every record and blob
    pub fn clear(&self) -> Result<()> {
        for sub in ["tasks", "blobs", "last"] {
            let p = self.dir.join(sub);
            if p.exists() {
                std::fs::remove_dir_all(&p)?;
            }
            std::fs::create_dir_all(&p)?;
        }
        Ok(())
    }

    /// (records, bytes on disk)
    pub fn stats(&self) -> Result<(usize, u64)> {
        let records = std::fs::read_dir(self.dir.join("tasks"))?.count();
        let mut bytes = 0;
        for e in WalkDir::new(&self.dir).into_iter().flatten() {
            if e.file_type().is_file() {
                bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
        Ok((records, bytes))
    }
}

/// Reject `..`, absolute paths and drive prefixes in stored output paths
fn safe_relative(path: &str) -> Result<PathBuf> {
    if path.is_empty() || path.contains(['\\', ':', '\0']) {
        return Err(anyhow!("refusing to restore unsafe output path `{}`", path));
    }
    let p = Path::new(path);
    for c in p.components() {
        match c {
            Component::Normal(_) => {}
            _ => return Err(anyhow!("refusing to restore unsafe output path `{}`", path)),
        }
    }
    Ok(p.to_path_buf())
}

/// Create `dir`, replacing any symlink along the way with a real directory
fn ensure_real_dir(root: &Path, relative: &Path) -> Result<()> {
    let mut dir = root.to_path_buf();
    for component in relative.components() {
        dir.push(component);
        if let Ok(meta) = std::fs::symlink_metadata(&dir) {
            if meta.file_type().is_symlink() {
                remove_symlink(&dir)?;
            } else if meta.is_dir() {
                continue;
            } else if meta.is_file() {
                std::fs::remove_file(&dir)?;
            }
        }
        std::fs::create_dir(&dir)?;
    }
    Ok(())
}

impl TaskRecord {
    /// Bind output ownership to the inputs whose digest the caller requested.
    /// A remote record cannot widen cleanup to sources by changing its globs.
    pub(crate) fn validate_inputs(&self) -> Result<()> {
        self.validate()?;
        if self.key != self.inputs.key()
            || self.outputs != self.inputs.config.outputs.clone().unwrap_or_default()
            || self.task_id != format!("{}#{}", self.inputs.project, self.inputs.task)
        {
            return Err(anyhow!(
                "cache record does not match its hashed task inputs"
            ));
        }
        Ok(())
    }

    pub(crate) fn validate(&self) -> Result<()> {
        safe_cache_name(&self.key)?;
        let spec = OutputSpec::new(&self.outputs)?;
        let mut seen = HashSet::new();
        for file in &self.files {
            safe_relative(&file.path)?;
            if file.path.split('/').any(protected_name) || !seen.insert(&file.path) {
                return Err(anyhow!("invalid or duplicate output path {}", file.path));
            }
            if !self.outputs.is_empty() && !spec.matches(&file.path) {
                return Err(anyhow!("output {} is outside declared outputs", file.path));
            }
            if !valid_hash(&file.hash) {
                return Err(anyhow!("invalid blob hash for {}", file.path));
            }
        }
        Ok(())
    }
}

fn protected_name(name: &str) -> bool {
    matches!(name, ".git" | ".neex" | "node_modules")
}

struct OutputSpec {
    include: globset::GlobSet,
    exclude: globset::GlobSet,
}

impl OutputSpec {
    fn new(outputs: &[String]) -> Result<Self> {
        let mut include = GlobSetBuilder::new();
        let mut exclude = GlobSetBuilder::new();
        for pattern in outputs {
            let body = pattern
                .strip_prefix('!')
                .unwrap_or(pattern)
                .strip_prefix("./")
                .unwrap_or_else(|| pattern.strip_prefix('!').unwrap_or(pattern));
            safe_relative(body)?;
            if !pattern.starts_with('!') && body.split('/').any(protected_name) {
                return Err(anyhow!("protected output pattern {}", pattern));
            }
            let glob = compile_glob(body)?;
            if pattern.starts_with('!') {
                exclude.add(glob);
            } else {
                include.add(glob);
            }
        }
        Ok(Self {
            include: include.build()?,
            exclude: exclude.build()?,
        })
    }
    fn matches(&self, path: &str) -> bool {
        self.include.is_match(path) && !self.exclude.is_match(path)
    }
}

fn remove_symlink(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt;
        if std::fs::symlink_metadata(path)?
            .file_type()
            .is_symlink_dir()
        {
            return std::fs::remove_dir(path);
        }
    }
    std::fs::remove_file(path)
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn safe_cache_name(name: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(anyhow!("invalid cache key"));
    }
    Ok(())
}

// A pid alone collides when several tasks write the same blob concurrently.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let tmp = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        file.write_all(bytes)?;
        drop(file);
        std::fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
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

#[cfg(unix)]
fn set_executable(path: &Path, exec: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    let mode = if exec { 0o755 } else { 0o644 };
    perms.set_mode(mode);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path, _exec: bool) -> Result<()> {
    Ok(())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn inputs() -> HashInputs {
        HashInputs {
            key_version: "t".into(),
            neex_version: "0".into(),
            platform: "p".into(),
            project: "web".into(),
            task: "build".into(),
            command: "c".into(),
            config: Default::default(),
            files: BTreeMap::new(),
            dep_projects: BTreeMap::new(),
            dep_tasks: BTreeMap::new(),
            env: BTreeMap::new(),
            global_files: BTreeMap::new(),
        }
    }

    #[test]
    fn store_and_restore_outputs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let proj = root.join("web");
        std::fs::create_dir_all(proj.join("dist/sub")).unwrap();
        std::fs::create_dir_all(proj.join("dist/cache")).unwrap();
        std::fs::write(proj.join("dist/a.js"), "A").unwrap();
        std::fs::write(proj.join("dist/sub/b.js"), "B").unwrap();
        std::fs::write(proj.join("dist/cache/c.js"), "C").unwrap();
        std::fs::write(proj.join("src.ts"), "S").unwrap();

        let store = ArtifactStore::open(root).unwrap();
        let rec = store
            .store(
                "k1",
                "web#build",
                &proj,
                &["dist/**".into(), "!dist/cache/**".into()],
                vec![],
                5,
                inputs(),
            )
            .unwrap();
        let paths: Vec<&str> = rec.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["dist/a.js", "dist/sub/b.js"]);

        std::fs::remove_dir_all(proj.join("dist")).unwrap();
        let rec2 = store.get("k1").unwrap().unwrap();
        let written = store.restore(&rec2, &proj).unwrap();
        assert_eq!(written, 2);
        assert_eq!(
            std::fs::read_to_string(proj.join("dist/a.js")).unwrap(),
            "A"
        );
        assert!(!proj.join("dist/cache/c.js").exists());

        // second restore touches nothing
        assert_eq!(store.restore(&rec2, &proj).unwrap(), 0);
        assert!(store.get("missing").unwrap().is_none());
    }

    #[test]
    fn restore_replaces_symlinked_output_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let proj = root.join("web");
        std::fs::create_dir_all(proj.join("dist")).unwrap();
        std::fs::write(proj.join("dist/a.js"), "A").unwrap();
        let store = ArtifactStore::open(root).unwrap();
        let rec = store
            .store(
                "k",
                "web#build",
                &proj,
                &["dist/**".into()],
                vec![],
                1,
                inputs(),
            )
            .unwrap();
        std::fs::remove_dir_all(proj.join("dist")).unwrap();
        std::fs::create_dir_all(proj.join("src")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(proj.join("src"), proj.join("dist")).unwrap();
        store.restore(&rec, &proj).unwrap();
        assert!(!std::fs::symlink_metadata(proj.join("dist"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(!proj.join("src/a.js").exists());
        assert!(proj.join("dist/a.js").exists());
    }

    #[test]
    fn unsafe_paths_are_rejected() {
        assert!(safe_relative("../x").is_err());
        assert!(safe_relative("/etc/passwd").is_err());
        assert!(safe_relative("").is_err());
        assert!(safe_relative("..\\outside").is_err());
        assert!(safe_relative("C:/outside").is_err());
        assert!(safe_relative("dist/a.js").is_ok());
    }

    fn fixture() -> (tempfile::TempDir, ArtifactStore, TaskRecord) {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("web");
        std::fs::create_dir_all(proj.join("dist/sub")).unwrap();
        std::fs::write(proj.join("dist/sub/a.js"), "cached").unwrap();
        let store = ArtifactStore::open(tmp.path()).unwrap();
        let record = store
            .store(
                "key",
                "web#build",
                &proj,
                &["dist/**".into()],
                vec![],
                1,
                inputs(),
            )
            .unwrap();
        (tmp, store, record)
    }

    #[test]
    #[cfg(unix)]
    fn restore_checks_all_symlink_ancestors_before_reading_or_writing() {
        let (tmp, store, record) = fixture();
        let proj = tmp.path().join("web");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(outside.join("sub")).unwrap();
        std::fs::write(outside.join("sub/a.js"), "outside").unwrap();
        std::fs::remove_dir_all(proj.join("dist")).unwrap();
        std::os::unix::fs::symlink(&outside, proj.join("dist")).unwrap();
        assert_eq!(store.restore(&record, &proj).unwrap(), 1);
        assert_eq!(
            std::fs::read_to_string(outside.join("sub/a.js")).unwrap(),
            "outside"
        );
        assert_eq!(
            std::fs::read_to_string(proj.join("dist/sub/a.js")).unwrap(),
            "cached"
        );
    }

    #[test]
    fn corrupt_blobs_and_unsafe_records_are_rejected() {
        let (tmp, store, mut record) = fixture();
        let target = tmp.path().join("web/dist/sub/a.js");
        std::fs::write(&target, "current").unwrap();
        std::fs::write(store.blob_path(&record.files[0].hash), "corrupt").unwrap();
        assert!(store.restore(&record, &tmp.path().join("web")).is_err());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "current");
        record.files[0].hash = "../../outside".into();
        assert!(store.put_record(&record).is_err());
        assert!(!store.has_blob("../../outside"));
        assert!(store.get("../../outside").is_err());
    }

    #[test]
    fn successful_rebuild_repairs_a_corrupt_existing_blob() {
        let (tmp, store, record) = fixture();
        let project = tmp.path().join("web");
        std::fs::write(store.blob_path(&record.files[0].hash), "broken").unwrap();
        let rebuilt = store
            .store(
                "rebuilt",
                "web#build",
                &project,
                &["dist/**".into()],
                vec![],
                1,
                inputs(),
            )
            .unwrap();
        std::fs::remove_dir_all(project.join("dist")).unwrap();
        assert_eq!(store.restore(&rebuilt, &project).unwrap(), 1);
        assert_eq!(
            std::fs::read_to_string(project.join("dist/sub/a.js")).unwrap(),
            "cached"
        );
    }

    #[test]
    fn concurrent_blob_writers_do_not_share_temporary_files() {
        let tmp = tempfile::tempdir().unwrap();
        let store = std::sync::Arc::new(ArtifactStore::open(tmp.path()).unwrap());
        let bytes = vec![42; 65536];
        let hash = blake3::hash(&bytes).to_hex().to_string();
        std::thread::scope(|scope| {
            for _ in 0..16 {
                let store = &store;
                let bytes = &bytes;
                let hash = &hash;
                scope.spawn(move || store.put_blob(hash, bytes).unwrap());
            }
        });
        assert_eq!(store.read_blob(&hash).unwrap(), bytes);
    }

    #[test]
    fn restore_matches_a_cold_build_and_preserves_excluded_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let project = root.join("app");
        std::fs::create_dir_all(project.join("dist/cache")).unwrap();
        std::fs::write(project.join("dist/app.js"), "fresh").unwrap();
        std::fs::write(project.join("dist/cache/hint"), "keep").unwrap();
        std::fs::write(project.join("source.ts"), "source").unwrap();
        let store = ArtifactStore::open(root).unwrap();
        let record = store
            .store(
                "snapshot",
                "app#build",
                &project,
                &["dist/**".into(), "!dist/cache/**".into()],
                vec![],
                1,
                inputs(),
            )
            .unwrap();
        std::fs::write(project.join("dist/app.js"), "old").unwrap();
        std::fs::create_dir_all(project.join("dist/old/chunks")).unwrap();
        std::fs::write(project.join("dist/old/chunks/stale.js"), "stale").unwrap();
        store.restore(&record, &project).unwrap();
        assert_eq!(
            std::fs::read_to_string(project.join("dist/app.js")).unwrap(),
            "fresh"
        );
        assert!(!project.join("dist/old").exists());
        assert_eq!(
            std::fs::read_to_string(project.join("dist/cache/hint")).unwrap(),
            "keep"
        );
        assert_eq!(
            std::fs::read_to_string(project.join("source.ts")).unwrap(),
            "source"
        );
        assert_eq!(store.restore(&record, &project).unwrap(), 0);
    }

    #[test]
    fn corrupt_later_blob_does_not_modify_any_output_or_delete_stale_files() {
        let (tmp, store, _) = fixture();
        let project = tmp.path().join("web");
        std::fs::write(project.join("dist/z.js"), "fresh-z").unwrap();
        let record = store
            .store(
                "all",
                "web#build",
                &project,
                &["dist/**".into()],
                vec![],
                1,
                inputs(),
            )
            .unwrap();
        std::fs::write(project.join("dist/sub/a.js"), "existing-a").unwrap();
        std::fs::write(project.join("dist/extra.js"), "existing-extra").unwrap();
        std::fs::write(
            store.blob_path(&record.files.last().unwrap().hash),
            "broken",
        )
        .unwrap();
        assert!(store.restore(&record, &project).is_err());
        assert_eq!(
            std::fs::read_to_string(project.join("dist/sub/a.js")).unwrap(),
            "existing-a"
        );
        assert!(project.join("dist/extra.js").exists());
    }

    #[test]
    #[cfg(unix)]
    fn restore_corrects_executable_bit_without_changing_other_files() {
        use std::os::unix::fs::PermissionsExt;
        let (tmp, store, _) = fixture();
        let project = tmp.path().join("web");
        let executable = project.join("dist/sub/a.js");
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let record = store
            .store(
                "exec",
                "web#build",
                &project,
                &["dist/**".into()],
                vec![],
                1,
                inputs(),
            )
            .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(store.restore(&record, &project).unwrap(), 1);
        assert!(is_executable(&executable));
        std::fs::remove_dir_all(project.join("dist")).unwrap();
        assert_eq!(store.restore(&record, &project).unwrap(), 1);
        assert!(is_executable(&executable));
    }

    #[test]
    fn undeclared_files_cannot_be_inserted_by_a_remote_record() {
        let (tmp, store, mut record) = fixture();
        record.files[0].path = "src/code.ts".into();
        assert!(store.restore(&record, &tmp.path().join("web")).is_err());
        record.files[0].path = ".neex/config.json".into();
        record.outputs = vec!["**".into()];
        assert!(store.put_record(&record).is_err());
    }
}
