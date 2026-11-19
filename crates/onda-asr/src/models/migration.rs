use super::*;

impl ModelManager {
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
}
