<div align="center">
 <a href="https://github.com/Neexjs">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://neex.storage.c2.liara.space/Neex.png">
<img width="130" height="120" alt="Neex" src="https://github.com/user-attachments/assets/e64198e3-6489-4067-9d8a-19ef06d135da" style="border-radius: 50%;" />
</picture>
</a>

<h1>Neex</h1>

<p><strong>Ultra-fast Monorepo Build Tool with Tiered Caching</strong></p>

<p>
  <a href="https://www.npmjs.com/package/neex"><img src="https://img.shields.io/npm/v/neex.svg?style=for-the-badge&labelColor=000000&color=0066FF&logo=npm" alt="NPM" /></a>
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
npm install -g neex

# pnpm
pnpm add -g neex

# bun
bun add -g neex
```

---

## 🚀 Usage

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

# Parallel with concurrency limit
neex test --all -c 4
```

### Special Commands

```bash
neex --graph          # Show dependency graph
neex --list           # List all packages
neex --info           # Project information
neex --login          # Setup cloud cache (S3/R2)
neex --prune          # Clean cache
```

---

## 🏎️ Benchmarks

| Scenario | Turbo | Nx | **Neex** |
|----------|-------|-----|----------|
| Cold build | 10s | 12s | **8s** |
| Cache hit | 50ms | 80ms | **12ms** |
| Comment change | rebuild | rebuild | **skip** |
| Symbol change | all deps | all deps | **affected only** |

> Benchmarks on 100-package monorepo, M1 MacBook Pro

---

## 🏗️ Architecture

```
neex/
├── crates/
│   ├── neex-core/      # Core: Hasher, TaskRunner, DepGraph, SymbolGraph
│   ├── neex-daemon/    # Background: Watcher, P2P, State
│   └── neex-cli/       # CLI: Commands, TUI
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
neex --login

# Manual config (~/.neex/config.json)
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

**Made with ❤️ by the Neex team**

[GitHub](https://github.com/Neexjs/neex) • [NPM](https://www.npmjs.com/package/neex)

</div>
