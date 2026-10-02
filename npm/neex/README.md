<div align="center">
 <a href="https://github.com/Neexjs">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://i.ibb.co/6ct76YVN/neex.png">
<img width="130" height="120" alt="Neex" src="https://i.ibb.co/6ct76YVN/neex.png" style="border-radius: 50%;" />
</picture>
</a>

<h1>Neex</h1>

<p><strong>Fast, polyglot monorepo task runner.<br/>One small config. Caching you can trust. Any language.</strong></p>

<p>
  <a href="https://www.npmjs.com/package/neex"><img src="https://img.shields.io/npm/v/neex.svg?style=for-the-badge&labelColor=000000&color=0066FF&logo=npm" alt="NPM" /></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Built%20with-Rust-0066FF.svg?style=for-the-badge&labelColor=000000&logo=rust" alt="Rust" /></a>
  <a href="https://github.com/Neexjs/neex/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT-0066FF.svg?style=for-the-badge&labelColor=000000" alt="MIT" /></a>
</p>

</div>

---

## Why neex

- **Every language, zero config.** neex reads the manifests you already have: JS workspaces (npm, pnpm, yarn, bun), Cargo workspaces, `go.work`, and uv workspaces. Anything else joins with a three-line `neex.json`. No fake `package.json` next to your Go module, and no per-project YAML.
- **Turborepo-simple.** One optional `neex.json` at the root. An existing `turbo.json` works as-is.
- **A cache you can trust.**
  - Failed runs are never cached.
  - Keys contain only portable, workspace-relative inputs, so they match across machines.
  - Upstream projects are part of every key, so compiled languages invalidate correctly.
  - Restores never follow symlinks or write outside the project.
- **It tells you why.** `neex why build` shows exactly which file, env var or dependency caused a cache miss.
- **Honest in CI.**
  - The exit code is the highest task exit code.
  - `--affected` refuses to guess when it can't find the base branch.
  - Logs are grouped on GitHub Actions, and failed tasks are never folded away.
- **No daemon required.** Results never depend on background state.

## Install

```bash
npm install -D neex        # or: pnpm add -D neex / yarn add -D neex / bun add -d neex
```

Prebuilt binaries: macOS (arm64, x64), Linux x64, Windows x64. Or build from source with `cargo install --path crates/neex-cli`.

## Quick start

```bash
neex init          # writes neex.json and adds .neex/ to .gitignore
neex ls            # the projects neex found, in every language
neex build         # builds everything, dependencies first
neex build         # again: configured builds can hit the cache
neex why build     # after an edit: what changed, per task
```

Inside a project directory, `neex build` builds that project and what it depends on. Use `--all` to build everything from anywhere.

## Languages

| Ecosystem | Projects come from | Dependencies | Default tasks |
| --- | --- | --- | --- |
| JavaScript / TypeScript | `pnpm-workspace.yaml`, `package.json` `workspaces`, or a standalone `package.json` | `dependencies`, `devDependencies`, `peerDependencies`, `optionalDependencies` | `package.json` scripts, run through your package manager |
| Rust | `Cargo.toml` `[workspace]` members / exclude | `path` deps and `workspace = true` deps | `build` `test` `lint` `check` → `cargo … -p <crate>` |
| Go | `go.work` `use` (or a single `go.mod`) | local `require` and `replace => ./path` | `build` `test` `lint` → `go build/test/vet ./...` |
| Python | `pyproject.toml` `[tool.uv.workspace]` | `[tool.uv.sources]` `workspace = true` or `path` | `build` `test` `lint` → `uv build`, `uv run pytest`, `uv run ruff check .` |
| Anything else | a `neex.json` in a directory listed in `projects` | `deps` in that file | the commands you declare |

Lockfiles and toolchain pins (`pnpm-lock.yaml`, `Cargo.lock`, `rust-toolchain.toml`, `go.work.sum`, `uv.lock`, `.nvmrc`, `.python-version`, …) are part of every key automatically. neex does not install toolchains. It uses what is on your `PATH` and hashes the pins.

## neex.json

Zero config works. When you want more, the whole file looks like this:

```jsonc
{
  "$schema": "https://raw.githubusercontent.com/Neexjs/neex/main/schema.json",
  "projects": ["services/*"],          // extra dirs with their own neex.json
  "globalEnv": ["CI"],                 // env vars that affect every task
  "globalInputs": ["tsconfig.base.json"],
  "tasks": {
    "build": { "dependsOn": ["^build"], "outputs": ["dist/**", ".next/**", "!.next/cache/**"] },
    "test":  { "dependsOn": ["build"], "inputs": ["src/**", "tests/**"] },
    "lint":  {},
    "dev":   { "persistent": true },
    "web#build": { "env": ["NEXT_PUBLIC_API_URL"] },
    "//#format": { "command": "prettier --check ." }
  }
}
```

A project that no provider knows (proto files, a Makefile, a C++ lib) declares itself:

```json
{ "name": "proto", "deps": ["schemas"], "tasks": { "generate": "buf generate", "build": "make" } }
```

| Task field | Meaning |
| --- | --- |
| `dependsOn` | `^build`: the task in upstream projects first. `build`: a task in the same project. `api#gen`: a specific task. |
| `inputs` | Project-relative globs. By default every non-ignored file counts. Positive globs restrict the set, `!` excludes, `//path` adds a root file. |
| `outputs` | Stored after a successful run and restored on a hit. `!` excludes. |
| `env` | Env vars whose **values** go into the key. `NEXT_PUBLIC_*` matches a prefix. |
| `passThroughEnv` | Available to the task but **not** in the key. Use sparingly. |
| `cache` | On by default for `build`, `test`, `lint`, `check`, `typecheck` and anything you configure. Off for unknown tasks, so `deploy` is never replayed. |
| `persistent` | Dev servers. Never cached, and nothing may depend on them. |
| `command` | Shell command for `project#task`, `//#task`, or in project files. |

Precedence runs from lowest to highest: built-in defaults → root `tasks.build` → root `tasks["web#build"]` → the project's own `neex.json`. Unknown fields are errors, so typos are caught.

### Coming from Turborepo

neex reads `turbo.json` (both `pipeline` and `tasks`) when there is no `neex.json`. Run `neex migrate` to convert it permanently. It prints a warning for anything that doesn't translate, like `dotEnv`.

## Commands

| Command | |
| --- | --- |
| `neex <task>…` / `neex run <task>…` | Run tasks. Anything after `--` is passed to the requested tasks. |
| `neex why <task>` | Explain, per task, whether it would hit the cache and what changed since the last run |
| `neex ls` | Projects, their language, path and dependencies |
| `neex graph [task]` | The project graph in build order, or the task graph for a task |
| `neex init` | Add neex to this repo, or scaffold a new one in an empty directory |
| `neex migrate` | Convert `turbo.json` to `neex.json` |
| `neex info` | Root, config, cache size, remote cache |
| `neex prune` | Delete the local cache |
| `neex login` / `neex logout` | Configure the S3/R2 remote cache |

| Flag | |
| --- | --- |
| `-F, --filter <p>` | `web`, `./apps/web`, `@acme/*`, `web...` (with its dependencies), `...ui` (with its dependents), `!docs`. Repeatable. |
| `--affected [--base <ref>]` | Only projects changed since `--base`, `NEEX_SCM_BASE`, `GITHUB_BASE_REF`, `origin/main` or `main`, plus their dependents |
| `-a, --all` | Every project, even inside a project directory |
| `-c, --concurrency <n\|n%>` | Parallel tasks. The default is the number of CPUs. |
| `--force` / `--no-cache` | Ignore cache reads / skip the cache entirely |
| `--continue[=dependencies-successful\|always]` | Keep going after a failure |
| `--dry[=json]` | Show the plan, hashes and hit/miss without running |
| `--output-logs=full\|new-only\|errors-only\|none` | How much output to show |
| `--log-order=auto\|stream\|grouped` | `auto` groups logs on GitHub Actions |
| `--summarize` | Write a JSON run summary to `.neex/runs/` |
| `--tui` | Interactive terminal UI |

## Caching

A task's key is a BLAKE3 hash of:

- neex version, OS and architecture
- the project, the task, its effective configuration and its exact command (including the script body)
- every input file (path and content, plus the executable bit)
- a fingerprint of every project it depends on, directly or transitively
- the keys of the tasks it depends on
- digests of declared env var values (unset vars are recorded as unset; raw values are not stored)
- lockfiles, toolchain pins, `neex.json` and `globalInputs`

The input rules:

- Paths are workspace-relative, so moving a checkout does not change its file fingerprints. Keys also include OS and architecture.
- `.gitignore` inside the workspace is honoured. Ignore files above the workspace are not, so a stray `~/.gitignore` can't hide sources.
- Declared outputs are never inputs.

`build` and `compile` tasks require declared `outputs` to use the cache. Without them, they execute every time so missing build products are never silently skipped. Use `outputs: []` only for a deliberately log-only build.

Only successful runs are stored. On a hit, logs are replayed and outputs are restored. All blobs are verified before any output changes. Files matching declared `outputs` that are absent from the snapshot are removed; excluded files and files outside these globs are preserved. Declare only paths owned by the task, and avoid overlapping outputs between concurrently running tasks. Unchanged files aren't rewritten, so file watchers stay quiet. Everything lives in `.neex/cache/`.

The current key format is `neex-key-v3`; older snapshots are cache misses. Old records may contain raw env values from previous versions; migrating keys does not erase those records.

Cancelling a task run terminates its subprocess group on Unix and its Job Object on Windows, including nested package managers. The npm launcher forwards termination to the CLI; Unix Ctrl+C, SIGTERM and SIGHUP wait for cleanup.

### Remote cache

```bash
neex login    # S3, Cloudflare R2, MinIO, or any S3-compatible storage
```

Artifacts are content-addressed and verified on download.

To protect against cache poisoning, **only trusted CI runs upload**: `CI` is set and the run isn't a pull request. Local machines and PR builds only read. Override with `NEEX_REMOTE_CACHE_WRITE=always|never`. Credentials are stored in `~/.neex/config.json` with mode `600`.

## CI (GitHub Actions)

```yaml
- uses: actions/checkout@v6
  with:
    fetch-depth: 0              # --affected needs the base branch
- run: npm ci
- run: npx neex build test --affected
```

Exit code: `0` when everything succeeded. Otherwise the highest exit code of a failed task.

## Architecture

```
crates/
├── neex-core/    workspace discovery (providers), config, task graph, hashing,
│                 executor, artifact store, remote cache
├── neex-cli/     the `neex` binary: commands, output, TUI
└── neex-daemon/  optional file watcher (never required for correct results)
npm/
├── neex/         npm launcher + per-platform binaries
└── create-neex/  project scaffolder
```

The daemon's sled database contains file hash hints. Task results always use `neex-core::ArtifactStore`. LAN/P2P sharing is a prototype compiled only with the opt-in `neex-daemon/experimental-p2p` feature; it is disabled in default CLI builds. `neex-napi` is a development placeholder and has `publish = false`; it is not a supported npm API.

## Development checks

```bash
pnpm install --frozen-lockfile
pnpm run build
cargo test --workspace --locked
cargo build --release -p neex-cli --locked
pnpm test:consumer    # pack, install and run the actual npm launcher
pnpm test:templates   # install both templates, production build/HTTP and cache replay (Bun required)
cargo test -p neex-daemon --features experimental-p2p --locked
```

CI runs Rust and packed npm consumer checks on Linux, macOS and Windows, and full template checks on Linux. The consumer fixture packs the newly built binary into a local platform package; it does not depend on a previously published binary. S3 protocol tests use a local HTTP object service with dummy credentials, including missing/corrupt artifacts and write-policy enforcement.

## Roadmap

- Turborepo-compatible remote cache API
- `neex prune --docker <app>`: a minimal subset of the repo for container builds
- `neex watch`
- More providers: Gradle/Maven, .NET, Poetry, Deno

## License

MIT © [Neexjs](https://github.com/Neexjs)
