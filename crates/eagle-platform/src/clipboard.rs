use crate::{InsertionOutcome, PasteShortcut, PlatformError, TargetSnapshot, WindowsTargetService};
use std::{
    ptr,
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{DataExchange::*, LibraryLoader::GetModuleHandleW, Memory::*, Ole::OleDuplicateData},
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

const UNICODE_TEXT: u32 = 13;
const BITMAP_FORMAT: u32 = 2;
const PALETTE_FORMAT: u32 = 9;
const ENHANCED_METAFILE_FORMAT: u32 = 14;
const MAX_CLIPBOARD_BYTES: usize = 64 * 1024 * 1024;
const MAX_CLIPBOARD_FORMATS: usize = 128;
const PASTE_RECEIPT_TIMEOUT: Duration = Duration::from_millis(1000);
/// Clipboard managers and history briefly hold the clipboard after every change.
const OPEN_ATTEMPTS: u32 = 40;
const OPEN_RETRY_DELAY: Duration = Duration::from_millis(15);
const READ_SETTLE_DELAY: Duration = Duration::from_millis(200);
const INPUT_MARKER: usize = 0x4541474c;
static TRANSACTION: Mutex<()> = Mutex::new(());

#[cfg(test)]
#[path = "desktop_tests.rs"]
mod desktop_tests;

struct SavedFormat {
    format: u32,
    handle: HANDLE,
}
impl Drop for SavedFormat {
    fn drop(&mut self) {
        if self.handle.is_null() {
            return;
        }
        unsafe {
            match self.format {
                BITMAP_FORMAT | PALETTE_FORMAT => {
                    DeleteObject(self.handle);
                }
                ENHANCED_METAFILE_FORMAT => {
                    DeleteEnhMetaFile(self.handle);
                }
                _ => {
                    GlobalFree(self.handle);
                }
            }
        }
    }
}

struct ClipboardLease;
impl ClipboardLease {
    fn open(owner: HWND) -> Result<Self, PlatformError> {
        for _ in 0..OPEN_ATTEMPTS {
            if unsafe { OpenClipboard(owner) } != 0 {
                return Ok(Self);
            }
            thread::sleep(OPEN_RETRY_DELAY);
        }
        Err(PlatformError::Clipboard("clipboard busy".into()))
    }
}
impl Drop for ClipboardLease {
    fn drop(&mut self) {
        unsafe {
            CloseClipboard();
        }
    }
}

struct RenderState {
    text: Option<SavedFormat>,
    read_at: Option<Instant>,
    sequence: u32,
}
struct OwnerWindow {
    hwnd: HWND,
    state: Box<RenderState>,
}
impl Drop for OwnerWindow {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            DestroyWindow(self.hwnd);
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_RENDERFORMAT && wparam as u32 == UNICODE_TEXT {
        let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RenderState;
        if !state.is_null() {
            let state = &mut *state;
            if let Some(mut text) = state.text.take() {
                // Windows already opened the clipboard for WM_RENDERFORMAT.
                if !SetClipboardData(UNICODE_TEXT, text.handle).is_null() {
                    text.handle = ptr::null_mut();
                    state.read_at = Some(Instant::now());
                    state.sequence = GetClipboardSequenceNumber();
                } else {
                    state.text = Some(text);
                }
            }
        }
        return 0;
    }
    DefWindowProcW(hwnd, message, wparam, lparam)
}

fn owner_window(text: &str) -> Result<OwnerWindow, PlatformError> {
    let class_name = wide("EagleClipboardTransaction");
    let instance = unsafe { GetModuleHandleW(ptr::null()) };
    let class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        ..unsafe { std::mem::zeroed() }
    };
    unsafe {
        SetLastError(0);
    }
    if unsafe { RegisterClassW(&class) } == 0
        && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS
    {
        return Err(PlatformError::Clipboard(
            "clipboard owner registration failed".into(),
        ));
    }
    let utf16 = wide(text);
    let bytes = unsafe { std::slice::from_raw_parts(utf16.as_ptr().cast::<u8>(), utf16.len() * 2) };
    let text = allocate_format(UNICODE_TEXT, bytes)?;
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            ptr::null_mut(),
            instance,
            ptr::null(),
        )
    };
    if hwnd.is_null() {
        return Err(PlatformError::Clipboard(
            "clipboard owner window failed".into(),
        ));
    }
    let mut owner = OwnerWindow {
        hwnd,
        state: Box::new(RenderState {
            text: Some(text),
            read_at: None,
            sequence: 0,
        }),
    };
    unsafe {
        SetWindowLongPtrW(
            hwnd,
            GWLP_USERDATA,
            owner.state.as_mut() as *mut RenderState as isize,
        );
    }
    Ok(owner)
}

pub(crate) fn paste(
    service: &WindowsTargetService,
    target: &TargetSnapshot,
    text: &str,
    shortcut: PasteShortcut,
    should_cancel: &dyn Fn() -> bool,
) -> Result<InsertionOutcome, PlatformError> {
    let _transaction = TRANSACTION
        .try_lock()
        .map_err(|_| PlatformError::Clipboard("another insertion is active".into()))?;
    ensure_modifiers_released()?;
    let mut owner = owner_window(text)?;
    // Allocate every temporary payload before destroying any existing clipboard content.
    let mut privacy_formats = privacy_formats()?;
    let mut saved = {
        let _lease = ClipboardLease::open(owner.hwnd)?;
        let mut saved = snapshot_formats()?;
        service.revalidate(target)?;
        ensure_modifiers_released()?;
        if should_cancel() {
            return Err(PlatformError::Cancelled);
        }
        if unsafe { EmptyClipboard() } == 0 {
            return Err(PlatformError::Clipboard("clipboard clear failed".into()));
        }
        let publish_result = publish(&mut owner, &mut privacy_formats);
        if let Err(error) = publish_result {
            let _ = restore_formats(&mut saved);
            return Err(error);
        }
        saved
    };
    let insertion = (|| {
        service.revalidate(target)?;
        ensure_modifiers_released()?;
        let mut events = chord(shortcut);
        if should_cancel() {
            return Err(PlatformError::Cancelled);
        }
        let accepted = unsafe {
            SendInput(
                events.len() as u32,
                events.as_mut_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        };
        if accepted != events.len() as u32 {
            // Complete key releases if Windows accepted only a chord prefix.
            let releases: Vec<INPUT> = events
                .into_iter()
                .filter(|event| unsafe { event.Anonymous.ki.dwFlags & KEYEVENTF_KEYUP != 0 })
                .collect();
            unsafe {
                SendInput(
                    releases.len() as u32,
                    releases.as_ptr(),
                    std::mem::size_of::<INPUT>() as i32,
                );
            }
            return Err(PlatformError::InjectionFailed);
        }
        Ok(())
    })();
    if insertion.is_ok() {
        wait_for_read(&mut owner);
    }
    let restored = restore_if_unchanged(&owner, &mut saved)?;
    insertion?;
    Ok(InsertionOutcome {
        input_injected: true,
        clipboard_read_observed: owner.state.read_at.is_some(),
        clipboard_restored: restored,
        clipboard_changed_by_other: !restored,
    })
}

pub(crate) fn copy_text(text: &str) -> Result<(), PlatformError> {
    let _transaction = TRANSACTION
        .try_lock()
        .map_err(|_| PlatformError::Clipboard("another clipboard operation is active".into()))?;
    let mut owner = owner_window(text)?;
    let mut privacy = privacy_formats()?;
    let _lease = ClipboardLease::open(owner.hwnd)?;
    if unsafe { EmptyClipboard() } == 0 {
        return Err(PlatformError::Clipboard("clipboard clear failed".into()));
    }
    for format in &mut privacy {
        transfer(format)?;
    }
    let mut payload = owner
        .state
        .text
        .take()
        .ok_or_else(|| PlatformError::Clipboard("clipboard payload missing".into()))?;
    transfer(&mut payload)
}

fn snapshot_formats() -> Result<Vec<SavedFormat>, PlatformError> {
    let mut saved = Vec::new();
    let mut format = 0;
    let mut total_bytes: usize = 0;
    loop {
        unsafe {
            SetLastError(0);
        }
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            if unsafe { GetLastError() } != 0 {
                return Err(PlatformError::Clipboard(
                    "clipboard enumeration failed".into(),
                ));
            }
            break;
        }
        if saved.len() >= MAX_CLIPBOARD_FORMATS || !preservable_format(format) {
            return Err(PlatformError::UnsupportedClipboardFormat(format));
        }
        let source = unsafe { GetClipboardData(format) };
        if source.is_null() {
            // A delayed-render owner that no longer answers: keep every format that can be kept
            // rather than refusing the user's dictation over one unrecoverable format.
            continue;
        }
        let size = format_size(format, source)?;
        total_bytes = total_bytes
            .checked_add(size)
            .ok_or_else(|| PlatformError::Clipboard("clipboard size overflow".into()))?;
        if total_bytes > MAX_CLIPBOARD_BYTES {
            return Err(PlatformError::Clipboard(
                "clipboard exceeds 64 MiB preservation limit".into(),
            ));
        }
        let handle = if format == ENHANCED_METAFILE_FORMAT {
            unsafe { CopyEnhMetaFileW(source, ptr::null()) }
        } else {
            unsafe { OleDuplicateData(source, format as u16, 0) }
        };
        if handle.is_null() {
            continue;
        }
        saved.push(SavedFormat { format, handle });
    }
    Ok(saved)
}

fn preservable_format(format: u32) -> bool {
    // Owner-display, private handles, and old metafile-pictures have custom ownership contracts.
    !matches!(format, 0 | 3 | 0x80..=0x8e | 0x200..=0x2ff) && format <= u16::MAX as u32
}

fn format_size(format: u32, handle: HANDLE) -> Result<usize, PlatformError> {
    let size = match format {
        BITMAP_FORMAT => {
            let mut bitmap: BITMAP = unsafe { std::mem::zeroed() };
            if unsafe {
                GetObjectW(
                    handle,
                    std::mem::size_of::<BITMAP>() as i32,
                    &mut bitmap as *mut _ as *mut _,
                )
            } == 0
            {
                return Err(PlatformError::UnsupportedClipboardFormat(format));
            }
            (bitmap.bmWidthBytes.unsigned_abs() as usize)
                .checked_mul(bitmap.bmHeight.unsigned_abs() as usize)
                .ok_or_else(|| PlatformError::Clipboard("bitmap size overflow".into()))?
        }
        PALETTE_FORMAT => {
            (unsafe { GetPaletteEntries(handle, 0, 0, ptr::null_mut()) }) as usize
                * std::mem::size_of::<PALETTEENTRY>()
        }
        ENHANCED_METAFILE_FORMAT => {
            (unsafe { GetEnhMetaFileBits(handle, 0, ptr::null_mut()) }) as usize
        }
        _ => unsafe { GlobalSize(handle) },
    };
    if size == 0 || size > MAX_CLIPBOARD_BYTES {
        return Err(PlatformError::UnsupportedClipboardFormat(format));
    }
    Ok(size)
}

fn publish(owner: &mut OwnerWindow, privacy: &mut [SavedFormat]) -> Result<(), PlatformError> {
    for format in privacy {
        transfer(format)?;
    }
    unsafe {
        SetLastError(0);
        SetClipboardData(UNICODE_TEXT, ptr::null_mut());
    }
    if unsafe { GetLastError() } != 0 {
        return Err(PlatformError::Clipboard(
            "delayed clipboard publication failed".into(),
        ));
    }
    owner.state.sequence = unsafe { GetClipboardSequenceNumber() };
    if owner.state.sequence == 0 {
        return Err(PlatformError::Clipboard(
            "clipboard sequence unavailable".into(),
        ));
    }
    Ok(())
}

fn restore_if_unchanged(
    owner: &OwnerWindow,
    saved: &mut [SavedFormat],
) -> Result<bool, PlatformError> {
    let _lease = ClipboardLease::open(owner.hwnd)?;
    // Compare after acquiring the clipboard lock to close the check/mutation race.
    if !may_restore(owner.state.sequence, unsafe {
        GetClipboardSequenceNumber()
    }) || unsafe { GetClipboardOwner() } != owner.hwnd
    {
        return Ok(false);
    }
    restore_formats(saved)?;
    Ok(true)
}

fn restore_formats(saved: &mut [SavedFormat]) -> Result<(), PlatformError> {
    if unsafe { EmptyClipboard() } == 0 {
        return Err(PlatformError::Clipboard(
            "clipboard restoration clear failed".into(),
        ));
    }
    for format in saved {
        transfer(format)?;
    }
    Ok(())
}

fn transfer(format: &mut SavedFormat) -> Result<(), PlatformError> {
    if unsafe { SetClipboardData(format.format, format.handle) }.is_null() {
        return Err(PlatformError::Clipboard(format!(
            "clipboard format {} publication failed",
            format.format
        )));
    }
    format.handle = ptr::null_mut();
    Ok(())
}

fn allocate_format(format: u32, bytes: &[u8]) -> Result<SavedFormat, PlatformError> {
    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) };
    if handle.is_null() {
        return Err(PlatformError::Clipboard(
            "clipboard allocation failed".into(),
        ));
    }
    let data = SavedFormat { format, handle };
    let target = unsafe { GlobalLock(handle) };
    if target.is_null() {
        return Err(PlatformError::Clipboard(
            "clipboard allocation lock failed".into(),
        ));
    }
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), target.cast(), bytes.len());
        GlobalUnlock(handle);
    }
    Ok(data)
}

fn privacy_formats() -> Result<Vec<SavedFormat>, PlatformError> {
    let mut formats = Vec::with_capacity(3);
    for (name, value) in [
        ("ExcludeClipboardContentFromMonitorProcessing", 1u32),
        ("CanIncludeInClipboardHistory", 0),
        ("CanUploadToCloudClipboard", 0),
    ] {
        let name = wide(name);
        let format = unsafe { RegisterClipboardFormatW(name.as_ptr()) };
        if format == 0 {
            return Err(PlatformError::Clipboard(
                "clipboard privacy marker failed".into(),
            ));
        }
        formats.push(allocate_format(format, &value.to_ne_bytes())?);
    }
    Ok(formats)
}

fn wait_for_read(owner: &mut OwnerWindow) {
    let started = Instant::now();
    loop {
        let mut message: MSG = unsafe { std::mem::zeroed() };
        while unsafe { PeekMessageW(&mut message, owner.hwnd, 0, 0, PM_REMOVE) } != 0 {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        if owner
            .state
            .read_at
            .is_some_and(|at| at.elapsed() >= READ_SETTLE_DELAY)
            || started.elapsed() >= PASTE_RECEIPT_TIMEOUT
        {
            break;
        }
        if unsafe { GetClipboardOwner() } != owner.hwnd {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn ensure_modifiers_released() -> Result<(), PlatformError> {
    if [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN]
        .iter()
        .any(|key| unsafe { GetAsyncKeyState(*key as i32) } < 0)
    {
        return Err(PlatformError::ModifiersHeld);
    }
    Ok(())
}

fn chord(shortcut: PasteShortcut) -> Vec<INPUT> {
    let keys: &[u16] = match shortcut {
        PasteShortcut::ControlV => &[VK_CONTROL, 0x56],
        PasteShortcut::ControlShiftV => &[VK_CONTROL, VK_SHIFT, 0x56],
        PasteShortcut::ShiftInsert => &[VK_SHIFT, VK_INSERT],
    };
    keys.iter()
        .map(|&key| key_event(key, false))
        .chain(keys.iter().rev().map(|&key| key_event(key, true)))
        .collect()
}
fn key_event(key: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: INPUT_MARKER,
            },
        },
    }
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}
fn may_restore(expected: u32, actual: u32) -> bool {
    expected != 0 && expected == actual
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn user_clipboard_change_and_unavailable_sequence_prevent_restore() {
        assert!(may_restore(42, 42));
        assert!(!may_restore(42, 43));
        assert!(!may_restore(0, 0));
    }
    #[test]
    fn private_or_owner_managed_formats_fail_closed() {
        for format in [3, 0x80, 0x200, 0x2ff] {
            assert!(!preservable_format(format));
        }
        for format in [2, 13, 14, 0xc001] {
            assert!(preservable_format(format));
        }
    }
    #[test]
    fn shortcut_releases_are_in_reverse_order() {
        let events = chord(PasteShortcut::ControlShiftV);
        assert_eq!(events.len(), 6);
        unsafe {
            assert_eq!(events[3].Anonymous.ki.wVk, 0x56);
            assert_eq!(events[5].Anonymous.ki.wVk, VK_CONTROL);
        }
    }
}
