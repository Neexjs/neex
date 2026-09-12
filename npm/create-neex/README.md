<div align="center">
  <a href="https://github.com/Neexjs">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://neex.storage.c2.liara.space/Neex.png">
      <img alt="Neex logo" src="https://neex.storage.c2.liara.space/Neex.png" height="150" style="border-radius: 12px;">
    </picture>
  </a>

<h1>create-neex</h1>

<p><strong>Create a new Neex monorepo with one command</strong></p>

<p>
  <a href="https://www.npmjs.com/package/create-neex"><img src="https://img.shields.io/npm/v/create-neex.svg?style=for-the-badge&labelColor=000000&color=0066FF&logo=npm" alt="NPM" /></a>
  <a href="https://github.com/Neexjs/neex/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT-0066FF.svg?style=for-the-badge&labelColor=000000" alt="MIT" /></a>
</p>

</div>

---

## ⚡ Quick Start

```bash
# pnpm
pnpm create neex
```

---

## 🎯 Interactive Setup

```
███╗   ██╗███████╗███████╗██╗  ██╗
████╗  ██║██╔════╝██╔════╝╚██╗██╔╝
██╔██╗ ██║█████╗  █████╗   ╚███╔╝ 
██║╚██╗██║██╔══╝  ██╔══╝   ██╔██╗ 
██║ ╚████║███████╗███████╗██╔╝ ██╗
╚═╝  ╚═══╝╚══════╝╚══════╝╚═╝  ╚═╝

? What is your project name? my-app
? Select a stack:
  ❯ ⚡ Next.js + Hono (Recommended)
    🚀 Next.js + Express
? Select package manager:
  ❯ 📦 pnpm
? Initialize a git repository? Yes

✓ Project my-app created successfully!
```

---

## 📦 Templates

| Template | Frontend | Backend | Best For |
|----------|----------|---------|----------|
| `next-hono` | Next.js 15 | Hono + Bun | ⚡ Performance |
| `next-express` | Next.js 15 | Express 5 | 🔧 Compatibility |

---

## 🗂️ Generated Structure

```
my-app/
├── apps/
│   ├── web/          # Next.js 15 frontend
│   └── api/          # Hono or Express backend
├── packages/
│   ├── ui/           # Shared components
│   └── utils/        # Shared utilities
├── package.json      # Workspace config
└── README.md
```

---

## 🚀 After Creation

```bash
cd my-app
neex dev --all    # Start all apps
```

---

## 📄 License

MIT © [Neexjs](https://github.com/Neexjs)
