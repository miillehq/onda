use super::*;

/// Presses âŒ˜V into the frontmost app with the text on the pasteboard, then puts the earlier
/// pasteboard text back. A Mac pastes with âŒ˜V in terminals too, so an app profile's paste keys,
/// which are Windows and Linux keys, do not apply.
pub(crate) fn paste(
    target: &TargetSnapshot,
    text: &str,
    _shortcut: PasteShortcut,
    should_cancel: &dyn Fn() -> bool,
) -> Result<InsertionOutcome, PlatformError> {
    if !macos_typing::trusted() {
        return Err(PlatformError::UnsupportedPlatform);
    }
    wait_for_modifiers()?;
    // Checked again after the wait, right before the keys go down.
    let field = field(target)?;
    let before = crate::text_clipboard::paste_baseline(edit_state(&field))?;
    let clipboard = match crate::macos_clipboard::BorrowedClipboard::borrow(text, should_cancel) {
        Err(PlatformError::ClipboardNotPreservable) => {
            return crate::text_clipboard::edit_without_clipboard(should_cancel, || {
                let original = self::field(target)?;
                let attribute = CFString::from_static_string("AXSelectedText");
                let replacement = CFString::new(text);
                if should_cancel() {
                    return Err(PlatformError::Cancelled);
                }
                let status = unsafe {
                    AXUIElementSetAttributeValue(
                        original.raw(),
                        attribute.as_concrete_TypeRef(),
                        replacement.as_CFTypeRef(),
                    )
                };
                if status != AX_SUCCESS {
                    return Err(PlatformError::ClipboardNotPreservable);
                }
                Ok(InsertionOutcome {
                    input_injected: true,
                    clipboard_read_observed: true,
                    clipboard_restored: true,
                    clipboard_changed_by_other: false,
                })
            });
        }
        result => result?,
    };
    if let Err(error) = crate::WindowsTargetService::new().revalidate(target) {
        clipboard.restore(text);
        return Err(error);
    }
    if should_cancel() {
        clipboard.restore(text);
        return Err(PlatformError::Cancelled);
    }
    if let Err(error) = press_paste() {
        clipboard.restore(text);
        return Err(error);
    }
    // A field that reports its length or caret shows whether the paste landed; others cannot
    // be checked.
    let mut observed = false;
    let deadline = Instant::now() + PASTE_SETTLE;
    while Instant::now() < deadline {
        if edit_state(&field).is_some_and(|now| now != before) {
            observed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    std::thread::sleep(CLIPBOARD_HOLD);
    let restored = clipboard.restore(text);
    Ok(InsertionOutcome {
        input_injected: true,
        clipboard_read_observed: observed,
        clipboard_restored: restored,
        clipboard_changed_by_other: false,
    })
}

/// The field's length and caret, whichever it reports; a paste changes at least one.
pub(super) fn edit_state(field: &Element) -> Option<(Option<i64>, Option<isize>)> {
    let length = field.integer("AXNumberOfCharacters");
    let caret = field
        .range("AXSelectedTextRange")
        .map(|range| range.location);
    (length.is_some() || caret.is_some()).then_some((length, caret))
}

/// A shortcut's own keys can still be down when the text is ready; âŒ˜V with Control or Shift
/// held would be another shortcut.
pub(super) fn wait_for_modifiers() -> Result<(), PlatformError> {
    let deadline = Instant::now() + MODIFIERS_WAIT;
    loop {
        if unsafe { CGEventSourceFlagsState(HID_SYSTEM_STATE) } & MODIFIER_FLAGS == 0 {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(PlatformError::ModifiersHeld);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub(super) fn press_paste() -> Result<(), PlatformError> {
    let source = unsafe { CGEventSourceCreate(HID_SYSTEM_STATE) };
    if source.is_null() {
        return Err(PlatformError::InjectionFailed);
    }
    let source = unsafe { CFType::wrap_under_create_rule(source) };
    // Both events exist before either is posted, so V is never left down.
    let event = |down| {
        let event = unsafe { CGEventCreateKeyboardEvent(source.as_CFTypeRef(), KEY_V, down) };
        (!event.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(event) })
    };
    let (Some(down), Some(up)) = (event(true), event(false)) else {
        return Err(PlatformError::InjectionFailed);
    };
    for event in [&down, &up] {
        unsafe {
            CGEventSetFlags(event.as_CFTypeRef(), FLAG_COMMAND);
            CGEventPost(HID_EVENT_TAP, event.as_CFTypeRef());
        }
    }
    Ok(())
}
