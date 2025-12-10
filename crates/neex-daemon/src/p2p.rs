//! P2P LAN Cache Sharing - The Killer Feature
//!
//! Share cache artifacts with teammates on the same network.
//! Zero config, automatic discovery via mDNS.
//!
//! Architecture:
//! - Discovery: mDNS broadcasts "_neex._tcp" service
//! - Server: HTTP artifact server on random port
//! - Client: Fetches from discovered peers

use anyhow::Result;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

const SERVICE_TYPE: &str = "_neex._tcp.local.";
const SERVICE_NAME_PREFIX: &str = "neex-daemon";

/// Peer info discovered via mDNS
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub id: String,
    pub addr: SocketAddr,
    pub hostname: String,
}

/// Manages discovered peers
pub struct PeerManager {
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
    local_id: String,
    mdns: Option<ServiceDaemon>,
}

impl PeerManager {
    pub fn new() -> Self {
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
            local_id: Uuid::new_v4().to_string()[..8].to_string(),
            mdns: None,
        }
    }

    /// Get local peer ID
    pub fn local_id(&self) -> &str {
        &self.local_id
    }

    /// Start mDNS discovery and advertisement
    pub async fn start(&mut self, local_port: u16) -> Result<()> {
        let mdns = ServiceDaemon::new()?;

        // Advertise our service
        let hostname = gethostname::gethostname().to_string_lossy().to_string();

        let service_name = format!("{}-{}", SERVICE_NAME_PREFIX, &self.local_id);
        let service =
            ServiceInfo::new(SERVICE_TYPE, &service_name, &hostname, "", local_port, None)?;

        mdns.register(service)?;
        tracing::info!(
            "📡 mDNS: Advertising as {} on port {}",
            service_name,
            local_port
        );

        // Browse for peers
        let receiver = mdns.browse(SERVICE_TYPE)?;
        let peers = Arc::clone(&self.peers);
        let local_id = self.local_id.clone();

        tokio::spawn(async move {
            while let Ok(event) = receiver.recv() {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        // Skip self
                        if info.get_fullname().contains(&local_id) {
                            continue;
                        }

                        for addr in info.get_addresses() {
                            let peer = PeerInfo {
                                id: info.get_fullname().to_string(),
                                addr: SocketAddr::new(*addr, info.get_port()),
                                hostname: info.get_hostname().to_string(),
                            };

                            tracing::info!("🔗 Peer found: {} at {}", peer.hostname, peer.addr);
                            peers.write().await.insert(peer.id.clone(), peer);
                        }
                    }
                    ServiceEvent::ServiceRemoved(_, name) => {
                        peers.write().await.remove(&name);
                        tracing::info!("🔌 Peer left: {}", name);
                    }
                    _ => {}
                }
            }
        });

        self.mdns = Some(mdns);
        Ok(())
    }

    /// Get list of active peers
    pub async fn get_peers(&self) -> Vec<PeerInfo> {
        self.peers.read().await.values().cloned().collect()
    }

    /// Fetch artifact from a peer
    pub async fn fetch_from_peer(&self, peer: &PeerInfo, hash: &str) -> Result<Vec<u8>> {
        let url = format!("http://{}/artifact/{}", peer.addr, hash);
        let resp = reqwest::get(&url).await?;

        if resp.status().is_success() {
            Ok(resp.bytes().await?.to_vec())
        } else {
            Err(anyhow::anyhow!("Peer {} doesn't have artifact", peer.addr))
        }
    }

    /// Try to fetch artifact from any peer
    pub async fn fetch_from_network(&self, hash: &str) -> Option<Vec<u8>> {
        let peers = self.get_peers().await;

        for peer in peers {
            match self.fetch_from_peer(&peer, hash).await {
                Ok(data) => {
                    tracing::info!("📥 Got artifact from peer: {}", peer.hostname);
                    return Some(data);
                }
                Err(_) => continue,
            }
        }

        None
    }
}

impl Default for PeerManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared state for artifact server
pub struct ArtifactServerState {
    pub cache_db: sled::Db,
}

/// Start HTTP artifact server
pub async fn start_artifact_server(
    cache_db: sled::Db,
) -> Result<(u16, tokio::task::JoinHandle<()>)> {
    let state = Arc::new(ArtifactServerState { cache_db });

    let app = Router::new()
        .route("/artifact/:hash", get(get_artifact))
        .route("/health", get(health_check))
        .with_state(state);

    // Bind to random port
    let listener = tokio::net::TcpListener::bind("0.0.0.0:0").await?;
    let port = listener.local_addr()?.port();

    tracing::info!("🌐 Artifact server listening on port {}", port);

    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("Artifact server error: {}", e);
        }
    });

    Ok((port, handle))
}

/// GET /artifact/:hash - Return cached artifact
async fn get_artifact(
    State(state): State<Arc<ArtifactServerState>>,
    Path(hash): Path<String>,
) -> impl IntoResponse {
    match state.cache_db.get(hash.as_bytes()) {
        Ok(Some(data)) => (StatusCode::OK, data.to_vec()),
        Ok(None) => (StatusCode::NOT_FOUND, vec![]),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, vec![]),
    }
}

/// GET /health - Simple health check
async fn health_check() -> &'static str {
    "OK"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_artifact_server() {
        // Create temp DB
        let db = sled::Config::new().temporary(true).open().unwrap();

        // Store test artifact
        db.insert("test-hash", b"Hello P2P!").unwrap();

        // Start server
        let (port, _handle) = start_artifact_server(db).await.unwrap();

        // Fetch artifact
        let url = format!("http://127.0.0.1:{}/artifact/test-hash", port);
        let resp = reqwest::get(&url).await.unwrap();

        assert!(resp.status().is_success());
        assert_eq!(resp.text().await.unwrap(), "Hello P2P!");
    }

    #[tokio::test]
    async fn test_peer_manager_creation() {
        let pm = PeerManager::new();
        assert!(!pm.local_id().is_empty());

        let peers = pm.get_peers().await;
        assert!(peers.is_empty());
    }
}
