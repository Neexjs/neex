# {{projectName}}

> Built with [Neexp](https://github.com/Neexjs/neex) - Ultra-fast Monorepo Build Tool

## 🚀 Quick Start

```bash
# Development
neexp dev --all

# Or run separately
neexp dev --filter=web   # Frontend
neexp dev --filter=api   # Backend
```

## 📁 Structure

```
├── apps/
│   ├── web/        # Next.js 15 frontend
│   └── api/        # Express 5 backend
├── packages/
│   ├── ui/         # Shared UI components
│   └── utils/      # Shared utilities
└── package.json
```

## 🛠 Commands

| Command | Description |
|---------|-------------|
| `neexp dev --all` | Start all apps in dev mode |
| `neexp build --all` | Build all packages |
| `neexp --graph` | Show dependency graph |
| `neexp --list` | List all packages |

## 📦 Tech Stack

- **Frontend**: Next.js 15, React 19, TypeScript
- **Backend**: Express 5, Node.js
- **Build Tool**: Neexp
