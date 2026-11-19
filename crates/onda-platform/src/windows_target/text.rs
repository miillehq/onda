use super::*;

pub(crate) fn read_text(
    target: &TargetSnapshot,
    max_characters: usize,
    selection: bool,
) -> Result<String, PlatformError> {
    let _apartment = ComApartment::initialize()?;
    let automation = automation()?;
    let (actual, element) = snapshot_with(&automation)?;
    validate_identity(target, &actual)?;
    let pattern: IUIAutomationTextPattern = unsafe {
        element
            .GetCurrentPatternAs(UIA_TextPatternId)
            .map_err(|_| PlatformError::SelectionUnavailable)?
    };
    let range = if selection {
        let ranges = unsafe { pattern.GetSelection().map_err(uia_error)? };
        match unsafe { ranges.Length().map_err(uia_error)? } {
            0 => None,
            1 => Some(unsafe { ranges.GetElement(0).map_err(uia_error)? }),
            _ => return Err(PlatformError::SelectionUnavailable),
        }
    } else {
        Some(unsafe { pattern.DocumentRange().map_err(uia_error)? })
    };
    let text = match range {
        Some(range) => unsafe {
            range
                .GetText((max_characters + 1) as i32)
                .map_err(uia_error)?
        }
        .to_string(),
        None => String::new(),
    };
    if text.encode_utf16().count() > max_characters {
        return Err(PlatformError::TextLimit);
    }
    // Some providers can switch focus while servicing a range request.
    let (after, _) = snapshot_with(&automation)?;
    validate_identity(target, &after)?;
    if selection && text.is_empty() {
        return Err(PlatformError::EmptySelection);
    }
    Ok(text)
}

/// Up to `max_characters` immediately before the caret (or before the selection, which a
/// paste replaces). Empty at the start of the field or when the control exposes no caret.
pub(crate) fn preceding_text(
    target: &TargetSnapshot,
    max_characters: usize,
) -> Result<String, PlatformError> {
    let _apartment = ComApartment::initialize()?;
    let automation = automation()?;
    let (actual, element) = snapshot_with(&automation)?;
    validate_identity(target, &actual)?;
    let pattern: IUIAutomationTextPattern = unsafe {
        element
            .GetCurrentPatternAs(UIA_TextPatternId)
            .map_err(|_| PlatformError::SelectionUnavailable)?
    };
    let ranges = unsafe { pattern.GetSelection().map_err(uia_error)? };
    if unsafe { ranges.Length().map_err(uia_error)? } != 1 {
        return Ok(String::new());
    }
    let caret = unsafe { ranges.GetElement(0).map_err(uia_error)? };
    let before = unsafe { caret.Clone().map_err(uia_error)? };
    let moved = unsafe {
        before
            .MoveEndpointByRange(
                TextPatternRangeEndpoint_End,
                &caret,
                TextPatternRangeEndpoint_Start,
            )
            .map_err(uia_error)?;
        before
            .MoveEndpointByUnit(
                TextPatternRangeEndpoint_Start,
                TextUnit_Character,
                -(max_characters as i32),
            )
            .map_err(uia_error)?
    };
    if moved == 0 {
        return Ok(String::new());
    }
    // Surrogate pairs count as two UTF-16 units for GetText.
    let text = unsafe {
        before
            .GetText((max_characters * 2) as i32)
            .map_err(uia_error)?
    }
    .to_string();
    let (after, _) = snapshot_with(&automation)?;
    validate_identity(target, &after)?;
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
    let _apartment = ComApartment::initialize()?;
    let automation = automation()?;
    let (actual, element) = snapshot_with(&automation)?;
    validate_identity(target, &actual)?;
    let pattern: IUIAutomationTextPattern = unsafe {
        element
            .GetCurrentPatternAs(UIA_TextPatternId)
            .map_err(|_| PlatformError::SelectionUnavailable)?
    };
    let document = unsafe { pattern.DocumentRange().map_err(uia_error)? };
    let current = unsafe {
        document
            .GetText((crate::target::MAX_TEXT_CHARACTERS + 1) as i32)
            .map_err(uia_error)?
    }
    .to_string();
    if current != expected_document {
        return Err(PlatformError::TargetChanged);
    }
    let found = unsafe {
        document
            .FindText(&BSTR::from(needle), false, false)
            .map_err(|_| PlatformError::SelectionUnavailable)?
    };
    let found_text = unsafe {
        found
            .GetText((needle.encode_utf16().count() + 1) as i32)
            .map_err(uia_error)?
    }
    .to_string();
    if found_text != needle {
        return Err(PlatformError::TargetChanged);
    }
    let (before, _) = snapshot_with(&automation)?;
    validate_identity(target, &before)?;
    if should_cancel() {
        return Err(PlatformError::Cancelled);
    }
    unsafe {
        found.Select().map_err(uia_error)?;
    }
    Ok(())
}
