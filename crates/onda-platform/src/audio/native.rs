mod formats;
use super::*;
use crate::AudioBackendFailure;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    ErrorKind, FromSample, Sample, SampleFormat, SizedSample,
};
use formats::configured_stream;
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
            Self::DeviceBusy => PlatformError::AudioBackendFailed(AudioBackendFailure::DeviceBusy),
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
            value if value == Self::UnsupportedOperation as u8 => Some(Self::UnsupportedOperation),
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
        StreamFailure::from_code(self.fatal.load(Ordering::Acquire)).map(StreamFailure::into_error)
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
    let (producer, mut consumer) = RingBuffer::<f32>::new(sample_rate as usize * TRANSPORT_SECONDS);
    let stream = configured_stream(&device, &config, producer, &notifications, &overflow)?;
    let mut samples = RecordingBuffer::new(sample_rate, channels, options.max_duration_seconds)?;
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
                            PlatformError::AudioBackendFailed(AudioBackendFailure::BackendError)
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
