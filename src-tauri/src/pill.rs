//! The non-activating dictation pill. Native state decides when it is visible; the
//! webview only draws it. Showing it must never take focus from the application the
//! user is dictating into, so visibility bypasses tao (whose show() activates after
//! creation): the window is created visible and unfocusable, hidden natively at once,
//! and from then on shown and hidden only with non-activating Win32 calls.
use eagle_core::contracts::{OperationMode, OperationState, Phase};
use eagle_platform::Cue;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};

pub const LABEL: &str = "pill";
/// Logical size of the transparent window; the pill itself is drawn centred at its bottom.
pub const WIDTH: f64 = 440.0;
pub const HEIGHT: f64 = 190.0;
const BOTTOM_MARGIN: f64 = 6.0;
const HIDE_AFTER_COMPLETED: Duration = Duration::from_millis(1500);
const HIDE_AFTER_ERROR: Duration = Duration::from_millis(7000);
const HIDE_AFTER_CANCELLED: Duration = Duration::from_millis(450);

struct Tracked {
    id: String,
    visible: bool,
    phase: Phase,
}

pub struct Pill {
    enabled: AtomicBool,
    sounds: AtomicBool,
    generation: Arc<AtomicU64>,
    current: Mutex<Option<Tracked>>,
}

fn is_active(phase: &Phase) -> bool {
    matches!(
        phase,
        Phase::Recording
            | Phase::FinalizingAudio
            | Phase::Transcribing
            | Phase::Enhancing
            | Phase::Inserting
    )
}

impl Pill {
    pub fn new(enabled: bool, sounds: bool) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            sounds: AtomicBool::new(sounds),
            generation: Arc::new(AtomicU64::new(0)),
            current: Mutex::new(None),
        }
    }

    pub fn configure(&self, app: &AppHandle, enabled: bool, sounds: bool) {
        self.enabled.store(enabled, Ordering::Release);
        self.sounds.store(sounds, Ordering::Release);
        if !enabled {
            self.hide_now(app);
        }
    }

    fn cue(&self, cue: Cue) {
        if self.sounds.load(Ordering::Acquire) {
            eagle_platform::play_cue(cue);
        }
    }

    /// Called for every emitted state, possibly from worker threads.
    pub fn on_state(&self, app: &AppHandle, state: &OperationState) {
        let Some(id) = state.operation_id.clone() else {
            self.hide_now(app);
            return;
        };
        let Ok(mut current) = self.current.lock() else {
            return;
        };
        let new_operation = current.as_ref().is_none_or(|tracked| tracked.id != id);
        let (visible, previous) = if new_operation {
            // Rewrite previews and recovery started from the main window stay in that window.
            let main_focused = app
                .get_webview_window("main")
                .and_then(|window| window.is_focused().ok())
                .unwrap_or(false);
            let visible = self.enabled.load(Ordering::Acquire)
                && (state.mode != OperationMode::Transform || !main_focused);
            *current = Some(Tracked {
                id,
                visible,
                phase: state.phase.clone(),
            });
            (visible, None)
        } else {
            let Some(tracked) = current.as_mut() else {
                return;
            };
            if tracked.phase == state.phase {
                return;
            }
            let previous = std::mem::replace(&mut tracked.phase, state.phase.clone());
            (tracked.visible, Some(previous))
        };
        drop(current);

        match (&previous, &state.phase) {
            (None, Phase::Recording) => self.cue(Cue::Start),
            (Some(Phase::Recording), Phase::FinalizingAudio) => self.cue(Cue::Stop),
            (Some(previous), Phase::Error) if is_active(previous) => self.cue(Cue::Error),
            _ => {}
        }
        if !visible {
            return;
        }
        match state.phase {
            ref phase if is_active(phase) => {
                if previous.is_none() {
                    self.show(app);
                } else {
                    self.generation.fetch_add(1, Ordering::AcqRel);
                }
            }
            Phase::Completed => self.hide_after(app, HIDE_AFTER_COMPLETED),
            Phase::Error => {
                if previous.is_none() {
                    self.show(app);
                }
                if let Some(window) = app.get_webview_window(LABEL) {
                    // Only the error card is interactive; it offers a way to fix the problem.
                    let _ = window.set_ignore_cursor_events(false);
                }
                self.hide_after(app, HIDE_AFTER_ERROR);
            }
            Phase::Cancelled => self.hide_after(app, HIDE_AFTER_CANCELLED),
            _ => self.hide_now(app),
        }
    }

    fn show(&self, app: &AppHandle) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        let Some(window) = app.get_webview_window(LABEL) else {
            return;
        };
        let monitor = app
            .cursor_position()
            .ok()
            .and_then(|point| app.monitor_from_point(point.x, point.y).ok().flatten())
            .or_else(|| app.primary_monitor().ok().flatten());
        if let Some(monitor) = monitor {
            let scale = monitor.scale_factor();
            let area = monitor.work_area();
            let width = (WIDTH * scale).round() as i32;
            let height = (HEIGHT * scale).round() as i32;
            let x = area.position.x + (area.size.width as i32 - width) / 2;
            let y = area.position.y + area.size.height as i32
                - height
                - (BOTTOM_MARGIN * scale).round() as i32;
            let _ = window.set_position(PhysicalPosition::new(x, y));
            let _ = window.set_size(PhysicalSize::new(width as u32, height as u32));
        }
        let _ = window.set_ignore_cursor_events(true);
        if let Ok(hwnd) = window.hwnd() {
            eagle_platform::show_without_activation(hwnd.0 as isize);
        }
    }

    fn hide_after(&self, app: &AppHandle, delay: Duration) {
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let counter = self.generation.clone();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(delay).await;
            if counter.load(Ordering::Acquire) == generation {
                hide(&app);
            }
        });
    }

    pub fn hide_now(&self, app: &AppHandle) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        hide(app);
    }
}

/// Hide natively; tao keeps believing the window is visible, which is what keeps its own
/// style updates from re-hiding or activating it.
pub fn hide(app: &AppHandle) {
    if let Some(Ok(hwnd)) = app.get_webview_window(LABEL).map(|window| window.hwnd()) {
        eagle_platform::hide_window(hwnd.0 as isize);
    }
}
