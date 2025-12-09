//! Neex Daemon - Background File Watcher
//!
//! The killer feature that makes neex faster than Turbo/Nx
//!
//! Features:
//! - Always-running background process
//! - File watching with notify (FSEvents on macOS)
//! - RAM-cached file hashes
//! - Unix socket for CLI communication
//! - Zero startup time for builds

pub mod watcher;
pub mod state;
pub mod server;

pub use watcher::FileWatcher;
pub use state::DaemonState;
pub use server::DaemonServer;
