use super::*;

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

impl ControlFingerprint {
    /// Chromium-based apps (Electron, WebView2, browsers) can rebuild their accessibility tree
    /// while Onda waits for transcription or polish, giving the same field a new runtime ID.
    /// The same process, control type, automation ID and position still identify that field;
    /// a different field in the same window sits elsewhere.
    pub(super) fn same_field(&self, other: &Self) -> bool {
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

    /// The largest shift in left edge, top edge or width, for content-free diagnostics.
    pub(super) fn distance(&self, other: &Self) -> i32 {
        let [left, top, right, _] = self.bounds;
        let [other_left, other_top, other_right, _] = other.bounds;
        (left - other_left)
            .abs()
            .max((top - other_top).abs())
            .max(((right - left) - (other_right - other_left)).abs())
    }
}

impl TargetSnapshot {
    /// No window or control identity; app profiles never match it.
    #[cfg_attr(windows, allow(dead_code))]
    pub(crate) fn unidentified() -> Self {
        Self {
            window_handle: 0,
            process_id: 0,
            process_created_at: 0,
            executable_name: String::new(),
            focused_runtime_id: Vec::new(),
            is_password: false,
            integrity_level: 0,
            focused_control: ControlFingerprint::default(),
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

/// A Mac app that shows Onda no field is still named, with its window: the paste goes ahead
/// only while the same app, started at the same moment, has the same window in front.
pub(crate) fn validate_frontmost_app(
    expected: &TargetSnapshot,
    actual: &TargetSnapshot,
) -> Result<(), PlatformError> {
    if expected.is_password || actual.is_password {
        return Err(PlatformError::SecureField);
    }
    if expected.process_id == 0 || expected.window_handle == 0 {
        return Err(PlatformError::TargetUnavailable(
            "no frontmost app or window".into(),
        ));
    }
    let same_process = expected.process_id == actual.process_id
        && expected.process_created_at == actual.process_created_at;
    if !same_process || expected.window_handle != actual.window_handle {
        tracing::warn!(
            subsystem = "target",
            app = %expected.executable_name,
            same_process,
            same_window = expected.window_handle == actual.window_handle,
            "frontmost app or window changed"
        );
        return Err(PlatformError::TargetChanged);
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
            same_type = expected.focused_control.control_type == actual.focused_control.control_type,
            expected_type = expected.focused_control.control_type,
            actual_type = actual.focused_control.control_type,
            same_automation_id =
                expected.focused_control.automation_id == actual.focused_control.automation_id,
            framework = %expected.focused_control.framework_id,
            same_framework =
                expected.focused_control.framework_id == actual.focused_control.framework_id,
            moved_px = expected.focused_control.distance(&actual.focused_control),
            "target identity changed"
        );
        return Err(PlatformError::TargetChanged);
    }
    Ok(())
}
