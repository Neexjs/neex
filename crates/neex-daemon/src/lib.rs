//! Optional background file watcher. Its sled database holds file hash
//! hints, not task artifacts. Task caching uses neex-core's ArtifactStore
//! and always remains correct without the daemon.
//!
//! LAN sharing is an isolated prototype behind `experimental-p2p`.

#[cfg(feature = "experimental-p2p")]
pub mod p2p;
pub mod server;
pub mod state;
pub mod watcher;

#[cfg(feature = "experimental-p2p")]
pub use p2p::{start_artifact_server, PeerManager};
pub use server::{DaemonRequest, DaemonResponse, DaemonServer};
pub use state::DaemonState;
pub use watcher::FileWatcher;
