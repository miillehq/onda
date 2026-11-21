use super::*;

/// Keep sample-format dispatch outside the real-time capture loop.
pub(super) fn configured_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    producer: Producer<f32>,
    notifications: &Arc<StreamNotifications>,
    overflow: &Arc<AtomicBool>,
) -> Result<cpal::Stream, PlatformError> {
    macro_rules! stream {
        ($sample:ty) => {
            build_stream::<$sample>(
                device,
                config,
                producer,
                Arc::clone(notifications),
                Arc::clone(overflow),
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
    Ok(stream)
}
