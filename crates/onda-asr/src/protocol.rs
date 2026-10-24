//! Private bounded pipe protocol used by the local ASR worker.
use crate::{ExecutionBackend, MAX_AUDIO_SECONDS, SAMPLE_RATE_HZ};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const PROTOCOL_VERSION: u32 = 2;
pub const MAX_HEADER_BYTES: usize = 16 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
pub const MAX_INITIAL_PROMPT_BYTES: usize = 4096;
pub const MAX_SAMPLES: usize = SAMPLE_RATE_HZ as usize * MAX_AUDIO_SECONDS;

/// First JSON line. The worker loads the model as soon as it arrives, so the client can send
/// it when recording starts and the load overlaps with the user speaking.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerSetup {
    pub version: u32,
    pub model_path: PathBuf,
    pub language: Option<String>,
    pub initial_prompt: String,
    pub threads: usize,
    pub backend: ExecutionBackend,
}

/// Second JSON line, followed by exactly `sample_count` little endian f32 values.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerAudio {
    pub sample_count: usize,
}

/// The worker writes exactly one bounded JSON response; native diagnostics use stderr.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerResponse {
    pub version: u32,
    pub text: String,
    pub backend: ExecutionBackend,
    pub model_load_ms: u64,
    pub inference_ms: u64,
}

/// Acceleration support is tested in a sacrificial worker before inference.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorProbe {
    pub version: u32,
    pub vulkan_devices: usize,
}
