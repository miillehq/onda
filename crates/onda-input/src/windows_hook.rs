use onda_input::{
    HelperCommand, HelperConfig, HelperEvent, InputError, ShortcutMatcher, EVENT_QUEUE_CAPACITY,
    HEARTBEAT_INTERVAL_MS, MAX_PROTOCOL_LINE_BYTES,
};
use std::{
    cell::RefCell,
    io::{BufRead, Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{
        LibraryLoader::GetModuleHandleW,
        StationsAndDesktops::{GetThreadDesktop, GetUserObjectInformationW, HDESK, UOI_IO},
        Threading::{
            GetCurrentProcess, GetCurrentThreadId, SetPriorityClass, ABOVE_NORMAL_PRIORITY_CLASS,
        },
    },
    UI::{Input::KeyboardAndMouse::GetAsyncKeyState, WindowsAndMessaging::*},
};

const HEARTBEAT_TIMER: usize = 1;
const REARM_INTERVAL_MS: u64 = 30_000;
const OPERATION_ACTIVE_MESSAGE: u32 = WM_APP + 7;
const MAX_CONTROL_LINE_BYTES: usize = 256;
const MODIFIER_KEYS: [u16; 8] = [0x5b, 0x5c, 0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5];
static QUEUE_FAILED: AtomicBool = AtomicBool::new(false);
thread_local! { static STATE: RefCell<Option<HookState>> = const { RefCell::new(None) }; }

#[derive(Clone, Copy)]
enum HookEvent {
    Ready,
    Heartbeat(u64),
    Pressed(usize),
    Released(usize),
    Reset,
}
struct HookState {
    matcher: ShortcutMatcher,
    sender: SyncSender<HookEvent>,
}

/// Borrows the owning thread's desktop; querying its input flag never observes user keys.
struct InputDesktop {
    handle: HDESK,
}

impl InputDesktop {
    fn current() -> Result<Self, InputError> {
        let handle = unsafe { GetThreadDesktop(GetCurrentThreadId()) };
        if handle.is_null() {
            return Err(InputError::Helper("hookDesktopUnavailable".into()));
        }
        Ok(Self { handle })
    }

    /// Read only input-desktop availability; callers fail closed on unavailable metadata.
    fn receives_input(&self) -> Option<bool> {
        let mut receives_input: BOOL = 0;
        let available = unsafe {
            GetUserObjectInformationW(
                self.handle,
                UOI_IO,
                (&mut receives_input as *mut BOOL).cast(),
                std::mem::size_of::<BOOL>() as u32,
                std::ptr::null_mut(),
            )
        };
        (available != 0).then_some(receives_input != 0)
    }
}

/// The callback never writes stdout, waits on a mutex, allocates, or records arbitrary keys.
unsafe extern "system" fn keyboard_hook(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    if code != HC_ACTION as i32 {
        return CallNextHookEx(std::ptr::null_mut(), code, message, data);
    }
    let keyboard = &*(data as *const KBDLLHOOKSTRUCT);
    let down = match message as u32 {
        WM_KEYDOWN | WM_SYSKEYDOWN => true,
        WM_KEYUP | WM_SYSKEYUP => false,
        _ => return CallNextHookEx(std::ptr::null_mut(), code, message, data),
    };
    let suppress = STATE.with(|state| {
        let Ok(mut state) = state.try_borrow_mut() else {
            return false;
        };
        let Some(state) = state.as_mut() else {
            return false;
        };
        let matched = state.matcher.handle(
            keyboard.vkCode as u16,
            down,
            keyboard.flags & LLKHF_INJECTED != 0,
        );
        if let (Some((_, pressed)), Some(binding_index)) = (matched.event, matched.binding_index) {
            let event = if pressed {
                HookEvent::Pressed(binding_index)
            } else {
                HookEvent::Released(binding_index)
            };
            if state.sender.try_send(event).is_err() {
                QUEUE_FAILED.store(true, Ordering::Relaxed);
            }
        }
        matched.suppress
    });
    if suppress {
        1
    } else {
        CallNextHookEx(std::ptr::null_mut(), code, message, data)
    }
}

pub fn run() -> Result<(), InputError> {
    let config = read_config()?;
    let bindings = config.bindings.clone();
    let mut matcher = ShortcutMatcher::new(config)?;
    let input_desktop = InputDesktop::current()?;
    matcher.set_desktop_active(input_desktop.receives_input().unwrap_or(false));
    seed_modifiers(&mut matcher);
    let (sender, receiver) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
    let output_worker = thread::Builder::new()
        .name("onda-hook-output".into())
        .spawn(move || {
            let stdout = std::io::stdout();
            let mut output = stdout.lock();
            while let Ok(event) = receiver.recv() {
                let event = match event {
                    HookEvent::Ready => HelperEvent::Ready {
                        protocol_version: onda_input::PROTOCOL_VERSION,
                    },
                    HookEvent::Heartbeat(uptime_ms) => HelperEvent::Heartbeat { uptime_ms },
                    HookEvent::Pressed(index) => HelperEvent::Pressed {
                        action: bindings[index].action,
                        transform_id: bindings[index].transform_id.clone(),
                    },
                    HookEvent::Released(index) => HelperEvent::Released {
                        action: bindings[index].action,
                        transform_id: bindings[index].transform_id.clone(),
                    },
                    HookEvent::Reset => HelperEvent::Reset {
                        reason: "inputDesktopUnavailable".into(),
                    },
                };
                if serde_json::to_writer(&mut output, &event).is_err()
                    || output.write_all(b"\n").is_err()
                    || output.flush().is_err()
                {
                    QUEUE_FAILED.store(true, Ordering::Relaxed);
                    break;
                }
            }
        })?;
    STATE.with(|state| *state.borrow_mut() = Some(HookState { matcher, sender }));
    // A helper must terminate when the owning process disappears rather than persist as a hook.
    let thread_id = unsafe { GetCurrentThreadId() };
    let mut initial_message: MSG = unsafe { std::mem::zeroed() };
    unsafe {
        PeekMessageW(
            &mut initial_message,
            std::ptr::null_mut(),
            0,
            0,
            PM_NOREMOVE,
        );
    }
    unsafe {
        SetPriorityClass(GetCurrentProcess(), ABOVE_NORMAL_PRIORITY_CLASS);
    }
    let input_worker = thread::Builder::new()
        .name("onda-hook-owner".into())
        .spawn(move || {
            let mut input = std::io::stdin().lock();
            loop {
                let mut line = String::new();
                let read = (&mut input)
                    .take((MAX_CONTROL_LINE_BYTES + 1) as u64)
                    .read_line(&mut line);
                if !matches!(read, Ok(length) if length > 0 && length <= MAX_CONTROL_LINE_BYTES) {
                    break;
                }
                match serde_json::from_str::<HelperCommand>(&line) {
                    Ok(HelperCommand::SetOperationActive { active }) => unsafe {
                        PostThreadMessageW(
                            thread_id,
                            OPERATION_ACTIVE_MESSAGE,
                            usize::from(active),
                            0,
                        );
                    },
                    Ok(HelperCommand::Shutdown) | Err(_) => break,
                }
            }
            unsafe {
                PostThreadMessageW(thread_id, WM_QUIT, 0, 0);
            }
        })?;
    let mut hook = unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_hook),
            GetModuleHandleW(std::ptr::null()),
            0,
        )
    };
    if hook.is_null() {
        return Err(InputError::Helper("hookInstallFailed".into()));
    }
    let timer = unsafe {
        SetTimer(
            std::ptr::null_mut(),
            HEARTBEAT_TIMER,
            HEARTBEAT_INTERVAL_MS,
            None,
        )
    };
    if timer == 0 {
        unsafe {
            UnhookWindowsHookEx(hook);
        }
        return Err(InputError::Helper("hookTimerFailed".into()));
    }
    emit(HookEvent::Ready);
    let started = Instant::now();
    let mut rearmed_at = Instant::now();
    let mut message: MSG = unsafe { std::mem::zeroed() };
    let result = loop {
        let status = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
        if status == 0 {
            break Ok(());
        }
        if status == -1 {
            break Err(InputError::Helper("hookMessageLoopFailed".into()));
        }
        if QUEUE_FAILED.load(Ordering::Relaxed) {
            break Err(InputError::Helper("hookQueueOverflow".into()));
        }
        if message.message == OPERATION_ACTIVE_MESSAGE {
            STATE.with(|state| {
                if let Some(state) = state.borrow_mut().as_mut() {
                    state.matcher.set_operation_active(message.wParam != 0);
                }
            });
            continue;
        }
        if message.message == WM_TIMER {
            emit(HookEvent::Heartbeat(started.elapsed().as_millis() as u64));
            let desktop_active = input_desktop.receives_input().unwrap_or(false);
            let (desktop_lost, desktop_restored, idle_for_rearm) = STATE.with(|state| {
                let mut state = state.borrow_mut();
                let Some(state) = state.as_mut() else {
                    return (false, false, false);
                };
                let restored = desktop_active && !state.matcher.desktop_active();
                let lost = state.matcher.set_desktop_active(desktop_active);
                if restored {
                    seed_modifiers(&mut state.matcher);
                }
                (lost, restored, state.matcher.is_idle_for_rearm())
            });
            if desktop_lost {
                emit(HookEvent::Reset);
            }
            if desktop_restored {
                emit(HookEvent::Ready);
            }
            // Consumed key-downs do not reliably update Windows' asynchronous key state.
            // Genuine hook key-up is authoritative; heartbeat must never unlatch a held chord.
            if idle_for_rearm && rearmed_at.elapsed().as_millis() as u64 >= REARM_INTERVAL_MS {
                // Windows exposes no hook-removal notification; periodic idle reinstallation limits silent loss.
                unsafe {
                    UnhookWindowsHookEx(hook);
                }
                hook = unsafe {
                    SetWindowsHookExW(
                        WH_KEYBOARD_LL,
                        Some(keyboard_hook),
                        GetModuleHandleW(std::ptr::null()),
                        0,
                    )
                };
                if hook.is_null() {
                    break Err(InputError::Helper("hookRearmFailed".into()));
                }
                STATE.with(|state| {
                    if let Some(state) = state.borrow_mut().as_mut() {
                        let operation_active = state.matcher.operation_active();
                        state.matcher.reset();
                        state.matcher.set_operation_active(operation_active);
                        seed_modifiers(&mut state.matcher);
                    }
                });
                rearmed_at = Instant::now();
            }
        }
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    };
    unsafe {
        KillTimer(std::ptr::null_mut(), timer);
        UnhookWindowsHookEx(hook);
    }
    STATE.with(|state| *state.borrow_mut() = None);
    let _ = output_worker.join();
    // stdin may remain open at shutdown; process exit ends the blocked ownership reader.
    drop(input_worker);
    result
}

fn emit(event: HookEvent) {
    STATE.with(|state| {
        if let Some(state) = state.borrow().as_ref() {
            if state.sender.try_send(event).is_err() {
                QUEUE_FAILED.store(true, Ordering::Relaxed);
            }
        }
    });
}

fn seed_modifiers(matcher: &mut ShortcutMatcher) {
    for key in MODIFIER_KEYS {
        matcher.seed_key(key, unsafe { GetAsyncKeyState(key as i32) } < 0);
    }
}

fn read_config() -> Result<HelperConfig, InputError> {
    use std::io::Read;
    let input = std::io::stdin();
    let mut reader = input.lock().take((MAX_PROTOCOL_LINE_BYTES + 1) as u64);
    let mut bytes = Vec::new();
    // Read one byte at a time only once at startup to avoid stealing later commands into a buffer.
    loop {
        let mut byte = [0];
        if reader.read(&mut byte)? == 0 {
            return Err(InputError::Protocol("missing configuration".into()));
        }
        bytes.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    if bytes.len() > MAX_PROTOCOL_LINE_BYTES {
        return Err(InputError::Protocol("oversized configuration".into()));
    }
    let config: HelperConfig =
        serde_json::from_slice(&bytes).map_err(|error| InputError::Protocol(error.to_string()))?;
    config.validate()?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_input_flag_is_readable_without_installing_a_keyboard_hook() {
        let desktop = InputDesktop::current().unwrap();
        assert!(
            desktop.receives_input().is_some(),
            "UOI_IO desktop metadata query failed: {}",
            unsafe { GetLastError() }
        );
    }
}
