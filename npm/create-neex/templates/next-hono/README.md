# {{projectName}}

> Built with [Neex](https://github.com/Neexjs/neex) - Ultra-fast Monorepo Build Tool

## 🚀 Quick Start

```bash
# Development
neex dev --all

# Or run separately
neex dev --filter=web   # Frontend
neex dev --filter=api   # Backend
```

## 📁 Structure

```
├── apps/
│   ├── web/        # Next.js 15 frontend
│   └── api/        # Hono backend
├── packages/
│   ├── ui/         # Shared UI components
│   └── utils/      # Shared utilities
└── package.json
```

## 🛠 Commands

| Command | Description |
|---------|-------------|
| `neex dev --all` | Start all apps in dev mode |
| `neex build --all` | Build all packages |
| `neex --graph` | Show dependency graph |
| `neex --list` | List all packages |

## 📦 Tech Stack

- **Frontend**: Next.js 15, React 19, TypeScript
- **Backend**: Hono, Bun
- **Build Tool**: Neex
