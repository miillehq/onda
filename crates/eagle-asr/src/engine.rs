use crate::{
    protocol::{
        AcceleratorProbe, WorkerRequest, WorkerResponse, MAX_INITIAL_PROMPT_BYTES,
        MAX_OUTPUT_BYTES, MAX_SAMPLES, PROTOCOL_VERSION,
    },
    AsrError, CancellationToken, ExecutionBackend, ExecutionPreference, HardwareInfo,
    SAMPLE_RATE_HZ,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::Semaphore,
};

const DEFAULT_TIMEOUT_SECONDS: u64 = 180;
const MAX_TIMEOUT_SECONDS: u64 = 600;
const ACCELERATOR_PROBE_TIMEOUT_SECONDS: u64 = 8;
const ACCELERATOR_TIMEOUT_SHARE: u32 = 2;
const PCM_WRITE_CHUNK_SAMPLES: usize = 4096;
#[cfg(windows)]
const CREATE_NO_WINDOW_BELOW_NORMAL: u32 = windows::Win32::System::Threading::CREATE_NO_WINDOW.0
    | windows::Win32::System::Threading::BELOW_NORMAL_PRIORITY_CLASS.0;

#[derive(Clone, Debug)]
pub struct TranscriptionOptions {
    pub model_path: PathBuf,
    pub language: Option<String>,
    pub initial_prompt: String,
    pub preference: ExecutionPreference,
    pub thread_count: Option<usize>,
    pub timeout_seconds: u64,
}

impl TranscriptionOptions {
    pub fn new(model_path: PathBuf) -> Self {
        Self {
            model_path,
            language: None,
            initial_prompt: String::new(),
            preference: ExecutionPreference::Auto,
            thread_count: None,
            timeout_seconds: DEFAULT_TIMEOUT_SECONDS,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionResult {
    pub text: String,
    pub backend: ExecutionBackend,
    pub accelerator_fallback: bool,
    pub model_load_ms: u64,
    pub inference_ms: u64,
    pub total_ms: u64,
    pub audio_duration_ms: u64,
    pub thread_count: usize,
}

/// A cheap, clonable process client. Exactly one inference runs per engine.
#[derive(Clone)]
pub struct AsrEngine {
    cpu_worker: PathBuf,
    vulkan_worker: Option<PathBuf>,
    hardware: HardwareInfo,
    admission: Arc<Semaphore>,
}

impl AsrEngine {
    /// Worker directory is the installed executable directory, never the current working directory.
    pub fn new(worker_directory: &Path) -> Self {
        let extension = if cfg!(windows) { ".exe" } else { "" };
        Self::with_workers(
            worker_directory.join(format!("eagle-asr-worker{extension}")),
            Some(worker_directory.join(format!("eagle-asr-vulkan-worker{extension}"))),
            HardwareInfo::detect(),
        )
    }

    pub fn with_workers(
        cpu_worker: PathBuf,
        vulkan_worker: Option<PathBuf>,
        hardware: HardwareInfo,
    ) -> Self {
        Self {
            cpu_worker,
            vulkan_worker,
            hardware,
            admission: Arc::new(Semaphore::new(1)),
        }
    }

    pub fn hardware(&self) -> &HardwareInfo {
        &self.hardware
    }

    /// Validate PCM, probe optional acceleration in isolation, and kill/reap on cancellation.
    /// Complexity: O(n) PCM transfer, O(1) additional transfer memory (native model memory excluded).
    pub async fn transcribe(
        &self,
        samples: &[f32],
        options: TranscriptionOptions,
        cancellation: CancellationToken,
    ) -> Result<TranscriptionResult, AsrError> {
        validate_input(samples, &options)?;
        if cancellation.is_cancelled() {
            return Err(AsrError::Cancelled);
        }
        let _permit = self
            .admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| AsrError::Busy)?;
        if !self.cpu_worker.is_file() {
            return Err(AsrError::WorkerMissing);
        }
        let started = Instant::now();
        let threads = options
            .thread_count
            .unwrap_or_else(|| self.hardware.inference_threads());
        let timeout = Duration::from_secs(options.timeout_seconds);
        let mut fallback = false;
        if self.hardware.should_probe_vulkan(options.preference) {
            if let Some(worker) = self
                .vulkan_worker
                .as_ref()
                .filter(|worker| worker.is_file())
            {
                match self.probe_vulkan(worker, &cancellation).await {
                    Ok(true) => {
                        // A hung driver must leave time for the mandatory CPU recovery path.
                        let accelerator_timeout =
                            timeout.saturating_sub(started.elapsed()) / ACCELERATOR_TIMEOUT_SHARE;
                        match self
                            .run_worker(
                                worker,
                                ExecutionBackend::Vulkan,
                                samples,
                                &options,
                                threads,
                                accelerator_timeout,
                                &cancellation,
                            )
                            .await
                        {
                            Ok(response) => {
                                return Ok(result_from(
                                    response,
                                    false,
                                    started,
                                    samples.len(),
                                    threads,
                                ))
                            }
                            Err(AsrError::Cancelled) => return Err(AsrError::Cancelled),
                            Err(_) => fallback = true,
                        }
                    }
                    Err(AsrError::Cancelled) => return Err(AsrError::Cancelled),
                    _ => fallback = true,
                }
            } else {
                fallback = true;
            }
        } else if options.preference == ExecutionPreference::GpuPreferred {
            fallback = true;
        }
        let remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(AsrError::Timeout);
        }
        let response = self
            .run_worker(
                &self.cpu_worker,
                ExecutionBackend::Cpu,
                samples,
                &options,
                threads,
                remaining,
                &cancellation,
            )
            .await?;
        Ok(result_from(
            response,
            fallback,
            started,
            samples.len(),
            threads,
        ))
    }

    async fn probe_vulkan(
        &self,
        path: &Path,
        cancellation: &CancellationToken,
    ) -> Result<bool, AsrError> {
        let mut command = worker_command(path);
        command.arg("--probe");
        #[cfg(windows)]
        let _worker_job = crate::worker_job::WorkerJob::new()?;
        let mut child = command.spawn()?;
        #[cfg(windows)]
        if let Err(error) = _worker_job.assign(&child) {
            let _ = child.kill().await;
            return Err(error.into());
        }
        let stdout = child.stdout.take().ok_or(AsrError::WorkerProtocol)?;
        let operation = async {
            let mut output = Vec::new();
            stdout
                .take(MAX_OUTPUT_BYTES as u64 + 1)
                .read_to_end(&mut output)
                .await?;
            let status = child.wait().await?;
            if !status.success() || output.len() > MAX_OUTPUT_BYTES {
                return Err(AsrError::WorkerFailed(status.code()));
            }
            let probe: AcceleratorProbe =
                serde_json::from_slice(&output).map_err(|_| AsrError::WorkerProtocol)?;
            Ok(probe.version == PROTOCOL_VERSION && probe.vulkan_devices > 0)
        };
        tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(ACCELERATOR_PROBE_TIMEOUT_SECONDS), operation) => {
                match result { Ok(result) => result, Err(_) => { let _ = child.kill().await; Err(AsrError::Timeout) } }
            }
            _ = cancellation.cancelled() => { let _ = child.kill().await; Err(AsrError::Cancelled) }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_worker(
        &self,
        path: &Path,
        backend: ExecutionBackend,
        samples: &[f32],
        options: &TranscriptionOptions,
        threads: usize,
        timeout: Duration,
        cancellation: &CancellationToken,
    ) -> Result<WorkerResponse, AsrError> {
        let request = WorkerRequest {
            version: PROTOCOL_VERSION,
            model_path: options.model_path.clone(),
            language: options.language.clone(),
            initial_prompt: options.initial_prompt.clone(),
            threads,
            backend,
            sample_count: samples.len(),
        };
        let mut header = serde_json::to_vec(&request).map_err(|_| AsrError::WorkerProtocol)?;
        header.push(b'\n');
        #[cfg(windows)]
        let _worker_job = crate::worker_job::WorkerJob::new()?;
        let mut child = worker_command(path).spawn()?;
        #[cfg(windows)]
        if let Err(error) = _worker_job.assign(&child) {
            let _ = child.kill().await;
            return Err(error.into());
        }
        let mut stdin = child.stdin.take().ok_or(AsrError::WorkerProtocol)?;
        let stdout = child.stdout.take().ok_or(AsrError::WorkerProtocol)?;
        let operation = async {
            stdin.write_all(&header).await?;
            let mut transfer = [0u8; PCM_WRITE_CHUNK_SAMPLES * 4];
            for chunk in samples.chunks(PCM_WRITE_CHUNK_SAMPLES) {
                for (sample, bytes) in chunk.iter().zip(transfer.chunks_exact_mut(4)) {
                    bytes.copy_from_slice(&sample.to_le_bytes());
                }
                stdin.write_all(&transfer[..chunk.len() * 4]).await?;
            }
            stdin.shutdown().await?;
            drop(stdin);
            let mut output = Vec::new();
            stdout
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

fn worker_command(path: &Path) -> Command {
    let mut command = Command::new(path);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW_BELOW_NORMAL);
    command
}

pub(crate) fn validate_input(
    samples: &[f32],
    options: &TranscriptionOptions,
) -> Result<(), AsrError> {
    if samples.is_empty() {
        return Err(AsrError::InvalidAudio("recording is empty"));
    }
    if samples.len() > MAX_SAMPLES {
        return Err(AsrError::InvalidAudio("recording exceeds ten minutes"));
    }
    if samples
        .iter()
        .any(|sample| !sample.is_finite() || sample.abs() > 1.0)
    {
        return Err(AsrError::InvalidAudio(
            "PCM must be finite normalized floats",
        ));
    }
    if options.initial_prompt.len() > MAX_INITIAL_PROMPT_BYTES
        || options.initial_prompt.contains('\0')
    {
        return Err(AsrError::InvalidOptions(
            "vocabulary prompt is too long or contains NUL",
        ));
    }
    if let Some(language) = options.language.as_ref() {
        if language.is_empty()
            || language.len() > 8
            || !language
                .bytes()
                .all(|byte| byte.is_ascii_alphabetic() || byte == b'-')
        {
            return Err(AsrError::InvalidOptions("invalid language code"));
        }
    }
    if options
        .thread_count
        .is_some_and(|threads| threads == 0 || threads > 16)
    {
        return Err(AsrError::InvalidOptions(
            "thread count must be between one and sixteen",
        ));
    }
    if options.timeout_seconds == 0 || options.timeout_seconds > MAX_TIMEOUT_SECONDS {
        return Err(AsrError::InvalidOptions(
            "timeout must be between one and 600 seconds",
        ));
    }
    if !options.model_path.is_file() {
        return Err(AsrError::ModelMissing);
    }
    Ok(())
}

fn result_from(
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_audio_and_nul_prompt_fail_before_worker_launch() {
        let mut options = TranscriptionOptions::new(PathBuf::new());
        assert!(matches!(
            validate_input(&[], &options),
            Err(AsrError::InvalidAudio(_))
        ));
        assert!(matches!(
            validate_input(&[f32::NAN], &options),
            Err(AsrError::InvalidAudio(_))
        ));
        assert!(matches!(
            validate_input(&[1.01], &options),
            Err(AsrError::InvalidAudio(_))
        ));
        options.initial_prompt = "invalid\0term".into();
        assert!(matches!(
            validate_input(&[0.1], &options),
            Err(AsrError::InvalidOptions(_))
        ));
    }
}
