# Contributing to Neex

Thank you for your interest in contributing to Neex! 🚀

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (1.75+)
- [Node.js](https://nodejs.org/) (18+)

### Development Setup

```bash
# Clone the repository
git clone https://github.com/Neexjs/neex.git
cd neex

# Build
cd crates
cargo build

# Run tests
cargo test

# Run the CLI
cargo run -p neex-cli -- --help
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
cargo test --package neex-core -- ast_hasher

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
├── neex-core/      # Core: Hasher, TaskRunner, DepGraph, SymbolGraph
├── neex-daemon/    # Background: Watcher, P2P, State
├── neex-cli/       # CLI: Commands, TUI
└── neex-napi/      # Node.js bindings (future)
```

## Questions?

Open an issue or start a discussion on GitHub.

---

Thank you for contributing! ❤️
