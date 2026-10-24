//! Explicit physical-GPU acceptance gate, separate from required CPU-only tests.
use eagle_asr::{
    AsrEngine, CancellationToken, ExecutionBackend, ExecutionPreference, TranscriptionOptions,
};
use std::path::PathBuf;

#[tokio::test]
#[ignore = "requires a physical compatible Vulkan GPU, packaged worker, EAGLE_ASR_TEST_MODEL and EAGLE_ASR_WORKER_DIR"]
async fn real_vulkan_model_recognizes_both_public_speech_clauses() {
    let model = PathBuf::from(std::env::var_os("EAGLE_ASR_TEST_MODEL").expect("set model"));
    let directory =
        PathBuf::from(std::env::var_os("EAGLE_ASR_WORKER_DIR").expect("set worker directory"));
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/asr/jfk.wav");
    let mut reader = hound::WavReader::open(fixture).expect("fixture");
    let samples = reader
        .samples::<i16>()
        .map(|sample| sample.expect("PCM16") as f32 / 32768.0)
        .collect::<Vec<_>>();
    let mut options = TranscriptionOptions::new(model);
    options.language = Some("en".into());
    options.preference = ExecutionPreference::GpuPreferred;
    let result = AsrEngine::new(&directory)
        .transcribe(&samples, options, CancellationToken::new())
        .await
        .expect("Vulkan transcription");
    assert_eq!(
        result.backend,
        ExecutionBackend::Vulkan,
        "GPU acceptance requires actual acceleration, received CPU fallback"
    );
    assert!(!result.accelerator_fallback);
    let words = result
        .text
        .to_lowercase()
        .split_whitespace()
        .map(|word| {
            word.trim_matches(|character: char| !character.is_alphanumeric())
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        words.contains("ask not what your country can do for you"),
        "first clause missing"
    );
    assert!(
        words.contains("ask what you can do for your country"),
        "second clause missing"
    );
    assert!(result.inference_ms > 0);
}
