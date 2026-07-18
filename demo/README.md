# 🚀 Neexp Demo Monorepo

Official demo showcasing **Neexp** - the ultra-fast monorepo build tool.

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
neexp dev

# Build all packages
neexp build --all
```

## 🛠️ Neexp Commands

```bash
# Development
neexp dev                    # Start all dev servers
neexp dev --filter=@demo/web # Start specific package

# Building
neexp build --all            # Build all packages
neexp build --filter=@demo/api # Build specific package
neexp build --symbols        # Symbol-level rebuild

# Utilities
neexp --list                 # List all packages
neexp --graph                # Show dependency graph
neexp --info                 # Project information
neexp --prune                # Clean cache

# Cloud Cache
neexp --login                # Setup cloud cache (S3/R2)
```

## 🏗️ Project Structure

```
neexp-demo/
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

**Built with [Neexp](https://github.com/Neexjs/neex)** ⚡
