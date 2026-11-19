use super::*;

pub(super) fn text_range(object: &Object, start: i32, end: i32) -> Result<String, PlatformError> {
    call(object, TEXT, "GetText", &(start, end))
}

/// The one selection as character offsets, or `None` when nothing is selected.
pub(super) fn selection(object: &Object) -> Result<Option<(i32, i32)>, PlatformError> {
    let count: i32 = call(object, TEXT, "GetNSelections", &())?;
    match count {
        0 => Ok(None),
        1 => {
            let (start, end): (i32, i32) = call(object, TEXT, "GetSelection", &(0,))?;
            Ok((start != end).then_some((start.min(end), start.max(end))))
        }
        _ => Err(PlatformError::SelectionUnavailable),
    }
}

pub(super) fn check_limit(text: &str, max_characters: usize) -> Result<(), PlatformError> {
    if text.encode_utf16().count() > max_characters {
        return Err(PlatformError::TextLimit);
    }
    Ok(())
}

pub(crate) fn read_text(
    target: &TargetSnapshot,
    max_characters: usize,
    selected: bool,
) -> Result<String, PlatformError> {
    if target.focused_runtime_id.is_empty() {
        return Err(PlatformError::TargetUnavailable(
            "the app shows no text field".into(),
        ));
    }
    let object = field(target)?;
    let text = if selected {
        match selection(&object)? {
            Some((start, end)) => text_range(&object, start, end)?,
            None => return Err(PlatformError::EmptySelection),
        }
    } else {
        let count = integer(&object, TEXT, "CharacterCount")?;
        if usize::try_from(count).unwrap_or(usize::MAX) > max_characters {
            return Err(PlatformError::TextLimit);
        }
        text_range(&object, 0, count)?
    };
    check_limit(&text, max_characters)?;
    field(target)?;
    if selected && text.is_empty() {
        return Err(PlatformError::EmptySelection);
    }
    Ok(text)
}

/// Up to `max_characters` just before the caret, or before the selection a paste replaces.
pub(crate) fn preceding_text(
    target: &TargetSnapshot,
    max_characters: usize,
) -> Result<String, PlatformError> {
    let object = field(target)?;
    let caret = match selection(&object) {
        Ok(Some((start, _))) => start,
        _ => integer(&object, TEXT, "CaretOffset")?,
    };
    if caret <= 0 {
        return Ok(String::new());
    }
    let start = caret.saturating_sub(i32::try_from(max_characters).unwrap_or(i32::MAX));
    let text = text_range(&object, start.max(0), caret)?;
    let skip = text.chars().count().saturating_sub(max_characters);
    Ok(text.chars().skip(skip).collect())
}

pub(crate) fn select_unique_text(
    target: &TargetSnapshot,
    expected_document: &str,
    needle: &str,
    should_cancel: &dyn Fn() -> bool,
) -> Result<(), PlatformError> {
    if needle.is_empty() || expected_document.match_indices(needle).count() != 1 {
        return Err(PlatformError::TargetChanged);
    }
    let object = field(target)?;
    let count = integer(&object, TEXT, "CharacterCount")?;
    if text_range(&object, 0, count)? != expected_document {
        return Err(PlatformError::TargetChanged);
    }
    let byte_start = expected_document
        .find(needle)
        .ok_or(PlatformError::TargetChanged)?;
    let start = expected_document[..byte_start].chars().count() as i32;
    let end = start + needle.chars().count() as i32;
    if should_cancel() {
        return Err(PlatformError::Cancelled);
    }
    let selected: bool = if selection(&object)?.is_some() {
        call(&object, TEXT, "SetSelection", &(0, start, end))?
    } else {
        call(&object, TEXT, "AddSelection", &(start, end))?
    };
    if !selected || selection(&object)? != Some((start, end)) {
        return Err(PlatformError::SelectionUnavailable);
    }
    Ok(())
}

/// Whether a dictation into `target` can be typed: with the paste permission anywhere, or
/// without it into a field that accepts edits through accessibility.
pub(crate) fn can_type(target: &TargetSnapshot) -> bool {
    if target.focused_runtime_id.is_empty() {
        return false;
    }
    let Ok(object) = field(target) else {
        return false;
    };
    if linux_typing::typing_access() == linux_typing::TypingAccess::Allowed {
        return integer(&object, TEXT, "CharacterCount").is_ok_and(|count| count >= 0);
    }
    editable(&object)
}

pub(super) fn editable(object: &Object) -> bool {
    states(object).is_ok_and(|states| has(&states, STATE_EDITABLE))
        && call::<_, Vec<String>>(object, ACCESSIBLE, "GetInterfaces", &())
            .is_ok_and(|interfaces| interfaces.iter().any(|name| name == EDITABLE_TEXT))
}

/// With the paste permission, the clipboard and the paste shortcut, as on Windows. Without it,
/// the text goes in through the field's own editing interface.
pub(crate) fn paste(
    target: &TargetSnapshot,
    text: &str,
    shortcut: PasteShortcut,
    should_cancel: &dyn Fn() -> bool,
) -> Result<InsertionOutcome, PlatformError> {
    let object = field(target)?;
    if linux_typing::typing_access() == linux_typing::TypingAccess::Allowed {
        return paste_with_shortcut(target, &object, text, shortcut, should_cancel);
    }
    if !editable(&object) {
        return Err(PlatformError::UnsupportedPlatform);
    }
    insert_directly(&object, text, should_cancel)
}

pub(super) fn paste_with_shortcut(
    target: &TargetSnapshot,
    object: &Object,
    text: &str,
    shortcut: PasteShortcut,
    should_cancel: &dyn Fn() -> bool,
) -> Result<InsertionOutcome, PlatformError> {
    // Terminals paste with Ctrl+Shift+V; Ctrl+V would reach the shell as a control character.
    let shortcut = match role(object) {
        Ok(ROLE_TERMINAL) if shortcut == PasteShortcut::ControlV => PasteShortcut::ControlShiftV,
        _ => shortcut,
    };
    let keys: &[i32] = match shortcut {
        PasteShortcut::ControlV => &[linux_typing::KEY_CONTROL, linux_typing::KEY_V],
        PasteShortcut::ControlShiftV => &[
            linux_typing::KEY_CONTROL,
            linux_typing::KEY_SHIFT,
            linux_typing::KEY_V,
        ],
        PasteShortcut::ShiftInsert => &[linux_typing::KEY_SHIFT, linux_typing::KEY_INSERT],
    };
    let before = crate::text_clipboard::paste_baseline(
        integer(object, TEXT, "CharacterCount")
            .ok()
            .filter(|count| *count >= 0),
    )?;
    let replaced = selection(object).ok().flatten();
    let inserted = crate::linux_edits::InsertSize::measure(text)?.characters;
    let clipboard = crate::linux_clipboard::BorrowedClipboard::borrow(text, should_cancel)?;
    if let Err(error) = field(target) {
        clipboard.restore(text);
        return Err(error);
    }
    if should_cancel() {
        clipboard.restore(text);
        return Err(PlatformError::Cancelled);
    }
    if let Err(error) = linux_typing::press_chord(keys) {
        clipboard.restore(text);
        return Err(error);
    }
    // A field that reports its text shows whether the paste landed; others cannot be checked.
    let mut observed = false;
    let deadline = Instant::now() + PASTE_SETTLE;
    while Instant::now() < deadline {
        if integer(object, TEXT, "CharacterCount").is_ok_and(|after| {
            crate::linux_edits::paste_observed(before, after, replaced, inserted, |start, end| {
                selection(object).is_ok_and(|selection| selection.is_none())
                    && text_range(object, start, end).is_ok_and(|current| current == text)
            })
        }) {
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

pub(super) fn insert_directly(
    object: &Object,
    text: &str,
    should_cancel: &dyn Fn() -> bool,
) -> Result<InsertionOutcome, PlatformError> {
    // AT-SPI exposes delete and insert as separate operations. A failed insertion must
    // never leave the user's original selection deleted.
    let position = crate::linux_edits::direct_caret(selection(object)?, || {
        integer(object, TEXT, "CaretOffset")
    })?;
    if should_cancel() {
        return Err(PlatformError::Cancelled);
    }
    let size = crate::linux_edits::InsertSize::measure(text)?;
    let inserted: bool = call(
        object,
        EDITABLE_TEXT,
        "InsertText",
        &(position, text, size.bytes),
    )?;
    if !inserted {
        return Err(PlatformError::InjectionFailed);
    }
    // Leave the caret after the new text, where typing would have left it.
    let _: Result<bool, _> = call(
        object,
        TEXT,
        "SetCaretOffset",
        &(position.saturating_add(size.characters),),
    );
    Ok(InsertionOutcome {
        input_injected: true,
        clipboard_read_observed: true,
        clipboard_restored: true,
        clipboard_changed_by_other: false,
    })
}
