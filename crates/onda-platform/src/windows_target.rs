use crate::{target::validate_identity, PlatformError, TargetSnapshot};
use std::ffi::c_void;
use windows::{
    core::{Error as WindowsError, BSTR},
    Win32::{
        Foundation::RPC_E_CHANGED_MODE,
        System::{
            Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
                COINIT_MULTITHREADED,
            },
            Ole::{SafeArrayDestroy, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound},
        },
        UI::Accessibility::*,
    },
};
use windows_sys::Win32::{
    Foundation::*, Security::*, System::Threading::*, UI::WindowsAndMessaging::*,
};

const UIA_TIMEOUT_MS: u32 = 750;
const MAX_RUNTIME_ID_ITEMS: usize = 64;
const MAX_TOKEN_BYTES: usize = 64 * 1024;

/// Joins the multithreaded apartment for the scope of one operation. A pooled worker that a
/// library already placed in a single-threaded apartment is used as it is (cross-process UI
/// Automation works from either); only an apartment this call entered is left again.
struct ComApartment {
    entered: bool,
}
impl ComApartment {
    fn initialize() -> Result<Self, PlatformError> {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result == RPC_E_CHANGED_MODE {
            return Ok(Self { entered: false });
        }
        result.ok().map_err(uia_error)?;
        Ok(Self { entered: true })
    }
}
impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.entered {
            unsafe {
                CoUninitialize();
            }
        }
    }
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn automation() -> Result<IUIAutomation2, PlatformError> {
    let automation: IUIAutomation2 = unsafe {
        CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).map_err(uia_error)?
    };
    unsafe {
        automation
            .SetConnectionTimeout(UIA_TIMEOUT_MS)
            .map_err(uia_error)?;
        automation
            .SetTransactionTimeout(UIA_TIMEOUT_MS)
            .map_err(uia_error)?;
    }
    Ok(automation)
}

#[cfg(debug_assertions)]
pub(crate) fn focus_owned_debug_edit(hwnd: isize) -> Result<(), PlatformError> {
    let mut process_id = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd as HWND, &mut process_id);
    }
    if process_id != unsafe { GetCurrentProcessId() } || unsafe { IsWindow(hwnd as HWND) } == 0 {
        return Err(PlatformError::TargetChanged);
    }
    let _apartment = ComApartment::initialize()?;
    let automation = automation()?;
    let element = unsafe {
        automation
            .ElementFromHandle(windows::Win32::Foundation::HWND(hwnd as *mut c_void))
            .map_err(uia_error)?
    };
    if unsafe { element.CurrentProcessId().map_err(uia_error)? } as u32 != process_id {
        return Err(PlatformError::TargetChanged);
    }
    unsafe {
        element.SetFocus().map_err(uia_error)?;
    }
    Ok(())
}

pub(crate) fn snapshot() -> Result<TargetSnapshot, PlatformError> {
    let _apartment = ComApartment::initialize()?;
    let automation = automation()?;
    let (snapshot, _) = snapshot_with(&automation)?;
    Ok(snapshot)
}

fn snapshot_with(
    automation: &IUIAutomation2,
) -> Result<(TargetSnapshot, IUIAutomationElement), PlatformError> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return Err(PlatformError::TargetUnavailable(
            "no foreground window".into(),
        ));
    }
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    if pid == 0 {
        return Err(PlatformError::TargetUnavailable(
            "foreground process unavailable".into(),
        ));
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return Err(PlatformError::TargetUnavailable(
            "foreground process cannot be inspected".into(),
        ));
    }
    let process = Handle(process);
    let integrity_level = integrity(process.0)?;
    if integrity_level > integrity(unsafe { GetCurrentProcess() })? {
        return Err(PlatformError::ElevatedTarget);
    }
    let mut created: FILETIME = unsafe { std::mem::zeroed() };
    let mut exited = created;
    let mut kernel = created;
    let mut user = created;
    if unsafe { GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user) } == 0
    {
        return Err(PlatformError::TargetUnavailable(
            "process lifetime unavailable".into(),
        ));
    }
    let mut path = vec![0u16; 32768];
    let mut length = path.len() as u32;
    if unsafe { QueryFullProcessImageNameW(process.0, 0, path.as_mut_ptr(), &mut length) } == 0 {
        return Err(PlatformError::TargetUnavailable(
            "process identity unavailable".into(),
        ));
    }
    let full_path = String::from_utf16_lossy(&path[..length as usize]);
    let executable_name = full_path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(&full_path)
        .to_owned();
    let element = unsafe { automation.GetFocusedElement().map_err(uia_error)? };
    let element_pid = unsafe { element.CurrentProcessId().map_err(uia_error)? } as u32;
    if element_pid != pid && !hosted_in(hwnd, element_pid)? {
        tracing::warn!(
            subsystem = "target",
            app = %executable_name,
            "focused control belongs to another process"
        );
        return Err(PlatformError::TargetChanged);
    }
    let is_password = unsafe { element.CurrentIsPassword().map_err(uia_error)? }.as_bool();
    if is_password {
        return Err(PlatformError::SecureField);
    }
    // A provider returning a new identity is safer to refuse than guess about the field.
    let focused_runtime_id = runtime_id(&element)?;
    let focused_control = fingerprint(&element, element_pid);
    let snapshot = TargetSnapshot {
        window_handle: hwnd as isize,
        process_id: pid,
        process_created_at: (created.dwHighDateTime as u64) << 32 | created.dwLowDateTime as u64,
        executable_name,
        focused_runtime_id,
        is_password,
        integrity_level,
        focused_control,
    };
    if unsafe { GetForegroundWindow() } != hwnd {
        return Err(PlatformError::TargetChanged);
    }
    Ok((snapshot, element))
}

/// Structural properties only; a property a provider cannot report stays empty, and an
/// empty rectangle disables the re-created-control allowance.
fn fingerprint(element: &IUIAutomationElement, process_id: u32) -> crate::ControlFingerprint {
    unsafe {
        crate::ControlFingerprint {
            process_id,
            control_type: element
                .CurrentControlType()
                .map(|kind| kind.0)
                .unwrap_or_default(),
            automation_id: element
                .CurrentAutomationId()
                .map(|id| id.to_string())
                .unwrap_or_default(),
            framework_id: element
                .CurrentFrameworkId()
                .map(|id| id.to_string())
                .unwrap_or_default(),
            bounds: element
                .CurrentBoundingRectangle()
                .map(|rect| [rect.left, rect.top, rect.right, rect.bottom])
                .unwrap_or_default(),
        }
    }
}

/// Hosted content (WebView2, CEF and similar) is rendered by a helper process whose child
/// window sits inside the app's top-level window. Accept such an element only when that
/// process owns the window holding keyboard focus inside `foreground`, and runs at no higher
/// integrity than this application. The target identity remains the top-level app.
fn hosted_in(foreground: HWND, element_pid: u32) -> Result<bool, PlatformError> {
    let thread = unsafe { GetWindowThreadProcessId(foreground, std::ptr::null_mut()) };
    let mut info: GUITHREADINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
    if unsafe { GetGUIThreadInfo(thread, &mut info) } == 0 || info.hwndFocus.is_null() {
        return Ok(false);
    }
    if unsafe { GetAncestor(info.hwndFocus, GA_ROOT) } != foreground {
        return Ok(false);
    }
    let mut focus_pid = 0;
    unsafe {
        GetWindowThreadProcessId(info.hwndFocus, &mut focus_pid);
    }
    if focus_pid != element_pid {
        return Ok(false);
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, element_pid) };
    if process.is_null() {
        return Ok(false);
    }
    let process = Handle(process);
    if integrity(process.0)? > integrity(unsafe { GetCurrentProcess() })? {
        return Err(PlatformError::ElevatedTarget);
    }
    Ok(true)
}

fn runtime_id(element: &IUIAutomationElement) -> Result<Vec<i32>, PlatformError> {
    let array = unsafe { element.GetRuntimeId().map_err(uia_error)? };
    if array.is_null() {
        return Err(PlatformError::TargetUnavailable(
            "empty focused runtime identity".into(),
        ));
    }
    let result = (|| {
        let lower = unsafe { SafeArrayGetLBound(array, 1).map_err(uia_error)? };
        let upper = unsafe { SafeArrayGetUBound(array, 1).map_err(uia_error)? };
        let length = (upper as i64 - lower as i64 + 1) as usize;
        if length == 0 || length > MAX_RUNTIME_ID_ITEMS {
            return Err(PlatformError::TargetUnavailable(
                "invalid focused runtime identity".into(),
            ));
        }
        let mut values = Vec::with_capacity(length);
        for index in lower..=upper {
            let mut value: i32 = 0;
            unsafe {
                SafeArrayGetElement(array, &index, &mut value as *mut _ as *mut c_void)
                    .map_err(uia_error)?;
            }
            values.push(value);
        }
        Ok(values)
    })();
    unsafe {
        let _ = SafeArrayDestroy(array);
    }
    result
}

fn integrity(process: HANDLE) -> Result<u32, PlatformError> {
    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(PlatformError::TargetUnavailable(
            "process token unavailable".into(),
        ));
    }
    let token = Handle(token);
    let mut required = 0;
    unsafe {
        GetTokenInformation(
            token.0,
            TokenIntegrityLevel,
            std::ptr::null_mut(),
            0,
            &mut required,
        );
    }
    if required == 0 || required as usize > MAX_TOKEN_BYTES {
        return Err(PlatformError::TargetUnavailable(
            "invalid integrity token".into(),
        ));
    }
    let mut storage = vec![0u64; (required as usize).div_ceil(std::mem::size_of::<u64>())];
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenIntegrityLevel,
            storage.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    } == 0
    {
        return Err(PlatformError::TargetUnavailable(
            "integrity inspection failed".into(),
        ));
    }
    let label = unsafe { &*(storage.as_ptr() as *const TOKEN_MANDATORY_LABEL) };
    let count = unsafe { GetSidSubAuthorityCount(label.Label.Sid) };
    if count.is_null() || unsafe { *count } == 0 {
        return Err(PlatformError::TargetUnavailable(
            "invalid integrity SID".into(),
        ));
    }
    let level = unsafe { GetSidSubAuthority(label.Label.Sid, (*count - 1) as u32) };
    if level.is_null() {
        return Err(PlatformError::TargetUnavailable(
            "invalid integrity SID authority".into(),
        ));
    }
    Ok(unsafe { *level })
}

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

fn uia_error(error: WindowsError) -> PlatformError {
    PlatformError::TargetUnavailable(format!("UI Automation HRESULT {}", error.code().0))
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
