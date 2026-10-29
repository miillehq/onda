//! Sensitive desktop operations remain in the native backend, outside the renderer. Windows has
//! the full set; Linux has microphone capture, the clipboard and process measurements.
mod audio;
#[cfg(windows)]
mod clipboard;
#[cfg(all(windows, debug_assertions))]
pub mod debug_fixture;
mod desktop;
#[cfg(target_os = "linux")]
mod linux_clipboard;
mod processes;
mod protect;
mod target;
#[cfg(windows)]
mod windows_target;

pub use audio::{
    resample_to_16khz, AudioCapture, AudioDiagnostics, CaptureCompletion, CaptureOptions,
    CapturedAudio, InputDevice, LevelMeter, OUTPUT_SAMPLE_RATE,
};
pub use desktop::{
    apps_use_light_theme, hide_window, launch_at_login_registered, local_date, play_cue,
    set_caption_colors, set_launch_at_login, show_without_activation, system_uses_light_theme, Cue,
};
pub use processes::{process_tree, MemoryMeasure, ProcessSample};
pub use protect::{protect_user_data, unprotect_user_data};

/// Whether Onda can type into the focused field of another app. Without it (Linux), dictation
/// completes by putting its words on the clipboard for the user to paste.
pub const TYPES_INTO_APPS: bool = cfg!(windows);
pub use target::{
    ControlFingerprint, InsertionOutcome, PasteShortcut, TargetSnapshot, WindowsTargetService,
    MAX_TEXT_CHARACTERS,
};

/// Explicit user-requested copy; replaces clipboard content and sets privacy markers.
pub fn copy_text_to_clipboard(text: &str) -> Result<(), PlatformError> {
    WindowsTargetService::new().copy_to_clipboard(text)
}

/// Display a sanitized startup error even when the release executable has no console.
pub fn show_startup_error(message: &str) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        const STARTUP_ERROR_TITLE: &str = "Application startup error";
        const MAX_STARTUP_MESSAGE_CHARACTERS: usize = 4096;
        let title: Vec<u16> = STARTUP_ERROR_TITLE.encode_utf16().chain([0]).collect();
        let message: Vec<u16> = message
            .chars()
            .filter(|character| *character != '\0')
            .take(MAX_STARTUP_MESSAGE_CHARACTERS)
            .collect::<String>()
            .encode_utf16()
            .chain([0])
            .collect();
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                message.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("{message}");
    }
}

/// Stable, content-free categories of fatal audio backend failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AudioBackendFailure {
    #[error("audio device is busy")]
    DeviceBusy,
    #[error("audio subsystem is unavailable")]
    HostUnavailable,
    #[error("audio backend rejected its input")]
    InvalidInput,
    #[error("audio subsystem exhausted a resource")]
    ResourceExhausted,
    #[error("audio configuration is unsupported")]
    UnsupportedConfig,
    #[error("audio operation is unsupported")]
    UnsupportedOperation,
    #[error("audio backend failed")]
    BackendError,
    #[error("audio stream failed")]
    Other,
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("native operation was cancelled")]
    Cancelled,
    #[error("Windows is required for this native operation")]
    UnsupportedPlatform,
    #[error("audio capture failed: {0}")]
    Audio(String),
    #[error("audio capture already active")]
    AlreadyRecording,
    #[error("no audio capture is active")]
    NotRecording,
    #[error("microphone was disconnected or capture stopped unexpectedly")]
    DeviceDisconnected,
    #[error("microphone route changed during capture")]
    AudioRouteChanged,
    #[error("microphone access was denied")]
    AudioPermissionDenied,
    #[error("audio capture failed: {0}")]
    AudioBackendFailed(AudioBackendFailure),
    #[error("recording exceeded the configured duration")]
    RecordingLimit,
    #[error("audio transport overflow; recording is incomplete")]
    AudioOverflow,
    #[error("native operation timed out")]
    Timeout,
    #[error("foreground window or focused control changed")]
    TargetChanged,
    #[error("password or secure control is excluded")]
    SecureField,
    #[error("target integrity exceeds the application's integrity")]
    ElevatedTarget,
    #[error("focused control identity could not be verified: {0}")]
    TargetUnavailable(String),
    #[error("the focused control has no selected text")]
    EmptySelection,
    #[error("selected text is unavailable in this application's accessibility provider")]
    SelectionUnavailable,
    #[error("text exceeds the configured safety limit")]
    TextLimit,
    #[error("clipboard operation failed: {0}")]
    Clipboard(String),
    #[error("clipboard format {0} cannot be safely preserved")]
    UnsupportedClipboardFormat(u32),
    #[error("paste input could not be injected")]
    InjectionFailed,
    #[error("physical modifier keys are held; release them before insertion")]
    ModifiersHeld,
    #[error("invalid native operation parameter: {0}")]
    InvalidParameter(String),
    #[error("per-user data protection failed")]
    Protection,
}

/// Restrict implicit DLL lookup to system directories. Call once before optional native engines.
/// Windows-only: other systems have no current-directory library search to restrict.
pub fn harden_dll_search() -> Result<(), PlatformError> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::LibraryLoader::{
            SetDefaultDllDirectories, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
        };
        if unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS) } == 0 {
            return Err(PlatformError::TargetUnavailable(
                "DLL search hardening failed".into(),
            ));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}
