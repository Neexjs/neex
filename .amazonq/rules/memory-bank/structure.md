# Neex Monorepo - Project Structure

## Directory Organization

### Root Level
```
neex-turbo/
├── apps/           # Applications
├── packages/       # Reusable packages
├── test/          # Test application
├── .husky/        # Git hooks
├── .turbo/        # Turborepo cache
└── .amazonq/      # AI assistant rules
```

### Applications (`apps/`)
- **`apps/docs/`** - Documentation website built with Next.js and Fumadocs
  - Content management with MDX
  - Search functionality
  - Component documentation

### Packages (`packages/`)
- **`packages/core/`** - Main Neex framework (`neex` npm package)
  - CLI commands and utilities
  - Express + Next.js integration
  - Development server functionality

- **`packages/cli/`** - Project generator (`create-neex` npm package)
  - Interactive project creation
  - Template generation
  - Package manager detection

- **`packages/eslint-config/`** - Shared ESLint configurations
  - Base configuration
  - Next.js specific rules
  - React internal rules

- **`packages/typescript-config/`** - Shared TypeScript configurations
  - Base TypeScript config
  - Next.js specific config
  - React library config

### Test Environment (`test/`)
- **Development Testing**: Next.js application for framework testing
- **Integration Testing**: Real-world usage scenarios
- **Feature Validation**: New feature development and testing

## Architectural Patterns

### Monorepo Architecture
- **Turborepo**: Build system orchestration and caching
- **Workspace Management**: Bun workspaces for dependency management
- **Shared Configurations**: Centralized linting, formatting, and TypeScript configs

### Package Relationships
```
create-neex (CLI) → generates projects using → neex (Core)
                                          ↓
                                    Express + Next.js
```

### Development Workflow
1. **Core Development**: Framework features in `packages/core/`
2. **CLI Updates**: Project generation in `packages/cli/`
3. **Documentation**: Updates in `apps/docs/`
4. **Testing**: Validation in `test/` application

## Core Components

### Framework Integration
- **Backend**: Express.js server with TypeScript
- **Frontend**: Next.js application with React
- **Database**: Prisma ORM integration
- **Development**: Hot reload and parallel execution

### Build System
- **Turborepo**: Task orchestration and caching
- **TypeScript**: Compilation and type checking
- **ESLint**: Code quality and consistency
- **Prettier**: Code formatting