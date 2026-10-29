//! Linux clipboard handoff for dictated text (X11, or Wayland through XWayland).
use crate::PlatformError;
use arboard::{Clipboard, SetExtLinux};
use std::sync::{Mutex, OnceLock};

/// X11 serves clipboard data from the owning process, so the handle lives as long as Onda;
/// dropping it would empty the clipboard unless a clipboard manager had copied it.
static CLIPBOARD: OnceLock<Mutex<Option<Clipboard>>> = OnceLock::new();

/// Replaces the clipboard text and asks clipboard managers that honour the hint (KDE Klipper,
/// several GNOME extensions) not to keep it in their history, like Windows' privacy markers.
pub fn copy_text(text: &str) -> Result<(), PlatformError> {
    let mut slot = CLIPBOARD
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| PlatformError::Clipboard("clipboard lock poisoned".into()))?;
    if slot.is_none() {
        *slot = Some(Clipboard::new().map_err(|_| {
            PlatformError::Clipboard("no X11 or XWayland clipboard is available".into())
        })?);
    }
    let clipboard = slot.as_mut().expect("clipboard initialized above");
    clipboard
        .set()
        .exclude_from_history()
        .text(text.to_owned())
        .map_err(|_| PlatformError::Clipboard("the clipboard refused the text".into()))
}
