//! Remote cache - S3-compatible storage (AWS S3, Cloudflare R2, MinIO)
//!
//! Layout in the bucket mirrors the local store:
//!   v1/tasks/<key>.json   task record
//!   v1/blobs/<hash>       output file contents (content-addressed)
//!
//! Blobs are verified against their hash on download, so a tampered or
//! truncated blob is rejected. Records are uploaded last, so a reader never
//! sees a record whose blobs are missing.
//!
//! Trust: by default only trusted CI runs write to the remote cache (see
//! [`WritePolicy`]); local machines and pull-request builds only read. This
//! closes the "poisoned first write" class of attack on shared caches.

use crate::artifacts::{ArtifactStore, TaskRecord};
use crate::cloud::{load_config, S3Config};
use anyhow::{anyhow, Result};
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritePolicy {
    /// Write only from CI runs that are not pull requests (default)
    TrustedCi,
    Always,
    Never,
}

impl WritePolicy {
    /// `NEEX_REMOTE_CACHE_WRITE=always|never|ci`
    pub fn from_env() -> Self {
        match std::env::var("NEEX_REMOTE_CACHE_WRITE").as_deref() {
            Ok("always") | Ok("1") | Ok("true") => WritePolicy::Always,
            Ok("never") | Ok("0") | Ok("false") => WritePolicy::Never,
            _ => WritePolicy::TrustedCi,
        }
    }

    pub fn allows_write(&self) -> bool {
        match self {
            WritePolicy::Always => true,
            WritePolicy::Never => false,
            WritePolicy::TrustedCi => {
                let ci = std::env::var("CI")
                    .map(|v| v != "false" && !v.is_empty())
                    .unwrap_or(false);
                let event = std::env::var("GITHUB_EVENT_NAME").unwrap_or_default();
                let is_pr = event == "pull_request"
                    || event == "pull_request_target"
                    || std::env::var("CI_MERGE_REQUEST_ID").is_ok();
                ci && !is_pr
            }
        }
    }
}

pub struct RemoteCache {
    bucket: Bucket,
    credentials: Credentials,
    client: reqwest::Client,
    policy: WritePolicy,
}

impl RemoteCache {
    pub fn from_s3(config: &S3Config, policy: WritePolicy) -> Result<Self> {
        let endpoint = config
            .endpoint
            .parse()
            .map_err(|_| anyhow!("invalid remote cache endpoint `{}`", config.endpoint))?;
        let bucket = Bucket::new(
            endpoint,
            UrlStyle::Path,
            config.bucket.clone(),
            config.region.clone(),
        )?;
        let credentials = Credentials::new(config.access_key.clone(), config.secret_key.clone());
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self {
            bucket,
            credentials,
            client,
            policy,
        })
    }

    /// Remote cache from `~/.neex/config.json`, if configured and enabled
    pub fn from_user_config() -> Result<Option<Self>> {
        let cfg = load_config()?;
        match cfg.s3 {
            Some(s3) if s3.enabled && !s3.endpoint.is_empty() => {
                Ok(Some(Self::from_s3(&s3, WritePolicy::from_env())?))
            }
            _ => Ok(None),
        }
    }

    pub fn can_write(&self) -> bool {
        self.policy.allows_write()
    }

    fn get_url(&self, key: &str) -> reqwest::Url {
        self.bucket
            .get_object(Some(&self.credentials), key)
            .sign(Duration::from_secs(300))
    }

    fn put_url(&self, key: &str) -> reqwest::Url {
        self.bucket
            .put_object(Some(&self.credentials), key)
            .sign(Duration::from_secs(300))
    }

    fn head_url(&self, key: &str) -> reqwest::Url {
        self.bucket
            .head_object(Some(&self.credentials), key)
            .sign(Duration::from_secs(300))
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let resp = self.client.get(self.get_url(key)).send().await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND
            || resp.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Ok(None);
        }
        let resp = resp.error_for_status()?;
        Ok(Some(resp.bytes().await?.to_vec()))
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let resp = self.client.head(self.head_url(key)).send().await?;
        Ok(resp.status().is_success())
    }

    async fn put(&self, key: &str, body: Vec<u8>) -> Result<()> {
        self.client
            .put(self.put_url(key))
            .body(body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Fetch a record and its blobs into the local store. Returns false on a
    /// remote miss.
    pub async fn download(&self, store: &ArtifactStore, key: &str) -> Result<bool> {
        let Some(bytes) = self.get(&format!("v1/tasks/{}.json", key)).await? else {
            return Ok(false);
        };
        let record: TaskRecord = serde_json::from_slice(&bytes)?;
        if record.key != key {
            return Err(anyhow!(
                "remote record for {} carries key {}",
                key,
                record.key
            ));
        }
        record.validate_inputs()?;
        for f in &record.files {
            if store.has_blob(&f.hash) {
                continue;
            }
            let Some(blob) = self.get(&format!("v1/blobs/{}", f.hash)).await? else {
                return Ok(false);
            };
            store.put_blob(&f.hash, &blob)?;
        }
        store.put_record(&record)?;
        Ok(true)
    }

    /// Upload blobs first, then the record
    pub async fn upload(&self, store: &ArtifactStore, record: &TaskRecord) -> Result<()> {
        if !self.can_write() {
            return Err(anyhow!("remote cache write policy prohibits upload"));
        }
        record.validate_inputs()?;
        for f in &record.files {
            let key = format!("v1/blobs/{}", f.hash);
            if self.exists(&key).await.unwrap_or(false) {
                continue;
            }
            self.put(&key, store.read_blob(&f.hash)?).await?;
        }
        self.put(
            &format!("v1/tasks/{}.json", record.key),
            serde_json::to_vec(record)?,
        )
        .await
    }

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

    type Objects = std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>>;

    async fn object_get(
        axum::extract::State(objects): axum::extract::State<Objects>,
        axum::extract::Path(key): axum::extract::Path<String>,
    ) -> (axum::http::StatusCode, Vec<u8>) {
        match objects.lock().await.get(&key) {
            Some(bytes) => (axum::http::StatusCode::OK, bytes.clone()),
            None => (axum::http::StatusCode::NOT_FOUND, vec![]),
        }
    }

    async fn object_put(
        axum::extract::State(objects): axum::extract::State<Objects>,
        axum::extract::Path(key): axum::extract::Path<String>,
        axum::extract::RawQuery(query): axum::extract::RawQuery,
        body: axum::body::Bytes,
    ) -> axum::http::StatusCode {
        assert!(
            query.unwrap().contains("X-Amz-Signature="),
            "S3 requests must be signed"
        );
        let mut objects = objects.lock().await;
        if key.starts_with("v1/tasks/") {
            let record: TaskRecord = serde_json::from_slice(&body).unwrap();
            for file in record.files {
                assert!(objects.contains_key(&format!("v1/blobs/{}", file.hash)));
            }
        }
        objects.insert(key, body.to_vec());
        axum::http::StatusCode::OK
    }

    async fn server() -> (RemoteCache, Objects, tokio::task::JoinHandle<()>) {
        let objects = Objects::default();
        let app = axum::Router::new()
            .route(
                "/bucket/*key",
                axum::routing::get(object_get).put(object_put),
            )
            .with_state(objects.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let config = S3Config {
            endpoint: format!("http://{}", listener.local_addr().unwrap()),
            bucket: "bucket".into(),
            region: "us-east-1".into(),
            access_key: "test".into(),
            secret_key: "test-secret".into(),
            enabled: true,
        };
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (
            RemoteCache::from_s3(&config, WritePolicy::Always).unwrap(),
            objects,
            handle,
        )
    }

    fn record(store: &ArtifactStore, root: &std::path::Path) -> TaskRecord {
        std::fs::create_dir_all(root.join("dist")).unwrap();
        std::fs::write(root.join("dist/app.js"), "fresh output").unwrap();
        let inputs: crate::task_hash::HashInputs = serde_json::from_value(serde_json::json!({
            "key_version":"test", "neex_version":"test", "platform":"test", "project":"app", "task":"build", "command":"build",
            "config":{"outputs":["dist/**"]}, "files":{}, "dep_projects":{}, "dep_tasks":{}, "env":{}, "global_files":{}
        })).unwrap();
        let key = inputs.key();
        store
            .store(
                &key,
                "app#build",
                root,
                &["dist/**".into()],
                vec![],
                1,
                inputs,
            )
            .unwrap()
    }

    #[tokio::test]
    async fn s3_round_trip_verifies_artifacts_and_uploads_record_last() {
        let (remote, objects, server) = server().await;
        let source = tempfile::tempdir().unwrap();
        let store = ArtifactStore::open(source.path()).unwrap();
        let record = record(&store, source.path());
        remote.upload(&store, &record).await.unwrap();
        let destination = tempfile::tempdir().unwrap();
        let downloaded = ArtifactStore::open(destination.path()).unwrap();
        assert!(remote.download(&downloaded, &record.key).await.unwrap());
        let result = downloaded.get(&record.key).unwrap().unwrap();
        downloaded.restore(&result, destination.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(destination.path().join("dist/app.js")).unwrap(),
            "fresh output"
        );
        assert!(!remote.download(&downloaded, "missing").await.unwrap());
        objects
            .lock()
            .await
            .remove(&format!("v1/blobs/{}", record.files[0].hash));
        let empty = tempfile::tempdir().unwrap();
        assert!(!remote
            .download(&ArtifactStore::open(empty.path()).unwrap(), &record.key)
            .await
            .unwrap());
        server.abort();
    }

    #[tokio::test]
    async fn s3_rejects_corrupt_downloads_and_read_only_uploads() {
        let (mut remote, objects, server) = server().await;
        let source = tempfile::tempdir().unwrap();
        let store = ArtifactStore::open(source.path()).unwrap();
        let record = record(&store, source.path());
        remote.policy = WritePolicy::Never;
        assert!(remote.upload(&store, &record).await.is_err());
        assert!(objects.lock().await.is_empty());
        remote.policy = WritePolicy::Always;
        remote.upload(&store, &record).await.unwrap();
        objects.lock().await.insert(
            format!("v1/blobs/{}", record.files[0].hash),
            b"tampered".to_vec(),
        );
        let destination = tempfile::tempdir().unwrap();
        let downloaded = ArtifactStore::open(destination.path()).unwrap();
        assert!(remote.download(&downloaded, &record.key).await.is_err());
        assert!(downloaded.get(&record.key).unwrap().is_none());
        server.abort();
    }

    #[tokio::test]
    async fn s3_record_cannot_widen_output_ownership() {
        let (remote, objects, server) = server().await;
        let source = tempfile::tempdir().unwrap();
        let store = ArtifactStore::open(source.path()).unwrap();
        let mut record = record(&store, source.path());
        remote.upload(&store, &record).await.unwrap();
        record.outputs = vec!["**".into()];
        record.files[0].path = "src/code.ts".into();
        objects.lock().await.insert(
            format!("v1/tasks/{}.json", record.key),
            serde_json::to_vec(&record).unwrap(),
        );
        let destination = tempfile::tempdir().unwrap();
        let downloaded = ArtifactStore::open(destination.path()).unwrap();
        assert!(remote.download(&downloaded, &record.key).await.is_err());
        assert!(downloaded.get(&record.key).unwrap().is_none());
        server.abort();
    }

    #[test]
    fn write_policy_from_env() {
        std::env::set_var("NEEX_REMOTE_CACHE_WRITE", "never");
        assert!(!WritePolicy::from_env().allows_write());
        std::env::set_var("NEEX_REMOTE_CACHE_WRITE", "always");
        assert!(WritePolicy::from_env().allows_write());
        std::env::remove_var("NEEX_REMOTE_CACHE_WRITE");
    }
}
