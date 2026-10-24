//! Process boundary keeps ASR and WebView starvation away from the keyboard hook.
#[cfg(windows)]
mod windows_hook;

fn main() {
    #[cfg(windows)]
    if let Err(error) = windows_hook::run() {
        use std::io::Write;
        let event = eagle_input::HelperEvent::Error {
            code: error.to_string(),
        };
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = writeln!(std::io::stdout(), "{json}");
        }
        std::process::exit(1);
    }
    #[cfg(not(windows))]
    {
        eprintln!("eagle-input requires Windows");
        std::process::exit(1);
    }
}
