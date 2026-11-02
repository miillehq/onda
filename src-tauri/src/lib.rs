//! Thin desktop host for Windows and Linux. Application policy lives in Rust services.
pub mod api;
mod errors;
mod pill;
mod runtime;
use onda_core::{
    contracts::{ThemePreference, TrayIconStyle},
    error::{AppError, AppResult, ErrorCode},
};
use runtime::Runtime;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    window::Color,
    Manager,
};

/// Data written by builds published under the provisional product identity.
const LEGACY_IDENTIFIER: &str = "org.eagle.dictation";
const MAIN_LABEL: &str = "main";
/// Sidebar/title-bar surface and text colours, matching the frontend tokens.
const LIGHT_CHROME: (u32, u32) = (0xF3F3F4, 0x0F0F10);
const DARK_CHROME: (u32, u32) = (0x0B0B0C, 0xF4F4F5);

struct LogGuard(Mutex<Option<tracing_appender::non_blocking::WorkerGuard>>);

/// Production builds never honor the isolated desktop-test profile.
fn debug_smoke_directory() -> Option<PathBuf> {
    debug_directory("ONDA_SMOKE_DIRECTORY")
}

/// Debug-only interactive profile: its own data folder, with no global shortcut hook,
/// single-instance lock or registry changes, so a development build can run beside an
/// installed copy without touching its data.
fn debug_dev_profile() -> Option<PathBuf> {
    debug_directory("ONDA_DEV_PROFILE")
}

fn debug_directory(variable: &str) -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|directory| directory.is_absolute())
}

fn isolated() -> bool {
    debug_smoke_directory().is_some() || debug_dev_profile().is_some()
}

#[tauri::command]
async fn application_call(
    request: api::ApiRequest,
    service: tauri::State<'_, Arc<Runtime>>,
) -> AppResult<api::ApiResponse> {
    service.inner().dispatch(request).await
}

pub(crate) fn show_main(app: &tauri::AppHandle) {
    show_main_at(app, None);
}

/// Show the main window, creating it when it was closed. Returns true when it was created,
/// in which case it opens at `section` itself and needs no navigate event.
pub(crate) fn show_main_at(app: &tauri::AppHandle, section: Option<&str>) -> bool {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return false;
    }
    match build_main_window(app, section, true) {
        Ok(window) => {
            let _ = window.set_focus();
            true
        }
        Err(error) => {
            tracing::warn!(subsystem = "window", error = %error, "main window could not be created");
            false
        }
    }
}

/// The main window carries a full WebView renderer (about 100 MB). It exists only while open:
/// built when shown and destroyed when closed, so Onda in the tray keeps just the pill loaded.
fn build_main_window(
    app: &tauri::AppHandle,
    section: Option<&str>,
    visible: bool,
) -> Result<tauri::WebviewWindow, Box<dyn std::error::Error>> {
    let configuration = app
        .config()
        .app
        .windows
        .first()
        .ok_or("Main window configuration missing")?
        .clone();
    let theme = app
        .try_state::<Arc<Runtime>>()
        .and_then(|service| service.settings().ok())
        .map_or(ThemePreference::System, |settings| settings.theme);
    let mut window = tauri::WebviewWindowBuilder::from_config(app, &configuration)?
        .title(onda_core::APP_NAME)
        .theme(window_theme(theme))
        .background_color(chrome_color(is_dark(theme)))
        .visible(visible);
    // Isolated profiles keep their own browser data, so they can run beside the real app.
    if let Some(directory) = debug_smoke_directory().or_else(debug_dev_profile) {
        window = window.data_directory(directory.join("EBWebView"));
    }
    if let Some(section) = section {
        // The page reads its starting route from the hash before rendering.
        let hash = serde_json::to_string(&format!("#{section}"))?;
        window = window.initialization_script(format!(
            "if (!location.hash) history.replaceState(null, '', {hash});"
        ));
    }
    if debug_smoke_directory().is_some() {
        window = window.initialization_script("window.__ONDA_SMOKE__ = true;");
        if std::env::var("ONDA_SMOKE_REPLAY").as_deref() == Ok("1") {
            window = window.initialization_script("window.__ONDA_REPLAY__ = true;");
        }
    }
    let window = window.build()?;
    apply_theme(app, theme);
    Ok(window)
}

fn is_dark(theme: ThemePreference) -> bool {
    match theme {
        ThemePreference::Dark => true,
        ThemePreference::Light => false,
        ThemePreference::System => !onda_platform::apps_use_light_theme(),
    }
}

fn chrome_color(dark: bool) -> Color {
    let (rgb, _) = if dark { DARK_CHROME } else { LIGHT_CHROME };
    Color((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
}

fn window_theme(theme: ThemePreference) -> Option<tauri::Theme> {
    match theme {
        ThemePreference::System => None,
        ThemePreference::Light => Some(tauri::Theme::Light),
        ThemePreference::Dark => Some(tauri::Theme::Dark),
    }
}

/// Window theme drives the webview's `prefers-color-scheme`; the caption is tinted to
/// continue the sidebar so the native title bar reads as part of the app.
pub(crate) fn apply_theme(app: &tauri::AppHandle, theme: ThemePreference) {
    let Some(window) = app.get_webview_window(MAIN_LABEL) else {
        return;
    };
    let dark = is_dark(theme);
    let _ = window.set_theme(window_theme(theme));
    let _ = window.set_background_color(Some(chrome_color(dark)));
    // Elsewhere the title bar follows the desktop's theme.
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        let (caption, text) = if dark { DARK_CHROME } else { LIGHT_CHROME };
        onda_platform::set_caption_colors(hwnd.0 as isize, caption, text);
    }
}

pub(crate) fn apply_launch_at_login(enabled: bool) -> AppResult<()> {
    if isolated() {
        return Ok(());
    }
    let command = if enabled {
        // An AppImage runs from a new mount point each time; $APPIMAGE is the file itself.
        let executable = std::env::var_os("APPIMAGE")
            .filter(|_| cfg!(target_os = "linux"))
            .map(PathBuf::from)
            .map(Ok)
            .unwrap_or_else(std::env::current_exe)
            .map_err(|_| {
                AppError::new(
                    ErrorCode::Internal,
                    "The application location could not be read.",
                    true,
                )
            })?;
        Some(format!("\"{}\" --background", executable.display()))
    } else {
        None
    };
    onda_platform::set_launch_at_login(onda_core::APP_NAME, command.as_deref()).map_err(|_| {
        AppError::new(
            ErrorCode::Internal,
            "The system didn't allow changing the startup setting.",
            true,
        )
    })
}

/// Move data from the provisional identity once, so downloaded models, the dictionary and
/// accounts survive the rename. Never overwrites existing data. If the old folder is still in
/// use (an older build is running), use it in place and try again on a later launch.
fn resolve_data_directory(app: &tauri::App) -> tauri::Result<PathBuf> {
    let directory = app.path().app_local_data_dir()?;
    if directory.exists() {
        return Ok(directory);
    }
    let Ok(local) = app.path().local_data_dir() else {
        return Ok(directory);
    };
    let legacy = local.join(LEGACY_IDENTIFIER);
    if legacy.is_dir() && std::fs::rename(&legacy, &directory).is_err() {
        return Ok(legacy);
    }
    Ok(directory)
}

/// The flat mark below 32 px, following the brand guide. `Auto` is white on a dark taskbar and
/// black on a light one; Linux panels cannot be read, so there `Auto` is white, which suits the
/// dark top bars of GNOME and most themes.
fn tray_icon(style: TrayIconStyle) -> tauri::Result<tauri::image::Image<'static>> {
    let black = match style {
        TrayIconStyle::Auto => onda_platform::system_uses_light_theme(),
        TrayIconStyle::Black => true,
        TrayIconStyle::White => false,
    };
    let bytes: &'static [u8] = if black {
        include_bytes!("../icons/tray-light.png")
    } else {
        include_bytes!("../icons/tray-dark.png")
    };
    tauri::image::Image::from_bytes(bytes)
}

/// Swap the tray mark after the setting or the taskbar theme changes.
pub(crate) fn apply_tray_icon(app: &tauri::AppHandle, style: TrayIconStyle) {
    if let Some(tray) = app.tray_by_id("main") {
        match tray_icon(style) {
            Ok(icon) => {
                if tray.set_icon(Some(icon)).is_err() {
                    tracing::warn!(subsystem = "tray", "tray icon not updated");
                }
            }
            Err(_) => tracing::warn!(subsystem = "tray", "tray icon could not be decoded"),
        }
    }
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    onda_platform::harden_dll_search()?;
    let smoke_directory = debug_smoke_directory();
    let data_directory = match smoke_directory.clone().or_else(debug_dev_profile) {
        Some(directory) => directory,
        None => resolve_data_directory(app)?,
    };
    std::fs::create_dir_all(&data_directory)?;
    let logger = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("onda")
        .filename_suffix("log")
        .max_log_files(7)
        .build(data_directory.join("logs"))?;
    let (writer, guard) = tracing_appender::non_blocking(logger);
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_writer(writer)
        .with_max_level(tracing::Level::INFO)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
    app.manage(LogGuard(Mutex::new(Some(guard))));
    let executable = std::env::current_exe()?;
    let worker_directory = executable
        .parent()
        .ok_or("Executable directory is unavailable")?
        .to_path_buf();
    let runtime = Runtime::new(
        app.handle().clone(),
        data_directory,
        worker_directory,
        !isolated(),
    )?;
    let settings = runtime.settings()?;
    app.manage(runtime);
    // Starting at login leaves the main window unbuilt until it is opened; the smoke test
    // needs it running but hidden.
    if smoke_directory.is_some() {
        build_main_window(app.handle(), None, false)?;
    } else if !std::env::args()
        .any(|argument| argument == "--background" || argument == DICTATE_ARGUMENT)
    {
        build_main_window(app.handle(), None, true)?;
    }
    let webview_data = smoke_directory
        .clone()
        .or_else(debug_dev_profile)
        .map(|directory| directory.join("EBWebView"));
    let mut pill_window = tauri::WebviewWindowBuilder::new(
        app,
        pill::LABEL,
        tauri::WebviewUrl::App("pill.html".into()),
    );
    if let Some(directory) = webview_data {
        pill_window = pill_window.data_directory(directory);
    }
    pill_window
        .title(format!("{} dictation", onda_core::APP_NAME))
        .inner_size(pill::WIDTH, pill::HEIGHT)
        .decorations(false)
        .transparent(true)
        .background_color(Color(0, 0, 0, 0))
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        // Windows: created visible (tao's only non-activating show), then hidden natively; see
        // pill.rs. Linux: created hidden, so it is an overlay before it is first shown.
        .position(-32000.0, -32000.0)
        .visible(cfg!(windows))
        .build()?;
    #[cfg(target_os = "linux")]
    pill::make_overlay(app.handle());
    pill::hide(app.handle());
    if settings.launch_at_login {
        // Re-register so the entry follows the executable after updates or moves.
        if let Err(error) = apply_launch_at_login(true) {
            tracing::warn!(subsystem = "startup", code = ?error.code, "login entry not refreshed");
        }
    }
    let open = MenuItem::with_id(
        app,
        "open",
        format!("Open {}", onda_core::APP_NAME),
        true,
        None::<&str>,
    )?;
    // On Linux the tray can start dictation too, for desktops without a shortcuts portal.
    let dictate = MenuItem::with_id(
        app,
        "dictate",
        "Start or finish dictation",
        true,
        None::<&str>,
    )?;
    let cancel = MenuItem::with_id(app, "cancel", "Cancel dictation", true, None::<&str>)?;
    let quit = MenuItem::with_id(
        app,
        "quit",
        format!("Quit {}", onda_core::APP_NAME),
        true,
        None::<&str>,
    )?;
    let menu = if cfg!(windows) {
        Menu::with_items(app, &[&open, &cancel, &quit])?
    } else {
        Menu::with_items(app, &[&open, &dictate, &cancel, &quit])?
    };
    TrayIconBuilder::with_id("main")
        .icon(tray_icon(settings.tray_icon)?)
        .tooltip(format!("{} · Ready", onda_core::APP_NAME))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "dictate" => app.state::<Arc<Runtime>>().toggle_dictation(),
            "cancel" => {
                let service = app.state::<Arc<Runtime>>().inner().clone();
                tauri::async_runtime::spawn(async move {
                    let _ = service.dispatch(api::ApiRequest::CancelOperation).await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    tracing::info!(
        subsystem = "application",
        version = onda_core::APP_VERSION,
        "application started"
    );
    if std::env::args().any(|argument| argument == DICTATE_ARGUMENT) {
        app.state::<Arc<Runtime>>().toggle_dictation();
    }
    Ok(())
}

/// `onda --dictate` starts hands-free dictation, or finishes it if one is recording. On Linux
/// desktops without a shortcuts portal, bind it to a desktop shortcut.
const DICTATE_ARGUMENT: &str = "--dictate";

/// A second launch either toggles dictation or opens the window.
fn second_instance(app: &tauri::AppHandle, arguments: Vec<String>) {
    if arguments
        .iter()
        .any(|argument| argument == DICTATE_ARGUMENT)
    {
        if let Some(service) = app.try_state::<Arc<Runtime>>() {
            service.toggle_dictation();
        }
    } else {
        show_main(app);
    }
}

/// Native startup failures remain visible instead of silent exits.
pub fn run() {
    // A Wayland compositor places windows itself, ignores "always on top" and may focus a window
    // as it appears, which would put the pill where it likes and take the keyboard from the app
    // being dictated into. Through XWayland the pill keeps its place and never takes focus.
    #[cfg(target_os = "linux")]
    if std::env::var_os("GDK_BACKEND").is_none()
        && std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var_os("DISPLAY").is_some()
    {
        std::env::set_var("GDK_BACKEND", "x11");
    }
    let mut builder = tauri::Builder::default();
    // An isolated profile must neither activate the user's app nor share its global shortcuts.
    if !isolated() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, arguments, _| {
            second_instance(app, arguments)
        }));
    }
    let application = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Used only from native commands (profile picture, saving the stats image); the
        // webview's capabilities grant it nothing.
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![application_call])
        .setup(setup)
        .on_window_event(|window, event| match event {
            // Closing the main window destroys it to free its renderer; the pill only hides.
            tauri::WindowEvent::CloseRequested { api, .. } if window.label() != MAIN_LABEL => {
                api.prevent_close();
                let _ = window.hide();
            }
            // The pill window always exists, so its theme events keep the tray mark current
            // even while the main window is closed.
            tauri::WindowEvent::ThemeChanged(_) => {
                let app = window.app_handle();
                if let Some(service) = app.try_state::<Arc<Runtime>>() {
                    if let Ok(settings) = service.settings() {
                        if window.label() == MAIN_LABEL && settings.theme == ThemePreference::System
                        {
                            apply_theme(app, settings.theme);
                        }
                        if settings.tray_icon == TrayIconStyle::Auto {
                            apply_tray_icon(app, settings.tray_icon);
                        }
                    }
                }
            }
            _ => {}
        })
        .build(tauri::generate_context!());
    match application {
        Ok(application) => application.run(|app, event| {
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                if let Some(service) = app.try_state::<Arc<Runtime>>() {
                    service.shutdown();
                }
                if let Some(guard) = app.try_state::<LogGuard>() {
                    if let Ok(mut guard) = guard.0.lock() {
                        guard.take();
                    }
                }
            }
        }),
        Err(startup_error) => {
            if cfg!(debug_assertions) {
                if let Some(directory) = debug_smoke_directory() {
                    let report = serde_json::json!({"passed":false,"checks":["native startup failed"],"startupFailure":startup_error.to_string()});
                    let _ = std::fs::write(directory.join("report.json"), report.to_string());
                    return;
                }
            }
            let error = AppError::new(
                ErrorCode::Internal,
                format!(
                    "{} could not start. Reinstall the complete application with its helper executables.",
                    onda_core::APP_NAME
                ),
                false,
            );
            onda_platform::show_startup_error(&error.message);
        }
    }
}
