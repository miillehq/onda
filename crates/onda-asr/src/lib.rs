//! Local ASR remains useful without a GPU or cloud account.
//!
//! Native model inference executes in a separately packaged worker. The desktop
//! shell links only this process client, so GPU loader faults, native crashes,
//! inference timeouts and cancellation cannot starve Windows input integration.

mod engine;
mod hardware;
mod languages;
mod manifest;
mod models;
pub mod protocol;
#[cfg(windows)]
mod worker_job;

pub use engine::{AsrEngine, PreparedTranscription, TranscriptionOptions, TranscriptionResult};
pub use hardware::{ExecutionBackend, ExecutionPreference, GpuInfo, HardwareInfo};
pub use languages::{is_supported_language, WHISPER_LANGUAGES};
pub use manifest::{builtin_models, recommended_model, ModelSpec, QualityProfile};
pub use models::{DownloadProgress, DownloadStage, InstalledModel, ModelManager};
pub use tokio_util::sync::CancellationToken;

/// Audio contract shared with microphone capture and the native worker.
pub const SAMPLE_RATE_HZ: u32 = 16_000;
/// Bound transient memory and decoding work for accidental very long recordings.
pub const MAX_AUDIO_SECONDS: usize = 600;

/// Errors contain diagnostics, never microphone samples or transcript contents.
#[derive(Debug, thiserror::Error)]
pub enum AsrError {
    #[error("operation cancelled")]
    Cancelled,
    #[error("local transcription timed out")]
    Timeout,
    #[error("another local transcription is running")]
    Busy,
    #[error("invalid audio: {0}")]
    InvalidAudio(&'static str),
    #[error("invalid transcription option: {0}")]
    InvalidOptions(&'static str),
    #[error("ASR worker is not installed")]
    WorkerMissing,
    #[error("ASR worker failed; native process exit code: {0:?}")]
    WorkerFailed(Option<i32>),
    #[error("ASR worker protocol failed")]
    WorkerProtocol,
    #[error("native inference failed: {0}")]
    Inference(&'static str),
    #[error("unknown model")]
    UnknownModel,
    #[error("model is not installed")]
    ModelMissing,
    #[error("model checksum or expected size does not match")]
    ChecksumMismatch,
    #[error("model download already in progress")]
    DownloadBusy,
    #[error("not enough disk space: need {required_bytes} bytes; have {available_bytes}")]
    InsufficientDisk {
        required_bytes: u64,
        available_bytes: u64,
    },
    #[error("unsafe model path")]
    UnsafePath,
    #[error("model server returned an invalid byte range or size")]
    InvalidDownload,
    #[error("model server request failed")]
    DownloadNetwork(#[source] reqwest::Error),
    #[error("local filesystem or process I/O failed")]
    Io(#[from] std::io::Error),
}
