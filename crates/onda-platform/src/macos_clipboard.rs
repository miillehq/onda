//! The macOS pasteboard: dictation waits there for ⌘V, and a paste Onda presses itself borrows it
//! and puts the earlier text back.
use crate::text_clipboard::{mac_format, PreviousText};
use crate::PlatformError;
use arboard::{Clipboard, SetExtApple};
use objc2_app_kit::NSPasteboard;

/// Replaces the pasteboard text and marks it `org.nspasteboard.ConcealedType`, which clipboard
/// history apps honour by not keeping it, like Windows' and Linux's privacy markers.
pub fn copy_text(text: &str) -> Result<(), PlatformError> {
    write_text(text, true)
}

fn write_text(text: &str, concealed: bool) -> Result<(), PlatformError> {
    let mut clipboard = Clipboard::new()
        .map_err(|_| PlatformError::Clipboard("the pasteboard is not available".into()))?;
    let writer = clipboard.set();
    let writer = if concealed {
        writer.exclude_from_history()
    } else {
        writer
    };
    writer
        .text(text)
        .map_err(|_| PlatformError::Clipboard("the pasteboard refused the text".into()))
}

fn previous_text(
    board: &NSPasteboard,
    read: impl FnOnce() -> Option<String>,
) -> Result<PreviousText, PlatformError> {
    let items = board
        .pasteboardItems()
        .ok_or(PlatformError::ClipboardNotPreservable)?;
    if items.is_empty() {
        PreviousText::inspect(0, [], || None)
    } else {
        let formats = board
            .types()
            .ok_or(PlatformError::ClipboardNotPreservable)?;
        PreviousText::inspect(
            items.len(),
            formats.iter().map(|name| mac_format(&name.to_string())),
            read,
        )
    }
}

pub(super) struct BorrowedClipboard {
    change_count: isize,
    previous: PreviousText,
}

impl BorrowedClipboard {
    pub(super) fn borrow(text: &str, cancelled: &dyn Fn() -> bool) -> Result<Self, PlatformError> {
        // Arboard also checks that a general pasteboard exists in this process's session.
        let mut clipboard = Clipboard::new().map_err(|_| PlatformError::ClipboardNotPreservable)?;
        let board = NSPasteboard::generalPasteboard();
        let before = board.changeCount();
        let previous = previous_text(&board, || clipboard.get_text().ok())?;
        if cancelled() {
            return Err(PlatformError::Cancelled);
        }
        if before != board.changeCount() {
            return Err(PlatformError::ClipboardNotPreservable);
        }
        copy_text(text)?;
        Ok(Self {
            change_count: board.changeCount(),
            previous,
        })
    }

    pub(super) fn restore(self, dictated: &str) -> bool {
        let board = NSPasteboard::generalPasteboard();
        if board.changeCount() != self.change_count
            || read_text().as_deref() != Some(dictated)
            || board.changeCount() != self.change_count
        {
            return false;
        }
        match self.previous.value {
            Some(text) => write_text(&text, self.previous.concealed).is_ok(),
            None => clear().is_ok(),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_foundation::NSString;

    #[test]
    fn a_private_mac_pasteboard_refuses_rich_data_and_keeps_plain_text() {
        let board = NSPasteboard::pasteboardWithUniqueName();
        board.clearContents();
        let plain = NSString::from_str("public.utf8-plain-text");
        let rich = NSString::from_str("public.html");
        let sample = NSString::from_str("synthetic text");
        assert!(board.setString_forType(&sample, &plain));
        let old = previous_text(&board, || {
            board.stringForType(&plain).map(|s| s.to_string())
        })
        .unwrap();
        assert_eq!(old.value.as_deref(), Some("synthetic text"));
        assert!(board.setString_forType(&sample, &rich));
        let change = board.changeCount();
        assert!(previous_text(&board, || panic!("read rich item")).is_err());
        assert_eq!(board.changeCount(), change);
        assert_eq!(
            board.stringForType(&rich).unwrap().to_string(),
            "synthetic text"
        );
        board.clearContents();
    }
}
