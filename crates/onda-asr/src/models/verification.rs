use super::*;

pub(super) fn installation(model: &ModelSpec, path: PathBuf) -> InstalledModel {
    InstalledModel {
        model_id: model.id.into(),
        version: model.version.into(),
        path,
        size_bytes: model.expected_size_bytes,
        manifest_version: MODEL_MANIFEST_VERSION,
    }
}

pub(super) fn download_progress(
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

pub(super) fn trusted_model_host(host: &str) -> bool {
    host == "huggingface.co"
        || host.ends_with(".huggingface.co")
        || host == "hf.co"
        || host.ends_with(".hf.co")
}

pub(super) fn validate_content_range(
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

pub(super) async fn verify_file(
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
