# Contributing to Neexp

Thank you for your interest in contributing to Neexp! 🚀

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (1.75+)
- [Node.js](https://nodejs.org/) (18+)

### Development Setup

```bash
# Clone the repository
git clone https://github.com/Neexjs/neex.git
cd neexp

# Build
cd crates
cargo build

# Run tests
cargo test

# Run the CLI
cargo run -p neexp-cli -- --help
```

## Development Workflow

### Code Style

- **Rust**: Follow standard Rust conventions. Run `cargo fmt` before committing.
- **Commits**: Use conventional commits (`feat:`, `fix:`, `chore:`, `docs:`).

### Testing

```bash
# Run all tests
cargo test

# Run specific test
cargo test --package neexp-core -- ast_hasher

# Run with output
cargo test -- --nocapture
```

### Pull Request Process

1. Fork the repository
2. Create a feature branch (`git checkout -b feat/amazing-feature`)
3. Make your changes
4. Run tests (`cargo test`)
5. Commit with conventional commit message
6. Push and open a Pull Request

## Architecture

```
crates/
├── neexp-core/      # Core: Hasher, TaskRunner, DepGraph, SymbolGraph
├── neexp-daemon/    # Background: Watcher, P2P, State
├── neexp-cli/       # CLI: Commands, TUI
└── neexp-napi/      # Node.js bindings (future)
```

## 🚀 Release Process (Maintainers)

### 1. Update Version Files

```bash
# Edit these files:
# - crates/Cargo.toml → version = "X.Y.Z"
# - npm/neexp/package.json → version + optionalDependencies
```

### 2. Create Version PR

```bash
git checkout -b chore/version-X.Y.Z
git add -A
git commit -m "chore: bump version to X.Y.Z"
git push -u origin chore/version-X.Y.Z
# Create PR and merge
```

### 3. Tag and Release

```bash
git checkout main
git pull origin main
git tag vX.Y.Z
git push origin vX.Y.Z
```

**⚡ Workflow auto:**
- Build (macOS, Linux, Windows)
- Publish to NPM
- Create GitHub Release

---

## Questions?

Open an issue or start a discussion on GitHub.

---

Thank you for contributing! ❤️
