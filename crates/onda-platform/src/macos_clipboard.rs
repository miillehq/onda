//! macOS clipboard handoff for dictated text: the words wait on the pasteboard for Cmd+V.
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
