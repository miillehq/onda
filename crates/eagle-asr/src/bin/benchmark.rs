//! Reproducible real-model transcription harness; transcripts are explicitly requested output.
use eagle_asr::{
    AsrEngine, CancellationToken, ExecutionPreference, HardwareInfo, ModelManager,
    TranscriptionOptions,
};
use std::{path::PathBuf, process::ExitCode};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ASR benchmark: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().map(String::as_str) == Some("hardware") {
        println!("{}", serde_json::to_string_pretty(&HardwareInfo::detect())?);
        return Ok(());
    }
    if arguments.first().map(String::as_str) == Some("download") && arguments.len() == 3 {
        let manager = ModelManager::new(PathBuf::from(&arguments[1]))?;
        let installed = manager
            .download(&arguments[2], &CancellationToken::new(), |progress| {
                eprintln!(
                    "{:?} {} / {}",
                    progress.stage, progress.completed_bytes, progress.total_bytes
                );
            })
            .await?;
        println!("{}", serde_json::to_string_pretty(&installed)?);
        return Ok(());
    }
    if arguments.len() < 3 || arguments.len() > 5 {
        return Err("usage: eagle-asr-benchmark <model-path> <mono-16k-pcm16.wav> <worker-directory> [repetitions] [cpu|auto|gpu]; or hardware; or download <directory> <model-id>".into());
    }
    let mut reader = hound::WavReader::open(&arguments[1])?;
    let specification = reader.spec();
    if specification.channels != 1
        || specification.sample_rate != 16_000
        || specification.bits_per_sample != 16
        || specification.sample_format != hound::SampleFormat::Int
    {
        return Err("fixture must be mono 16kHz signed PCM16 WAV".into());
    }
    let samples: Vec<f32> = reader
        .samples::<i16>()
        .map(|sample| sample.map(|value| value as f32 / 32768.0))
        .collect::<Result<_, _>>()?;
    let engine = AsrEngine::new(&PathBuf::from(&arguments[2]));
    let repetitions = arguments
        .get(3)
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(1);
    if repetitions == 0 || repetitions > 20 {
        return Err("repetitions must be between one and twenty".into());
    }
    for iteration in 0..repetitions {
        let mut options = TranscriptionOptions::new(PathBuf::from(&arguments[0]));
        options.language = Some("en".into());
        options.preference = match arguments.get(4).map(String::as_str).unwrap_or("cpu") {
            "cpu" => ExecutionPreference::CpuOnly,
            "auto" => ExecutionPreference::Auto,
            "gpu" => ExecutionPreference::GpuPreferred,
            _ => return Err("execution preference must be cpu, auto or gpu".into()),
        };
        let result = engine
            .transcribe(&samples, options, CancellationToken::new())
            .await?;
        println!(
            "{}",
            serde_json::json!({ "iteration": iteration + 1, "result": result })
        );
    }
    Ok(())
}
