//! Configuration - `neex.json` at the workspace root, optional `neex.json`
//! per project, and read-only compatibility with `turbo.json`
//!
//! Zero config is valid: with no file at all every task gets the built-in
//! defaults (see [`TaskConfig::builtin`]).
//!
//! Precedence, lowest to highest:
//!   built-in defaults → root `tasks.<name>` → root `tasks.<project>#<name>`
//!   → project `neex.json`

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const ROOT_CONFIG_FILE: &str = "neex.json";
pub const TURBO_CONFIG_FILES: &[&str] = &["turbo.json", "turbo.jsonc"];

/// Where the root configuration came from
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigSource {
    /// `neex.json`
    Neex,
    /// `turbo.json` / `turbo.jsonc`, translated
    Turbo,
    /// No file: built-in defaults only
    #[default]
    Default,
}

/// Per-task configuration, as written in the config file (all optional so
/// layers can be merged)
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskConfig {
    /// `^build` = build in upstream projects, `build` = same project,
    /// `proj#build` = that project's task
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,
    /// Project-relative globs; `!` negates. Default: every non-ignored file
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<Vec<String>>,
    /// Project-relative globs stored on success and restored on a hit
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<String>>,
    /// Env var names whose values are part of the cache key
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<Vec<String>>,
    /// Env vars forwarded to the task but NOT hashed. Use sparingly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pass_through_env: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<bool>,
    /// Long-running task (dev server); never cached, nothing may depend on it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent: Option<bool>,
    /// Shell command. Only valid for `proj#task`, `//#task` and project files
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

impl TaskConfig {
    /// Overlay `other` on top of `self`; set fields in `other` win
    pub fn merge(&self, other: &TaskConfig) -> TaskConfig {
        TaskConfig {
            depends_on: other.depends_on.clone().or_else(|| self.depends_on.clone()),
            inputs: other.inputs.clone().or_else(|| self.inputs.clone()),
            outputs: other.outputs.clone().or_else(|| self.outputs.clone()),
            env: other.env.clone().or_else(|| self.env.clone()),
            pass_through_env: other
                .pass_through_env
                .clone()
                .or_else(|| self.pass_through_env.clone()),
            cache: other.cache.or(self.cache),
            persistent: other.persistent.or(self.persistent),
            command: other.command.clone().or_else(|| self.command.clone()),
        }
    }

    /// Built-in defaults for well-known task names, applied when no config
    /// mentions the task at all
    pub fn builtin(task: &str) -> TaskConfig {
        match task {
            "build" | "compile" => TaskConfig {
                depends_on: Some(vec!["^build".into()]),
                cache: Some(true),
                ..Default::default()
            },
            "test" | "lint" | "check" | "typecheck" | "format:check" => TaskConfig {
                depends_on: Some(vec!["^build".into()]),
                cache: Some(true),
                outputs: Some(vec![]),
                ..Default::default()
            },
            "dev" | "start" | "serve" | "watch" | "preview" => TaskConfig {
                cache: Some(false),
                persistent: Some(true),
                ..Default::default()
            },
            // Unknown tasks run every time unless the config says otherwise;
            // this keeps `deploy` or `db:migrate` from being replayed
            _ => TaskConfig {
                cache: Some(false),
                ..Default::default()
            },
        }
    }
}

/// A task entry in a project file may be a bare command string
#[derive(Debug, Clone)]
enum TaskEntry {
    Command(String),
    Full(TaskConfig),
}

// Hand-written so a typo like `output` reports "unknown field `output`,
// expected one of ..." instead of serde's opaque untagged-enum error
impl<'de> Deserialize<'de> for TaskEntry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match serde_json::Value::deserialize(d)? {
            serde_json::Value::String(s) => Ok(TaskEntry::Command(s)),
            other => TaskConfig::deserialize(other)
                .map(TaskEntry::Full)
                .map_err(serde::de::Error::custom),
        }
    }
}

impl From<TaskEntry> for TaskConfig {
    fn from(e: TaskEntry) -> Self {
        match e {
            TaskEntry::Command(c) => TaskConfig {
                command: Some(c),
                ..Default::default()
            },
            TaskEntry::Full(t) => t,
        }
    }
}

/// Root `neex.json`
#[derive(Debug, Clone, Default, Serialize)]
pub struct RootConfig {
    pub source: ConfigSource,
    /// Extra project globs, added to what the language providers find
    pub projects: Vec<String>,
    /// Env var names hashed into every task
    pub global_env: Vec<String>,
    /// Root-relative globs hashed into every task
    pub global_inputs: Vec<String>,
    /// Keys: `build`, `web#build`, `//#format`
    pub tasks: BTreeMap<String, TaskConfig>,
    /// Non-fatal problems found while reading (unknown turbo keys etc.)
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RootConfigFile {
    #[serde(rename = "$schema", default)]
    _schema: Option<String>,
    #[serde(default)]
    projects: Vec<String>,
    #[serde(default)]
    global_env: Vec<String>,
    #[serde(default)]
    global_inputs: Vec<String>,
    #[serde(default)]
    tasks: BTreeMap<String, TaskEntry>,
}

/// Per-project `neex.json`
#[derive(Debug, Clone, Default, Serialize)]
pub struct ProjectConfig {
    pub name: Option<String>,
    /// Other projects this one depends on (names, idents or paths)
    pub deps: Vec<String>,
    pub tasks: BTreeMap<String, TaskConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectConfigFile {
    #[serde(rename = "$schema", default)]
    _schema: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    deps: Vec<String>,
    #[serde(default)]
    tasks: BTreeMap<String, TaskEntry>,
}

impl RootConfig {
    /// Load the root config: `neex.json`, else a translated `turbo.json`,
    /// else built-in defaults
    pub fn load(root: &Path) -> Result<Self> {
        let neex_path = root.join(ROOT_CONFIG_FILE);
        if neex_path.exists() {
            return Self::from_neex_file(&neex_path);
        }
        for name in TURBO_CONFIG_FILES {
            let path = root.join(name);
            if path.exists() {
                return Self::from_turbo_file(&path);
            }
        }
        Ok(Self::default())
    }

    pub fn from_neex_file(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::from_neex_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn from_neex_str(text: &str) -> Result<Self> {
        let file: RootConfigFile = serde_json::from_str(&strip_json_comments(text))?;
        let mut tasks = BTreeMap::new();
        for (name, entry) in file.tasks {
            let task: TaskConfig = entry.into();
            validate_task_key(&name, &task)?;
            tasks.insert(name, task);
        }
        Ok(RootConfig {
            source: ConfigSource::Neex,
            projects: file.projects,
            global_env: file.global_env,
            global_inputs: file.global_inputs,
            tasks,
            warnings: vec![],
        })
    }

    pub fn from_turbo_file(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::from_turbo_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    /// Translate a `turbo.json` (1.x `pipeline` or 2.x `tasks`)
    pub fn from_turbo_str(text: &str) -> Result<Self> {
        let value: serde_json::Value = serde_json::from_str(&strip_json_comments(text))?;
        let obj = value
            .as_object()
            .ok_or_else(|| anyhow!("turbo.json must be an object"))?;

        let mut cfg = RootConfig {
            source: ConfigSource::Turbo,
            ..Default::default()
        };

        for (key, val) in obj {
            match key.as_str() {
                "$schema"
                | "ui"
                | "envMode"
                | "remoteCache"
                | "cacheDir"
                | "concurrency"
                | "daemon"
                | "noUpdateNotifier"
                | "dangerouslyDisablePackageManagerCheck"
                | "experimentalUI"
                | "futureFlags"
                | "cacheMaxAge"
                | "cacheMaxSize" => {}
                "globalDependencies" => cfg.global_inputs = string_list(val),
                "globalEnv" => cfg.global_env = string_list(val),
                "globalPassThroughEnv" => cfg.warnings.push(
                    "turbo.json: globalPassThroughEnv is ignored; neex forwards the full \
                     environment and hashes only declared `env`"
                        .into(),
                ),
                "pipeline" | "tasks" => {
                    if let Some(tasks) = val.as_object() {
                        for (name, tval) in tasks {
                            let task = translate_turbo_task(name, tval, &mut cfg.warnings);
                            cfg.tasks.insert(name.clone(), task);
                        }
                    }
                }
                "extends" => cfg.warnings.push(
                    "turbo.json: package-level `extends` is not supported yet; only the root \
                     file is read"
                        .into(),
                ),
                other => cfg
                    .warnings
                    .push(format!("turbo.json: unknown key `{}` ignored", other)),
            }
        }
        Ok(cfg)
    }

    /// Effective config for `project#task`, before project-file overrides:
    /// built-in → `task` → `project#task`
    pub fn task_config(&self, project: &str, task: &str) -> TaskConfig {
        let mut cfg = TaskConfig::builtin(task);
        if let Some(generic) = self.tasks.get(task) {
            cfg = cfg.merge(generic);
            // A task the config knows about is cached unless told otherwise
            if generic.cache.is_none() && cfg.persistent != Some(true) {
                cfg.cache = Some(true);
            }
        }
        if let Some(specific) = self.tasks.get(&format!("{}#{}", project, task)) {
            cfg = cfg.merge(specific);
        }
        if cfg.persistent == Some(true) {
            cfg.cache = Some(false);
        }
        cfg
    }

    /// Root tasks declared as `//#name`
    pub fn root_tasks(&self) -> impl Iterator<Item = (&str, &TaskConfig)> {
        self.tasks
            .iter()
            .filter_map(|(k, v)| k.strip_prefix("//#").map(|n| (n, v)))
    }
}

impl ProjectConfig {
    pub fn load(dir: &Path) -> Result<Option<Self>> {
        let path = dir.join(ROOT_CONFIG_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let file: ProjectConfigFile = serde_json::from_str(&strip_json_comments(&text))
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(Some(ProjectConfig {
            name: file.name,
            deps: file.deps,
            tasks: file.tasks.into_iter().map(|(k, v)| (k, v.into())).collect(),
        }))
    }
}

fn validate_task_key(key: &str, task: &TaskConfig) -> Result<()> {
    if key.is_empty() {
        return Err(anyhow!("empty task name"));
    }
    if task.command.is_some() && !key.contains('#') {
        return Err(anyhow!(
            "task `{}`: `command` is only allowed on `project#task` or `//#task` entries; \
             a plain task name configures the task for every project",
            key
        ));
    }
    Ok(())
}

fn string_list(val: &serde_json::Value) -> Vec<String> {
    val.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn translate_turbo_task(
    name: &str,
    val: &serde_json::Value,
    warnings: &mut Vec<String>,
) -> TaskConfig {
    let mut task = TaskConfig::default();
    let Some(obj) = val.as_object() else {
        return task;
    };
    for (key, v) in obj {
        match key.as_str() {
            "dependsOn" => {
                let deps: Vec<String> = string_list(v)
                    .into_iter()
                    .filter(|d| {
                        if d.starts_with('$') {
                            warnings.push(format!(
                                "turbo.json task `{}`: `{}` in dependsOn is the pre-1.5 env \
                                 syntax; move it to `env`",
                                name, d
                            ));
                            false
                        } else {
                            true
                        }
                    })
                    .collect();
                task.depends_on = Some(deps);
            }
            "inputs" => {
                let mut inputs = Vec::new();
                let mut uses_default = false;
                for item in v.as_array().into_iter().flatten() {
                    match item.as_str() {
                        Some("$TURBO_DEFAULT$") => uses_default = true,
                        Some(s) => inputs.push(s.replace("$TURBO_ROOT$/", "//")),
                        None => warnings.push(format!(
                            "turbo.json task `{}`: structured inputs are not supported",
                            name
                        )),
                    }
                }
                // neex: positive globs restrict the project's files, `!`
                // excludes, `//` root globs are added. With `$TURBO_DEFAULT$`
                // turbo keeps every project file, so only the negations and
                // root globs carry meaning.
                let inputs: Vec<String> = if uses_default {
                    inputs
                        .into_iter()
                        .filter(|i| i.starts_with('!') || i.starts_with("//"))
                        .collect()
                } else {
                    inputs
                };
                if !inputs.is_empty() {
                    task.inputs = Some(inputs);
                }
            }
            "outputs" => task.outputs = Some(string_list(v)),
            "env" => task.env = Some(string_list(v)),
            "passThroughEnv" => task.pass_through_env = Some(string_list(v)),
            "cache" => task.cache = v.as_bool(),
            "persistent" => task.persistent = v.as_bool(),
            "outputMode" | "outputLogs" | "interactive" | "interruptible" | "description"
            | "with" | "extends" => {}
            "dotEnv" => warnings.push(format!(
                "turbo.json task `{}`: `dotEnv` was removed in turbo 2; list the files in \
                 `inputs` instead",
                name
            )),
            other => warnings.push(format!(
                "turbo.json task `{}`: unknown key `{}` ignored",
                name, other
            )),
        }
    }
    task
}

/// Remove `//` and `/* */` comments outside string literals (for `.jsonc`)
pub fn strip_json_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut in_string = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            out.push(c);
            if c == '\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            i += 1;
        } else if c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
        } else if c == '/' && i + 1 < bytes.len() && bytes[i + 1] == '/' {
            while i < bytes.len() && bytes[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && i + 1 < bytes.len() && bytes[i + 1] == '*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == '*' && bytes[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Write a starter `neex.json`
pub fn starter_config() -> &'static str {
    r#"{
  "$schema": "https://raw.githubusercontent.com/Neexjs/neex/main/schema.json",
  "tasks": {
    "build": { "dependsOn": ["^build"], "outputs": ["dist/**", ".next/**", "!.next/cache/**"] },
    "test": { "dependsOn": ["^build"] },
    "lint": {},
    "dev": { "persistent": true }
  }
}
"#
}

pub fn config_path(root: &Path) -> PathBuf {
    root.join(ROOT_CONFIG_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_object_is_valid() {
        let cfg = RootConfig::from_neex_str("{}").unwrap();
        assert_eq!(cfg.source, ConfigSource::Neex);
        assert!(cfg.tasks.is_empty());
    }

    #[test]
    fn full_example_parses() {
        let cfg = RootConfig::from_neex_str(
            r#"{
              "$schema": "x",
              "projects": ["services/*"],
              "globalEnv": ["CI"],
              "globalInputs": [".tool-versions"],
              "tasks": {
                "build": { "dependsOn": ["^build"], "outputs": ["dist/**"] },
                "dev": { "persistent": true },
                "web#build": { "env": ["NEXT_PUBLIC_API_URL"] },
                "//#format": { "command": "prettier --check ." },
                "proto#generate": "buf generate"
              }
            }"#,
        )
        .unwrap();
        assert_eq!(cfg.projects, vec!["services/*"]);
        assert_eq!(
            cfg.tasks["proto#generate"].command.as_deref(),
            Some("buf generate")
        );
        let web = cfg.task_config("web", "build");
        assert_eq!(web.depends_on.unwrap(), vec!["^build"]);
        assert_eq!(web.env.unwrap(), vec!["NEXT_PUBLIC_API_URL"]);
        assert_eq!(web.cache, Some(true));
        let dev = cfg.task_config("web", "dev");
        assert_eq!(dev.cache, Some(false));
        assert_eq!(dev.persistent, Some(true));
        assert_eq!(cfg.root_tasks().count(), 1);
    }

    #[test]
    fn unknown_field_is_an_error() {
        assert!(RootConfig::from_neex_str(r#"{"tasks": {"build": {"output": []}}}"#).is_err());
        assert!(RootConfig::from_neex_str(r#"{"pipeline": {}}"#).is_err());
    }

    #[test]
    fn command_on_generic_task_is_an_error() {
        assert!(RootConfig::from_neex_str(r#"{"tasks": {"build": {"command": "make"}}}"#).is_err());
    }

    #[test]
    fn builtin_defaults() {
        let cfg = RootConfig::default();
        let build = cfg.task_config("x", "build");
        assert_eq!(build.cache, Some(true));
        assert_eq!(build.depends_on.unwrap(), vec!["^build"]);
        let deploy = cfg.task_config("x", "deploy");
        assert_eq!(deploy.cache, Some(false));
        let dev = cfg.task_config("x", "dev");
        assert_eq!(dev.persistent, Some(true));
    }

    #[test]
    fn config_mention_enables_cache_for_unknown_task() {
        let cfg = RootConfig::from_neex_str(r#"{"tasks": {"codegen": {"outputs": ["gen/**"]}}}"#)
            .unwrap();
        assert_eq!(cfg.task_config("x", "codegen").cache, Some(true));
    }

    #[test]
    fn turbo_v1_and_v2() {
        let v1 = RootConfig::from_turbo_str(
            r#"{ "pipeline": { "build": { "dependsOn": ["^build", "$FOO"], "outputs": ["dist/**"], "outputMode": "new-only" } } }"#,
        )
        .unwrap();
        assert_eq!(v1.source, ConfigSource::Turbo);
        assert_eq!(
            v1.tasks["build"].depends_on.as_ref().unwrap(),
            &vec!["^build"]
        );
        assert!(v1.warnings.iter().any(|w| w.contains("$FOO")));

        let v2 = RootConfig::from_turbo_str(
            r#"// comment
            { "$schema": "https://turborepo.dev/schema.json",
              "globalDependencies": ["tsconfig.base.json"],
              "globalEnv": ["CI"],
              "tasks": {
                "build": { "inputs": ["$TURBO_DEFAULT$", "!README.md"], "outputs": ["dist/**"], "env": ["NODE_ENV"] },
                "dev": { "cache": false, "persistent": true }
              } }"#,
        )
        .unwrap();
        assert_eq!(v2.global_inputs, vec!["tsconfig.base.json"]);
        assert_eq!(
            v2.tasks["build"].inputs.as_ref().unwrap(),
            &vec!["!README.md"]
        );
        assert_eq!(v2.tasks["dev"].persistent, Some(true));
    }

    #[test]
    fn comment_stripping_keeps_strings() {
        let s = strip_json_comments(
            r#"{"a": "http://x", /* c */ "b": 1 // t
        }"#,
        );
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["a"], "http://x");
        assert_eq!(v["b"], 1);
    }
}
