//! Disposable inference process. No network access, shell arguments, or audio files.
use onda_asr::{
    protocol::{
        AcceleratorProbe, WorkerAudio, WorkerResponse, WorkerSetup, MAX_HEADER_BYTES,
        MAX_INITIAL_PROMPT_BYTES, MAX_OUTPUT_BYTES, MAX_SAMPLES, PROTOCOL_VERSION,
    },
    ExecutionBackend,
};
use std::{
    ffi::{c_char, c_int, c_void, CStr},
    io::{BufRead, BufReader, Read, Write},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

const MAX_THREADS: usize = 16;
const GREEDY_BEST_OF: i32 = 1;
const SILENCE_RMS_THRESHOLD: f64 = 0.0001;
/// Whisper's encoder always covers 30 s (1500 frames of 20 ms). A short dictation only needs
/// its own length; the margin keeps word endings and avoids the repetition tiny contexts cause.
const FULL_AUDIO_CONTEXT_FRAMES: usize = 1500;
const SAMPLES_PER_AUDIO_FRAME: usize = 320;
const AUDIO_CONTEXT_MARGIN_FRAMES: usize = 128;
const NATIVE_GPU_MISSING_MESSAGE: &[u8] = b"whisper_backend_init_gpu: no GPU found";
const NATIVE_INITIALIZATION_FAILURE_MESSAGE: &[u8] =
    b"whisper_backend_init_gpu: failed to initialize";
static NATIVE_ACCELERATOR_FAILED: AtomicBool = AtomicBool::new(false);

fn main() {
    harden_dll_search();
    request_full_speed();
    whisper_rs::install_logging_hooks();
    // SAFETY: the callback never unwinds, retains no native pointers and uses only atomics.
    unsafe { whisper_rs::set_log_callback(Some(native_diagnostic_gate), std::ptr::null_mut()) };
    if let Err(error) = run() {
        // Errors are fixed diagnostics; no prompt, model path or transcription is logged.
        eprintln!("onda-asr-worker: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), &'static str> {
    if std::env::args().nth(1).as_deref() == Some("--probe") {
        let response = AcceleratorProbe {
            version: PROTOCOL_VERSION,
            vulkan_devices: vulkan_device_count(),
        };
        serde_json::to_writer(std::io::stdout().lock(), &response)
            .map_err(|_| "probe response failed")?;
        return Ok(());
    }
    let mut input = BufReader::new(std::io::stdin().lock());
    let setup: WorkerSetup = read_header(&mut input)?;
    validate_setup(&setup)?;
    // Load before the audio arrives: a client that sends setup when recording starts gets
    // this done while the user is still speaking.
    let started = Instant::now();
    let mut state = load(&setup)?;
    let model_load_ms = started.elapsed().as_millis() as u64;
    let audio: WorkerAudio = read_header(&mut input)?;
    if audio.sample_count == 0 || audio.sample_count > MAX_SAMPLES {
        return Err("invalid sample count");
    }
    let mut samples = Vec::with_capacity(audio.sample_count);
    let mut buffer = [0u8; 4 * 4096];
    let mut remaining = audio.sample_count;
    while remaining > 0 {
        let count = remaining.min(buffer.len() / 4);
        input
            .read_exact(&mut buffer[..count * 4])
            .map_err(|_| "truncated PCM")?;
        for bytes in buffer[..count * 4].chunks_exact(4) {
            let value = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            if !value.is_finite() || value.abs() > 1.0 {
                return Err("invalid PCM");
            }
            samples.push(value);
        }
        remaining -= count;
    }
    let mut extra = [0u8; 1];
    if input.read(&mut extra).map_err(|_| "PCM end failed")? != 0 {
        return Err("extra PCM bytes");
    }
    let (text, inference_ms) = if rms(&samples) < SILENCE_RMS_THRESHOLD {
        (String::new(), 0)
    } else {
        transcribe(&mut state, &setup, &samples)?
    };
    let response = WorkerResponse {
        version: PROTOCOL_VERSION,
        text,
        backend: setup.backend,
        model_load_ms,
        inference_ms,
    };
    let output = serde_json::to_vec(&response).map_err(|_| "response serialization failed")?;
    if output.len() > MAX_OUTPUT_BYTES {
        return Err("transcript exceeded output limit");
    }
    std::io::stdout()
        .lock()
        .write_all(&output)
        .map_err(|_| "response write failed")
}

/// One bounded JSON line.
fn read_header<T: serde::de::DeserializeOwned>(
    input: &mut impl BufRead,
) -> Result<T, &'static str> {
    let mut header = Vec::new();
    input
        .take((MAX_HEADER_BYTES + 1) as u64)
        .read_until(b'\n', &mut header)
        .map_err(|_| "request read failed")?;
    if header.len() > MAX_HEADER_BYTES || header.last() != Some(&b'\n') {
        return Err("invalid header length");
    }
    serde_json::from_slice(&header).map_err(|_| "invalid request")
}

fn validate_setup(setup: &WorkerSetup) -> Result<(), &'static str> {
    if setup.version != PROTOCOL_VERSION {
        return Err("unsupported protocol");
    }
    if setup.threads == 0 || setup.threads > MAX_THREADS {
        return Err("invalid thread count");
    }
    if setup.initial_prompt.len() > MAX_INITIAL_PROMPT_BYTES || setup.initial_prompt.contains('\0')
    {
        return Err("invalid prompt");
    }
    if setup.language.as_ref().is_some_and(|language| {
        language.is_empty()
            || language.len() > 8
            || !language
                .bytes()
                .all(|byte| byte.is_ascii_alphabetic() || byte == b'-')
    }) {
        return Err("invalid language");
    }
    if !setup.model_path.is_file() {
        return Err("missing model");
    }
    if setup.backend == ExecutionBackend::Vulkan && vulkan_device_count() == 0 {
        return Err("no Vulkan device");
    }
    Ok(())
}

fn load(setup: &WorkerSetup) -> Result<WhisperState, &'static str> {
    let mut context_options = WhisperContextParameters::default();
    context_options.use_gpu(setup.backend == ExecutionBackend::Vulkan);
    let context = WhisperContext::new_with_params(&setup.model_path, context_options)
        .map_err(|_| "model load failed")?;
    let state = context
        .create_state()
        .map_err(|_| "state allocation failed")?;
    if setup.backend == ExecutionBackend::Vulkan
        && NATIVE_ACCELERATOR_FAILED.load(Ordering::Relaxed)
    {
        return Err("native accelerator initialization failed");
    }
    Ok(state)
}

fn transcribe(
    state: &mut WhisperState,
    setup: &WorkerSetup,
    samples: &[f32],
) -> Result<(String, u64), &'static str> {
    let mut parameters = FullParams::new(SamplingStrategy::Greedy {
        best_of: GREEDY_BEST_OF,
    });
    parameters.set_n_threads(setup.threads as i32);
    parameters.set_audio_ctx(audio_context(samples.len()));
    parameters.set_language(setup.language.as_deref());
    parameters.set_translate(false);
    parameters.set_no_context(true);
    parameters.set_print_special(false);
    parameters.set_print_progress(false);
    parameters.set_print_realtime(false);
    parameters.set_print_timestamps(false);
    if !setup.initial_prompt.is_empty() {
        parameters.set_initial_prompt(&setup.initial_prompt);
    }
    let inference_started = Instant::now();
    state
        .full(parameters, samples)
        .map_err(|_| "inference failed")?;
    let inference_ms = inference_started.elapsed().as_millis() as u64;
    let mut text = String::new();
    for segment in state.as_iter() {
        text.push_str(
            segment
                .to_str()
                .map_err(|_| "invalid transcript encoding")?,
        );
        if text.len() > MAX_OUTPUT_BYTES {
            return Err("transcript too long");
        }
    }
    Ok((text.trim().to_owned(), inference_ms))
}

/// Encoder frames for this clip, or 0 (whisper's full window) once it would not save work.
fn audio_context(sample_count: usize) -> i32 {
    let frames = sample_count.div_ceil(SAMPLES_PER_AUDIO_FRAME) + AUDIO_CONTEXT_MARGIN_FRAMES;
    if frames >= FULL_AUDIO_CONTEXT_FRAMES {
        0
    } else {
        frames as i32
    }
}

fn vulkan_device_count() -> usize {
    #[cfg(feature = "vulkan")]
    {
        whisper_rs::vulkan::list_devices().len()
    }
    #[cfg(not(feature = "vulkan"))]
    {
        0
    }
}

// Complexity: O(n) samples; O(1) additional memory.
fn rms(samples: &[f32]) -> f64 {
    (samples
        .iter()
        .map(|sample| (*sample as f64).powi(2))
        .sum::<f64>()
        / samples.len() as f64)
        .sqrt()
}

fn harden_dll_search() {
    #[cfg(windows)]
    {
        use windows::Win32::System::LibraryLoader::{
            SetDefaultDllDirectories, LOAD_LIBRARY_SEARCH_APPLICATION_DIR,
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        };
        // SAFETY: process-wide search flags deliberately exclude the current working directory.
        let _ = unsafe {
            SetDefaultDllDirectories(
                LOAD_LIBRARY_SEARCH_SYSTEM32 | LOAD_LIBRARY_SEARCH_APPLICATION_DIR,
            )
        };
    }
}

/// Windows moves windowless background processes onto efficiency cores ("power throttling"),
/// and a worker that waited while the user spoke looks like one. On hybrid CPUs that made
/// inference several times slower, so the worker opts out.
fn request_full_speed() {
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::{
            GetCurrentProcess, ProcessPowerThrottling, SetProcessInformation,
            PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            PROCESS_POWER_THROTTLING_STATE,
        };
        let state = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: 0,
        };
        // SAFETY: the pointer and size describe a live, correctly typed struct for this call.
        let _ = unsafe {
            SetProcessInformation(
                GetCurrentProcess(),
                ProcessPowerThrottling,
                std::ptr::from_ref(&state).cast(),
                std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            )
        };
    }
}

/// Capture only fixed initialization failure evidence; discard every sensitive native message.
/// whisper.cpp 1.8.3 can otherwise silently initialize CPU after GPU initialization fails.
unsafe extern "C" fn native_diagnostic_gate(
    _level: c_int,
    message: *const c_char,
    _user_data: *mut c_void,
) {
    if message.is_null() {
        return;
    }
    // SAFETY: whisper.cpp supplies a valid NUL-terminated message for the callback duration.
    let bytes = unsafe { CStr::from_ptr(message) }.to_bytes();
    if contains_bytes(bytes, NATIVE_GPU_MISSING_MESSAGE)
        || contains_bytes(bytes, NATIVE_INITIALIZATION_FAILURE_MESSAGE)
    {
        NATIVE_ACCELERATOR_FAILED.store(true, Ordering::Relaxed);
    }
}

// Complexity: O(n*m) for bounded diagnostic text/fixed markers; O(1) additional memory.
fn contains_bytes(message: &[u8], marker: &[u8]) -> bool {
    message
        .windows(marker.len())
        .any(|candidate| candidate == marker)
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
