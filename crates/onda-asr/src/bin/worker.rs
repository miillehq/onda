//! Disposable inference process. No network access, shell arguments, or audio files.
use onda_asr::{
    protocol::{
        AcceleratorProbe, WorkerRequest, WorkerResponse, MAX_HEADER_BYTES,
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
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

const MAX_THREADS: usize = 16;
const GREEDY_BEST_OF: i32 = 1;
const SILENCE_RMS_THRESHOLD: f64 = 0.0001;
const NATIVE_GPU_MISSING_MESSAGE: &[u8] = b"whisper_backend_init_gpu: no GPU found";
const NATIVE_INITIALIZATION_FAILURE_MESSAGE: &[u8] =
    b"whisper_backend_init_gpu: failed to initialize";
static NATIVE_ACCELERATOR_FAILED: AtomicBool = AtomicBool::new(false);

fn main() {
    harden_dll_search();
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
    let mut header = Vec::new();
    (&mut input)
        .take((MAX_HEADER_BYTES + 1) as u64)
        .read_until(b'\n', &mut header)
        .map_err(|_| "request read failed")?;
    if header.len() > MAX_HEADER_BYTES || header.last() != Some(&b'\n') {
        return Err("invalid header length");
    }
    let request: WorkerRequest = serde_json::from_slice(&header).map_err(|_| "invalid request")?;
    validate_request(&request)?;
    let mut samples = Vec::with_capacity(request.sample_count);
    let mut buffer = [0u8; 4 * 4096];
    let mut remaining = request.sample_count;
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
    let response = if rms(&samples) < SILENCE_RMS_THRESHOLD {
        WorkerResponse {
            version: PROTOCOL_VERSION,
            text: String::new(),
            backend: request.backend,
            model_load_ms: 0,
            inference_ms: 0,
        }
    } else {
        transcribe(&request, &samples)?
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

fn validate_request(request: &WorkerRequest) -> Result<(), &'static str> {
    if request.version != PROTOCOL_VERSION {
        return Err("unsupported protocol");
    }
    if request.sample_count == 0 || request.sample_count > MAX_SAMPLES {
        return Err("invalid sample count");
    }
    if request.threads == 0 || request.threads > MAX_THREADS {
        return Err("invalid thread count");
    }
    if request.initial_prompt.len() > MAX_INITIAL_PROMPT_BYTES
        || request.initial_prompt.contains('\0')
    {
        return Err("invalid prompt");
    }
    if request.language.as_ref().is_some_and(|language| {
        language.is_empty()
            || language.len() > 8
            || !language
                .bytes()
                .all(|byte| byte.is_ascii_alphabetic() || byte == b'-')
    }) {
        return Err("invalid language");
    }
    if !request.model_path.is_file() {
        return Err("missing model");
    }
    if request.backend == ExecutionBackend::Vulkan && vulkan_device_count() == 0 {
        return Err("no Vulkan device");
    }
    Ok(())
}

fn transcribe(request: &WorkerRequest, samples: &[f32]) -> Result<WorkerResponse, &'static str> {
    let started = Instant::now();
    let mut context_options = WhisperContextParameters::default();
    context_options.use_gpu(request.backend == ExecutionBackend::Vulkan);
    let context = WhisperContext::new_with_params(&request.model_path, context_options)
        .map_err(|_| "model load failed")?;
    let mut state = context
        .create_state()
        .map_err(|_| "state allocation failed")?;
    if request.backend == ExecutionBackend::Vulkan
        && NATIVE_ACCELERATOR_FAILED.load(Ordering::Relaxed)
    {
        return Err("native accelerator initialization failed");
    }
    let model_load_ms = started.elapsed().as_millis() as u64;
    let mut parameters = FullParams::new(SamplingStrategy::Greedy {
        best_of: GREEDY_BEST_OF,
    });
    parameters.set_n_threads(request.threads as i32);
    parameters.set_language(request.language.as_deref());
    parameters.set_translate(false);
    parameters.set_no_context(true);
    parameters.set_print_special(false);
    parameters.set_print_progress(false);
    parameters.set_print_realtime(false);
    parameters.set_print_timestamps(false);
    if !request.initial_prompt.is_empty() {
        parameters.set_initial_prompt(&request.initial_prompt);
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
    Ok(WorkerResponse {
        version: PROTOCOL_VERSION,
        text: text.trim().to_owned(),
        backend: request.backend,
        model_load_ms,
        inference_ms,
    })
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
