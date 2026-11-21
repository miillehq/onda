//! Resume negotiation and bounded streaming for verified model files.
use super::*;
pub(super) async fn resume_offset(
    partial: &Path,
    directory: &Path,
    model: &ModelSpec,
) -> Result<u64, AsrError> {
    let mut completed = match fs::metadata(partial).await {
        Ok(metadata) if metadata.is_file() => metadata.len(),
        Ok(_) => return Err(AsrError::UnsafePath),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error.into()),
    };
    if completed > model.expected_size_bytes {
        fs::remove_file(partial).await?;
        completed = 0;
    }
    let available = fs2::available_space(directory)?;
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
    Ok(completed)
}
pub(super) async fn response(
    client: &reqwest::Client,
    source_url: &str,
    mut completed: u64,
    model: &ModelSpec,
    cancellation: &CancellationToken,
) -> Result<(reqwest::Response, u64, bool), AsrError> {
    let mut request = client.get(source_url);
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
    Ok((response, completed, append))
}
/// O(received bytes) time and one network chunk of working memory.
pub(super) async fn write_body(
    partial: &Path,
    response: reqwest::Response,
    mut completed: u64,
    append: bool,
    model: &ModelSpec,
    cancellation: &CancellationToken,
    progress: &(impl Fn(DownloadProgress) + Send + Sync),
) -> Result<u64, AsrError> {
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(partial)
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
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(error) => {
                // A tokio file finishes writes in the background: flush, so the bytes
                // that arrived are in the partial file to resume from before this returns.
                file.flush().await?;
                return Err(AsrError::DownloadNetwork(error));
            }
        };
        if completed + chunk.len() as u64 > model.expected_size_bytes {
            drop(file);
            fs::remove_file(partial).await?;
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
    Ok(completed)
}
