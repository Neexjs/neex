//! Neex Daemon - Background File Watcher & P2P Cache
//!
//! The killer feature that makes neex faster than Turbo/Nx
//!
//! Features:
//! - Always-running background process
//! - File watching with notify (FSEvents on macOS)
//! - RAM-cached file hashes
//! - Unix socket for CLI communication
//! - P2P LAN cache sharing via mDNS
//! - Zero startup time for builds

pub mod p2p;
pub mod server;
pub mod state;
pub mod watcher;

pub use p2p::{start_artifact_server, PeerManager};
pub use server::{DaemonRequest, DaemonResponse, DaemonServer};
pub use state::DaemonState;
pub use watcher::FileWatcher;
