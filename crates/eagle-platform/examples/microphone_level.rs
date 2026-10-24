//! Opt-in endpoint enumeration and a short level check; never prints or saves audio samples.
use eagle_platform::{AudioCapture, CaptureOptions};
use std::{error::Error, time::Duration};

const LEVEL_CHECK_DURATION: Duration = Duration::from_secs(3);
const CAPTURE_SAFETY_LIMIT_SECONDS: u32 = 10;
const USAGE: &str = "Usage: microphone_level list | level [device-id]";

/// Uses the same capture implementation as dictation without transcription or insertion.
fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next().unwrap_or_else(|| "list".into());
    let device_id = arguments.next();
    if arguments.next().is_some() {
        return Err(USAGE.into());
    }
    match command.as_str() {
        "list" if device_id.is_none() => {
            for device in AudioCapture::enumerate_devices()? {
                println!("{device:#?}");
            }
        }
        "level" => {
            let mut capture = AudioCapture::new();
            capture.start(CaptureOptions {
                device_id,
                max_duration_seconds: CAPTURE_SAFETY_LIMIT_SECONDS,
            })?;
            std::thread::sleep(LEVEL_CHECK_DURATION);
            let audio = capture.stop()?;
            println!("{:#?}", audio.diagnostics);
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}
