use super::*;

pub(super) struct RunningWorker {
    pub(super) path: PathBuf,
    pub(super) backend: ExecutionBackend,
    pub(super) child: Child,
    pub(super) stdin: ChildStdin,
    pub(super) stdout: ChildStdout,
    #[cfg(windows)]
    pub(super) _job: crate::worker_job::WorkerJob,
}

impl PreparedTranscription {
    pub fn backend(&self) -> ExecutionBackend {
        self.worker.backend
    }

    /// Send the recording to the prepared worker. A failed GPU worker is retried on the CPU,
    /// and a failed AVX2 worker on the baseline build, within the remaining time.
    pub async fn finish(
        self,
        samples: &[f32],
        cancellation: CancellationToken,
    ) -> Result<TranscriptionResult, AsrError> {
        validate_samples(samples)?;
        let backend = self.backend();
        let Self {
            engine,
            options,
            threads,
            worker,
            mut accelerator_fallback,
            _permit,
        } = self;
        let started = Instant::now();
        let timeout = Duration::from_secs(options.timeout_seconds);
        let failed_path = worker.path.clone();
        let budget = if backend != ExecutionBackend::Cpu {
            // A hung driver must leave time for the mandatory CPU recovery path.
            timeout / ACCELERATOR_TIMEOUT_SHARE
        } else {
            timeout
        };
        let first = worker.run(samples, budget, &cancellation).await;
        let retry = match &first {
            Ok(_) | Err(AsrError::Cancelled) => None,
            Err(_) if backend != ExecutionBackend::Cpu => {
                if backend == ExecutionBackend::Vulkan {
                    engine.forget_vulkan();
                }
                accelerator_fallback = true;
                Some(engine.cpu_worker.clone())
            }
            Err(AsrError::WorkerFailed(_)) if failed_path == engine.cpu_worker => {
                engine.cpu_fallback.clone()
            }
            Err(_) => None,
        };
        let response = match retry {
            Some(path) => {
                let remaining = timeout.saturating_sub(started.elapsed());
                if remaining.is_zero() {
                    return Err(AsrError::Timeout);
                }
                engine
                    .spawn_worker(&path, ExecutionBackend::Cpu, &options, threads)
                    .await?
                    .run(samples, remaining, &cancellation)
                    .await?
            }
            None => first?,
        };
        Ok(result_from(
            response,
            accelerator_fallback,
            started,
            samples.len(),
            threads,
        ))
    }
}

impl RunningWorker {
    pub(super) async fn run(
        self,
        samples: &[f32],
        timeout: Duration,
        cancellation: &CancellationToken,
    ) -> Result<WorkerResponse, AsrError> {
        let backend = self.backend;
        let mut child = self.child;
        let mut stdin = Some(self.stdin);
        let mut stdout = self.stdout;
        #[cfg(windows)]
        let _job = self._job;
        let mut header = serde_json::to_vec(&WorkerAudio {
            sample_count: samples.len(),
        })
        .map_err(|_| AsrError::WorkerProtocol)?;
        header.push(b'\n');
        let operation = async {
            let pipe = stdin.as_mut().ok_or(AsrError::WorkerProtocol)?;
            pipe.write_all(&header).await?;
            let mut transfer = [0u8; PCM_WRITE_CHUNK_SAMPLES * 4];
            for chunk in samples.chunks(PCM_WRITE_CHUNK_SAMPLES) {
                for (sample, bytes) in chunk.iter().zip(transfer.chunks_exact_mut(4)) {
                    bytes.copy_from_slice(&sample.to_le_bytes());
                }
                pipe.write_all(&transfer[..chunk.len() * 4]).await?;
            }
            pipe.shutdown().await?;
            // Closing the pipe is the end of the audio; the worker refuses trailing bytes.
            drop(stdin.take());
            let mut output = Vec::new();
            (&mut stdout)
                .take(MAX_OUTPUT_BYTES as u64 + 1)
                .read_to_end(&mut output)
                .await?;
            if output.len() > MAX_OUTPUT_BYTES {
                return Err(AsrError::WorkerProtocol);
            }
            let status = child.wait().await?;
            if !status.success() {
                return Err(AsrError::WorkerFailed(status.code()));
            }
            let response: WorkerResponse =
                serde_json::from_slice(&output).map_err(|_| AsrError::WorkerProtocol)?;
            if response.version != PROTOCOL_VERSION || response.backend != backend {
                return Err(AsrError::WorkerProtocol);
            }
            Ok(response)
        };
        let result = tokio::select! {
            result = tokio::time::timeout(timeout, operation) => match result { Ok(result) => result, Err(_) => Err(AsrError::Timeout) },
            _ = cancellation.cancelled() => Err(AsrError::Cancelled),
        };
        if result.is_err() {
            let _ = child.kill().await;
        }
        result
    }
}

pub(super) fn worker_command(path: &Path) -> Command {
    let mut command = Command::new(path);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

pub(super) fn result_from(
    response: WorkerResponse,
    fallback: bool,
    started: Instant,
    sample_count: usize,
    threads: usize,
) -> TranscriptionResult {
    TranscriptionResult {
        text: response.text,
        backend: response.backend,
        accelerator_fallback: fallback,
        model_load_ms: response.model_load_ms,
        inference_ms: response.inference_ms,
        total_ms: started.elapsed().as_millis() as u64,
        audio_duration_ms: (sample_count as u64 * 1000) / SAMPLE_RATE_HZ as u64,
        thread_count: threads,
    }
}
