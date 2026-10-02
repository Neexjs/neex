# {{projectName}}

> Built with [Neex](https://github.com/Neexjs/neex) - Ultra-fast Monorepo Build Tool

## 🚀 Quick Start

```bash
# Development
pnpm dev

# Or run separately
pnpm exec neex dev --filter=@{{projectName}}/web   # Frontend
pnpm exec neex dev --filter=@{{projectName}}/api   # Backend
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
| `pnpm dev` | Start all apps in dev mode |
| `pnpm build` | Build all packages |
| `neex graph` | Show dependency graph |
| `neex ls` | List all packages |

## 📦 Tech Stack

- **Frontend**: Next.js 15, React 19, TypeScript
- **Backend**: Hono, Bun
- **Build Tool**: Neex
