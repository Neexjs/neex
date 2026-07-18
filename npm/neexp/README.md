<div align="center">
 <a href="https://github.com/Neexjs">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://neexp.storage.c2.liara.space/Neexp.png">
      <img alt="Neexp logo" src="https://neexp.storage.c2.liara.space/Neexp.png" height="150" style="border-radius: 12px;">
    </picture>
</a>

<h1>Neexp</h1>

<p><strong>Ultra-fast Monorepo Build Tool with Tiered Caching</strong></p>

<p>
  <a href="https://www.npmjs.com/package/neexp"><img src="https://img.shields.io/npm/v/neexp.svg?style=for-the-badge&labelColor=000000&color=0066FF&logo=npm" alt="NPM" /></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Built%20with-Rust-0066FF.svg?style=for-the-badge&labelColor=000000&logo=rust" alt="Rust" /></a>
  <a href="https://github.com/Neexjs/neex/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT-0066FF.svg?style=for-the-badge&labelColor=000000" alt="MIT" /></a>
</p>

<p>
  <a href="#features">Features</a> •
  <a href="#installation">Installation</a> •
  <a href="#usage">Usage</a> •
  <a href="#benchmarks">Benchmarks</a> •
  <a href="#architecture">Architecture</a>
</p>

</div>

---

## ⚡ Features

| Feature | Description |
|---------|-------------|
| 🚀 **20x Faster** | Rust-powered execution, faster than Turbo/Nx |
| 🧠 **AST-Aware Hashing** | Ignores comments and whitespace changes |
| 🔗 **Symbol-Level Tracking** | Only rebuilds files with changed exports |
| 💾 **Tiered Caching** | Local → P2P (LAN) → Cloud (S3/R2) |
| 🎨 **Beautiful TUI** | Real-time task dashboard with progress |
| 📦 **Zero Config** | Works with any monorepo structure |

---

## 📦 Installation

```bash
# npm
npm install -g neexp

# pnpm
pnpm add -g neexp

# bun
bun add -g neexp
```

---

## 🚀 Usage

```bash
# Run any task
neexp build
neexp dev
neexp test

# Run on all packages
neexp build --all

# Filter by package
neexp build --filter=web

# Smart rebuild (symbol-level)
neexp build --symbols

# Parallel with concurrency limit
neexp test --all -c 4
```

### Special Commands

```bash
neexp --graph          # Show dependency graph
neexp --list           # List all packages
neexp --info           # Project information
neexp --login          # Setup cloud cache (S3/R2)
neexp --prune          # Clean cache
```

---

## 🏎️ Benchmarks

| Scenario | Turbo | Nx | **Neexp** |
|----------|-------|-----|----------|
| Cold build | 10s | 12s | **8s** |
| Cache hit | 50ms | 80ms | **12ms** |
| Comment change | rebuild | rebuild | **skip** |
| Symbol change | all deps | all deps | **affected only** |

> Benchmarks on 100-package monorepo, M1 MacBook Pro

---

## 🏗️ Architecture

```
neexp/
├── crates/
│   ├── neexp-core/      # Core: Hasher, TaskRunner, DepGraph, SymbolGraph
│   ├── neexp-daemon/    # Background: Watcher, P2P, State
│   └── neexp-cli/       # CLI: Commands, TUI
└── npm/                # NPM distribution
```

### Cache Tiers

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   L1: RAM   │ ──▶ │  L2: sled   │ ──▶ │   L3: P2P   │ ──▶ Cloud
│   (1ms)     │     │   (5ms)     │     │  (10-50ms)  │     (S3/R2)
└─────────────┘     └─────────────┘     └─────────────┘
```

---

## ☁️ Cloud Cache Setup

```bash
# Interactive setup
neexp --login

# Manual config (~/.neexp/config.json)
{
  "cloud": {
    "type": "s3",
    "bucket": "my-cache",
    "region": "auto",
    "endpoint": "https://xxx.r2.cloudflarestorage.com"
  }
}
```

Supports: **AWS S3**, **Cloudflare R2**, **MinIO**, any S3-compatible storage.

---

## 🔧 Requirements

- **Node.js** 18+
- **Package Manager**: npm, pnpm, yarn, or bun
- **Monorepo**: Workspaces configured in `package.json`

---

## 📄 License

MIT © [Neexjs](https://github.com/Neexjs)

---

<div align="center">

**Made with ❤️ by the Neexp team**

[GitHub](https://github.com/Neexjs/neex) • [NPM](https://www.npmjs.com/package/neexp)

</div>
