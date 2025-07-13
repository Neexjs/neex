<div align="center">
 <a href="https://github.com/Neexjs">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://neex.storage.c2.liara.space/Neex.png">
<img alt="Neex logo" src="https://neex.storage.c2.liara.space/Neex.png" height="150" style="border-radius: 12px;">
</picture>
</a>

# Neex v0.6.37

### 🚀 Neex: The Modern Build System for Polyrepo-in-Monorepo Architecture

[![NPM version](https://img.shields.io/npm/v/neex.svg?style=for-the-badge&labelColor=000000&color=0066FF&borderRadius=8)](https://www.npmjs.com/package/neex)
[![Download Count](https://img.shields.io/npm/dt/neex.svg?style=for-the-badge&labelColor=000000&color=0066FF&borderRadius=8)](https://www.npmjs.com/package/neex)
[![MIT License](https://img.shields.io/badge/license-MIT-0066FF.svg?style=for-the-badge&labelColor=000000&borderRadius=8)](https://github.com/neexjs/blob/main/LICENSE)
[![GitHub](https://img.shields.io/badge/GitHub-Neex-0066FF.svg?style=for-the-badge&logo=github&labelColor=000000&logoWidth=20&borderRadius=8)](https://github.com/Neexjs)
</div>

## 🎯 Overview

next + express = neex 🌱

Neex is a modern build system and script runner designed for **Polyrepo-in-Monorepo** architectures, but powerful enough for any project. It simplifies managing and running multiple scripts across your project, whether they are microservices, frontend applications, or build tasks. Neex offers robust features like parallel and sequential execution, live-reloading for development (`watch`), optimized server running (`servers`), and even basic process management (`pm2`-like commands), all aimed at making your development workflow more efficient, organized, and visually clear.

## ✨ Key Features

- 🎨 **Colored Output** - Distinguish commands with unique colors
- ⚡ **Dual Execution Modes** - Run commands in parallel (`px`) or sequence (`s`, `run`)
- ⏱️ **Smart Timing** - Track execution time for each command
- 🛑 **Error Control** - Stop on first error (perfect for CI/CD)
- 🔢 **Parallel Control** - Limit concurrent processes with `--max-parallel`
- 💻 **Clean Output** - Structured and readable command output
- 🛡️ **Safe Shutdown** - Graceful process termination on interrupt
- 🤫 **Flexible Display** - Control prefixes, timing, and output visibility
- 🧰 **Node.js API** - Programmatic usage in your applications

## 🚀 Installation

Install Neex globally to use it anywhere, or locally in your project.

```bash
# Global install
npm install -g neex

# Local install
npm install --save-dev neex    # npm
yarn add --dev neex          # yarn
pnpm add --save-dev neex     # pnpm
bun add --dev neex           # bun
```

When installed locally, you can run Neex commands using `npx neex ...` or by adding them to your `package.json` scripts.

## 🖥️ Usage

```bash
# Global install
npm i -g neex

# Local install
npm i -D neex    # npm
yarn add -D neex # yarn
pnpm add -D neex # pnpm
bun add -D neex  # bun
```

## 🖥️ Usage

### Core Commands

Neex provides several commands to manage and run your scripts:

- **`px <commands...>`** (*default command*)
  - Runs specified commands in **parallel** by default.
  - Use the `-q` or `--sequential` flag to run them sequentially.
  - Ideal for build steps, tests, or any tasks that can run concurrently.

- **`run <commands...>`** (alias: `s`)
  - Runs specified commands **sequentially**.
  - Useful for tasks that depend on the completion of previous ones (e.g., build then deploy).

- **`servers <commands...>`** (alias: `srv`)
  - Optimized for running multiple development **servers** (e.g., backend API, frontend app) in parallel.
  - Provides grouped and clear output for each server, making it easy to monitor logs.
  - Detects and displays server ports and URLs.

- **`watch <commands...>`** (alias: `w`)
  - Runs commands and **watches for file changes** to automatically restart them (Nodemon-like functionality).
  - Highly configurable with options for watched paths, ignored patterns, extensions, and restart delays.
  - Perfect for development workflows where you need instant feedback on code changes.

- **Process Management Commands**
  - Neex provides built-in process management capabilities, similar to PM2, for long-running applications. These commands operate directly under `neex`:
    - **`start <script_path_or_command> [--name <name>] [--watch] [-- <args...>]`**: Start a new process.
    - **`stop <name_or_id>`**: Stop a running process.
    - **`restart <name_or_id>`**: Restart a running process.
    - **`delete <name_or_id>`**: Stop and delete a process from the list.
    - **`list`** (aliases: `ls`, `status`): List all managed processes.
    - **`logs [name_or_id] [--lines <number>] [--follow]`**: Display logs for a specific process or all if no ID is given.
    - **`monit`**: Launch a monitoring interface for all managed processes.
    - **`save`**: Save the current list of running processes.
    - **`resurrect`**: Restart processes that were previously saved.
    - **`startup`**: Generate a sample startup script (e.g., for systemd).

### General Command Examples

```bash
# Parallel execution (default behavior for px)
neex px "npm run build:api" "npm run build:frontend" "npm run lint"
# px is the command for parallel execution
neex px "npm run test:unit" "npm run test:integration"

# Sequential execution
neex run "npm run clean" "npm run build" "npm run deploy"
# Alias for sequential
neex s "echo First" "echo Second" "echo Third"

# Run 'px' commands sequentially using the -q flag
neex px -q "npm run step1" "npm run step2"
```

### `servers` Command Examples

```bash
# Start frontend and backend development servers
neex servers "cd frontend && npm start" "cd backend && npm start"

# Group output for better readability
neex servers --group-output "npm run dev:service-a" "npm run dev:service-b"
```

### `watch` Command Examples

```bash
# Watch for changes in './src' and restart 'npm run build'
neex watch "npm run build" -w ./src

# Watch for .ts file changes in 'services/' and restart two commands, ignoring 'node_modules'
neex watch "npm run start:service1" "npm run start:service2" -w services/ -e ts,tsx -i node_modules/**

# Clear console on restart and set a delay
neex watch "node server.js" --clear --delay 1500
```

### Process Management Command Examples

```bash
# Start a Node.js app and give it a name
neex start server.js --name my-app

# Start an app and watch for file changes
neex start api/index.js --name my-api --watch

# List all running processes managed by neex
neex list

# View logs for 'my-app'
neex logs my-app

# Stop 'my-api'
neex stop my-api

# Restart 'my-app'
neex restart my-app

# Delete 'my-api' from neex management
neex delete my-api
```

### `dev` Command Examples

The `dev` command is designed for development environments, providing automatic file watching and restart functionality. Here are various ways to use it:

#### 1. Automatic Usage

When no arguments are provided, neex dev will automatically:

- If `package.json` has a `"dev"` script:
  ```bash
  neex dev  # runs: npm run dev
  ```

- If only a `"start"` script exists:
  ```bash
  neex dev  # runs: npm run start
  ```

- If only a `"main"` field exists:
  ```bash
  neex dev  # runs: node main.js (for JavaScript)
  neex dev  # runs: npx ts-node main.ts (for TypeScript)
  ```

#### 2. Direct File Execution

Run JavaScript or TypeScript files directly:

```bash
# JavaScript file
neex dev app.js  # runs: node app.js

# TypeScript file
neex dev server.ts  # runs: npx ts-node server.ts

# ES modules
neex dev index.mjs

# CommonJS TypeScript
neex dev main.cts
```

#### 3. Package Manager Scripts

Run npm/yarn/pnpm scripts:

```bash
# npm scripts
neex dev "npm run dev"
neex dev "npm run start"
neex dev "npm run build"

# yarn scripts
neex dev "yarn dev"
neex dev "yarn start"

# pnpm scripts
neex dev "pnpm dev"
neex dev "pnpm start"
```

#### 4. System Commands

Execute various system commands:

```bash
neex dev "python app.py"
neex dev "php server.php"
neex dev "go run main.go"
neex dev "cargo run"
neex dev "dotnet run"
```

#### 5. Configuration Options

Customize the development experience with these options:

```bash
# Watch specific directories
neex dev --watch src --watch public "npm run dev"

# Ignore certain files/directories
neex dev --ignore "*.log" --ignore "temp/**" "node server.js"

# Specify file extensions
neex dev --ext js,ts,vue,jsx "npm run dev"

# Set restart delay
neex dev --delay 2000 "npm run dev"

# Clear console on restart
neex dev --clear "node app.js"

# Minimal output
neex dev --minimal "npm run dev"

# Disable colors
neex dev --no-color "npm run dev"

# Disable timing display
neex dev --no-timing "npm run dev"

# Stop on first error
neex dev --stop-on-error "npm run test"
```

#### 6. Framework-Specific Examples

```bash
# Express.js
neex dev server.js

# Fastify
neex dev --watch src "npm run dev"

# Nest.js
neex dev --watch src --ext ts "npm run start:dev"

# Koa.js
neex dev --watch app --ext js "node app.js"

# Electron
neex dev --watch src --ext js,html "npm run electron"

# Webpack Dev Server
neex dev --watch src --ext js,css,html "npm run dev"
```

#### 7. Error Handling

```bash
# No command specified and no default script
neex dev
# Error: No command specified for 'neex dev' and no default script

# Non-existent file
neex dev nonexistent.js
# Warning: File "nonexistent.js" not found. Attempting to run as command.
```

### 📚 Examples

### Parallel Execution

```bash
# Parallel execution (default behavior for p)
neex p "npm run build:api" "npm run build:frontend" "npm run lint"
neex par "npm run test:unit" "npm run test:integration"
neex parallel "npm run test:frontend" "npm run test:backend"
```

### Sequential Execution

```bash
# Sequential execution using s command
neex s "npm run clean" "npm run build" "npm run deploy"
neex seq "echo First" "echo Second" "echo Third"
neex sequential "npm run step1" "npm run step2" "npm run step3"
```

### Parallel with Sequential Flag

```bash
# Run parallel commands sequentially using the -q flag
neex p -q "npm run step1" "npm run step2"
neex par -q "npm run build" "npm run test"
neex parallel -q "npm run lint" "npm run format"
```

### Advanced Options

```bash
# With retry options
neex p --retry 3 --retry-delay 1000 "npm run test"

# With output control
neex s --no-color --no-timing --no-output "npm run build"

# With parallel limits
neex p --max-parallel 2 "npm run build:frontend" "npm run build:backend" "npm run build:api"
```

### Advanced Example

```bash
# Run tests & build with max 2 parallel tasks, stop on error
neex p -s -m 2 -t "npm test" "npm run build" "npm run lint"
```

## 📦 Node.js API

```javascript
import { run } from 'neex';
// or: const { run } = require('neex');

async function main() {
  try {
    // Sequential execution
    await run(['echo Step 1', 'echo Step 2'], {
      parallel: false,
      stopOnError: true,
      color: true
    });

    // Parallel execution (max 2)
    await run(['npm test', 'npm run build'], {
      parallel: true,
      maxParallel: 2,
      stopOnError: true
    });
  } catch (error) {
    console.error('Failed:', error);
    process.exit(1);
  }
}
```

### API Options (`RunOptions`)

When using Neex programmatically, you can pass an options object to the `run` function:

```typescript
export interface RunOptions {
  // Run in parallel or sequentially
  parallel: boolean;
  // Maximum number of parallel processes (default: CPU count)
  maxParallel?: number;
  // Show command output (default: true)
  printOutput: boolean;
  // Color output (default: true)
  color: boolean;
  // Show timing information (default: true)
  showTiming: boolean;
  // Show command prefix (default: true)
  prefix: boolean;
  // Stop on error (default: false)
  stopOnError: boolean;
  // Use minimal output format (default: false)
  minimalOutput: boolean;
  // Group output by command (default: false, mainly for server mode)
  groupOutput: boolean;
  // Use server mode formatting (default: false)
  isServerMode: boolean;
  // Number of times to retry a failed command (default: 0)
  retry?: number;
  // Delay in milliseconds between retries (default: 1000)
  retryDelay?: number;
  // Callback to register a cleanup function, called on SIGINT/SIGTERM
  registerCleanup?: (cleanupFn: () => void) => void;
}
```

## 🔄 CI/CD Integration

```yaml
# GitHub Actions example
steps:
  - name: Test & Build
    run: neex s -s "npm test" "npm run build"

  - name: Parallel Tasks
    run: neex p -s -m 4 "npm run lint" "npm test" "npm run e2e"
```

## 💡 Real-world Scenarios & `package.json` Integration

Neex shines when integrated into your `package.json` scripts.

**Example `package.json` scripts:**

```json
{
  "scripts": {
    "dev:frontend": "cd packages/frontend && npm run dev",
    "dev:backend": "cd packages/api && npm run dev",
    "dev": "neex servers \"npm run dev:frontend\" \"npm run dev:backend\" --group-output",

    "build:ui": "cd packages/ui-library && npm run build",
    "build:app": "cd packages/main-app && npm run build",
    "build": "neex runx \"npm run build:ui\" \"npm run build:app\"",

    "test": "neex runx -s \"npm run test:unit\" \"npm run test:e2e\"",
    "test:unit": "jest",
    "test:e2e": "playwright test",

    "lint": "eslint .",
    "format": "prettier --write .",
    "check-all": "neex p \"npm run lint\" \"npm run format -- --check\" \"npm run test\"",

    "start:prod": "neex pm2 start dist/server.js --name my-prod-app",
    "watch:build": "neex watch \"npm run build:app\" -w packages/main-app/src -e ts,tsx"
  }
}
```

**Running these scripts:**

```bash
# Start all development servers with grouped output
npm run dev

# Build UI library and main application in parallel
npm run build

# Run linters, format check, and all tests in parallel
npm run check-all

# Start the production application using neex's pm2
npm run start:prod

# Watch for changes in the main app's src and rebuild it
npm run watch:build
```

## 🤝 Contributing

We welcome contributions! Check our [issues page](https://github.com/Neexjs).

## 📄 License

MIT