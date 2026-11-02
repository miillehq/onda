//! Process boundary keeps ASR and WebView starvation away from the keyboard hook, or on Linux
//! the desktop portal session that reports shortcuts.
#[cfg(target_os = "linux")]
mod linux_portal;
#[cfg(windows)]
mod windows_hook;

fn main() {
    #[cfg(windows)]
    let result = windows_hook::run();
    #[cfg(target_os = "linux")]
    let result = linux_portal::run();
    #[cfg(not(any(windows, target_os = "linux")))]
    let result: Result<(), onda_input::InputError> =
        Err(onda_input::InputError::Helper("unsupported".into()));
    if let Err(error) = result {
        use std::io::Write;
        // On Linux the code is "unsupported" (no shortcuts portal) or "declined" (the user
        // dismissed the approval), which the application tells apart from a failure.
        let code = match error {
            onda_input::InputError::Helper(code) if cfg!(not(windows)) => code,
            error => error.to_string(),
        };
        let event = onda_input::HelperEvent::Error { code };
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = writeln!(std::io::stdout(), "{json}");
        }
        std::process::exit(1);
    }
}
