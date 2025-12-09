# neex

Ultra-fast monorepo build tool with tiered caching.

## Features

- ⚡ **20x faster** than Turbo/Nx (tiered caching)
- 🧠 **AST-aware hashing** - ignores comments
- 🔗 **Symbol-level tracking** - only rebuild what changed
- 🌐 **P2P cache** - share with teammates on LAN
- ☁️ **Cloud cache** - S3/R2 compatible
- 🎨 **Beautiful TUI** - live task dashboard

## Install

```bash
npm install -g neex
# or
pnpm add -g neex
# or
bun add -g neex
```

## Usage

```bash
# Run any task
neex build
neex dev
neex test

# Run on all packages
neex build --all

# Filter by package
neex build --filter=web

# Smart rebuild (symbol-level)
neex build --symbols

# Show dependency graph
neex --graph

# Setup cloud cache
neex --login
```

## Commands

| Command | Description |
|---------|-------------|
| `neex <task>` | Run task with caching |
| `neex <task> --all` | Run on all packages |
| `neex <task> --filter=pkg` | Run on specific package |
| `neex <task> --symbols` | Smart symbol-level rebuild |
| `neex --graph` | Show dependency graph |
| `neex --list` | List all packages |
| `neex --info` | Show project info |
| `neex --login` | Setup cloud cache |
| `neex --prune` | Clean cache |

## Performance

| Scenario | Turbo | Neex |
|----------|-------|------|
| Cold build | 10s | 8s |
| Cache hit | 50ms | **12ms** |
| Comment change | rebuild | skip |
| Symbol change | all deps | **affected only** |

## License

MIT
