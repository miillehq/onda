//! Linux clipboard handoff for dictated text (X11, or Wayland through XWayland).
use crate::text_clipboard::PreviousText;
use crate::PlatformError;
use arboard::{Clipboard, SetExtLinux};
use std::sync::{Mutex, OnceLock};
mod offers;

/// X11 serves clipboard data from the owning process, so the handle lives as long as Onda;
/// dropping it would empty the clipboard unless a clipboard manager had copied it.
static CLIPBOARD: OnceLock<Mutex<Option<Clipboard>>> = OnceLock::new();

fn with_clipboard<T>(
    action: impl FnOnce(&mut Clipboard) -> Result<T, PlatformError>,
) -> Result<T, PlatformError> {
    let mut slot = CLIPBOARD
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| PlatformError::Clipboard("clipboard lock poisoned".into()))?;
    if slot.is_none() {
        *slot = Some(Clipboard::new().map_err(|_| {
            PlatformError::Clipboard("no X11 or XWayland clipboard is available".into())
        })?);
    }
    action(slot.as_mut().expect("clipboard initialized above"))
}

/// Replaces the clipboard text and asks clipboard managers that honour the hint (KDE Klipper,
/// several GNOME extensions) not to keep it in their history, like Windows' privacy markers.
pub fn copy_text(text: &str) -> Result<(), PlatformError> {
    write_text(text, true)
}

fn write_text(text: &str, concealed: bool) -> Result<(), PlatformError> {
    with_clipboard(|clipboard| {
        let writer = clipboard.set();
        let writer = if concealed {
            writer.exclude_from_history()
        } else {
            writer
        };
        writer
            .text(text)
            .map_err(|_| PlatformError::Clipboard("the clipboard refused the text".into()))
    })
}

pub(super) struct BorrowedClipboard {
    reader: offers::OfferReader,
    owner: u32,
    previous: PreviousText,
}

impl BorrowedClipboard {
    pub(super) fn borrow(text: &str, cancelled: &dyn Fn() -> bool) -> Result<Self, PlatformError> {
        let reader = offers::OfferReader::open()?;
        let before = reader.owner()?;
        let previous = if before == x11rb::NONE {
            PreviousText::inspect(0, [], || None)?
        } else {
            PreviousText::inspect(1, reader.formats()?, read_text)?
        };
        if cancelled() {
            return Err(PlatformError::Cancelled);
        }
        if reader.owner()? != before {
            return Err(PlatformError::ClipboardNotPreservable);
        }
        copy_text(text)?;
        let owner = reader.owner()?;
        Ok(Self {
            reader,
            owner,
            previous,
        })
    }

    pub(super) fn restore(self, dictated: &str) -> bool {
        if self.reader.owner().ok() != Some(self.owner) {
            return false;
        }
        let current = self
            .reader
            .formats()
            .and_then(|formats| PreviousText::inspect(1, formats, read_text));
        if !current.is_ok_and(|current| current.value.as_deref() == Some(dictated))
            || self.reader.owner().ok() != Some(self.owner)
        {
            return false;
        }
        match self.previous.value {
            Some(text) => write_text(&text, self.previous.concealed).is_ok(),
            None => clear().is_ok(),
        }
    }
}

/// The clipboard's text, if it holds any.
pub fn read_text() -> Option<String> {
    with_clipboard(|clipboard| Ok(clipboard.get_text().ok()))
        .ok()
        .flatten()
}

pub fn clear() -> Result<(), PlatformError> {
    with_clipboard(|clipboard| {
        clipboard
            .clear()
            .map_err(|_| PlatformError::Clipboard("the clipboard could not be emptied".into()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "writes fixtures to the clipboard of a disposable Xvfb display"]
    fn isolated_x11_keeps_rich_content_and_restores_plain_text() {
        assert_eq!(std::env::var("ONDA_TEST_X11").as_deref(), Ok("isolated"));
        let mut owner = Clipboard::new().unwrap();
        let html = "<b>synthetic clipboard fixture</b>";
        owner
            .set_html(html, Some("synthetic clipboard fixture"))
            .unwrap();
        assert!(matches!(
            BorrowedClipboard::borrow("dictation", &|| false),
            Err(PlatformError::ClipboardNotPreservable)
        ));
        assert_eq!(owner.get().html().unwrap(), html);
        owner.set_text("old text").unwrap();
        let loan = BorrowedClipboard::borrow("dictation", &|| false).unwrap();
        assert_eq!(owner.get_text().unwrap(), "dictation");
        assert!(loan.restore("dictation"));
        assert_eq!(owner.get_text().unwrap(), "old text");
        let loan = BorrowedClipboard::borrow("dictation", &|| false).unwrap();
        owner.set_html(html, Some("dictation")).unwrap();
        assert!(!loan.restore("dictation"));
        assert_eq!(owner.get().html().unwrap(), html);
    }
}
