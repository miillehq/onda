use crate::{
    manifest::{find_model, MODEL_MANIFEST_VERSION},
    AsrError, CancellationToken, ModelSpec,
};
use fs2::FileExt;
use futures_util::StreamExt;
use reqwest::{
    header::{CONTENT_RANGE, RANGE},
    redirect::Policy,
    StatusCode,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};
use tokio::{
    fs::{self, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
};

const DISK_RESERVE_BYTES: u64 = 64 * 1024 * 1024;
const CHECKSUM_BUFFER_BYTES: usize = 128 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(3600);
const WINDOWS_ERROR_LOCK_VIOLATION: i32 = 33;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DownloadStage {
    Downloading,
    Verifying,
    Installed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub model_id: String,
    pub stage: DownloadStage,
    pub completed_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledModel {
    pub model_id: String,
    pub version: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub manifest_version: u32,
}

/// Model assets are immutable, versioned and independent of application upgrades.
#[derive(Clone)]
pub struct ModelManager {
    directory: Arc<PathBuf>,
    client: reqwest::Client,
    /// Files hashed this session, by size and modification time. Re-hashing a 75–190 MB model
    /// before every dictation cost 0.1–1.5 s; any change to the file triggers a full check.
    verified: Arc<Mutex<HashMap<PathBuf, (u64, SystemTime)>>>,
}

impl ModelManager {
    /// Initialize a user-selected model directory. Canonicalization establishes a path boundary.
    pub fn new(directory: PathBuf) -> Result<Self, AsrError> {
        std::fs::create_dir_all(&directory)?;
        let directory = std::fs::canonicalize(directory)?;
        let client = reqwest::Client::builder()
            .https_only(true)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(DOWNLOAD_TIMEOUT)
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() >= 8 {
                    return attempt.error("too many redirects");
                }
                let url = attempt.url();
                let trusted =
                    url.scheme() == "https" && url.host_str().is_some_and(trusted_model_host);
                if trusted {
                    attempt.follow()
                } else {
                    attempt.error("untrusted model redirect")
                }
            }))
            .user_agent("Onda/0.1 model-manager")
            .build()
            .map_err(AsrError::DownloadNetwork)?;
        Ok(Self {
            directory: Arc::new(directory),
            client,
            verified: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn location(&self) -> &Path {
        &self.directory
    }

    /// Enumerate only current catalog revisions with the expected complete size.
    pub fn installed(&self) -> Result<Vec<InstalledModel>, AsrError> {
        let mut installed = Vec::new();
        for model in crate::builtin_models() {
            let path = self.path_for(model, "bin")?;
            match std::fs::metadata(&path) {
                Ok(metadata)
                    if metadata.is_file() && metadata.len() == model.expected_size_bytes =>
                {
                    installed.push(installation(model, path))
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(installed)
    }

    /// Validate local bytes against the trust root before passing a model to native inference.
    pub async fn verified_model_path(
        &self,
        id: &str,
        cancellation: &CancellationToken,
    ) -> Result<PathBuf, AsrError> {
        let model = find_model(id)?;
        let path = self.path_for(model, "bin")?;
        if !path.is_file() {
            return Err(AsrError::ModelMissing);
        }
        let identity = fs::metadata(&path)
            .await
            .ok()
            .and_then(|metadata| Some((metadata.len(), metadata.modified().ok()?)));
        let known = identity.is_some_and(|identity| {
            self.verified
                .lock()
                .is_ok_and(|verified| verified.get(&path) == Some(&identity))
        });
        if !known {
            verify_file(&path, model, cancellation).await?;
            if let (Some(identity), Ok(mut verified)) = (identity, self.verified.lock()) {
                verified.insert(path.clone(), identity);
            }
        }
        Ok(path)
    }

    /// Resume a bounded download, hash it, fsync, then atomically install on the same volume.
    /// Complexity: O(n) network/hash bytes; O(1) buffering irrespective of model size.
    pub async fn download(
        &self,
        id: &str,
        cancellation: &CancellationToken,
        progress: impl Fn(DownloadProgress) + Send + Sync,
    ) -> Result<InstalledModel, AsrError> {
        let model = find_model(id)?;
        self.download_from(model, &model.download_url(), cancellation, progress)
            .await
    }

    async fn download_from(
        &self,
        model: &ModelSpec,
        source_url: &str,
        cancellation: &CancellationToken,
        progress: impl Fn(DownloadProgress) + Send + Sync,
    ) -> Result<InstalledModel, AsrError> {
        if cancellation.is_cancelled() {
            return Err(AsrError::Cancelled);
        }
        let _lock = self.acquire_lock(model)?;
        let destination = self.path_for(model, "bin")?;
        if destination.is_file() {
            match verify_file(&destination, model, cancellation).await {
                Ok(()) => {
                    progress(download_progress(
                        model,
                        DownloadStage::Installed,
                        model.expected_size_bytes,
                    ));
                    return Ok(installation(model, destination));
                }
                Err(AsrError::ChecksumMismatch) => fs::remove_file(&destination).await?,
                Err(error) => return Err(error),
            }
        }
        let partial = self.path_for(model, "part")?;
        let mut completed = match fs::metadata(&partial).await {
            Ok(metadata) if metadata.is_file() => metadata.len(),
            Ok(_) => return Err(AsrError::UnsafePath),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error.into()),
        };
        if completed > model.expected_size_bytes {
            fs::remove_file(&partial).await?;
            completed = 0;
        }
        let available = fs2::available_space(&*self.directory)?;
        let required = model
            .expected_size_bytes
            .saturating_sub(completed)
            .saturating_add(DISK_RESERVE_BYTES);
        if available < required {
            return Err(AsrError::InsufficientDisk {
                required_bytes: required,
                available_bytes: available,
            });
        }
        progress(download_progress(
            model,
            DownloadStage::Downloading,
            completed,
        ));
        if completed < model.expected_size_bytes {
            let mut request = self.client.get(source_url);
            if completed > 0 {
                request = request.header(RANGE, format!("bytes={completed}-"));
            }
            let response = tokio::select! {
                response = request.send() => response.map_err(AsrError::DownloadNetwork)?,
                _ = cancellation.cancelled() => return Err(AsrError::Cancelled),
            };
            let append = if completed > 0 && response.status() == StatusCode::PARTIAL_CONTENT {
                validate_content_range(
                    response
                        .headers()
                        .get(CONTENT_RANGE)
                        .and_then(|value| value.to_str().ok()),
                    completed,
                    model.expected_size_bytes,
                )?;
                true
            } else if response.status() == StatusCode::OK {
                completed = 0;
                false
            } else {
                return Err(AsrError::InvalidDownload);
            };
            if response
                .content_length()
                .is_some_and(|length| length != model.expected_size_bytes - completed)
            {
                return Err(AsrError::InvalidDownload);
            }
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .append(append)
                .truncate(!append)
                .open(&partial)
                .await?;
            let mut body = response.bytes_stream();
            let mut last_progress = Instant::now();
            progress(download_progress(
                model,
                DownloadStage::Downloading,
                completed,
            ));
            loop {
                let chunk = tokio::select! {
                    chunk = body.next() => chunk,
                    _ = cancellation.cancelled() => { file.flush().await?; return Err(AsrError::Cancelled); }
                };
                let Some(chunk) = chunk else {
                    break;
                };
                let chunk = chunk.map_err(AsrError::DownloadNetwork)?;
                if completed + chunk.len() as u64 > model.expected_size_bytes {
                    drop(file);
                    fs::remove_file(&partial).await?;
                    return Err(AsrError::InvalidDownload);
                }
                file.write_all(&chunk).await?;
                completed += chunk.len() as u64;
                if last_progress.elapsed() >= PROGRESS_INTERVAL {
                    progress(download_progress(
                        model,
                        DownloadStage::Downloading,
                        completed,
                    ));
                    last_progress = Instant::now();
                }
            }
            file.flush().await?;
            file.sync_all().await?;
        }
        if completed != model.expected_size_bytes {
            return Err(AsrError::InvalidDownload);
        }
        progress(download_progress(
            model,
            DownloadStage::Verifying,
            completed,
        ));
        match verify_file(&partial, model, cancellation).await {
            Ok(()) => {}
            Err(AsrError::ChecksumMismatch) => {
                fs::remove_file(&partial).await?;
                return Err(AsrError::ChecksumMismatch);
            }
            Err(error) => return Err(error),
        }
        if cancellation.is_cancelled() {
            return Err(AsrError::Cancelled);
        }
        fs::rename(&partial, &destination).await?;
        progress(download_progress(
            model,
            DownloadStage::Installed,
            completed,
        ));
        Ok(installation(model, destination))
    }

    /// Removing the current model does not remove settings or other revision assets.
    pub async fn uninstall(&self, id: &str) -> Result<(), AsrError> {
        let model = find_model(id)?;
        let _lock = self.acquire_lock(model)?;
        for extension in ["bin", "part"] {
            match fs::remove_file(self.path_for(model, extension)?).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    /// Copy current verified installations to another location, preserving the source.
    /// Switch the saved setting only after this returns; cancellation never invalidates the old location.
    /// Complexity: O(n) installed model bytes; O(1) bounded transfer memory.
    pub async fn migrate_to(
        &self,
        directory: PathBuf,
        cancellation: &CancellationToken,
    ) -> Result<Self, AsrError> {
        if cancellation.is_cancelled() {
            return Err(AsrError::Cancelled);
        }
        let destination_manager = Self::new(directory)?;
        if destination_manager.directory == self.directory {
            return Ok(destination_manager);
        }
        let installed = self.installed()?;
        for installation in installed {
            if cancellation.is_cancelled() {
                return Err(AsrError::Cancelled);
            }
            let model = find_model(&installation.model_id)?;
            let _source_lock = self.acquire_lock(model)?;
            let _destination_lock = destination_manager.acquire_lock(model)?;
            verify_file(&installation.path, model, cancellation).await?;
            let destination = destination_manager.path_for(model, "bin")?;
            if destination.is_file() {
                verify_file(&destination, model, cancellation).await?;
                continue;
            }
            let required = model.expected_size_bytes.saturating_add(DISK_RESERVE_BYTES);
            let available = fs2::available_space(&*destination_manager.directory)?;
            if available < required {
                return Err(AsrError::InsufficientDisk {
                    required_bytes: required,
                    available_bytes: available,
                });
            }
            let partial = destination_manager.path_for(model, "part")?;
            let mut source = fs::File::open(&installation.path).await?;
            let mut target = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&partial)
                .await?;
            let mut buffer = vec![0u8; CHECKSUM_BUFFER_BYTES];
            loop {
                let count = tokio::select! {
                    count = source.read(&mut buffer) => count?,
                    _ = cancellation.cancelled() => return Err(AsrError::Cancelled),
                };
                if count == 0 {
                    break;
                }
                target.write_all(&buffer[..count]).await?;
            }
            target.flush().await?;
            target.sync_all().await?;
            drop(target);
            verify_file(&partial, model, cancellation).await?;
            fs::rename(partial, destination).await?;
        }
        Ok(destination_manager)
    }

    fn path_for(&self, model: &ModelSpec, extension: &str) -> Result<PathBuf, AsrError> {
        if !model
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || !model.version.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(AsrError::UnsafePath);
        }
        let path = self
            .directory
            .join(format!("{}-{}.{}", model.id, model.version, extension));
        if path.parent() != Some(&*self.directory) {
            return Err(AsrError::UnsafePath);
        }
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(AsrError::UnsafePath);
            }
        }
        Ok(path)
    }

    fn acquire_lock(&self, model: &ModelSpec) -> Result<ModelLock, AsrError> {
        let path = self.path_for(model, "lock")?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.try_lock_exclusive().map_err(|error| {
            if error.kind() == std::io::ErrorKind::WouldBlock
                || error.raw_os_error() == Some(WINDOWS_ERROR_LOCK_VIOLATION)
            {
                AsrError::DownloadBusy
            } else {
                error.into()
            }
        })?;
        Ok(ModelLock(file))
    }
}

struct ModelLock(File);
impl Drop for ModelLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

fn installation(model: &ModelSpec, path: PathBuf) -> InstalledModel {
    InstalledModel {
        model_id: model.id.into(),
        version: model.version.into(),
        path,
        size_bytes: model.expected_size_bytes,
        manifest_version: MODEL_MANIFEST_VERSION,
    }
}
fn download_progress(
    model: &ModelSpec,
    stage: DownloadStage,
    completed_bytes: u64,
) -> DownloadProgress {
    DownloadProgress {
        model_id: model.id.into(),
        stage,
        completed_bytes,
        total_bytes: model.expected_size_bytes,
    }
}
fn trusted_model_host(host: &str) -> bool {
    host == "huggingface.co"
        || host.ends_with(".huggingface.co")
        || host == "hf.co"
        || host.ends_with(".hf.co")
}

fn validate_content_range(
    header: Option<&str>,
    expected_start: u64,
    expected_total: u64,
) -> Result<(), AsrError> {
    let range = header
        .and_then(|value| value.strip_prefix("bytes "))
        .ok_or(AsrError::InvalidDownload)?;
    let (bounds, total) = range.split_once('/').ok_or(AsrError::InvalidDownload)?;
    let (start, end) = bounds.split_once('-').ok_or(AsrError::InvalidDownload)?;
    let start = start
        .parse::<u64>()
        .map_err(|_| AsrError::InvalidDownload)?;
    let end = end.parse::<u64>().map_err(|_| AsrError::InvalidDownload)?;
    let total = total
        .parse::<u64>()
        .map_err(|_| AsrError::InvalidDownload)?;
    if start != expected_start || total != expected_total || end != expected_total - 1 {
        return Err(AsrError::InvalidDownload);
    }
    Ok(())
}

async fn verify_file(
    path: &Path,
    model: &ModelSpec,
    cancellation: &CancellationToken,
) -> Result<(), AsrError> {
    let mut file = fs::File::open(path).await?;
    if file.metadata().await?.len() != model.expected_size_bytes {
        return Err(AsrError::ChecksumMismatch);
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHECKSUM_BUFFER_BYTES];
    loop {
        let count = tokio::select! {
            count = file.read(&mut buffer) => count?,
            _ = cancellation.cancelled() => return Err(AsrError::Cancelled),
        };
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    if hex::encode(hasher.finalize()) != model.sha256 {
        return Err(AsrError::ChecksumMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    fn test_model() -> ModelSpec {
        ModelSpec {
            id: "test",
            version: "abcd",
            display_name: "Test",
            filename: "test.bin",
            expected_size_bytes: 8,
            sha256: "9c56cc51b374c3ba189210d5b6d4bf57790d351c96c47c02190ecf1e430635ab",
            english_only: true,
            estimated_memory_bytes: 0,
            license: "MIT",
        }
    }

    async fn local_model_server(response: String) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock server");
        let address = listener.local_addr().expect("server address");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("mock connection");
            let mut buffer = [0u8; 4096];
            let count = stream.read(&mut buffer).await.expect("mock request");
            let request = String::from_utf8_lossy(&buffer[..count]).into_owned();
            stream
                .write_all(response.as_bytes())
                .await
                .expect("mock response");
            request
        });
        (format!("http://{address}/model"), server)
    }

    #[tokio::test]
    async fn resumed_download_verifies_then_installs_atomically() {
        let directory = tempfile::tempdir().expect("tempdir");
        let mut manager = ModelManager::new(directory.path().to_path_buf()).expect("manager");
        manager.client = reqwest::Client::new();
        let model = test_model();
        fs::write(manager.path_for(&model, "part").expect("partial"), b"abcd")
            .await
            .expect("prefix");
        let (url, server) = local_model_server("HTTP/1.1 206 Partial Content\r\nContent-Length: 4\r\nContent-Range: bytes 4-7/8\r\nConnection: close\r\n\r\nefgh".into()).await;
        let result = manager
            .download_from(&model, &url, &CancellationToken::new(), |_| {})
            .await
            .expect("verified download");
        assert_eq!(
            fs::read(&result.path).await.expect("installed bytes"),
            b"abcdefgh"
        );
        assert!(server
            .await
            .expect("mock task")
            .to_lowercase()
            .contains("range: bytes=4-"));
        assert!(!manager.path_for(&model, "part").expect("partial").exists());
    }

    #[tokio::test]
    async fn server_ignoring_resume_restarts_without_duplication() {
        let directory = tempfile::tempdir().expect("tempdir");
        let mut manager = ModelManager::new(directory.path().to_path_buf()).expect("manager");
        manager.client = reqwest::Client::new();
        let model = test_model();
        fs::write(manager.path_for(&model, "part").expect("partial"), b"abcd")
            .await
            .expect("prefix");
        let (url, server) = local_model_server(
            "HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nabcdefgh".into(),
        )
        .await;
        let result = manager
            .download_from(&model, &url, &CancellationToken::new(), |_| {})
            .await
            .expect("restarted download");
        server.await.expect("mock task");
        assert_eq!(
            fs::read(&result.path).await.expect("installed bytes"),
            b"abcdefgh"
        );
    }

    #[tokio::test]
    async fn dishonest_range_cannot_install_or_extend_partial() {
        let directory = tempfile::tempdir().expect("tempdir");
        let mut manager = ModelManager::new(directory.path().to_path_buf()).expect("manager");
        manager.client = reqwest::Client::new();
        let model = test_model();
        let partial = manager.path_for(&model, "part").expect("partial");
        fs::write(&partial, b"abcd").await.expect("prefix");
        let (url, server) = local_model_server("HTTP/1.1 206 Partial Content\r\nContent-Length: 4\r\nContent-Range: bytes 0-3/8\r\nConnection: close\r\n\r\nefgh".into()).await;
        assert!(matches!(
            manager
                .download_from(&model, &url, &CancellationToken::new(), |_| {})
                .await,
            Err(AsrError::InvalidDownload)
        ));
        server.await.expect("mock task");
        assert_eq!(fs::read(partial).await.expect("preserved prefix"), b"abcd");
        assert!(!manager
            .path_for(&model, "bin")
            .expect("destination")
            .exists());
    }

    #[tokio::test]
    async fn truncated_http_body_keeps_a_resumable_prefix() {
        let directory = tempfile::tempdir().expect("tempdir");
        let mut manager = ModelManager::new(directory.path().to_path_buf()).expect("manager");
        manager.client = reqwest::Client::new();
        let model = test_model();
        let (url, server) = local_model_server(
            "HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nabcd".into(),
        )
        .await;
        assert!(matches!(
            manager
                .download_from(&model, &url, &CancellationToken::new(), |_| {})
                .await,
            Err(AsrError::DownloadNetwork(_))
        ));
        server.await.expect("mock task");
        assert_eq!(
            fs::read(manager.path_for(&model, "part").expect("partial"))
                .await
                .expect("prefix"),
            b"abcd"
        );
        assert!(!manager
            .path_for(&model, "bin")
            .expect("destination")
            .exists());
    }

    #[tokio::test]
    async fn untrusted_model_path_never_escapes_the_model_directory() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manager = ModelManager::new(directory.path().to_path_buf()).expect("manager");
        let mut model = test_model();
        model.id = "../outside";
        assert!(matches!(
            manager.path_for(&model, "bin"),
            Err(AsrError::UnsafePath)
        ));
        model.id = "test";
        model.version = "../outside";
        assert!(matches!(
            manager.path_for(&model, "bin"),
            Err(AsrError::UnsafePath)
        ));
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        assert!(matches!(
            manager
                .migrate_to(directory.path().join("new-location"), &cancelled)
                .await,
            Err(AsrError::Cancelled)
        ));
        assert!(!directory.path().join("new-location").exists());
    }
    #[test]
    fn range_and_redirects_are_strict() {
        assert!(validate_content_range(Some("bytes 5-9/10"), 5, 10).is_ok());
        assert!(validate_content_range(Some("bytes 0-9/10"), 5, 10).is_err());
        assert!(validate_content_range(Some("bytes 5-8/10"), 5, 10).is_err());
        assert!(trusted_model_host("cas-bridge.xethub.hf.co"));
        assert!(!trusted_model_host("huggingface.co.evil.example"));
    }
    #[tokio::test]
    async fn corrupt_partial_is_deleted_before_install() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manager = ModelManager::new(directory.path().to_path_buf()).expect("manager");
        let model = ModelSpec {
            id: "test",
            version: "abcd",
            display_name: "Test",
            filename: "test.bin",
            expected_size_bytes: 4,
            sha256: "invalid",
            english_only: true,
            estimated_memory_bytes: 0,
            license: "MIT",
        };
        let partial = manager.path_for(&model, "part").expect("path");
        fs::write(&partial, b"data").await.expect("fixture");
        assert!(matches!(
            manager
                .download_from(
                    &model,
                    "https://example.invalid",
                    &CancellationToken::new(),
                    |_| {}
                )
                .await,
            Err(AsrError::ChecksumMismatch)
        ));
        assert!(!partial.exists());
        assert!(!manager.path_for(&model, "bin").expect("path").exists());
    }
    #[tokio::test]
    async fn cancellation_and_cross_process_lock_are_explicit() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manager = ModelManager::new(directory.path().to_path_buf()).expect("manager");
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        assert!(matches!(
            manager
                .download("whisper-tiny-en", &cancellation, |_| {})
                .await,
            Err(AsrError::Cancelled)
        ));
        let model = find_model("whisper-tiny-en").expect("model");
        let _lock = manager.acquire_lock(model).expect("first lock");
        assert!(matches!(
            manager.acquire_lock(model),
            Err(AsrError::DownloadBusy)
        ));
        assert!(matches!(
            manager.uninstall("../../bad").await,
            Err(AsrError::UnknownModel)
        ));
    }
}
