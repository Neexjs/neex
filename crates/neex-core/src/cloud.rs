//! Cloud Storage Adapter - S3/R2 Support
//!
//! Features:
//! - Upload/download artifacts to S3/R2
//! - Config stored in ~/.neex/config.json
//! - Async background upload (non-blocking)
//! - Sync download (blocking for cache hit)

use anyhow::{anyhow, Result};
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Cloud configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CloudConfig {
    pub s3: Option<S3Config>,
}

/// S3/R2 configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool { true }

impl Default for S3Config {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            bucket: String::new(),
            region: "auto".to_string(),
            access_key: String::new(),
            secret_key: String::new(),
            enabled: true,
        }
    }
}

/// Get config file path (~/.neex/config.json)
pub fn get_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".neex")
        .join("config.json")
}

/// Load cloud config from disk
pub fn load_config() -> Result<CloudConfig> {
    let path = get_config_path();
    if !path.exists() {
        return Ok(CloudConfig::default());
    }
    
    let content = std::fs::read_to_string(&path)?;
    let config: CloudConfig = serde_json::from_str(&content)?;
    Ok(config)
}

/// Save cloud config to disk
pub fn save_config(config: &CloudConfig) -> Result<()> {
    let path = get_config_path();
    
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, content)?;
    
    tracing::info!("Config saved to {:?}", path);
    Ok(())
}

/// S3/R2 Cloud Cache Client
pub struct CloudCache {
    bucket: Bucket,
    credentials: Credentials,
    client: reqwest::Client,
    enabled: bool,
}

impl CloudCache {
    /// Create new cloud cache from config
    pub fn from_config(config: &S3Config) -> Result<Self> {
        let endpoint = config.endpoint.parse()
            .map_err(|_| anyhow!("Invalid endpoint URL"))?;
        
        let bucket = Bucket::new(
            endpoint,
            UrlStyle::Path,
            config.bucket.clone(),
            config.region.clone(),
        )?;

        let credentials = Credentials::new(
            config.access_key.clone(),
            config.secret_key.clone(),
        );

        let client = reqwest::Client::new();

        Ok(Self {
            bucket,
            credentials,
            client,
            enabled: config.enabled,
        })
    }

    /// Create from loaded config
    pub fn try_new() -> Result<Option<Self>> {
        let config = load_config()?;
        
        match config.s3 {
            Some(s3_config) if s3_config.enabled && !s3_config.endpoint.is_empty() => {
                let cache = Self::from_config(&s3_config)?;
                tracing::info!("☁️ Cloud cache: {}", s3_config.bucket);
                Ok(Some(cache))
            }
            _ => Ok(None),
        }
    }

    /// Check if cloud is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Upload artifact
    pub async fn upload(&self, hash: &str, data: Vec<u8>) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let key = format!("artifacts/{}", hash);
        let url = self.bucket.put_object(Some(&self.credentials), &key)
            .sign(Duration::from_secs(300));

        self.client
            .put(url)
            .body(data)
            .send()
            .await
            .map_err(|e| anyhow!("Upload failed: {}", e))?;

        tracing::info!("☁️ Uploaded: {}", hash);
        Ok(())
    }

    /// Upload in background (fire and forget)
    pub fn upload_background(hash: String, data: Vec<u8>) {
        tokio::spawn(async move {
            match CloudCache::try_new() {
                Ok(Some(cloud)) => {
                    if let Err(e) = cloud.upload(&hash, data).await {
                        tracing::error!("Background upload failed: {}", e);
                    }
                }
                Ok(None) => {}
                Err(e) => tracing::error!("Cloud init failed: {}", e),
            }
        });
    }

    /// Download artifact
    pub async fn download(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        if !self.enabled {
            return Ok(None);
        }

        let key = format!("artifacts/{}", hash);
        let url = self.bucket.get_object(Some(&self.credentials), &key)
            .sign(Duration::from_secs(300));

        let resp = self.client.get(url).send().await?;
        
        if resp.status().is_success() {
            let data = resp.bytes().await?.to_vec();
            tracing::info!("☁️ Downloaded: {}", hash);
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }

    /// Check connection
    pub async fn ping(&self) -> Result<bool> {
        let mut action = self.bucket.list_objects_v2(Some(&self.credentials));
        action.with_max_keys(1);
        let url = action.sign(Duration::from_secs(30));

        let resp = self.client.get(url).send().await?;
        Ok(resp.status().is_success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_path() {
        let path = get_config_path();
        assert!(path.to_string_lossy().contains(".neex"));
        assert!(path.to_string_lossy().ends_with("config.json"));
    }

    #[test]
    fn test_default_config() {
        let config = CloudConfig::default();
        assert!(config.s3.is_none());
    }

    #[test]
    fn test_config_serialization() {
        let config = CloudConfig {
            s3: Some(S3Config {
                endpoint: "https://s3.example.com".into(),
                bucket: "neex-cache".into(),
                region: "us-east-1".into(),
                access_key: "test-key".into(),
                secret_key: "test-secret".into(),
                enabled: true,
            }),
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("neex-cache"));
        
        let parsed: CloudConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.s3.unwrap().bucket, "neex-cache");
    }
}
