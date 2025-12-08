# Neex Framework - Technology Stack

## Core Technologies

### Runtime & Package Management
- **Node.js**: >=18 (specified in package.json engines)
- **Bun**: 1.3.1 (primary package manager)
- **TypeScript**: 5.9.2 (consistent across all packages)

### Framework Stack
- **Backend**: Express.js with TypeScript
- **Frontend**: Next.js with React
- **Database**: Prisma ORM
- **Styling**: Tailwind CSS (in generated projects)

### Build & Development Tools
- **Turborepo**: 2.5.8 (monorepo orchestration)
- **ESLint**: Code linting and quality
- **Prettier**: 3.6.2 (code formatting)
- **Husky**: 9.1.6 (git hooks)
- **lint-staged**: 15.2.10 (staged file linting)

## Development Commands

### Monorepo Management
```bash
# Install all dependencies
bun install

# Build all packages and apps
bun run build

# Build only packages
bun run build:packages

# Development mode (all)
bun run dev

# Development mode (packages only)
bun run dev:packages
```

### Code Quality
```bash
# Lint all code
bun run lint

# Fix linting issues
bun run lint:fix

# Format code
bun run format

# Check formatting
bun run format:check

# Type checking
bun run check-types
```

### Publishing
```bash
# Publish core framework
bun run publish:core

# Publish CLI tool
bun run publish:cli
```

## Configuration Files

### Turborepo (`turbo.json`)
- Task dependencies and caching
- Build outputs configuration
- Development server settings

### Package Management
- **`bun.lock`**: Dependency lock file
- **`bunfig.toml`**: Bun configuration
- **`.npmrc`**: NPM registry settings

### Code Quality
- **`.prettierrc`**: Formatting rules
- **`.prettierignore`**: Files to skip formatting
- **`commitlint.config.js`**: Commit message standards
- **`.lintstagedrc.js`**: Pre-commit linting

### Git Hooks (`.husky/`)
- **`pre-commit`**: Runs lint-staged
- **`commit-msg`**: Validates commit messages

## Workspace Configuration
- **Workspaces**: `apps/*` and `packages/*`
- **Private**: Root package is private (not published)
- **Engine Requirements**: Node.js >=18