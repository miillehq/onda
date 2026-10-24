//! Debug-only owned Win32 target for full application inference-to-insertion tests.
//! This module is excluded from every release build and never reads previous user fields.
use crate::{PlatformError, TargetSnapshot, WindowsTargetService, MAX_TEXT_CHARACTERS};
use std::{
    ptr,
    sync::mpsc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::GetModuleHandleW, Threading::*},
    UI::{Input::KeyboardAndMouse::SetFocus, WindowsAndMessaging::*},
};

const WINDOW_TITLE: &str = "Onda controlled application test";
const WINDOW_WIDTH: i32 = 540;
const WINDOW_HEIGHT: i32 = 220;
const EDIT_PADDING: i32 = 12;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT_MS: u32 = 1000;
const ACTIVATION_TIMEOUT: Duration = Duration::from_millis(1500);
const ACTIVATION_POLL_INTERVAL: Duration = Duration::from_millis(25);

struct ForegroundIdentity {
    hwnd: isize,
    process_id: u32,
    process_created_at: u64,
}
struct OwnedHandles {
    hwnd: isize,
    edit: isize,
    thread_id: u32,
    foreground_request_accepted: bool,
}

/// A dedicated UI thread hosts one empty edit control; all text reads address that control only.
pub struct OwnedEditWindow {
    handles: OwnedHandles,
    worker: Option<JoinHandle<()>>,
    original_foreground: Option<ForegroundIdentity>,
}

impl OwnedEditWindow {
    /// Create/focus the controlled target. Failure to obtain foreground focus is explicit.
    pub fn create() -> Result<Self, PlatformError> {
        let original_foreground = foreground_identity();
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("onda-owned-debug-edit".into())
            .spawn(move || unsafe {
                let instance = GetModuleHandleW(ptr::null());
                let class = wide("STATIC");
                let title = wide(WINDOW_TITLE);
                let hwnd = CreateWindowExW(
                    0,
                    class.as_ptr(),
                    title.as_ptr(),
                    WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                    100,
                    100,
                    WINDOW_WIDTH,
                    WINDOW_HEIGHT,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    instance,
                    ptr::null(),
                );
                if hwnd.is_null() {
                    let _ = sender.send(Err(PlatformError::TargetUnavailable(
                        "owned debug window creation failed".into(),
                    )));
                    return;
                }
                let edit_class = wide("EDIT");
                let edit = CreateWindowExW(
                    0,
                    edit_class.as_ptr(),
                    ptr::null(),
                    WS_CHILD | WS_VISIBLE | WS_BORDER | ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32,
                    EDIT_PADDING,
                    EDIT_PADDING,
                    WINDOW_WIDTH - EDIT_PADDING * 3,
                    WINDOW_HEIGHT - EDIT_PADDING * 5,
                    hwnd,
                    ptr::null_mut(),
                    instance,
                    ptr::null(),
                );
                if edit.is_null() {
                    DestroyWindow(hwnd);
                    let _ = sender.send(Err(PlatformError::TargetUnavailable(
                        "owned debug edit creation failed".into(),
                    )));
                    return;
                }
                // Hidden STARTUPINFO can override the first show; only this owned fixture is shown.
                ShowWindow(hwnd, SW_SHOWNORMAL);
                if IsWindowVisible(hwnd) == 0 {
                    ShowWindow(hwnd, SW_SHOWNORMAL);
                }
                let foreground_request_accepted = SetForegroundWindow(hwnd) != 0;
                SetFocus(edit);
                let mut message: MSG = std::mem::zeroed();
                PeekMessageW(&mut message, ptr::null_mut(), 0, 0, PM_NOREMOVE);
                if sender
                    .send(Ok(OwnedHandles {
                        hwnd: hwnd as isize,
                        edit: edit as isize,
                        thread_id: GetCurrentThreadId(),
                        foreground_request_accepted,
                    }))
                    .is_err()
                {
                    DestroyWindow(hwnd);
                    return;
                }
                while GetMessageW(&mut message, ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                if IsWindow(hwnd) != 0 {
                    DestroyWindow(hwnd);
                }
            })
            .map_err(|_| {
                PlatformError::TargetUnavailable("owned debug UI thread unavailable".into())
            })?;
        let handles = receiver
            .recv_timeout(STARTUP_TIMEOUT)
            .map_err(|_| PlatformError::Timeout)??;
        let window = Self {
            handles,
            worker: Some(worker),
            original_foreground,
        };
        window.wait_for_activation()?;
        Ok(window)
    }

    fn wait_for_activation(&self) -> Result<(), PlatformError> {
        if self.wait_for_owned_foreground()? {
            return Ok(());
        }
        self.validate_owned_control()?;
        // UIA addresses only this known owned edit and retains ordinary Windows security checks.
        if crate::windows_target::focus_owned_debug_edit(self.handles.edit).is_ok()
            && self.wait_for_owned_foreground()?
        {
            return Ok(());
        }
        let reason = if self.handles.foreground_request_accepted {
            "owned debug window activation did not settle within its deadline"
        } else {
            "Windows denied owned debug foreground activation"
        };
        Err(PlatformError::TargetUnavailable(reason.into()))
    }

    fn wait_for_owned_foreground(&self) -> Result<bool, PlatformError> {
        let started = Instant::now();
        while started.elapsed() < ACTIVATION_TIMEOUT {
            if unsafe { GetForegroundWindow() } == self.handles.hwnd as HWND {
                match self.snapshot() {
                    Ok(_) => return Ok(true),
                    Err(PlatformError::TargetChanged) => {}
                    Err(error) => return Err(error),
                }
            }
            thread::sleep(ACTIVATION_POLL_INTERVAL);
        }
        Ok(false)
    }

    /// Capture the native target only while this owned test window still has focus.
    pub fn snapshot(&self) -> Result<TargetSnapshot, PlatformError> {
        self.validate_owned_control()?;
        if unsafe { GetForegroundWindow() } != self.handles.hwnd as HWND {
            return Err(PlatformError::TargetChanged);
        }
        let target = WindowsTargetService::new().snapshot()?;
        if target.window_handle != self.handles.hwnd
            || target.process_id != unsafe { GetCurrentProcessId() }
        {
            return Err(PlatformError::TargetChanged);
        }
        Ok(target)
    }

    /// Read bounded text from this fixture's own edit control, never from a prior foreground field.
    pub fn read_text(&self) -> Result<String, PlatformError> {
        self.validate_owned_control()?;
        let mut buffer = vec![0u16; MAX_TEXT_CHARACTERS + 1];
        let mut length = 0;
        let accepted = unsafe {
            SendMessageTimeoutW(
                self.handles.edit as HWND,
                WM_GETTEXT,
                buffer.len(),
                buffer.as_mut_ptr() as isize,
                SMTO_ABORTIFHUNG,
                READ_TIMEOUT_MS,
                &mut length,
            )
        };
        if accepted == 0 {
            return Err(PlatformError::Timeout);
        }
        if length >= MAX_TEXT_CHARACTERS {
            return Err(PlatformError::TextLimit);
        }
        self.validate_owned_control()?;
        Ok(String::from_utf16_lossy(&buffer[..length]))
    }

    fn validate_owned_control(&self) -> Result<(), PlatformError> {
        if self.worker.as_ref().is_none_or(JoinHandle::is_finished) {
            return Err(PlatformError::TargetUnavailable(
                "owned debug UI thread ended".into(),
            ));
        }
        let mut process_id = 0;
        let thread_id =
            unsafe { GetWindowThreadProcessId(self.handles.edit as HWND, &mut process_id) };
        if thread_id != self.handles.thread_id
            || process_id != unsafe { GetCurrentProcessId() }
            || unsafe { IsWindow(self.handles.edit as HWND) } == 0
        {
            return Err(PlatformError::TargetUnavailable(
                "owned debug edit ended".into(),
            ));
        }
        Ok(())
    }
}

impl Drop for OwnedEditWindow {
    fn drop(&mut self) {
        if let Some(previous) = &self.original_foreground {
            let mut process_id = 0;
            unsafe {
                GetWindowThreadProcessId(previous.hwnd as HWND, &mut process_id);
            }
            if process_id == previous.process_id
                && process_created_at(process_id) == Some(previous.process_created_at)
                && unsafe { IsWindow(previous.hwnd as HWND) } != 0
                && unsafe { GetForegroundWindow() } == self.handles.hwnd as HWND
            {
                // Restore before joining so later user focus changes during teardown are preserved.
                unsafe {
                    SetForegroundWindow(previous.hwnd as HWND);
                }
            }
        }
        unsafe {
            PostThreadMessageW(self.handles.thread_id, WM_QUIT, 0, 0);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn foreground_identity() -> Option<ForegroundIdentity> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return None;
    }
    let mut process_id = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut process_id);
    }
    Some(ForegroundIdentity {
        hwnd: hwnd as isize,
        process_id,
        process_created_at: process_created_at(process_id)?,
    })
}

fn process_created_at(process_id: u32) -> Option<u64> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return None;
    }
    let mut created: FILETIME = unsafe { std::mem::zeroed() };
    let mut exited = created;
    let mut kernel = created;
    let mut user = created;
    let accepted =
        unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) };
    unsafe {
        CloseHandle(process);
    }
    (accepted != 0).then_some((created.dwHighDateTime as u64) << 32 | created.dwLowDateTime as u64)
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}
