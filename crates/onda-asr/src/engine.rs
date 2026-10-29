use crate::{
    protocol::{
        AcceleratorProbe, WorkerAudio, WorkerResponse, WorkerSetup, MAX_INITIAL_PROMPT_BYTES,
        MAX_OUTPUT_BYTES, MAX_SAMPLES, PROTOCOL_VERSION,
    },
    AsrError, CancellationToken, ExecutionBackend, ExecutionPreference, HardwareInfo,
    SAMPLE_RATE_HZ,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{OwnedSemaphorePermit, Semaphore},
};

const DEFAULT_TIMEOUT_SECONDS: u64 = 180;
const MAX_TIMEOUT_SECONDS: u64 = 600;
const ACCELERATOR_PROBE_TIMEOUT_SECONDS: u64 = 8;
const ACCELERATOR_TIMEOUT_SHARE: u32 = 2;
const PCM_WRITE_CHUNK_SAMPLES: usize = 4096;
/// In Auto, models below this size (Fast, Balanced) run on the CPU: an optimized CPU worker
/// finishes them sooner than a GPU worker can initialize its device.
const MIN_AUTO_GPU_MODEL_BYTES: u64 = 150 * 1024 * 1024;
/// Normal priority: Windows parks below-normal windowless processes on efficiency cores after
/// the machine idles, which made recognition up to ten times slower and erratic. The shortcut
/// helper runs above normal, so the keyboard hook still comes first.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = windows::Win32::System::Threading::CREATE_NO_WINDOW.0;

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
    /// The SSE2 build, retried if the AVX2 worker fails on an unusual CPU.
    cpu_fallback: Option<PathBuf>,
    vulkan_worker: Option<PathBuf>,
    hardware: HardwareInfo,
    admission: Arc<Semaphore>,
    /// Whether the Vulkan worker found a device, remembered for the session so each
    /// dictation does not start a probe process; a failed Vulkan run clears it to false.
    vulkan_probe: Arc<Mutex<Option<bool>>>,
}

/// A worker that already received its setup and is loading (or has loaded) the model.
/// Dropping it kills the worker.
pub struct PreparedTranscription {
    engine: AsrEngine,
    options: TranscriptionOptions,
    threads: usize,
    worker: RunningWorker,
    accelerator_fallback: bool,
    _permit: OwnedSemaphorePermit,
}

struct RunningWorker {
    path: PathBuf,
    backend: ExecutionBackend,
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    #[cfg(windows)]
    _job: crate::worker_job::WorkerJob,
}

impl AsrEngine {
    /// Worker directory is the installed executable directory, never the current working directory.
    /// CPUs with AVX2, FMA and F16C get the AVX2 build, several times faster than the SSE2
    /// baseline that every x86-64 CPU can run.
    pub fn new(worker_directory: &Path) -> Self {
        let extension = if cfg!(windows) { ".exe" } else { "" };
        let hardware = HardwareInfo::detect();
        let baseline = worker_directory.join(format!("onda-asr-worker{extension}"));
        let avx2 = worker_directory.join(format!("onda-asr-avx2-worker{extension}"));
        let mut engine = Self::with_workers(
            baseline.clone(),
            Some(worker_directory.join(format!("onda-asr-vulkan-worker{extension}"))),
            hardware,
        );
        if engine.hardware.supports_avx2_worker() && avx2.is_file() {
            engine.cpu_worker = avx2;
            engine.cpu_fallback = Some(baseline);
        }
        engine
    }

    pub fn with_workers(
        cpu_worker: PathBuf,
        vulkan_worker: Option<PathBuf>,
        hardware: HardwareInfo,
    ) -> Self {
        Self {
            cpu_worker,
            cpu_fallback: None,
            vulkan_worker,
            hardware,
            admission: Arc::new(Semaphore::new(1)),
            vulkan_probe: Arc::new(Mutex::new(None)),
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
        validate_samples(samples)?;
        self.prepare(options, &cancellation)
            .await?
            .finish(samples, cancellation)
            .await
    }

    /// Start the worker and send its setup now, so the model loads (and a GPU initializes)
    /// while the user is still speaking. Holds the engine's single inference slot until
    /// finished or dropped.
    pub async fn prepare(
        &self,
        options: TranscriptionOptions,
        cancellation: &CancellationToken,
    ) -> Result<PreparedTranscription, AsrError> {
        validate_options(&options)?;
        if cancellation.is_cancelled() {
            return Err(AsrError::Cancelled);
        }
        let permit = self
            .admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| AsrError::Busy)?;
        if !self.cpu_worker.is_file() {
            return Err(AsrError::WorkerMissing);
        }
        let threads = options
            .thread_count
            .unwrap_or_else(|| self.hardware.inference_threads());
        let mut accelerator_fallback = false;
        let mut worker = None;
        if self.wants_accelerator(&options) {
            match self.vulkan_worker.clone().filter(|path| path.is_file()) {
                Some(path) => match self.vulkan_available(&path, cancellation).await {
                    Ok(true) => {
                        match self
                            .spawn_worker(&path, ExecutionBackend::Vulkan, &options, threads)
                            .await
                        {
                            Ok(running) => worker = Some(running),
                            Err(AsrError::Cancelled) => return Err(AsrError::Cancelled),
                            Err(_) => accelerator_fallback = true,
                        }
                    }
                    Err(AsrError::Cancelled) => return Err(AsrError::Cancelled),
                    _ => accelerator_fallback = true,
                },
                None => accelerator_fallback = true,
            }
        } else if options.preference == ExecutionPreference::GpuPreferred {
            accelerator_fallback = true;
        }
        let worker = match worker {
            Some(worker) => worker,
            None => {
                self.spawn_worker(&self.cpu_worker, ExecutionBackend::Cpu, &options, threads)
                    .await?
            }
        };
        Ok(PreparedTranscription {
            engine: self.clone(),
            options,
            threads,
            worker,
            accelerator_fallback,
            _permit: permit,
        })
    }

    /// GPU preferred always tries the accelerator; Auto uses it for Quality-size models on a
    /// discrete GPU, where it is faster once its startup is paid.
    fn wants_accelerator(&self, options: &TranscriptionOptions) -> bool {
        if !self.hardware.should_probe_vulkan(options.preference) {
            return false;
        }
        options.preference == ExecutionPreference::GpuPreferred
            || std::fs::metadata(&options.model_path)
                .is_ok_and(|metadata| metadata.len() >= MIN_AUTO_GPU_MODEL_BYTES)
    }

    async fn vulkan_available(
        &self,
        path: &Path,
        cancellation: &CancellationToken,
    ) -> Result<bool, AsrError> {
        if let Some(known) = self.vulkan_probe.lock().ok().and_then(|probe| *probe) {
            return Ok(known);
        }
        let available = match self.probe_vulkan(path, cancellation).await {
            Ok(available) => available,
            Err(AsrError::Cancelled) => return Err(AsrError::Cancelled),
            Err(_) => false,
        };
        if let Ok(mut probe) = self.vulkan_probe.lock() {
            *probe = Some(available);
        }
        Ok(available)
    }

    fn forget_vulkan(&self) {
        if let Ok(mut probe) = self.vulkan_probe.lock() {
            *probe = Some(false);
        }
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

    /// Launch a worker inside a private job object and send its setup line.
    async fn spawn_worker(
        &self,
        path: &Path,
        backend: ExecutionBackend,
        options: &TranscriptionOptions,
        threads: usize,
    ) -> Result<RunningWorker, AsrError> {
        let setup = WorkerSetup {
            version: PROTOCOL_VERSION,
            model_path: options.model_path.clone(),
            language: options.language.clone(),
            initial_prompt: options.initial_prompt.clone(),
            threads,
            backend,
        };
        let mut header = serde_json::to_vec(&setup).map_err(|_| AsrError::WorkerProtocol)?;
        header.push(b'\n');
        #[cfg(windows)]
        let job = crate::worker_job::WorkerJob::new()?;
        let mut child = worker_command(path).spawn()?;
        #[cfg(windows)]
        if let Err(error) = job.assign(&child) {
            let _ = child.kill().await;
            return Err(error.into());
        }
        let mut stdin = child.stdin.take().ok_or(AsrError::WorkerProtocol)?;
        let stdout = child.stdout.take().ok_or(AsrError::WorkerProtocol)?;
        if let Err(error) = async {
            stdin.write_all(&header).await?;
            stdin.flush().await
        }
        .await
        {
            let _ = child.kill().await;
            return Err(error.into());
        }
        Ok(RunningWorker {
            path: path.to_owned(),
            backend,
            child,
            stdin,
            stdout,
            #[cfg(windows)]
            _job: job,
        })
    }
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
        let backend = worker.backend;
        let failed_path = worker.path.clone();
        let budget = if backend == ExecutionBackend::Vulkan {
            // A hung driver must leave time for the mandatory CPU recovery path.
            timeout / ACCELERATOR_TIMEOUT_SHARE
        } else {
            timeout
        };
        let first = worker.run(samples, budget, &cancellation).await;
        let retry = match &first {
            Ok(_) | Err(AsrError::Cancelled) => None,
            Err(_) if backend == ExecutionBackend::Vulkan => {
                engine.forget_vulkan();
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
    async fn run(
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

fn worker_command(path: &Path) -> Command {
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

#[cfg(test)]
fn validate_input(samples: &[f32], options: &TranscriptionOptions) -> Result<(), AsrError> {
    validate_samples(samples)?;
    validate_options(options)
}

fn validate_samples(samples: &[f32]) -> Result<(), AsrError> {
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
    Ok(())
}

fn validate_options(options: &TranscriptionOptions) -> Result<(), AsrError> {
    if options.initial_prompt.len() > MAX_INITIAL_PROMPT_BYTES
        || options.initial_prompt.contains('\0')
    {
        return Err(AsrError::InvalidOptions(
            "vocabulary prompt is too long or contains NUL",
        ));
    }
    if let Some(language) = options.language.as_ref() {
        if !crate::is_supported_language(language) {
            return Err(AsrError::InvalidOptions("unsupported language"));
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

    #[test]
    fn languages_the_models_lack_never_reach_the_worker() {
        let mut options = TranscriptionOptions::new(PathBuf::new());
        options.language = Some("yue".into());
        assert!(matches!(
            validate_input(&[0.1], &options),
            Err(AsrError::InvalidOptions("unsupported language"))
        ));
        options.language = Some("zh".into());
        assert!(!matches!(
            validate_input(&[0.1], &options),
            Err(AsrError::InvalidOptions("unsupported language"))
        ));
    }
}
