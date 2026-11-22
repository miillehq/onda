//! Thin desktop host for Windows and Linux. Application policy lives in Rust services.

mod desktop_window;
mod startup_log;
pub(crate) use desktop_window::*;
#[cfg(all(feature = "e2e", not(debug_assertions)))]
compile_error!("The e2e feature adds a WebDriver server and is for debug builds only.");
pub mod api;
mod errors;
mod navigation;
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
const BACKGROUND_ARGUMENT: &str = "--background";
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
    // Optional, so a call without a request gets the same refusal as any other malformed one.
    request: Option<serde_json::Value>,
    service: tauri::State<'_, Arc<Runtime>>,
) -> AppResult<api::ApiResponse> {
    service
        .inner()
        .dispatch(api::ApiRequest::decode(request.unwrap_or_default())?)
        .await
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
        Some(format!(
            "\"{}\" {BACKGROUND_ARGUMENT}",
            executable.display()
        ))
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
    let started = std::time::Instant::now();
    onda_platform::harden_dll_search()?;
    #[cfg(windows)]
    if !isolated() && std::env::args_os().any(|argument| argument == BACKGROUND_ARGUMENT) {
        onda_platform::prefetch_webview2_loader();
    }
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
    startup_log::stage(started, "startup logging ready");
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
    startup_log::stage(started, "desktop services ready");
    // Starting at login leaves the main window unbuilt until it is opened; the smoke test
    // needs it running but hidden.
    if smoke_directory.is_some() {
        build_main_window(app.handle(), None, false)?;
    } else if !std::env::args()
        .any(|argument| argument == BACKGROUND_ARGUMENT || argument == DICTATE_ARGUMENT)
    {
        build_main_window(app.handle(), None, true)?;
    }
    build_pill(app, smoke_directory)?;
    startup_log::stage(started, "dictation indicator ready");
    if settings.launch_at_login {
        // Re-register so the entry follows the executable after updates or moves.
        if let Err(error) = apply_launch_at_login(true) {
            tracing::warn!(subsystem = "startup", code = ?error.code, "login entry not refreshed");
        }
    }
    install_tray(app, settings.tray_icon)?;
    startup_log::ready(started);
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
    let builder = desktop_builder();
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

fn build_pill(app: &tauri::App, smoke_directory: Option<PathBuf>) -> tauri::Result<()> {
    let webview_data = smoke_directory
        .or_else(debug_dev_profile)
        .map(|directory| directory.join("EBWebView"));
    let mut pill_window = navigation::restrict(
        tauri::WebviewWindowBuilder::new(
            app,
            pill::LABEL,
            tauri::WebviewUrl::App("pill.html".into()),
        ),
        app.config().build.dev_url.as_ref(),
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
    Ok(())
}

fn install_tray(app: &tauri::App, style: TrayIconStyle) -> tauri::Result<()> {
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
        .icon(tray_icon(style)?)
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
    Ok(())
}

/// Register desktop and build-specific plugins before any webview is created.
fn desktop_builder() -> tauri::Builder<tauri::Wry> {
    let mut builder = tauri::Builder::default();
    // An isolated profile must neither activate the user's app nor share its global shortcuts.
    if !isolated() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, arguments, _| {
            second_instance(app, arguments)
        }));
    }
    // macOS: dictation shortcuts are system hotkeys, routed to the runtime by their id.
    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if let Some(service) = app.try_state::<Arc<Runtime>>() {
                        service.hotkey(
                            shortcut.id(),
                            event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed,
                        );
                    }
                })
                .build(),
        );
    }
    // Real-shell tests (L6): WebdriverIO's plugins exist only in debug builds made with
    // `--features e2e` (`npm run build:e2e`), so no shipped binary has a WebDriver server.
    #[cfg(all(debug_assertions, feature = "e2e"))]
    {
        builder = builder
            .plugin(tauri_plugin_wdio::init())
            .plugin(tauri_plugin_wdio_webdriver::init());
    }
    builder
}
