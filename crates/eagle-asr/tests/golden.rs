//! This ignored test requires installed native worker/model assets; it asserts semantic output.
use eagle_asr::{AsrEngine, CancellationToken, ExecutionPreference, TranscriptionOptions};
use std::path::PathBuf;

#[tokio::test]
#[ignore = "requires EAGLE_ASR_TEST_MODEL and EAGLE_ASR_WORKER_DIR; downloads no assets implicitly"]
async fn real_model_transcribes_public_domain_jfk_audio() {
    let model =
        PathBuf::from(std::env::var_os("EAGLE_ASR_TEST_MODEL").expect("set EAGLE_ASR_TEST_MODEL"));
    let worker =
        PathBuf::from(std::env::var_os("EAGLE_ASR_WORKER_DIR").expect("set EAGLE_ASR_WORKER_DIR"));
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/asr/jfk.wav");
    let mut reader = hound::WavReader::open(fixture).expect("fixture");
    let samples: Vec<f32> = reader
        .samples::<i16>()
        .map(|sample| sample.expect("PCM16") as f32 / 32768.0)
        .collect();
    let mut options = TranscriptionOptions::new(model);
    options.preference = ExecutionPreference::CpuOnly;
    options.language = Some("en".into());
    let result = AsrEngine::new(&worker)
        .transcribe(&samples, options, CancellationToken::new())
        .await
        .expect("native transcription");
    let words: Vec<_> = result
        .text
        .to_lowercase()
        .split_whitespace()
        .map(|word| {
            word.trim_matches(|character: char| !character.is_alphanumeric())
                .to_owned()
        })
        .collect();
    let recognized = words.join(" ");
    assert!(
        recognized.contains("ask not what your country can do for you"),
        "missing first speech clause: {}",
        result.text
    );
    assert!(
        recognized.contains("ask what you can do for your country"),
        "missing second speech clause: {}",
        result.text
    );
    assert!(result.inference_ms > 0);
}

#[tokio::test]
#[ignore = "requires real native worker/model assets"]
async fn gpu_probe_failure_falls_back_to_real_cpu_transcription() {
    let model = PathBuf::from(std::env::var_os("EAGLE_ASR_TEST_MODEL").expect("set model"));
    let worker_directory =
        PathBuf::from(std::env::var_os("EAGLE_ASR_WORKER_DIR").expect("set worker directory"));
    let cpu_worker = worker_directory.join("eagle-asr-worker.exe");
    let mut hardware = eagle_asr::HardwareInfo::detect();
    hardware.vulkan_loader_present = true;
    hardware.gpus = vec![eagle_asr::GpuInfo {
        name: "test evidence".into(),
        vendor_id: 0,
        dedicated_memory_bytes: 1 << 30,
        shared_memory_bytes: 0,
        software_adapter: false,
    }];
    // A CPU worker gives honest zero Vulkan device evidence, so the real CPU path must run.
    let engine = AsrEngine::with_workers(cpu_worker.clone(), Some(cpu_worker), hardware);
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/asr/jfk.wav");
    let mut reader = hound::WavReader::open(fixture).expect("fixture");
    let samples: Vec<f32> = reader
        .samples::<i16>()
        .map(|sample| sample.expect("PCM16") as f32 / 32768.0)
        .collect();
    let mut options = TranscriptionOptions::new(model);
    options.language = Some("en".into());
    options.preference = ExecutionPreference::GpuPreferred;
    let result = engine
        .transcribe(&samples, options, CancellationToken::new())
        .await
        .expect("fallback transcription");
    assert!(result.accelerator_fallback);
    assert_eq!(result.backend, eagle_asr::ExecutionBackend::Cpu);
    assert!(result.text.to_lowercase().contains("your country"));
}

#[tokio::test]
#[ignore = "requires real native worker/model assets"]
async fn cancelled_native_inference_kills_worker_without_losing_shell() {
    let model = PathBuf::from(std::env::var_os("EAGLE_ASR_TEST_MODEL").expect("set model"));
    let worker_directory =
        PathBuf::from(std::env::var_os("EAGLE_ASR_WORKER_DIR").expect("set worker directory"));
    let mut options = TranscriptionOptions::new(model);
    options.language = Some("en".into());
    options.preference = ExecutionPreference::CpuOnly;
    let samples = vec![0.1; 16_000 * 60];
    let cancellation = CancellationToken::new();
    let trigger = cancellation.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        trigger.cancel();
    });
    let result = AsrEngine::new(&worker_directory)
        .transcribe(&samples, options, cancellation)
        .await;
    assert!(matches!(result, Err(eagle_asr::AsrError::Cancelled)));
}
