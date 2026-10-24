//! Opt-in tests use only windows created by this test process and preserve the prior clipboard.
use super::*;
use std::sync::mpsc;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;

struct Fixture {
    hwnd: isize,
    edit: isize,
    thread_id: u32,
    worker: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    fn open(password: bool) -> Self {
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || unsafe {
            let instance = GetModuleHandleW(ptr::null());
            let class = wide("STATIC");
            let title = wide("Onda controlled integration test");
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                100,
                100,
                500,
                180,
                ptr::null_mut(),
                ptr::null_mut(),
                instance,
                ptr::null(),
            );
            let edit_class = wide("EDIT");
            let style = WS_CHILD
                | WS_VISIBLE
                | WS_BORDER
                | ES_MULTILINE as u32
                | if password { ES_PASSWORD as u32 } else { 0 };
            let edit = CreateWindowExW(
                0,
                edit_class.as_ptr(),
                ptr::null(),
                style,
                10,
                10,
                460,
                100,
                hwnd,
                ptr::null_mut(),
                instance,
                ptr::null(),
            );
            SetForegroundWindow(hwnd);
            SetFocus(edit);
            let _ = sender.send((hwnd as isize, edit as isize, GetCurrentThreadId()));
            let mut message: MSG = std::mem::zeroed();
            while GetMessageW(&mut message, ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            DestroyWindow(hwnd);
        });
        let (hwnd, edit, thread_id) = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("fixture creation timed out");
        assert_ne!(hwnd, 0);
        assert_ne!(edit, 0);
        Self {
            hwnd,
            edit,
            thread_id,
            worker: Some(worker),
        }
    }
    fn text(&self) -> String {
        let mut buffer = vec![0u16; 1024];
        let mut length = 0;
        let result = unsafe {
            SendMessageTimeoutW(
                self.edit as HWND,
                WM_GETTEXT,
                buffer.len(),
                buffer.as_mut_ptr() as isize,
                SMTO_ABORTIFHUNG,
                1000,
                &mut length,
            )
        };
        assert_ne!(result, 0, "fixture edit hung");
        String::from_utf16_lossy(&buffer[..length])
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        unsafe {
            PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct OriginalClipboard {
    owner: OwnerWindow,
    formats: Vec<SavedFormat>,
}
impl OriginalClipboard {
    fn save() -> Self {
        let owner = owner_window("fixture owner").expect("owner creation failed");
        let _lease = ClipboardLease::open(owner.hwnd).expect("clipboard unavailable");
        let formats = snapshot_formats().expect("prior clipboard cannot be preserved; do not run interactive tests with owner-managed clipboard content");
        Self { owner, formats }
    }
}
impl Drop for OriginalClipboard {
    fn drop(&mut self) {
        // This isolated test deliberately restores its fixture clipboard after its own operations.
        if let Ok(_lease) = ClipboardLease::open(self.owner.hwnd) {
            let _ = restore_formats(&mut self.formats);
        }
    }
}

#[test]
#[ignore = "requires ONDA_DESKTOP_TESTS=1 and an interactive Windows desktop; temporarily opens controlled windows"]
fn real_edit_unicode_paste_preserves_every_seeded_format_and_refuses_password() {
    assert_eq!(std::env::var("ONDA_DESKTOP_TESTS").as_deref(), Ok("1"));
    let fixture = Fixture::open(false);
    let _original = OriginalClipboard::save();
    let service = WindowsTargetService::new();
    let target = service
        .snapshot()
        .expect("UIA snapshot failed; activate the controlled test window and rerun");
    assert_eq!(
        target.window_handle, fixture.hwnd,
        "Windows did not grant test foreground activation"
    );
    service.copy_to_clipboard("clipboard fixture 🌊").unwrap();
    let rich_name = wide("Onda.Integration.CustomFormat");
    let rich_format = unsafe { RegisterClipboardFormatW(rich_name.as_ptr()) };
    let mut rich_payload = allocate_format(rich_format, b"fixture-rich-content\0").unwrap();
    {
        let _lease = ClipboardLease::open(_original.owner.hwnd).unwrap();
        transfer(&mut rich_payload).unwrap();
    }
    let text = "hello 🌊 café 日本語";
    let cancellation_checks = std::cell::Cell::new(0);
    const CANCEL_AT_INPUT_CHECK: u32 = 3;
    let cancelled = service.paste_guarded(
        &target,
        "cancelled text must not land",
        PasteShortcut::ControlV,
        || {
            let checks = cancellation_checks.get() + 1;
            cancellation_checks.set(checks);
            checks >= CANCEL_AT_INPUT_CHECK
        },
    );
    assert!(matches!(cancelled, Err(PlatformError::Cancelled)));
    assert_eq!(fixture.text(), "");
    let outcome = service
        .paste(&target, text, PasteShortcut::ControlV)
        .unwrap();
    assert!(outcome.input_injected);
    assert!(outcome.clipboard_read_observed);
    assert!(outcome.clipboard_restored);
    assert_eq!(fixture.text(), text);
    {
        let _lease = ClipboardLease::open(_original.owner.hwnd).unwrap();
        let source = unsafe { GetClipboardData(rich_format) };
        assert!(!source.is_null());
        let bytes = unsafe {
            std::slice::from_raw_parts(GlobalLock(source).cast::<u8>(), GlobalSize(source))
        };
        let expected = b"fixture-rich-content\0";
        assert_eq!(&bytes[..expected.len()], expected);
        unsafe {
            GlobalUnlock(source);
        }
    }
    let document = service.field_text(&target, 1000).unwrap();
    assert_eq!(document, text);
    let transformed = "transformed fixture 🌊";
    service
        .replace_unique_text(
            &target,
            &document,
            text,
            transformed,
            PasteShortcut::ControlV,
        )
        .unwrap();
    assert_eq!(fixture.text(), transformed);
    let after = service.field_text(&target, 1000).unwrap();
    service
        .replace_unique_text(&target, &after, transformed, text, PasteShortcut::ControlV)
        .unwrap();
    assert_eq!(fixture.text(), text);
    assert!(matches!(
        service.replace_unique_text(
            &target,
            "stale document",
            "stale",
            "unsafe replacement",
            PasteShortcut::ControlV
        ),
        Err(PlatformError::TargetChanged)
    ));
    assert_eq!(fixture.text(), text);
    let edit = fixture.edit;
    let copy_worker = thread::spawn(move || {
        let started = Instant::now();
        loop {
            let mut buffer = [0u16; 1024];
            let mut length = 0;
            let result = unsafe {
                SendMessageTimeoutW(
                    edit as HWND,
                    WM_GETTEXT,
                    buffer.len(),
                    buffer.as_mut_ptr() as isize,
                    SMTO_ABORTIFHUNG,
                    1000,
                    &mut length,
                )
            };
            if result != 0
                && String::from_utf16_lossy(&buffer[..length]).contains("clipboard race trigger")
            {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "paste did not reach fixture"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let mut external_owner = owner_window("simulated external copy").unwrap();
        let _lease = ClipboardLease::open(external_owner.hwnd).unwrap();
        assert_ne!(unsafe { EmptyClipboard() }, 0);
        let mut new_copy = external_owner.state.text.take().unwrap();
        transfer(&mut new_copy).unwrap();
    });
    let outcome = service
        .paste(&target, "clipboard race trigger", PasteShortcut::ControlV)
        .unwrap();
    copy_worker.join().unwrap();
    assert!(!outcome.clipboard_restored);
    assert!(outcome.clipboard_changed_by_other);
    drop(fixture);
    let _secure = Fixture::open(true);
    assert!(matches!(
        service.snapshot(),
        Err(PlatformError::SecureField)
    ));
}
