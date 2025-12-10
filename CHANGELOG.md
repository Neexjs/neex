# Changelog

## 1.0.0 (2025-12-10)


### Features

* Add `watch` command with smart, affected-only rebuilds for monorepos using a new `SmartWatcher` utility. ([61c6e82](https://github.com/Neexjs/neex/commit/61c6e82e456ba317fc42df34fab331d53b81f67a))
* add Affected Detection - only rebuild changed packages ([a48cf12](https://github.com/Neexjs/neex/commit/a48cf12a43e54ef663aa0a267ff10a6a097b0fdc))
* add build, dev, and start managers ([76f6431](https://github.com/Neexjs/neex/commit/76f643111548bf69a0eb585ea91d0ebec144f779))
* Add docs layout, core managers, native Zig graph, CLI templates, and update build configurations. ([9ecb07f](https://github.com/Neexjs/neex/commit/9ecb07fe79755bd928e355dbf9d49f7ee42d0e5a))
* add professional CI/CD pipeline ([c500fd7](https://github.com/Neexjs/neex/commit/c500fd70b3104046596dca8d30c3b634f65b2418))
* add Rust crates (neex-core, neex-daemon) and clean up project ([9460e0a](https://github.com/Neexjs/neex/commit/9460e0a6d6e53c00674d2f8982f58d8f882c0240))
* add Zero-Config detection - auto-detect config from package.json ([1ed1956](https://github.com/Neexjs/neex/commit/1ed19569b4167a990dd2304565d609485afc0173))
* Enterprise-ready project structure ([e776b30](https://github.com/Neexjs/neex/commit/e776b30bf1086d6a1202c6ceb3821b6272bf3e35))
* Enterprise-ready project structure ([dcc1d80](https://github.com/Neexjs/neex/commit/dcc1d8077ecd170034aafbd5b35b28c4242e8091))
* implement incremental project graph with SQLite persistence for monorepo management. ([821db59](https://github.com/Neexjs/neex/commit/821db5939560fee40d1d036be3704bcbb0af7231))
* introduce `neexa` for `dev`, `build`, and `start` commands while `neex` becomes a monorepo orchestrator with new `cache` ([1b2fe52](https://github.com/Neexjs/neex/commit/1b2fe52706ae87207564d4ab11f08a1d4f354ec3))
* Introduce a streaming task graph for dependency-aware, parallel task execution, replacing previous execution logic in `monorepo.ts`. ([6560ed2](https://github.com/Neexjs/neex/commit/6560ed2a176483d37b9b9ae5906538300f8fef6e))
* Introduce Bun-optimized zero-copy file I/O and integrate it into file hashing operations. ([ce9dac8](https://github.com/Neexjs/neex/commit/ce9dac8199afa0acc7f4da790779e0be375825dc))
* introduce content-addressable storage with compression utilities and integrate them into the cache manager. ([83c87d1](https://github.com/Neexjs/neex/commit/83c87d17f002e15eef0ec7f7a1722738228a10e0))
* Introduce interactive terminal UI and brand task graph console output ([232d673](https://github.com/Neexjs/neex/commit/232d673be0f5d6abfff78ce839a6b8a8a08df3ea))
* introduce interactive terminal UI for task execution and a simple logger. ([dcd1551](https://github.com/Neexjs/neex/commit/dcd1551b84fe1e0cd860b9e3458037c89e9e44d8))
* Phase 10 - NPM Distribution v0.8.0 ([ca3d517](https://github.com/Neexjs/neex/commit/ca3d51738b95483f7486f8f08d357d2671e67ee0))
* Phase 3 - AST Hashing with tree-sitter (killer feature) ([f7cb8e3](https://github.com/Neexjs/neex/commit/f7cb8e36f8680ef7552fb72a5ec28c65677b2741))
* Phase 3.5 - neex CLI with daemon integration ([592399e](https://github.com/Neexjs/neex/commit/592399e2ec29fc999ed2e1485c7292954aec89bb))
* Phase 4 - Task Runner with Persistent Caching (Turbo Feature!) ([4b7bc78](https://github.com/Neexjs/neex/commit/4b7bc78f12253dc6ea6df6d207f63891ffe4faad))
* Phase 5 - Dependency Graph for Monorepo Support ([4593c4a](https://github.com/Neexjs/neex/commit/4593c4aed8ad2e0401431fdad86ad169980803d3))
* Phase 6 - Parallel Scheduler with dependency-aware execution ([10ab0d7](https://github.com/Neexjs/neex/commit/10ab0d7ed2822ab39dfa6373f98c38ae750c588a))
* Phase 7.1 - P2P LAN cache sharing ([7519d76](https://github.com/Neexjs/neex/commit/7519d7665567a6d9d05df097b475539f7880f3f9))
* Phase 7.2 - Cloud S3/R2 adapter ([7c0a619](https://github.com/Neexjs/neex/commit/7c0a619ca80f5dfe4b30f94f8c63bce58ea04e73))
* Phase 7.3 - CLI & Tiered Caching Orchestrator ([b99f9d9](https://github.com/Neexjs/neex/commit/b99f9d9508314b6b66ec139a9b3d65a89fdfe8fb))
* Phase 8.1 - Symbol Extractor ([c9d12e4](https://github.com/Neexjs/neex/commit/c9d12e4dc332b1e0259df5d19875e7de254c9151))
* Phase 8.2 - Symbol Graph ([decf084](https://github.com/Neexjs/neex/commit/decf0848af7dce3b00a9bd69fc94d0036e00bd9f))
* Phase 8.3 - Smart Rebuild ([3a3db85](https://github.com/Neexjs/neex/commit/3a3db8530a4be2c8a4b7e79411fbead70f847d26))
* Phase 9 - Beautiful TUI (Command Center) ([a223869](https://github.com/Neexjs/neex/commit/a223869289e07bcfa8549beafea79ac81a52bb94))


### Bug Fixes

* **cli:** improve dev command error handling and logging ([5d9196f](https://github.com/Neexjs/neex/commit/5d9196f3bbde85825608d469a744ed2b23a39c34))
* **core:** refactor build, dev, and start commands for v0.7.0 release ([adf39a7](https://github.com/Neexjs/neex/commit/adf39a764df5bae55432c74dc9299e78b8cef9f7))
* simplify workflows configuration ([3bdce8a](https://github.com/Neexjs/neex/commit/3bdce8a7167dbda6fbfab810fe57b7df2ae39a2c))
* update build commands and remove deprecated start manager ([a6fd8cc](https://github.com/Neexjs/neex/commit/a6fd8ccf47e42af8629241b63b445a663af160e6))
