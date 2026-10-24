use crate::PlatformError;
use serde::{Deserialize, Serialize};

pub const MAX_TEXT_CHARACTERS: usize = 100_000;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetSnapshot {
    pub window_handle: isize,
    pub process_id: u32,
    pub process_created_at: u64,
    pub executable_name: String,
    pub focused_runtime_id: Vec<i32>,
    pub is_password: bool,
    pub integrity_level: u32,
    /// Structural description of the focused control, used when a provider re-creates it.
    #[serde(default)]
    pub focused_control: ControlFingerprint,
}

/// UI Automation properties that identify a control's kind and place, never its text.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlFingerprint {
    pub process_id: u32,
    pub control_type: i32,
    pub automation_id: String,
    pub framework_id: String,
    /// Left, top, right, bottom in screen pixels.
    pub bounds: [i32; 4],
}

/// Pixels a re-created control may shift; its height may grow with its content.
const BOUNDS_TOLERANCE: i32 = 4;

impl ControlFingerprint {
    /// Chromium-based apps (Electron, WebView2, browsers) can rebuild their accessibility tree
    /// while Onda waits for transcription or polish, giving the same field a new runtime ID.
    /// The same process, control type, automation ID and position still identify that field;
    /// a different field in the same window sits elsewhere.
    fn same_field(&self, other: &Self) -> bool {
        let [left, top, right, _] = self.bounds;
        let [other_left, other_top, other_right, _] = other.bounds;
        let width = right - left;
        width > 0
            && self.bounds[3] > top
            && self.process_id == other.process_id
            && self.control_type == other.control_type
            && self.automation_id == other.automation_id
            && self.framework_id == other.framework_id
            && (left - other_left).abs() <= BOUNDS_TOLERANCE
            && (top - other_top).abs() <= BOUNDS_TOLERANCE
            && ((right - left) - (other_right - other_left)).abs() <= BOUNDS_TOLERANCE
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PasteShortcut {
    #[default]
    ControlV,
    ControlShiftV,
    ShiftInsert,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertionOutcome {
    /// SendInput accepted the complete chord; this alone does not prove the application edited text.
    pub input_injected: bool,
    /// A clipboard consumer requested delayed text; consumers cannot be authenticated as the target.
    pub clipboard_read_observed: bool,
    pub clipboard_restored: bool,
    pub clipboard_changed_by_other: bool,
}

/// Each operation creates a bounded-time UIA client on the caller's native worker thread.
#[derive(Default)]
pub struct WindowsTargetService;
impl WindowsTargetService {
    pub fn new() -> Self {
        Self
    }
    /// Explicit user-requested copy replaces clipboard content without focus or restoration.
    pub fn copy_to_clipboard(&self, text: &str) -> Result<(), PlatformError> {
        if text.is_empty() || text.contains('\0') || text.chars().count() > MAX_TEXT_CHARACTERS {
            return Err(PlatformError::TextLimit);
        }
        #[cfg(windows)]
        {
            crate::clipboard::copy_text(text)
        }
        #[cfg(not(windows))]
        {
            Err(PlatformError::UnsupportedPlatform)
        }
    }
    pub fn snapshot(&self) -> Result<TargetSnapshot, PlatformError> {
        #[cfg(windows)]
        {
            crate::windows_target::snapshot()
        }
        #[cfg(not(windows))]
        {
            Err(PlatformError::UnsupportedPlatform)
        }
    }
    pub fn revalidate(&self, target: &TargetSnapshot) -> Result<(), PlatformError> {
        validate_identity(target, &self.snapshot()?)
    }
    /// Selected text requires a single UIA TextPattern range and refuses password controls.
    pub fn selected_text(
        &self,
        target: &TargetSnapshot,
        max_characters: usize,
    ) -> Result<String, PlatformError> {
        validate_limit(max_characters)?;
        #[cfg(windows)]
        {
            crate::windows_target::read_text(target, max_characters, true)
        }
        #[cfg(not(windows))]
        {
            let _ = target;
            Err(PlatformError::UnsupportedPlatform)
        }
    }
    /// Read only the focused control, once per explicitly triggered contextual operation.
    pub fn context(
        &self,
        target: &TargetSnapshot,
        max_characters: usize,
    ) -> Result<String, PlatformError> {
        self.field_text(target, max_characters)
    }
    /// A few characters just before the caret, read immediately before typing so a dictation
    /// can be spaced from earlier text. Never stored or sent anywhere.
    pub fn preceding_text(
        &self,
        target: &TargetSnapshot,
        max_characters: usize,
    ) -> Result<String, PlatformError> {
        validate_limit(max_characters)?;
        #[cfg(windows)]
        {
            crate::windows_target::preceding_text(target, max_characters)
        }
        #[cfg(not(windows))]
        {
            let _ = target;
            Err(PlatformError::UnsupportedPlatform)
        }
    }
    /// The caller may use this only inside an opt-in, bounded post-insertion correction window.
    pub fn field_text(
        &self,
        target: &TargetSnapshot,
        max_characters: usize,
    ) -> Result<String, PlatformError> {
        validate_limit(max_characters)?;
        #[cfg(windows)]
        {
            crate::windows_target::read_text(target, max_characters, false)
        }
        #[cfg(not(windows))]
        {
            let _ = target;
            Err(PlatformError::UnsupportedPlatform)
        }
    }
    pub fn paste(
        &self,
        target: &TargetSnapshot,
        text: &str,
        shortcut: PasteShortcut,
    ) -> Result<InsertionOutcome, PlatformError> {
        self.paste_guarded(target, text, shortcut, || false)
    }

    /// Check cancellation after native preparation and immediately before input injection.
    pub fn paste_guarded(
        &self,
        target: &TargetSnapshot,
        text: &str,
        shortcut: PasteShortcut,
        should_cancel: impl Fn() -> bool,
    ) -> Result<InsertionOutcome, PlatformError> {
        if should_cancel() {
            return Err(PlatformError::Cancelled);
        }
        if text.is_empty() || text.contains('\0') || text.chars().count() > MAX_TEXT_CHARACTERS {
            return Err(PlatformError::TextLimit);
        }
        self.revalidate(target)?;
        #[cfg(windows)]
        {
            crate::clipboard::paste(self, target, text, shortcut, &should_cancel)
        }
        #[cfg(not(windows))]
        {
            let _ = shortcut;
            Err(PlatformError::UnsupportedPlatform)
        }
    }
    /// Revalidate both focused control and selected source before replacing selected text.
    pub fn replace_selection(
        &self,
        target: &TargetSnapshot,
        expected_selection: &str,
        replacement: &str,
        shortcut: PasteShortcut,
    ) -> Result<InsertionOutcome, PlatformError> {
        self.replace_selection_guarded(target, expected_selection, replacement, shortcut, || false)
    }

    /// Cancellation-aware replacement of a still-matching selected source.
    pub fn replace_selection_guarded(
        &self,
        target: &TargetSnapshot,
        expected_selection: &str,
        replacement: &str,
        shortcut: PasteShortcut,
        should_cancel: impl Fn() -> bool,
    ) -> Result<InsertionOutcome, PlatformError> {
        if should_cancel() {
            return Err(PlatformError::Cancelled);
        }
        let current = self.selected_text(target, MAX_TEXT_CHARACTERS)?;
        if current != expected_selection {
            return Err(PlatformError::TargetChanged);
        }
        self.paste_guarded(target, replacement, shortcut, should_cancel)
    }

    /// Undo only an unchanged focused field containing exactly one matching transformed span.
    pub fn replace_unique_text(
        &self,
        target: &TargetSnapshot,
        expected_document: &str,
        needle: &str,
        replacement: &str,
        shortcut: PasteShortcut,
    ) -> Result<InsertionOutcome, PlatformError> {
        self.replace_unique_text_guarded(
            target,
            expected_document,
            needle,
            replacement,
            shortcut,
            || false,
        )
    }

    /// Cancellation-aware unique-span replacement, checked before selecting and before injection.
    pub fn replace_unique_text_guarded(
        &self,
        target: &TargetSnapshot,
        expected_document: &str,
        needle: &str,
        replacement: &str,
        shortcut: PasteShortcut,
        should_cancel: impl Fn() -> bool,
    ) -> Result<InsertionOutcome, PlatformError> {
        if should_cancel() {
            return Err(PlatformError::Cancelled);
        }
        validate_limit(expected_document.encode_utf16().count())?;
        #[cfg(windows)]
        {
            crate::windows_target::select_unique_text(
                target,
                expected_document,
                needle,
                &should_cancel,
            )?;
            self.replace_selection_guarded(target, needle, replacement, shortcut, should_cancel)
        }
        #[cfg(not(windows))]
        {
            let _ = (target, needle, replacement, shortcut);
            Err(PlatformError::UnsupportedPlatform)
        }
    }
}

pub(crate) fn validate_limit(limit: usize) -> Result<(), PlatformError> {
    if limit == 0 || limit > MAX_TEXT_CHARACTERS {
        return Err(PlatformError::InvalidParameter(
            "text limit must be 1–100000 characters".into(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_identity(
    expected: &TargetSnapshot,
    actual: &TargetSnapshot,
) -> Result<(), PlatformError> {
    if expected.is_password || actual.is_password {
        return Err(PlatformError::SecureField);
    }
    if expected.focused_runtime_id.is_empty() || actual.focused_runtime_id.is_empty() {
        return Err(PlatformError::TargetUnavailable(
            "focused control has no runtime identity".into(),
        ));
    }
    let same_window = expected.window_handle == actual.window_handle
        && expected.process_id == actual.process_id
        && expected.process_created_at == actual.process_created_at
        && expected.integrity_level == actual.integrity_level;
    if same_window
        && expected.focused_runtime_id != actual.focused_runtime_id
        && expected.focused_control.same_field(&actual.focused_control)
    {
        tracing::info!(
            subsystem = "target",
            app = %expected.executable_name,
            "focused control was re-created in place; same field kept"
        );
        return Ok(());
    }
    if !same_window || expected.focused_runtime_id != actual.focused_runtime_id {
        // Which part of the identity moved, never any text: tells a real focus change apart
        // from an app that re-creates its focused control.
        tracing::warn!(
            subsystem = "target",
            app = %expected.executable_name,
            same_window = expected.window_handle == actual.window_handle,
            same_process = expected.process_id == actual.process_id
                && expected.process_created_at == actual.process_created_at,
            same_control = expected.focused_runtime_id == actual.focused_runtime_id,
            same_integrity = expected.integrity_level == actual.integrity_level,
            "target identity changed"
        );
        return Err(PlatformError::TargetChanged);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn target() -> TargetSnapshot {
        TargetSnapshot {
            window_handle: 1,
            process_id: 2,
            process_created_at: 3,
            executable_name: "fixture.exe".into(),
            focused_runtime_id: vec![42, 7],
            is_password: false,
            integrity_level: 0x2000,
            focused_control: ControlFingerprint {
                process_id: 5,
                control_type: 50004,
                automation_id: "composer".into(),
                framework_id: "Chrome".into(),
                bounds: [100, 600, 900, 660],
            },
        }
    }
    #[test]
    fn recreated_field_in_place_passes_but_another_field_does_not() {
        let expected = target();
        let mut recreated = target();
        recreated.focused_runtime_id = vec![42, 99];
        recreated.focused_control.bounds = [101, 600, 901, 700];
        assert!(validate_identity(&expected, &recreated).is_ok());
        for change in [
            |control: &mut ControlFingerprint| control.bounds = [100, 80, 900, 120],
            |control: &mut ControlFingerprint| control.automation_id = "search".into(),
            |control: &mut ControlFingerprint| control.control_type = 50033,
            |control: &mut ControlFingerprint| control.process_id = 6,
        ] {
            let mut other = recreated.clone();
            change(&mut other.focused_control);
            assert!(matches!(
                validate_identity(&expected, &other),
                Err(PlatformError::TargetChanged)
            ));
        }
        let mut other_window = recreated.clone();
        other_window.window_handle = 9;
        assert!(validate_identity(&expected, &other_window).is_err());
        let mut unknown = target();
        unknown.focused_control = ControlFingerprint::default();
        let mut unknown_recreated = unknown.clone();
        unknown_recreated.focused_runtime_id = vec![42, 99];
        assert!(validate_identity(&unknown, &unknown_recreated).is_err());
    }
    #[test]
    fn changed_focus_or_reused_pid_never_pass_validation() {
        let expected = target();
        let mut changed = target();
        changed.focused_runtime_id[1] = 8;
        changed.focused_control.bounds = [100, 40, 400, 72];
        assert!(matches!(
            validate_identity(&expected, &changed),
            Err(PlatformError::TargetChanged)
        ));
        changed = target();
        changed.process_created_at += 1;
        assert!(matches!(
            validate_identity(&expected, &changed),
            Err(PlatformError::TargetChanged)
        ));
    }
    #[test]
    fn secure_or_unknown_control_is_rejected() {
        let expected = target();
        let mut changed = target();
        changed.is_password = true;
        assert!(matches!(
            validate_identity(&expected, &changed),
            Err(PlatformError::SecureField)
        ));
        changed = target();
        changed.focused_runtime_id.clear();
        assert!(validate_identity(&expected, &changed).is_err());
    }
}
