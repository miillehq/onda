//! Only borrow a clipboard whose entire content the native text writer can restore.
use crate::PlatformError;

/// An automatic paste needs an observable field before it takes the clipboard or sends keys.
pub(super) fn paste_baseline<T>(state: Option<T>) -> Result<T, PlatformError> {
    state.ok_or_else(|| PlatformError::TargetUnavailable("field cannot confirm a paste".into()))
}

#[derive(Clone, Copy)]
pub(super) enum Format {
    PlainText,
    Concealed,
    #[cfg(any(target_os = "linux", test))]
    Metadata,
    Unsupported,
}

#[cfg(any(target_os = "linux", test))]
pub(super) fn x11_format(name: &str) -> Format {
    match name {
        "UTF8_STRING"
        | "STRING"
        | "TEXT"
        | "COMPOUND_TEXT"
        | "text/plain"
        | "text/plain;charset=utf-8"
        | "text/plain;charset=UTF-8" => Format::PlainText,
        "x-kde-passwordManagerHint" => Format::Concealed,
        "TARGETS" | "TIMESTAMP" | "SAVE_TARGETS" | "MULTIPLE" => Format::Metadata,
        _ => Format::Unsupported,
    }
}

#[cfg(any(target_os = "macos", test))]
pub(super) fn mac_format(name: &str) -> Format {
    match name {
        "public.utf8-plain-text" | "NSStringPboardType" => Format::PlainText,
        "org.nspasteboard.ConcealedType" => Format::Concealed,
        _ => Format::Unsupported,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct PreviousText {
    pub value: Option<String>,
    pub concealed: bool,
}

impl PreviousText {
    pub(super) fn inspect(
        item_count: usize,
        formats: impl IntoIterator<Item = Format>,
        read_text: impl FnOnce() -> Option<String>,
    ) -> Result<Self, PlatformError> {
        if item_count > 1 {
            return Err(PlatformError::ClipboardNotPreservable);
        }
        let mut text = false;
        let mut concealed = false;
        for format in formats {
            match format {
                Format::PlainText => text = true,
                Format::Concealed => concealed = true,
                #[cfg(any(target_os = "linux", test))]
                Format::Metadata => {}
                Format::Unsupported => return Err(PlatformError::ClipboardNotPreservable),
            }
        }
        if (item_count > 0 || concealed) && !text {
            return Err(PlatformError::ClipboardNotPreservable);
        }
        let value = if text {
            Some(read_text().ok_or(PlatformError::ClipboardNotPreservable)?)
        } else {
            None
        };
        Ok(Self { value, concealed })
    }
}

/// A rich clipboard stays with its owner while the original field handles an in-place edit.
pub(super) fn edit_without_clipboard<T>(
    cancelled: &dyn Fn() -> bool,
    edit: impl FnOnce() -> Result<T, PlatformError>,
) -> Result<T, PlatformError> {
    if cancelled() {
        return Err(PlatformError::Cancelled);
    }
    edit().map_err(|error| match error {
        PlatformError::Cancelled | PlatformError::SecureField => error,
        // Other delivery errors normally trigger an automatic clipboard copy.
        _ => PlatformError::ClipboardNotPreservable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_unobservable_field_never_authorizes_injection() {
        assert!(paste_baseline::<i32>(None).is_err());
        assert_eq!(paste_baseline(Some(0)).unwrap(), 0);
    }

    #[test]
    fn a_rich_clipboard_allows_an_in_place_edit_but_never_a_late_edit_or_overwrite() {
        assert_eq!(edit_without_clipboard(&|| false, || Ok(7)).unwrap(), 7);
        assert!(matches!(
            edit_without_clipboard::<()>(&|| true, || panic!("late edit")),
            Err(PlatformError::Cancelled)
        ));
        assert!(matches!(
            edit_without_clipboard::<()>(&|| false, || Err(PlatformError::SelectionNeedsPaste)),
            Err(PlatformError::ClipboardNotPreservable)
        ));
    }

    #[test]
    fn rich_unreadable_and_multi_item_clipboards_are_left_to_their_owner() {
        for formats in [
            vec![x11_format("UTF8_STRING"), x11_format("text/html")],
            vec![x11_format("image/png")],
            vec![
                mac_format("public.utf8-plain-text"),
                mac_format("public.file-url"),
            ],
            vec![mac_format("public.rtf")],
        ] {
            assert!(
                PreviousText::inspect(1, formats, || panic!("rich clipboard text was read"))
                    .is_err()
            );
        }
        assert!(
            PreviousText::inspect(2, [Format::PlainText], || panic!("multiple items read"))
                .is_err()
        );
        assert!(PreviousText::inspect(1, [Format::PlainText], || None).is_err());
        let plain = PreviousText::inspect(
            1,
            [
                x11_format("TARGETS"),
                x11_format("UTF8_STRING"),
                x11_format("x-kde-passwordManagerHint"),
            ],
            || Some("Test text".into()),
        )
        .unwrap();
        assert_eq!(
            plain,
            PreviousText {
                value: Some("Test text".into()),
                concealed: true
            }
        );
        assert_eq!(PreviousText::inspect(0, [], || None).unwrap().value, None);
    }
}
