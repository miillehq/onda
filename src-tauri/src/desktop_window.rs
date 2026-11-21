use super::*;

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
pub(super) fn build_main_window(
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
    let mut window = navigation::restrict(
        tauri::WebviewWindowBuilder::from_config(app, &configuration)?,
        app.config().build.dev_url.as_ref(),
    )
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

pub(super) fn is_dark(theme: ThemePreference) -> bool {
    match theme {
        ThemePreference::Dark => true,
        ThemePreference::Light => false,
        ThemePreference::System => !onda_platform::apps_use_light_theme(),
    }
}

pub(super) fn chrome_color(dark: bool) -> Color {
    let (rgb, _) = if dark { DARK_CHROME } else { LIGHT_CHROME };
    Color((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
}

pub(super) fn window_theme(theme: ThemePreference) -> Option<tauri::Theme> {
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
