use crate::PlatformError;
use rubato::{FftFixedInOut, Resampler};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

pub const OUTPUT_SAMPLE_RATE: u32 = 16_000;
const MAX_CAPTURE_SECONDS: u32 = 600;
const MAX_SOURCE_SAMPLE_RATE: u32 = 192_000;
const RESAMPLE_CHUNK_SIZE: usize = 1024;
const SILENCE_RMS_THRESHOLD: f64 = 0.001;
const CLIPPING_THRESHOLD: f32 = 0.999;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureOptions {
    pub device_id: Option<String>,
    pub max_duration_seconds: u32,
}
impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            device_id: None,
            max_duration_seconds: 180,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDiagnostics {
    pub source_sample_rate: u32,
    pub source_channels: u16,
    pub duration_ms: u64,
    pub peak: f32,
    pub rms: f64,
    pub clipped_samples: u64,
    pub mostly_silent: bool,
    /// Captured audio remains valid when the bounded recording duration/frame cap finishes it.
    #[serde(default)]
    pub limit_reached: bool,
    /// Recoverable backend buffer discontinuities; no audio content is retained here.
    #[serde(default)]
    pub discontinuities: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaptureCompletion {
    Running,
    Finished,
}

#[derive(Debug)]
pub struct CapturedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub diagnostics: AudioDiagnostics,
}

/// Short-window RMS of the active capture, readable without the capture lock.
/// Holds 0 while nothing is recording. Contains no audio content.
#[derive(Clone, Debug, Default)]
pub struct LevelMeter(Arc<AtomicU32>);

impl LevelMeter {
    pub fn level(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
    fn set(&self, value: f32) {
        self.0.store(value.to_bits(), Ordering::Relaxed);
    }
}

/// Accumulates squared samples and publishes their RMS every window.
struct LevelWindow {
    meter: LevelMeter,
    energy: f64,
    count: usize,
    size: usize,
}

impl LevelWindow {
    fn new(meter: LevelMeter, sample_rate: u32) -> Self {
        meter.set(0.0);
        Self {
            meter,
            energy: 0.0,
            count: 0,
            size: (sample_rate as usize / 25).max(1),
        }
    }
    fn add(&mut self, sample: f32) {
        self.energy += f64::from(sample) * f64::from(sample);
        self.count += 1;
        if self.count >= self.size {
            self.meter
                .set((self.energy / self.count as f64).sqrt() as f32);
            self.energy = 0.0;
            self.count = 0;
        }
    }
}

impl Drop for LevelWindow {
    fn drop(&mut self) {
        self.meter.set(0.0);
    }
}

/// Capture ownership is explicit: Stop finalizes, Cancel discards, Drop closes the stream.
#[derive(Default)]
pub struct AudioCapture {
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    active: Option<native::CaptureSession>,
    meter: LevelMeter,
}

impl AudioCapture {
    pub fn new() -> Self {
        Self::default()
    }
    /// A shared handle to the live input level; valid across recordings.
    pub fn level_meter(&self) -> LevelMeter {
        self.meter.clone()
    }
    pub fn enumerate_devices() -> Result<Vec<InputDevice>, PlatformError> {
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        {
            // The audio backend enters a single-threaded COM apartment on its calling thread.
            // Keep that off shared worker threads, which later run UI Automation.
            std::thread::Builder::new()
                .name("onda-device-list".into())
                .spawn(native::enumerate_devices)
                .map_err(|error| PlatformError::Audio(error.to_string()))?
                .join()
                .map_err(|_| PlatformError::Audio("device enumeration stopped".into()))?
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            Err(PlatformError::UnsupportedPlatform)
        }
    }
    /// Resolve the default device anew per recording. Explicit missing devices fail loudly.
    pub fn start(&mut self, options: CaptureOptions) -> Result<(), PlatformError> {
        if !(1..=MAX_CAPTURE_SECONDS).contains(&options.max_duration_seconds) {
            return Err(PlatformError::InvalidParameter(
                "capture duration must be 1–600 seconds".into(),
            ));
        }
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        {
            if self.active.is_some() {
                return Err(PlatformError::AlreadyRecording);
            }
            self.active = Some(native::start(options, self.meter.clone())?);
            Ok(())
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            Err(PlatformError::UnsupportedPlatform)
        }
    }
    pub fn stop(&mut self) -> Result<CapturedAudio, PlatformError> {
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        {
            self.active
                .take()
                .ok_or(PlatformError::NotRecording)?
                .finish(false)
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            Err(PlatformError::UnsupportedPlatform)
        }
    }
    /// Nonblocking completion observation; finished audio/errors remain owned until Stop/Cancel.
    pub fn poll_completion(&mut self) -> Result<CaptureCompletion, PlatformError> {
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        {
            self.active
                .as_mut()
                .ok_or(PlatformError::NotRecording)?
                .poll_completion()
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            Err(PlatformError::UnsupportedPlatform)
        }
    }
    pub fn cancel(&mut self) -> Result<(), PlatformError> {
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        {
            let Some(session) = self.active.take() else {
                return Ok(());
            };
            session.finish(true).map(|_| ())
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            Err(PlatformError::UnsupportedPlatform)
        }
    }
    pub fn is_recording(&self) -> bool {
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        {
            self.active.is_some()
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            false
        }
    }
}
impl Drop for AudioCapture {
    fn drop(&mut self) {
        let _ = self.cancel();
    }
}

/// Band-limited FFT resampling avoids aliasing, trims filter delay and preserves exact duration.
// Time O(n log c), c = bounded FFT chunk size; space O(n + c).
pub fn resample_to_16khz(input: &[f32], input_rate: u32) -> Result<Vec<f32>, PlatformError> {
    if input_rate == 0
        || input_rate > MAX_SOURCE_SAMPLE_RATE
        || input.iter().any(|sample| !sample.is_finite())
    {
        return Err(PlatformError::Audio(
            "invalid sample rate or non-finite samples".into(),
        ));
    }
    if input.is_empty() {
        return Ok(Vec::new());
    }
    if input_rate == OUTPUT_SAMPLE_RATE {
        return Ok(input.to_vec());
    }
    let mut resampler = FftFixedInOut::<f32>::new(
        input_rate as usize,
        OUTPUT_SAMPLE_RATE as usize,
        RESAMPLE_CHUNK_SIZE,
        1,
    )
    .map_err(|error| PlatformError::Audio(error.to_string()))?;
    let delay = resampler.output_delay();
    let wanted = (input.len() as u64 * OUTPUT_SAMPLE_RATE as u64 / input_rate as u64) as usize;
    let mut output = Vec::with_capacity(wanted + delay + resampler.output_frames_max());
    let mut position = 0;
    while position < input.len() || output.len() < wanted + delay {
        let frames = resampler.input_frames_next();
        let end = (position + frames).min(input.len());
        let mut block = vec![0.0; frames];
        if position < input.len() {
            block[..end - position].copy_from_slice(&input[position..end]);
        }
        let resampled = resampler
            .process(&[block], None)
            .map_err(|error| PlatformError::Audio(error.to_string()))?;
        output.extend_from_slice(&resampled[0]);
        position = end;
    }
    output.drain(..delay);
    output.truncate(wanted);
    clamp_overshoot(&mut output);
    Ok(output)
}

/// Resampling a clipped recording rings slightly past full scale; the speech workers accept
/// only samples within ±1, so the overshoot is cut back to it.
fn clamp_overshoot(samples: &mut [f32]) {
    for sample in samples {
        *sample = sample.clamp(-1.0, 1.0);
    }
}

fn diagnostics(samples: &[f32], sample_rate: u32, channels: u16) -> AudioDiagnostics {
    let peak = samples
        .iter()
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    let energy: f64 = samples.iter().map(|sample| (*sample as f64).powi(2)).sum();
    let rms = (energy / samples.len().max(1) as f64).sqrt();
    AudioDiagnostics {
        source_sample_rate: sample_rate,
        source_channels: channels,
        duration_ms: samples.len() as u64 * 1000 / sample_rate as u64,
        peak,
        rms,
        clipped_samples: samples
            .iter()
            .filter(|sample| sample.abs() >= CLIPPING_THRESHOLD)
            .count() as u64,
        mostly_silent: rms < SILENCE_RMS_THRESHOLD,
        limit_reached: false,
        discontinuities: 0,
    }
}

/// Native-rate frames are resampled incrementally; only 16 kHz output stays resident.
struct RecordingBuffer {
    output: Vec<f32>,
    source_rate: u32,
    channels: u16,
    source_frames: usize,
    source_limit: usize,
    peak: f32,
    energy: f64,
    clipped: u64,
    resampler: Option<FftFixedInOut<f32>>,
    pending: Vec<Vec<f32>>,
    resampled: Vec<Vec<f32>>,
    delay_remaining: usize,
    limit_reached: bool,
}

impl RecordingBuffer {
    fn new(source_rate: u32, channels: u16, seconds: u32) -> Result<Self, PlatformError> {
        let resampler = if source_rate == OUTPUT_SAMPLE_RATE {
            None
        } else {
            Some(
                FftFixedInOut::new(
                    source_rate as usize,
                    OUTPUT_SAMPLE_RATE as usize,
                    RESAMPLE_CHUNK_SIZE,
                    1,
                )
                .map_err(|error| PlatformError::Audio(error.to_string()))?,
            )
        };
        let input_size = resampler.as_ref().map_or(0, Resampler::input_frames_next);
        let output_size = resampler.as_ref().map_or(0, Resampler::output_frames_max);
        let delay_remaining = resampler.as_ref().map_or(0, Resampler::output_delay);
        let mut output = Vec::new();
        output
            .try_reserve_exact(OUTPUT_SAMPLE_RATE as usize * seconds as usize + output_size)
            .map_err(|_| PlatformError::Audio("recording memory unavailable".into()))?;
        Ok(Self {
            output,
            source_rate,
            channels,
            source_frames: 0,
            source_limit: source_rate as usize * seconds as usize,
            peak: 0.0,
            energy: 0.0,
            clipped: 0,
            resampler,
            pending: vec![Vec::with_capacity(input_size)],
            resampled: vec![vec![0.0; output_size]],
            delay_remaining,
            limit_reached: false,
        })
    }

    fn push(&mut self, sample: f32) -> Result<(), PlatformError> {
        if self.source_frames >= self.source_limit {
            return Err(PlatformError::RecordingLimit);
        }
        self.source_frames += 1;
        self.peak = self.peak.max(sample.abs());
        self.energy += (sample as f64) * (sample as f64);
        if sample.abs() >= CLIPPING_THRESHOLD {
            self.clipped += 1;
        }
        let Some(resampler) = self.resampler.as_ref() else {
            self.output.push(sample);
            return Ok(());
        };
        let input_size = resampler.input_frames_next();
        self.pending[0].push(sample);
        if self.pending[0].len() == input_size {
            self.process_chunk()?;
        }
        Ok(())
    }

    fn process_chunk(&mut self) -> Result<(), PlatformError> {
        let Some(resampler) = self.resampler.as_mut() else {
            return Ok(());
        };
        let (_, produced) = resampler
            .process_into_buffer(&self.pending, &mut self.resampled, None)
            .map_err(|error| PlatformError::Audio(error.to_string()))?;
        let skipped = self.delay_remaining.min(produced);
        self.delay_remaining -= skipped;
        self.output
            .extend_from_slice(&self.resampled[0][skipped..produced]);
        self.pending[0].clear();
        Ok(())
    }

    fn finish(mut self) -> Result<CapturedAudio, PlatformError> {
        let wanted = (self.source_frames as u64 * OUTPUT_SAMPLE_RATE as u64
            / self.source_rate as u64) as usize;
        while self.output.len() < wanted {
            let Some(resampler) = self.resampler.as_ref() else {
                break;
            };
            self.pending[0].resize(resampler.input_frames_next(), 0.0);
            self.process_chunk()?;
        }
        self.output.truncate(wanted);
        clamp_overshoot(&mut self.output);
        let rms = (self.energy / self.source_frames.max(1) as f64).sqrt();
        let diagnostics = AudioDiagnostics {
            source_sample_rate: self.source_rate,
            source_channels: self.channels,
            duration_ms: self.source_frames as u64 * 1000 / self.source_rate as u64,
            peak: self.peak,
            rms,
            clipped_samples: self.clipped,
            mostly_silent: rms < SILENCE_RMS_THRESHOLD,
            limit_reached: self.limit_reached,
            discontinuities: 0,
        };
        Ok(CapturedAudio {
            samples: self.output,
            sample_rate: OUTPUT_SAMPLE_RATE,
            diagnostics,
        })
    }
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
mod native;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resampler_preserves_duration_and_speech_frequency() {
        let input: Vec<f32> = (0..48_000)
            .map(|index| {
                (2.0 * std::f32::consts::PI * 440.0 * index as f32 / 48_000.0).sin() * 0.25
            })
            .collect();
        let output = resample_to_16khz(&input, 48_000).unwrap();
        assert_eq!(output.len(), 16_000);
        let rms = diagnostics(&output[1000..15000], 16000, 1).rms;
        assert!((rms - 0.1768).abs() < 0.005);
    }
    #[test]
    fn resampler_rejects_nonfinite_and_attenuates_aliases() {
        assert!(resample_to_16khz(&[f32::NAN], 48_000).is_err());
        let input: Vec<f32> = (0..48_000)
            .map(|index| (2.0 * std::f32::consts::PI * 12_000.0 * index as f32 / 48_000.0).sin())
            .collect();
        let output = resample_to_16khz(&input, 48_000).unwrap();
        assert!(diagnostics(&output[1000..15000], 16000, 1).rms < 0.01);
    }
    #[test]
    fn clipped_input_resamples_within_full_scale() {
        // A loud source clipped to a square wave: band-limiting it rings past ±1.
        let input: Vec<f32> = (0..44_100)
            .map(|index| if (index / 50) % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let batch = resample_to_16khz(&input, 44_100).unwrap();
        let mut recording = RecordingBuffer::new(44_100, 1, 2).unwrap();
        for &sample in &input {
            recording.push(sample).unwrap();
        }
        let live = recording.finish().unwrap().samples;
        for output in [batch, live] {
            assert!(output.iter().all(|sample| sample.abs() <= 1.0));
            assert!(output.iter().any(|sample| sample.abs() > 0.99));
        }
    }
    #[test]
    fn silence_and_clipping_diagnostics_are_content_free() {
        assert!(diagnostics(&[0.0; 100], 16000, 1).mostly_silent);
        assert_eq!(diagnostics(&[-1.0, 0.0, 1.0], 16000, 1).clipped_samples, 2);
    }
    /// The app decides "no speech" from these diagnostics after Stop, so they are checked on the
    /// recording path (`RecordingBuffer::finish`), at a native rate that goes through resampling.
    #[test]
    fn a_finished_recording_reports_silence_speech_and_clipping() {
        fn record(samples: impl IntoIterator<Item = f32>) -> AudioDiagnostics {
            let mut recording = RecordingBuffer::new(48_000, 1, 2).unwrap();
            for sample in samples {
                recording.push(sample).unwrap();
            }
            recording.finish().unwrap().diagnostics
        }
        let tone = |amplitude: f32| {
            (0..48_000).map(move |index| {
                (2.0 * std::f32::consts::PI * 220.0 * index as f32 / 48_000.0).sin() * amplitude
            })
        };

        let silence = record(std::iter::repeat_n(0.0, 48_000));
        assert!(silence.mostly_silent, "silence must report no speech");
        assert_eq!(silence.rms, 0.0);
        assert_eq!(silence.clipped_samples, 0);
        assert_eq!(silence.duration_ms, 1000);

        let hiss = record(tone(0.0005));
        assert!(hiss.mostly_silent, "a faint hiss is not speech");

        let speech = record(tone(0.1));
        assert!(
            !speech.mostly_silent,
            "a speech-level tone must count as speech"
        );
        assert!((speech.rms - 0.0707).abs() < 0.001, "rms {}", speech.rms);
        assert_eq!(speech.clipped_samples, 0);

        // Full scale either way counts as clipped; just under the threshold does not.
        let clipped = record(tone(0.25).chain([1.0, -1.0, 0.9995, -0.9995, 0.998, -0.998]));
        assert!(!clipped.mostly_silent);
        assert_eq!(clipped.clipped_samples, 4);
        assert_eq!(clipped.peak, 1.0);
        assert_eq!(clipped.source_sample_rate, 48_000);
    }
    #[test]
    fn incremental_resampling_matches_batch_without_native_rate_retention() {
        let input: Vec<f32> = (0..12_345)
            .map(|index| (index as f32 * 0.03).sin() * 0.2)
            .collect();
        let mut recording = RecordingBuffer::new(48_000, 2, 1).unwrap();
        for &sample in &input {
            recording.push(sample).unwrap();
        }
        let actual = recording.finish().unwrap();
        let expected = resample_to_16khz(&input, 48_000).unwrap();
        assert_eq!(actual.samples.len(), expected.len());
        assert!(actual
            .samples
            .iter()
            .zip(expected)
            .all(|(actual, expected)| (actual - expected).abs() < 0.00001));
        assert_eq!(actual.diagnostics.source_channels, 2);
    }
}
