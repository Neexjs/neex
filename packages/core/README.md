<div align="center">
 <a href="https://github.com/Neexjs">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://neex.storage.c2.liara.space/Neex.png">
<img alt="Neex logo" src="https://neex.storage.c2.liara.space/Neex.png" height="150" style="border-radius: 12px;">
</picture>
</a>

# Neex

### Neex - Modern Fullstack Framework Built on Express and Next.js. Fast to Start, Easy to Build, Ready to Deploy.

[![NPM version](https://img.shields.io/npm/v/neex.svg?style=for-the-badge&labelColor=000000&color=0066FF&borderRadius=8)](https://www.npmjs.com/package/neex)
[![Download Count](https://img.shields.io/npm/dt/neex.svg?style=for-the-badge&labelColor=000000&color=0066FF&borderRadius=8)](https://www.npmjs.com/package/neex)
[![MIT License](https://img.shields.io/badge/license-MIT-0066FF.svg?style=for-the-badge&labelColor=000000&borderRadius=8)](https://github.com/neexjs/blob/main/LICENSE)
[![GitHub](https://img.shields.io/badge/GitHub-Neex-0066FF.svg?style=for-the-badge&logo=github&labelColor=000000&logoWidth=20&borderRadius=8)](https://github.com/Neexjs)

</div>

## 🎯 Overview

**Neex = nextjs + express 🌱**

**Neex: A Powerful Fusion of Next.js and Express for Fullstack Development**

**Neex** is a modern and advanced fullstack framework that combines **Next.js** and **Express** to deliver an exceptional development experience. Designed for speed, simplicity, and scalability, Neex empowers developers to effortlessly build and deploy robust, high-performance applications.

## ✨ Why Choose Neex?

Unlock the full potential of your fullstack projects with Neex’s powerful features, tailored for modern web development:

- 🏗️ **Fullstack Architecture**: Independent codebases for **frontend (Next.js)** and **backend (Express)** with isolated dependencies for maximum flexibility.
- 🔄 **Monorepo Workflow**: Enjoy the simplicity of a single repository with clear separation between frontend and backend domains.
- ⚡ **Zero Configuration**: Out-of-the-box setup for **TypeScript**, **Prisma**, and **environment variable management** for instant development readiness.
- 🛠️ **Production-Ready**: Built-in best practices for performance, scalability, and reliability at an enterprise level.
- 📦 **Isolated Dependencies**: Each part of the stack has its own `package.json` to ensure modular and scalable development.
- 🧩 **Separation of Concerns**: Clearly divided frontend and backend logic leads to cleaner, more maintainable codebases.
- 🔒 **Secure Environment Management**: Built-in support for `dotenv` to handle environment variables safely and efficiently.
- 📊 **Database Integration**: Native support for **Prisma ORM** enables fast, type-safe, and efficient database development.
- 🛠️ **Powerful CLI**: Intuitive CLI commands to simplify development, building, and deployment workflows.

With these features, **Neex** is the ideal choice for developers seeking a fullstack framework that’s **SEO-optimized, scalable, efficient, and developer-friendly**. Start building high-performance web applications today with Neex’s robust toolset and clean architecture.

## 🚀 Quick Start

### Create a New Neex Project – Step-by-Step

1. **Run one of the creation commands**:

   ```bash
   npx neex init
   # or
   npx create-neex
   ```

2. **Enter your project name** (e.g., `my-awesome-project`).

3. **Select your package manager**:
   - npm
   - yarn
   - pnpm
   - bun

4. **Wait for the project structure to be created** automatically.

5. **Install dependencies**:

   ```bash
   cd my-project
   npm install  # or yarn, pnpm install, bun install
   ```

6. **Start development**:

   ```bash
   npm run dev  # or yarn dev, pnpm dev, bun dev
   ```

7. **Access your project**:
   - client: `http://localhost:3000`
   - server: `http://localhost:8000`

## 🖥️ CLI Commands

Neex provides a powerful CLI with commands tailored for development, building, and production deployment.

### Core Commands

- **`neex init`**: Initializes a new Neex project using `create-neex`.
- **`neex dev [file]`**: Starts a fast TypeScript development server with live-reloading.
- **`neex build [source]`**: Compiles TypeScript projects for production.
- **`neex start [file]`**: Starts a production-ready application with process management (PM2-like).
- **`neex p <commands...>`** (aliases: `par`, `parallel`): Runs commands in parallel.
- **`neex s <commands...>`** (aliases: `seq`, `sequential`): Runs commands sequentially.
- **`neex dev:clean`**: Cleans development cache and temporary files.
- **`neex dev:check`**: Validates TypeScript configuration.
- **`neex dev:info`**: Displays development server information.

### Command Examples

#### Development (`dev` Command)

The `dev` command provides a fast, live-reloading development environment.

```bash
# Start development server for a TypeScript file
neex dev src/index.ts

# Watch specific directories
neex dev --watch src,public "npm run dev"

# Ignore patterns
neex dev --ignore "*.log,dist/**" "node server.js"

# Specify file extensions
neex dev --ext ts,tsx,js "npm run dev"

# Ultra-fast mode with 50ms delay
neex dev --fast "npm run dev"
```

#### Building (`build` Command)

Compile TypeScript projects for production.

```bash
# Build project from src to dist
neex build

# Build with source maps and watch mode
neex build src --sourcemap --watch

# Quick compilation
neex compile src
```

#### Production (`start` Command)

Run production-ready applications with advanced process management.

```bash
# Start production server
neex start dist/server.js

# Start with multiple workers
neex start dist/server.js --workers 4

# Enable health check endpoint
neex start dist/server.js --health-port 3001
```

### Explanation of Scripts

- **`dev`**: Uses `neex p` to run the frontend (`dev:client`) and backend (`dev:server`) development servers in parallel, leveraging Neex's ability to manage concurrent processes with clear output.
- **`dev:client`**: Runs the Next.js frontend development server in the `apps/client` directory using Bun.
- **`dev:server`**: Runs the Express backend development server in the `apps/server` directory using Bun.
- **`build`**: Uses `neex s` to sequentially execute Prisma-related tasks (`prisma:generate`, `prisma:migrate`) followed by building both the frontend (`build:client`) and backend (`build:server`) projects.
- **`build:client`**: Builds the Next.js frontend in the `apps/client` directory.
- **`build:server`**: Builds the Express backend in the `apps/server` directory.
- **`start`**: Uses `neex p` to start both the frontend (`start:client`) and backend (`start:server`) in production mode concurrently, utilizing Neex's advanced process management for clustering and health checks.
- **`start:client`**: Starts the Next.js frontend in production mode.
- **`start:server`**: Starts the Express backend in production mode.
- **`prisma:generate`**: Generates the Prisma client for the backend in `apps/server`.
- **`prisma:migrate`**: Applies Prisma database migrations in `apps/server`.

### Running the Scripts

```bash
npm run dev          # Starts frontend and backend development servers in parallel
npm run build        # Sequentially generates Prisma client, applies migrations, and builds both projects
npm run start        # Starts frontend and backend in production mode concurrently
```

#### Parallel and Sequential Execution

Run multiple scripts efficiently.

```bash
# Parallel execution
neex p "npm run build:frontend" "npm run build:backend"

# Sequential execution
neex s "npm run clean" "npm run build" "npm run deploy"

# Parallel with sequential flag
neex p -q "npm run step1" "npm run step2"
```

## 📂 Project Structure

Neex creates a polyrepo-in-monorepo structure for clear separation and scalability:

```
my-project/
├── .env                 # Root environment variables
├── apps/
│   ├── client/          # Next.js frontend project
│   │   ├── src/
│   │   │   ├── app/     # Next.js App Router
│   │   │   └── ...
│   │   ├── next.config.ts
│   │   ├── package.json # Frontend dependencies
│   │   └── tsconfig.json
│   └── server/          # Express backend project
│       ├── src/
│       │   ├── server.ts # Express server entry
│       │   └── ...
│       ├── package.json # Backend dependencies
│       └── tsconfig.json
├── package.json         # Root orchestration
└── README.md
```

Each `apps/client` and `apps/server` directory is a fully independent project that can be extracted into its own repository if needed, while benefiting from a unified monorepo workflow.

## ⚙️ Advanced Options

### `neex init`

```bash
# Create in current directory
npx create-neex@latest .

# Create with specific name
npx create-neex@latest my-project

# Enable debug mode
npx create-neex@latest my-project --debug
```

### `neex dev`

```bash
# Custom restart delay
neex dev --delay 2000 "npm run dev"

# Disable console clearing
neex dev --no-clear "node app.js"

# Framework-specific example (Nest.js)
neex dev --watch src --ext ts "npm run start:dev"
```

### `neex build`

```bash
# Custom output directory
neex build src --output build

# Specify TypeScript target
neex build src --target es2022

# Analyze bundle size
neex build src --analyze
```

### `neex start`

```bash
# Custom port
neex start dist/server.js --port 8080

# Enable Node.js inspector
neex start dist/server.js --inspect

# Set memory limit
neex start dist/server.js --max-memory 1G
```

## 💡 Real-World Scenarios

Integrate Neex into your `package.json` to streamline development, building, and deployment workflows for your fullstack projects. Whether you're working with an **Express-only backend** or a combined **Express + Next.js** application, Neex’s powerful CLI and monorepo architecture make it easy to manage your projects efficiently.

### Example 1: Express-Only Project

For a standalone Express backend, configure your `package.json` with minimal yet powerful scripts to handle development, building, and production:

```json
{
  "scripts": {
    "dev": "neex dev src/server.ts",
    "build": "neex build",
    "start": "neex start"
  }
}
```

**Run your scripts**:

```bash
npm run dev    # Starts the Express development server with live-reloading
npm run build  # Compiles TypeScript to production-ready JavaScript
npm run start  # Launches the Express server in production mode
```

This setup provides a clean, efficient workflow for Express-based projects, leveraging Neex’s live-reloading and production-ready features for rapid development and deployment.

### Example 2: Fullstack Express + Next.js Project => Neex

For a fullstack application with a **Next.js frontend** and an **Express backend**, Neex’s polyrepo-in-monorepo architecture shines. The following `package.json` example demonstrates how to manage both projects concurrently, with integrated Prisma ORM for database operations:

```json
{
  "scripts": {
    "dev": "neex p dev:client dev:server",
    "dev:client": "cd apps/client && npm run dev",
    "dev:server": "cd apps/server && npm run dev",
    "build": "neex s prisma:generate prisma:migrate build:client build:server",
    "build:client": "cd apps/client && npm run build",
    "build:server": "cd apps/server && npm run build",
    "start": "neex p start:client start:server",
    "start:client": "cd apps/client && npm run start",
    "start:server": "cd apps/server && npm run start",
    "prisma:generate": "cd apps/server && npx prisma generate",
    "prisma:migrate": "cd apps/server && npx prisma db push"
  }
}
```

**Run your scripts**:

```bash
npm run dev    # Starts Next.js frontend and Express backend in parallel with live-reloading
npm run build  # Sequentially generates Prisma client, applies migrations, and builds both projects
npm run start  # Launches frontend and backend in production mode concurrently
```

### Why It Works

- **Parallel Execution**: Use `neex p` to run frontend and backend tasks simultaneously, ensuring efficient development and production workflows.
- **Sequential Execution**: Use `neex s` for tasks like Prisma migrations and builds, ensuring dependencies are handled in the correct order.
- **Modular Architecture**: Neex’s polyrepo-in-monorepo structure keeps frontend and backend codebases independent yet unified, making it easy to scale or extract projects.
- **Prisma Integration**: Streamlined database management with `prisma:generate` and `prisma:migrate` for type-safe, rapid development.

This configuration is perfect for developers building fullstack applications with **Next.js**, **Express**, and **Prisma**, offering a seamless, scalable, and maintainable development experience. Start using Neex today to simplify your workflow and build high-performance applications with ease!

## 📋 System Requirements

- **Node.js**: 20.0.0 or later
- **OS**: macOS, Windows, or Linux
- **Package Manager**: npm, yarn, pnpm, or bun

## 🤝 Contributing

Contributions are welcome! Visit our [GitHub issues page](https://github.com/Neexjs) to get started.

## 📄 License

MIT License. See [LICENSE](https://github.com/neexjs/blob/main/LICENSE) for details.
