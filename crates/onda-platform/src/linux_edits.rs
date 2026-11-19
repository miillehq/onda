//! Decisions used by the AT-SPI insert and portal paste paths.
use crate::PlatformError;

pub(super) struct InsertSize {
    pub bytes: i32,
    pub characters: i32,
}

impl InsertSize {
    /// AT-SPI takes the insertion length in UTF-8 bytes and the caret offset in characters.
    pub(super) fn measure(text: &str) -> Result<Self, PlatformError> {
        let bytes = i32::try_from(text.len()).map_err(|_| PlatformError::TextLimit)?;
        Ok(Self {
            bytes,
            characters: text.chars().count() as i32,
        })
    }
}

pub(super) fn direct_caret(
    selection: Option<(i32, i32)>,
    caret: impl FnOnce() -> Result<i32, PlatformError>,
) -> Result<i32, PlatformError> {
    if selection.is_some() {
        return Err(PlatformError::SelectionNeedsPaste);
    }
    caret()
}

/// Count Unicode characters as AT-SPI does. Equal-length replacements require reading the
/// replaced range after its selection collapses. A count alone cannot acknowledge one.
pub(super) fn paste_observed(
    before: i32,
    after: i32,
    selection: Option<(i32, i32)>,
    inserted: i32,
    range_matches: impl FnOnce(i32, i32) -> bool,
) -> bool {
    match selection {
        Some((start, end)) => {
            let expected =
                i64::from(before) - (i64::from(end) - i64::from(start)) + i64::from(inserted);
            expected == i64::from(after)
                && start
                    .checked_add(inserted)
                    .is_some_and(|stop| range_matches(start, stop))
        }
        None => i64::from(after) == i64::from(before) + i64::from(inserted),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_insert_length_and_caret_use_their_native_units() {
        let size = InsertSize::measure("é🙂").unwrap();
        assert_eq!(size.bytes, 6);
        assert_eq!(size.characters, 2);
    }
    #[test]
    fn direct_edits_never_delete_an_existing_selection() {
        assert!(matches!(
            direct_caret(Some((2, 5)), || panic!("attempted direct edit")),
            Err(PlatformError::SelectionNeedsPaste)
        ));
        assert_eq!(direct_caret(None, || Ok(3)).unwrap(), 3);
    }
    #[test]
    fn a_rewrite_of_equal_length_needs_the_new_text_at_the_same_location() {
        let range = Some((2, 5));
        assert!(paste_observed(8, 8, range, 3, |start, end| (start, end) == (2, 5)));
        assert!(!paste_observed(8, 8, range, 3, |_, _| false));
        assert!(!paste_observed(8, 7, range, 3, |_, _| true));
        assert!(paste_observed(8, 10, None, 2, |_, _| unreachable!()));
    }
}
