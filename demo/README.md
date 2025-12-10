# 🚀 Neex Demo Monorepo

Official demo showcasing **Neex** - the ultra-fast monorepo build tool.

## 📦 Packages

| Package | Description |
|---------|-------------|
| `@demo/web` | Next.js 16 frontend |
| `@demo/api` | Express 5 backend |
| `@demo/ui` | Shared React components |
| `@demo/utils` | Shared utilities |
| `@demo/eslint-config` | ESLint configuration |
| `@demo/typescript-config` | TypeScript configuration |

## ⚡ Quick Start

```bash
# Install dependencies
bun install

# Start development
neex dev

# Build all packages
neex build --all
```

## 🛠️ Neex Commands

```bash
# Development
neex dev                    # Start all dev servers
neex dev --filter=@demo/web # Start specific package

# Building
neex build --all            # Build all packages
neex build --filter=@demo/api # Build specific package
neex build --symbols        # Symbol-level rebuild

# Utilities
neex --list                 # List all packages
neex --graph                # Show dependency graph
neex --info                 # Project information
neex --prune                # Clean cache

# Cloud Cache
neex --login                # Setup cloud cache (S3/R2)
```

## 🏗️ Project Structure

```
neex-demo/
├── apps/
│   ├── client/     → @demo/web (Next.js 16)
│   └── server/     → @demo/api (Express 5)
├── packages/
│   ├── ui/         → @demo/ui
│   ├── utils/      → @demo/utils
│   ├── eslint-config/
│   └── typescript-config/
└── package.json
```

## 💾 Caching

```
L1: RAM (1ms) → L2: Disk (5ms) → L3: Cloud (S3/R2)
```

---

**Built with [Neex](https://github.com/Neexjs/neex)** ⚡
