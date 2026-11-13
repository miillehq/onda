//! The macOS pasteboard: dictation waits there for ⌘V, and a paste Onda presses itself borrows it
//! and puts the earlier text back.
use crate::PlatformError;
use arboard::{Clipboard, SetExtApple};

/// Replaces the pasteboard text and marks it `org.nspasteboard.ConcealedType`, which clipboard
/// history apps honour by not keeping it, like Windows' and Linux's privacy markers.
pub fn copy_text(text: &str) -> Result<(), PlatformError> {
    Clipboard::new()
        .map_err(|_| PlatformError::Clipboard("the pasteboard is not available".into()))?
        .set()
        .exclude_from_history()
        .text(text.to_owned())
        .map_err(|_| PlatformError::Clipboard("the pasteboard refused the text".into()))
}

/// The pasteboard's text, or `None` when it holds none.
pub(crate) fn read_text() -> Option<String> {
    Clipboard::new().ok()?.get_text().ok()
}

pub(crate) fn clear() -> Result<(), PlatformError> {
    Clipboard::new()
        .and_then(|mut clipboard| clipboard.clear())
        .map_err(|_| PlatformError::Clipboard("the pasteboard could not be cleared".into()))
}
