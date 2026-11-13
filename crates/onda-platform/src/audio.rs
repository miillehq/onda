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
    #[cfg(any(windows, target_os = "linux"))]
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
        #[cfg(any(windows, target_os = "linux"))]
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
        #[cfg(not(any(windows, target_os = "linux")))]
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
        #[cfg(any(windows, target_os = "linux"))]
        {
            if self.active.is_some() {
                return Err(PlatformError::AlreadyRecording);
            }
            self.active = Some(native::start(options, self.meter.clone())?);
            Ok(())
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            Err(PlatformError::UnsupportedPlatform)
        }
    }
    pub fn stop(&mut self) -> Result<CapturedAudio, PlatformError> {
        #[cfg(any(windows, target_os = "linux"))]
        {
            self.active
                .take()
                .ok_or(PlatformError::NotRecording)?
                .finish(false)
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            Err(PlatformError::UnsupportedPlatform)
        }
    }
    /// Nonblocking completion observation; finished audio/errors remain owned until Stop/Cancel.
    pub fn poll_completion(&mut self) -> Result<CaptureCompletion, PlatformError> {
        #[cfg(any(windows, target_os = "linux"))]
        {
            self.active
                .as_mut()
                .ok_or(PlatformError::NotRecording)?
                .poll_completion()
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            Err(PlatformError::UnsupportedPlatform)
        }
    }
    pub fn cancel(&mut self) -> Result<(), PlatformError> {
        #[cfg(any(windows, target_os = "linux"))]
        {
            let Some(session) = self.active.take() else {
                return Ok(());
            };
            session.finish(true).map(|_| ())
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            Err(PlatformError::UnsupportedPlatform)
        }
    }
    pub fn is_recording(&self) -> bool {
        #[cfg(any(windows, target_os = "linux"))]
        {
            self.active.is_some()
        }
        #[cfg(not(any(windows, target_os = "linux")))]
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

#[cfg(any(windows, target_os = "linux"))]
mod native {
    use super::*;
    use crate::AudioBackendFailure;
    use cpal::{
        traits::{DeviceTrait, HostTrait, StreamTrait},
        ErrorKind, FromSample, Sample, SampleFormat, SizedSample,
    };
    use rtrb::{Consumer, Producer, RingBuffer};
    use std::{
        sync::{
            atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
            mpsc::{self, Receiver, TryRecvError},
            Arc,
        },
        thread::{self, JoinHandle},
        time::{Duration, Instant},
    };
    const TRANSPORT_SECONDS: usize = 2;
    const WORKER_INTERVAL: Duration = Duration::from_millis(5);
    const WORKER_TIMEOUT: Duration = Duration::from_secs(10);
    const NO_STREAM_FAILURE: u8 = 0;

    /// Fatal callback categories have compact codes so the real-time thread never allocates.
    #[derive(Clone, Copy)]
    #[repr(u8)]
    enum StreamFailure {
        Disconnected = 1,
        RouteChanged,
        PermissionDenied,
        DeviceBusy,
        HostUnavailable,
        InvalidInput,
        ResourceExhausted,
        UnsupportedConfig,
        UnsupportedOperation,
        BackendError,
        Other,
    }

    impl StreamFailure {
        fn from_kind(kind: ErrorKind) -> Self {
            match kind {
                ErrorKind::DeviceNotAvailable => Self::Disconnected,
                ErrorKind::DeviceChanged | ErrorKind::StreamInvalidated => Self::RouteChanged,
                ErrorKind::PermissionDenied => Self::PermissionDenied,
                ErrorKind::DeviceBusy => Self::DeviceBusy,
                ErrorKind::HostUnavailable => Self::HostUnavailable,
                ErrorKind::InvalidInput => Self::InvalidInput,
                ErrorKind::ResourceExhausted => Self::ResourceExhausted,
                ErrorKind::UnsupportedConfig => Self::UnsupportedConfig,
                ErrorKind::UnsupportedOperation => Self::UnsupportedOperation,
                ErrorKind::BackendError => Self::BackendError,
                _ => Self::Other,
            }
        }

        fn into_error(self) -> PlatformError {
            match self {
                Self::Disconnected => PlatformError::DeviceDisconnected,
                Self::RouteChanged => PlatformError::AudioRouteChanged,
                Self::PermissionDenied => PlatformError::AudioPermissionDenied,
                Self::DeviceBusy => {
                    PlatformError::AudioBackendFailed(AudioBackendFailure::DeviceBusy)
                }
                Self::HostUnavailable => {
                    PlatformError::AudioBackendFailed(AudioBackendFailure::HostUnavailable)
                }
                Self::InvalidInput => {
                    PlatformError::AudioBackendFailed(AudioBackendFailure::InvalidInput)
                }
                Self::ResourceExhausted => {
                    PlatformError::AudioBackendFailed(AudioBackendFailure::ResourceExhausted)
                }
                Self::UnsupportedConfig => {
                    PlatformError::AudioBackendFailed(AudioBackendFailure::UnsupportedConfig)
                }
                Self::UnsupportedOperation => {
                    PlatformError::AudioBackendFailed(AudioBackendFailure::UnsupportedOperation)
                }
                Self::BackendError => {
                    PlatformError::AudioBackendFailed(AudioBackendFailure::BackendError)
                }
                Self::Other => PlatformError::AudioBackendFailed(AudioBackendFailure::Other),
            }
        }

        fn from_code(code: u8) -> Option<Self> {
            match code {
                NO_STREAM_FAILURE => None,
                value if value == Self::Disconnected as u8 => Some(Self::Disconnected),
                value if value == Self::RouteChanged as u8 => Some(Self::RouteChanged),
                value if value == Self::PermissionDenied as u8 => Some(Self::PermissionDenied),
                value if value == Self::DeviceBusy as u8 => Some(Self::DeviceBusy),
                value if value == Self::HostUnavailable as u8 => Some(Self::HostUnavailable),
                value if value == Self::InvalidInput as u8 => Some(Self::InvalidInput),
                value if value == Self::ResourceExhausted as u8 => Some(Self::ResourceExhausted),
                value if value == Self::UnsupportedConfig as u8 => Some(Self::UnsupportedConfig),
                value if value == Self::UnsupportedOperation as u8 => {
                    Some(Self::UnsupportedOperation)
                }
                value if value == Self::BackendError as u8 => Some(Self::BackendError),
                _ => Some(Self::Other),
            }
        }
    }

    /// Callback state is lock-free; the first fatal failure survives later notifications.
    #[derive(Default)]
    struct StreamNotifications {
        fatal: AtomicU8,
        discontinuities: AtomicU64,
    }

    impl StreamNotifications {
        fn notify(&self, kind: ErrorKind) {
            match kind {
                ErrorKind::Xrun => {
                    self.discontinuities.fetch_add(1, Ordering::Relaxed);
                }
                ErrorKind::RealtimeDenied => {}
                _ => {
                    // Freezing the input endpoint prevents dictation silently switching microphones.
                    let failure = StreamFailure::from_kind(kind);
                    let _ = self.fatal.compare_exchange(
                        NO_STREAM_FAILURE,
                        failure as u8,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    );
                }
            }
        }

        fn failure(&self) -> Option<PlatformError> {
            StreamFailure::from_code(self.fatal.load(Ordering::Acquire))
                .map(StreamFailure::into_error)
        }

        fn finish(&self, recording: RecordingBuffer) -> Result<CapturedAudio, PlatformError> {
            let mut audio = recording.finish()?;
            audio.diagnostics.discontinuities = self.discontinuities.load(Ordering::Acquire);
            Ok(audio)
        }
    }

    pub(super) struct CaptureSession {
        stop: Arc<AtomicBool>,
        cancel: Arc<AtomicBool>,
        result: Receiver<Result<CapturedAudio, PlatformError>>,
        worker: Option<JoinHandle<()>>,
        completed: Option<Result<CapturedAudio, PlatformError>>,
    }
    impl CaptureSession {
        pub(super) fn poll_completion(&mut self) -> Result<CaptureCompletion, PlatformError> {
            if self.completed.is_none() {
                match self.result.try_recv() {
                    Ok(result) => self.completed = Some(result),
                    Err(TryRecvError::Empty) => return Ok(CaptureCompletion::Running),
                    Err(TryRecvError::Disconnected) => {
                        self.completed = Some(Err(PlatformError::Audio(
                            "audio worker terminated without a completion result".into(),
                        )))
                    }
                }
            }
            match self.completed.as_ref() {
                Some(Ok(_)) => Ok(CaptureCompletion::Finished),
                Some(Err(error)) => Err(error.clone()),
                None => Ok(CaptureCompletion::Running),
            }
        }
        pub(super) fn finish(mut self, cancel: bool) -> Result<CapturedAudio, PlatformError> {
            self.cancel.store(cancel, Ordering::Release);
            self.stop.store(true, Ordering::Release);
            let result = match self.completed.take() {
                Some(result) => result,
                None => self
                    .result
                    .recv_timeout(WORKER_TIMEOUT)
                    .map_err(|_| PlatformError::Timeout)?,
            };
            if let Some(worker) = self.worker.take() {
                worker
                    .join()
                    .map_err(|_| PlatformError::Audio("audio worker terminated".into()))?;
            }
            result
        }
    }
    impl Drop for CaptureSession {
        fn drop(&mut self) {
            self.cancel.store(true, Ordering::Release);
            self.stop.store(true, Ordering::Release);
        }
    }

    pub(super) fn enumerate_devices() -> Result<Vec<InputDevice>, PlatformError> {
        let host = cpal::default_host();
        let default_id = host
            .default_input_device()
            .and_then(|device| device.id().ok());
        let mut devices = Vec::new();
        for device in host.input_devices().map_err(cpal_error)? {
            let id = device.id().map_err(cpal_error)?;
            let name = device.description().map_err(cpal_error)?.name().to_owned();
            devices.push(InputDevice {
                id: id.to_string(),
                is_default: default_id.as_ref() == Some(&id),
                name,
            });
        }
        Ok(devices)
    }

    pub(super) fn start(
        options: CaptureOptions,
        meter: LevelMeter,
    ) -> Result<CaptureSession, PlatformError> {
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = Arc::new(AtomicBool::new(false));
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let (result_sender, result) = mpsc::sync_channel(1);
        let worker_stop = Arc::clone(&stop);
        let worker_cancel = Arc::clone(&cancel);
        let worker = thread::Builder::new()
            .name("onda-microphone".into())
            .spawn(move || {
                let result = record(options, meter, worker_stop, worker_cancel, &ready_sender);
                let _ = ready_sender.try_send(result.as_ref().map(|_| ()).map_err(Clone::clone));
                let _ = result_sender.send(result);
            })
            .map_err(audio_error)?;
        let session = CaptureSession {
            stop,
            cancel,
            result,
            worker: Some(worker),
            completed: None,
        };
        await_ready(session, ready_receiver)
    }

    fn await_ready(
        mut session: CaptureSession,
        ready_receiver: Receiver<Result<(), PlatformError>>,
    ) -> Result<CaptureSession, PlatformError> {
        match ready_receiver.recv_timeout(WORKER_TIMEOUT) {
            Ok(Ok(())) => Ok(session),
            Ok(Err(error)) => {
                session.stop.store(true, Ordering::Release);
                if let Some(worker) = session.worker.take() {
                    let _ = worker.join();
                }
                Err(error)
            }
            Err(_) => Err(PlatformError::Timeout),
        }
    }

    fn record(
        options: CaptureOptions,
        meter: LevelMeter,
        stop: Arc<AtomicBool>,
        cancel: Arc<AtomicBool>,
        ready: &mpsc::SyncSender<Result<(), PlatformError>>,
    ) -> Result<CapturedAudio, PlatformError> {
        let host = cpal::default_host();
        let device = match options.device_id {
            None => host
                .default_input_device()
                .ok_or(PlatformError::DeviceDisconnected)?,
            Some(id) => {
                let id: cpal::DeviceId = id.parse().map_err(audio_error)?;
                host.device_by_id(&id)
                    .filter(|device| device.supports_input())
                    .ok_or(PlatformError::DeviceDisconnected)?
            }
        };
        let config = device.default_input_config().map_err(cpal_error)?;
        let sample_rate = config.sample_rate();
        let channels = config.channels();
        if sample_rate == 0 || sample_rate > MAX_SOURCE_SAMPLE_RATE || channels == 0 {
            return Err(PlatformError::Audio(
                "unsupported device configuration".into(),
            ));
        }
        let notifications = Arc::new(StreamNotifications::default());
        let overflow = Arc::new(AtomicBool::new(false));
        let (producer, mut consumer) =
            RingBuffer::<f32>::new(sample_rate as usize * TRANSPORT_SECONDS);
        macro_rules! stream {
            ($sample:ty) => {
                build_stream::<$sample>(
                    &device,
                    &config,
                    producer,
                    Arc::clone(&notifications),
                    Arc::clone(&overflow),
                )?
            };
        }
        let stream = match config.sample_format() {
            SampleFormat::I8 => stream!(i8),
            SampleFormat::I16 => stream!(i16),
            SampleFormat::I24 => stream!(cpal::I24),
            SampleFormat::I32 => stream!(i32),
            SampleFormat::I64 => stream!(i64),
            SampleFormat::U8 => stream!(u8),
            SampleFormat::U16 => stream!(u16),
            SampleFormat::U24 => stream!(cpal::U24),
            SampleFormat::U32 => stream!(u32),
            SampleFormat::U64 => stream!(u64),
            SampleFormat::F32 => stream!(f32),
            SampleFormat::F64 => stream!(f64),
            _ => {
                return Err(PlatformError::Audio(
                    "unsupported microphone sample format".into(),
                ))
            }
        };
        let mut samples =
            RecordingBuffer::new(sample_rate, channels, options.max_duration_seconds)?;
        let mut level = LevelWindow::new(meter, sample_rate);
        stream.play().map_err(cpal_error)?;
        let _ = ready.send(Ok(()));
        let started = Instant::now();
        while !stop.load(Ordering::Acquire) {
            drain(&mut consumer, &mut samples, &mut level)?;
            if let Some(error) = notifications.failure() {
                return Err(error);
            }
            if overflow.load(Ordering::Acquire) {
                return Err(PlatformError::AudioOverflow);
            }
            if started.elapsed() >= Duration::from_secs(options.max_duration_seconds as u64) {
                samples.limit_reached = true;
            }
            if samples.limit_reached {
                break;
            }
            thread::sleep(WORKER_INTERVAL);
        }
        drop(stream);
        if cancel.load(Ordering::Acquire) {
            return Ok(CapturedAudio {
                samples: Vec::new(),
                sample_rate: OUTPUT_SAMPLE_RATE,
                diagnostics: diagnostics(&[], sample_rate, channels),
            });
        }
        if !samples.limit_reached {
            if let Some(error) = notifications.failure() {
                return Err(error);
            }
        }
        if !samples.limit_reached && overflow.load(Ordering::Acquire) {
            return Err(PlatformError::AudioOverflow);
        }
        drain(&mut consumer, &mut samples, &mut level)?;
        drop(level);
        notifications.finish(samples)
    }

    fn drain(
        consumer: &mut Consumer<f32>,
        samples: &mut RecordingBuffer,
        level: &mut LevelWindow,
    ) -> Result<(), PlatformError> {
        while samples.source_frames < samples.source_limit {
            let Ok(sample) = consumer.pop() else {
                return Ok(());
            };
            level.add(sample);
            samples.push(sample)?;
        }
        samples.limit_reached = true;
        Ok(())
    }
    fn build_stream<T>(
        device: &cpal::Device,
        config: &cpal::SupportedStreamConfig,
        mut producer: Producer<f32>,
        notifications: Arc<StreamNotifications>,
        overflow: Arc<AtomicBool>,
    ) -> Result<cpal::Stream, PlatformError>
    where
        T: Sample + SizedSample + Copy + Send + 'static,
        f32: FromSample<T>,
    {
        let channels = config.channels() as usize;
        device
            .build_input_stream(
                (*config).into(),
                move |data: &[T], _| {
                    for frame in data.chunks_exact(channels) {
                        let mono = frame
                            .iter()
                            .map(|sample| sample.to_sample::<f32>())
                            .sum::<f32>()
                            / channels as f32;
                        if !mono.is_finite() || producer.push(mono.clamp(-1.0, 1.0)).is_err() {
                            overflow.store(true, Ordering::Release);
                            return;
                        }
                    }
                },
                move |error| notifications.notify(error.kind()),
                None,
            )
            .map_err(cpal_error)
    }

    fn cpal_error(error: cpal::Error) -> PlatformError {
        StreamFailure::from_kind(error.kind()).into_error()
    }

    fn audio_error(error: impl std::fmt::Display) -> PlatformError {
        PlatformError::Audio(error.to_string())
    }

    #[cfg(test)]
    mod completion_tests {
        use super::*;

        fn session() -> (
            CaptureSession,
            mpsc::SyncSender<Result<CapturedAudio, PlatformError>>,
        ) {
            let (sender, result) = mpsc::sync_channel(1);
            (
                CaptureSession {
                    stop: Arc::new(AtomicBool::new(false)),
                    cancel: Arc::new(AtomicBool::new(false)),
                    result,
                    worker: None,
                    completed: None,
                },
                sender,
            )
        }
        fn fixture_audio() -> CapturedAudio {
            CapturedAudio {
                samples: vec![0.2, -0.1, 0.3],
                sample_rate: OUTPUT_SAMPLE_RATE,
                diagnostics: diagnostics(&[0.2, -0.1, 0.3], OUTPUT_SAMPLE_RATE, 1),
            }
        }

        #[test]
        fn repeated_completion_poll_preserves_audio_for_stop() {
            let (mut capture, sender) = session();
            assert_eq!(
                capture.poll_completion().unwrap(),
                CaptureCompletion::Running
            );
            sender.send(Ok(fixture_audio())).unwrap();
            assert_eq!(
                capture.poll_completion().unwrap(),
                CaptureCompletion::Finished
            );
            assert_eq!(
                capture.poll_completion().unwrap(),
                CaptureCompletion::Finished
            );
            drop(sender);
            let audio = capture.finish(false).unwrap();
            assert_eq!(audio.samples, [0.2, -0.1, 0.3]);
            assert!(!audio.diagnostics.limit_reached);
        }

        #[test]
        fn disconnect_is_immediately_observable_and_original_failure_survives_stop() {
            let (mut capture, sender) = session();
            let notifications = StreamNotifications::default();
            notifications.notify(ErrorKind::DeviceNotAvailable);
            sender.send(Err(notifications.failure().unwrap())).unwrap();
            drop(sender);
            for _ in 0..2 {
                assert!(matches!(
                    capture.poll_completion(),
                    Err(PlatformError::DeviceDisconnected)
                ));
            }
            assert!(matches!(
                capture.finish(false),
                Err(PlatformError::DeviceDisconnected)
            ));
        }

        #[test]
        fn worker_exit_without_result_is_detected_without_a_stop_timeout() {
            let (mut capture, sender) = session();
            drop(sender);
            assert!(matches!(
                capture.poll_completion(),
                Err(PlatformError::Audio(_))
            ));
            assert!(matches!(
                capture.finish(false),
                Err(PlatformError::Audio(_))
            ));
        }

        #[test]
        fn stop_of_running_session_signals_worker_and_collects_its_final_audio() {
            let (mut capture, sender) = session();
            let stop = Arc::clone(&capture.stop);
            capture.worker = Some(thread::spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(1));
                }
                sender.send(Ok(fixture_audio())).unwrap();
            }));
            assert_eq!(
                capture.poll_completion().unwrap(),
                CaptureCompletion::Running
            );
            assert_eq!(capture.finish(false).unwrap().samples, [0.2, -0.1, 0.3]);
        }

        #[test]
        fn recoverable_discontinuity_keeps_capture_running_and_survives_stop() {
            let (mut capture, sender) = session();
            let stop = Arc::clone(&capture.stop);
            capture.worker = Some(thread::spawn(move || {
                let notifications = StreamNotifications::default();
                let mut recording = RecordingBuffer::new(OUTPUT_SAMPLE_RATE, 1, 1).unwrap();
                recording.push(0.25).unwrap();
                notifications.notify(ErrorKind::Xrun);
                notifications.notify(ErrorKind::RealtimeDenied);
                assert!(notifications.failure().is_none());
                recording.push(-0.125).unwrap();
                recording.push(0.375).unwrap();
                while !stop.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(1));
                }
                sender.send(notifications.finish(recording)).unwrap();
            }));
            assert_eq!(
                capture.poll_completion().unwrap(),
                CaptureCompletion::Running
            );
            let audio = capture.finish(false).unwrap();
            assert_eq!(audio.samples, [0.25, -0.125, 0.375]);
            assert_eq!(audio.diagnostics.discontinuities, 1);
            assert!(!audio.diagnostics.limit_reached);
        }

        #[test]
        fn route_permission_and_backend_failures_remain_distinct_through_poll_and_stop() {
            for kind in [
                ErrorKind::DeviceChanged,
                ErrorKind::StreamInvalidated,
                ErrorKind::PermissionDenied,
                ErrorKind::BackendError,
            ] {
                let (mut capture, sender) = session();
                let notifications = StreamNotifications::default();
                notifications.notify(kind);
                sender.send(Err(notifications.failure().unwrap())).unwrap();
                let polled = capture.poll_completion().unwrap_err();
                let stopped = capture.finish(false).unwrap_err();
                for failure in [polled, stopped] {
                    match kind {
                        ErrorKind::DeviceChanged | ErrorKind::StreamInvalidated => {
                            assert!(matches!(failure, PlatformError::AudioRouteChanged));
                        }
                        ErrorKind::PermissionDenied => {
                            assert!(matches!(failure, PlatformError::AudioPermissionDenied));
                        }
                        ErrorKind::BackendError => {
                            assert!(matches!(
                                failure,
                                PlatformError::AudioBackendFailed(
                                    AudioBackendFailure::BackendError
                                )
                            ));
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }

        #[test]
        fn first_fatal_callback_survives_later_notifications() {
            let notifications = StreamNotifications::default();
            notifications.notify(ErrorKind::PermissionDenied);
            notifications.notify(ErrorKind::Xrun);
            notifications.notify(ErrorKind::StreamInvalidated);
            notifications.notify(ErrorKind::DeviceNotAvailable);
            assert!(matches!(
                notifications.failure(),
                Some(PlatformError::AudioPermissionDenied)
            ));
            assert_eq!(notifications.discontinuities.load(Ordering::Acquire), 1);
        }

        #[test]
        fn startup_failure_preserves_category_and_discards_backend_text() {
            let backend_error = cpal::Error::with_message(
                ErrorKind::PermissionDenied,
                "backend diagnostic must not enter application errors",
            );
            let (capture, _completion_sender) = session();
            let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
            ready_sender.send(Err(cpal_error(backend_error))).unwrap();
            let error = match await_ready(capture, ready_receiver) {
                Ok(_) => panic!("failed startup unexpectedly succeeded"),
                Err(error) => error,
            };
            assert!(matches!(error, PlatformError::AudioPermissionDenied));
            assert_eq!(error.to_string(), "microphone access was denied");
        }

        #[test]
        fn duration_frame_cap_keeps_valid_prefix_and_bounds_the_final_callback() {
            const EXTRA_CALLBACK_FRAMES: usize = 128;
            let frame_limit = OUTPUT_SAMPLE_RATE as usize;
            let (mut producer, mut consumer) = RingBuffer::new(frame_limit + EXTRA_CALLBACK_FRAMES);
            for _ in 0..frame_limit {
                producer.push(0.25).unwrap();
            }
            for _ in 0..EXTRA_CALLBACK_FRAMES {
                producer.push(0.95).unwrap();
            }
            let mut recording = RecordingBuffer::new(OUTPUT_SAMPLE_RATE, 1, 1).unwrap();
            let meter = LevelMeter::default();
            let mut level = LevelWindow::new(meter.clone(), OUTPUT_SAMPLE_RATE);
            drain(&mut consumer, &mut recording, &mut level).unwrap();
            assert!((meter.level() - 0.25).abs() < 1e-6);
            drop(level);
            assert_eq!(meter.level(), 0.0);
            assert!(recording.limit_reached);
            let audio = recording.finish().unwrap();
            assert_eq!(audio.samples.len(), frame_limit);
            assert_eq!(audio.diagnostics.duration_ms, 1000);
            assert!(audio.diagnostics.limit_reached);
            assert!(audio.samples.iter().all(|sample| *sample == 0.25));
            assert_eq!(audio.diagnostics.peak, 0.25);
        }
    }
}

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
