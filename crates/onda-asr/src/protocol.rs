//! Private bounded pipe protocol used by the local ASR worker.
use crate::{ExecutionBackend, MAX_AUDIO_SECONDS, SAMPLE_RATE_HZ};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const PROTOCOL_VERSION: u32 = 2;
pub const MAX_HEADER_BYTES: usize = 16 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
pub const MAX_INITIAL_PROMPT_BYTES: usize = 4096;
pub const MAX_SAMPLES: usize = SAMPLE_RATE_HZ as usize * MAX_AUDIO_SECONDS;
/// Whisper's encoder always covers 30 s (1500 frames of 20 ms). A short dictation only needs
/// its own length; the margin keeps word endings and avoids the repetition tiny contexts cause.
const FULL_AUDIO_CONTEXT_FRAMES: usize = 1500;
const SAMPLES_PER_AUDIO_FRAME: usize = 320;
const AUDIO_CONTEXT_MARGIN_FRAMES: usize = 128;

/// Encoder frames for this clip, or 0 (whisper's full window) once it would not save work.
/// The worker passes it to whisper.cpp as the audio context.
pub fn audio_context(sample_count: usize) -> i32 {
    let frames = sample_count.div_ceil(SAMPLES_PER_AUDIO_FRAME) + AUDIO_CONTEXT_MARGIN_FRAMES;
    if frames >= FULL_AUDIO_CONTEXT_FRAMES {
        0
    } else {
        frames as i32
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_clips_encode_only_their_own_length() {
        // 3 s is 150 encoder frames plus the margin.
        assert_eq!(audio_context(3 * 16_000), 278);
        assert_eq!(audio_context(1), 129);
        // Near and beyond the 30 s window, the full context is cheaper to keep.
        assert_eq!(audio_context(28 * 16_000), 0);
        assert_eq!(audio_context(60 * 16_000), 0);
    }
}
