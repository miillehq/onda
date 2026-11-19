mod identity;
use crate::PlatformError;
pub use identity::*;
use serde::{Deserialize, Serialize};

pub const MAX_TEXT_CHARACTERS: usize = 100_000;

/// Pixels a re-created control may shift; its height may grow with its content.
const BOUNDS_TOLERANCE: i32 = 4;

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
#[derive(Clone, Copy, Default)]
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
        #[cfg(target_os = "linux")]
        {
            crate::linux_clipboard::copy_text(text)
        }
        #[cfg(target_os = "macos")]
        {
            crate::macos_clipboard::copy_text(text)
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            Err(PlatformError::UnsupportedPlatform)
        }
    }
    /// On Linux the snapshot is unidentified when no app reports a focused field.
    pub fn snapshot(&self) -> Result<TargetSnapshot, PlatformError> {
        #[cfg(windows)]
        {
            crate::windows_target::snapshot()
        }
        #[cfg(target_os = "linux")]
        {
            crate::linux_target::snapshot()
        }
        #[cfg(target_os = "macos")]
        {
            crate::macos_target::snapshot()
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            Ok(TargetSnapshot::unidentified())
        }
    }
    pub fn revalidate(&self, target: &TargetSnapshot) -> Result<(), PlatformError> {
        let actual = self.snapshot()?;
        if cfg!(target_os = "macos") && target.focused_runtime_id.is_empty() {
            return validate_frontmost_app(target, &actual);
        }
        validate_identity(target, &actual)
    }
    /// Whether a dictation into `target` can be typed rather than handed over on the clipboard.
    pub fn can_type(&self, target: &TargetSnapshot) -> bool {
        #[cfg(windows)]
        {
            let _ = target;
            true
        }
        #[cfg(target_os = "linux")]
        {
            crate::linux_target::can_type(target)
        }
        #[cfg(target_os = "macos")]
        {
            crate::macos_target::can_type(target)
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            let _ = target;
            false
        }
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
        #[cfg(target_os = "linux")]
        {
            crate::linux_target::read_text(target, max_characters, true)
        }
        #[cfg(target_os = "macos")]
        {
            crate::macos_target::read_text(target, max_characters, true)
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            let _ = target;
            Err(PlatformError::UnsupportedPlatform)
        }
    }
    /// The selection a rewrite replaces, read when it starts and again just before the paste.
    pub fn selected_text_for_rewrite(
        &self,
        target: &TargetSnapshot,
        max_characters: usize,
    ) -> Result<String, PlatformError> {
        self.selected_text(target, max_characters)
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
        #[cfg(target_os = "linux")]
        {
            crate::linux_target::preceding_text(target, max_characters)
        }
        #[cfg(target_os = "macos")]
        {
            crate::macos_target::preceding_text(target, max_characters)
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
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
        #[cfg(target_os = "linux")]
        {
            crate::linux_target::read_text(target, max_characters, false)
        }
        #[cfg(target_os = "macos")]
        {
            crate::macos_target::read_text(target, max_characters, false)
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
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
        #[cfg(target_os = "linux")]
        {
            crate::linux_target::paste(target, text, shortcut, &should_cancel)
        }
        #[cfg(target_os = "macos")]
        {
            crate::macos_target::paste(target, text, shortcut, &should_cancel)
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
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
        let current = self.selected_text_for_rewrite(target, MAX_TEXT_CHARACTERS)?;
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
        #[cfg(target_os = "linux")]
        {
            crate::linux_target::select_unique_text(
                target,
                expected_document,
                needle,
                &should_cancel,
            )?;
            self.replace_selection_guarded(target, needle, replacement, shortcut, should_cancel)
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            let _ = (target, needle, replacement, shortcut);
            Err(PlatformError::UnsupportedPlatform)
        }
    }
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
    /// What a Mac snapshot holds for an app that shows Onda no field.
    fn app_without_field() -> TargetSnapshot {
        TargetSnapshot {
            focused_runtime_id: Vec::new(),
            focused_control: ControlFingerprint {
                process_id: 2,
                ..ControlFingerprint::default()
            },
            integrity_level: 0,
            ..target()
        }
    }
    #[test]
    fn an_app_without_a_field_is_typed_into_only_while_it_keeps_the_same_window_in_front() {
        let expected = app_without_field();
        assert!(validate_frontmost_app(&expected, &app_without_field()).is_ok());
        // The app shows a field by the time the text is ready: still the same window.
        assert!(validate_frontmost_app(&expected, &target()).is_ok());
        let mut other_window = app_without_field();
        other_window.window_handle = 9;
        let mut other_app = app_without_field();
        other_app.process_id = 77;
        let mut reused_pid = app_without_field();
        reused_pid.process_created_at += 1;
        for changed in [other_window, other_app, reused_pid] {
            assert!(matches!(
                validate_frontmost_app(&expected, &changed),
                Err(PlatformError::TargetChanged)
            ));
        }
        let mut secure_input = app_without_field();
        secure_input.is_password = true;
        assert!(matches!(
            validate_frontmost_app(&expected, &secure_input),
            Err(PlatformError::SecureField)
        ));
        for unknown in [TargetSnapshot::unidentified(), {
            let mut no_window = app_without_field();
            no_window.window_handle = 0;
            no_window
        }] {
            assert!(matches!(
                validate_frontmost_app(&unknown, &unknown.clone()),
                Err(PlatformError::TargetUnavailable(_))
            ));
        }
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
