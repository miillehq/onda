//! Small per-user desktop integrations: calendar day, autostart, theme, caption colour and cues.
use crate::PlatformError;

/// Short audible confirmations for hands-off dictation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cue {
    Start,
    Stop,
    Error,
}

/// The user's local calendar day as `YYYY-MM-DD`, for content-free daily aggregates.
pub fn local_date() -> String {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::GetLocalTime;
        let mut time = unsafe { std::mem::zeroed() };
        unsafe { GetLocalTime(&mut time) };
        format!("{:04}-{:02}-{:02}", time.wYear, time.wMonth, time.wDay)
    }
    #[cfg(not(windows))]
    {
        let days = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs() / 86_400)
            .unwrap_or(0) as i64;
        civil_from_days(days)
    }
}

#[cfg(not(windows))]
fn civil_from_days(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(windows)]
mod registry {
    use super::PlatformError;
    use windows_sys::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
        System::Registry::{
            RegCloseKey, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW, HKEY,
            HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
        },
    };

    pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    pub const PERSONALIZE_KEY: &str =
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

    pub fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    pub fn read_dword(key: &str, value: &str) -> Option<u32> {
        let key = wide(key);
        let value = wide(value);
        let mut data = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut data as *mut u32).cast(),
                &mut size,
            )
        };
        (status == ERROR_SUCCESS).then_some(data)
    }

    pub fn has_string(key: &str, value: &str) -> bool {
        let key = wide(key);
        let value = wide(value);
        let mut size = 0u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        status == ERROR_SUCCESS
    }

    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe { RegCloseKey(self.0) };
        }
    }

    pub fn write_or_delete_string(
        key: &str,
        value: &str,
        data: Option<&str>,
    ) -> Result<(), PlatformError> {
        let path = wide(key);
        let mut handle: HKEY = std::ptr::null_mut();
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                KEY_SET_VALUE,
                &mut handle,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(PlatformError::InvalidParameter(
                "startup registration is unavailable".into(),
            ));
        }
        let key = Key(handle);
        let name = wide(value);
        let status = match data {
            Some(data) => {
                let data = wide(data);
                unsafe {
                    RegSetValueExW(
                        key.0,
                        name.as_ptr(),
                        0,
                        REG_SZ,
                        data.as_ptr().cast(),
                        (data.len() * 2) as u32,
                    )
                }
            }
            None => match unsafe { RegDeleteValueW(key.0, name.as_ptr()) } {
                ERROR_FILE_NOT_FOUND => ERROR_SUCCESS,
                status => status,
            },
        };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(PlatformError::InvalidParameter(
                "startup registration could not be changed".into(),
            ))
        }
    }
}

/// Register (or remove) a per-user login entry. `command` is the complete quoted command line.
pub fn set_launch_at_login(name: &str, command: Option<&str>) -> Result<(), PlatformError> {
    if name.is_empty() || name.contains(['\\', '\0']) {
        return Err(PlatformError::InvalidParameter("startup name".into()));
    }
    #[cfg(windows)]
    {
        registry::write_or_delete_string(registry::RUN_KEY, name, command)
    }
    #[cfg(not(windows))]
    {
        let _ = command;
        Err(PlatformError::UnsupportedPlatform)
    }
}

pub fn launch_at_login_registered(name: &str) -> bool {
    #[cfg(windows)]
    {
        registry::has_string(registry::RUN_KEY, name)
    }
    #[cfg(not(windows))]
    {
        let _ = name;
        false
    }
}

/// Windows "app mode" preference. Defaults to light when the value is unavailable.
pub fn apps_use_light_theme() -> bool {
    #[cfg(windows)]
    {
        registry::read_dword(registry::PERSONALIZE_KEY, "AppsUseLightTheme")
            .is_none_or(|value| value != 0)
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// Taskbar and tray colour mode, which may differ from the app mode.
pub fn system_uses_light_theme() -> bool {
    #[cfg(windows)]
    {
        registry::read_dword(registry::PERSONALIZE_KEY, "SystemUsesLightTheme")
            .is_some_and(|value| value != 0)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Tint the native title bar so it continues the app's sidebar (Windows 11; ignored elsewhere).
/// Colours are 0xRRGGBB.
pub fn set_caption_colors(window_handle: isize, caption: u32, text: u32) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
        };
        let colorref = |rgb: u32| ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF);
        let hwnd = window_handle as windows_sys::Win32::Foundation::HWND;
        for (attribute, value) in [
            (DWMWA_CAPTION_COLOR, colorref(caption)),
            (DWMWA_BORDER_COLOR, colorref(caption)),
            (DWMWA_TEXT_COLOR, colorref(text)),
        ] {
            unsafe {
                DwmSetWindowAttribute(
                    hwnd,
                    attribute as u32,
                    (&value as *const u32).cast(),
                    std::mem::size_of::<u32>() as u32,
                );
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (window_handle, caption, text);
    }
}

/// Show a window above others without activating it, so the focused control in the
/// user's app keeps focus. Asynchronous: never waits on the window's thread.
pub fn show_without_activation(window_handle: isize) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, ShowWindowAsync, HWND_TOPMOST, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE,
            SWP_NOMOVE, SWP_NOSIZE, SW_SHOWNOACTIVATE,
        };
        let hwnd = window_handle as windows_sys::Win32::Foundation::HWND;
        unsafe {
            ShowWindowAsync(hwnd, SW_SHOWNOACTIVATE);
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
            );
        }
    }
    #[cfg(not(windows))]
    {
        let _ = window_handle;
    }
}

pub fn hide_window(window_handle: isize) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindowAsync, SW_HIDE};
        unsafe {
            ShowWindowAsync(
                window_handle as windows_sys::Win32::Foundation::HWND,
                SW_HIDE,
            );
        }
    }
    #[cfg(not(windows))]
    {
        let _ = window_handle;
    }
}

/// Play a short synthesized cue asynchronously. Failures are silent by design.
pub fn play_cue(cue: Cue) {
    #[cfg(windows)]
    {
        use std::sync::OnceLock;
        use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
        static START: OnceLock<Vec<u8>> = OnceLock::new();
        static STOP: OnceLock<Vec<u8>> = OnceLock::new();
        static ERROR: OnceLock<Vec<u8>> = OnceLock::new();
        let sound = match cue {
            Cue::Start => START.get_or_init(|| tone_wav(&[(587.33, 0.055), (880.0, 0.085)])),
            Cue::Stop => STOP.get_or_init(|| tone_wav(&[(880.0, 0.055), (587.33, 0.085)])),
            Cue::Error => {
                ERROR.get_or_init(|| tone_wav(&[(311.13, 0.09), (0.0, 0.05), (311.13, 0.12)]))
            }
        };
        unsafe {
            PlaySoundW(
                sound.as_ptr().cast(),
                std::ptr::null_mut(),
                SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
            );
        }
    }
    #[cfg(not(windows))]
    {
        let _ = cue;
    }
}

/// 16-bit mono PCM; each note has a short raised-cosine envelope so cues never click.
fn tone_wav(notes: &[(f32, f32)]) -> Vec<u8> {
    const RATE: u32 = 44_100;
    const VOLUME: f32 = 0.16;
    let mut samples = Vec::new();
    for (frequency, seconds) in notes {
        let count = (RATE as f32 * seconds) as usize;
        let fade = (RATE as f32 * 0.012) as usize;
        for index in 0..count {
            let envelope = if index < fade {
                0.5 - 0.5 * (std::f32::consts::PI * index as f32 / fade as f32).cos()
            } else if index + fade > count {
                0.5 - 0.5 * (std::f32::consts::PI * (count - index) as f32 / fade as f32).cos()
            } else {
                1.0
            };
            let phase = 2.0 * std::f32::consts::PI * frequency * index as f32 / RATE as f32;
            // A quiet second harmonic softens the pure sine.
            let value = (phase.sin() + 0.18 * (2.0 * phase).sin()) / 1.18;
            samples.push((value * envelope * VOLUME * i16::MAX as f32) as i16);
        }
    }
    let data_bytes = (samples.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_bytes as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());
    for sample in samples {
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_date_is_a_day_key() {
        let day = local_date();
        assert_eq!(day.len(), 10);
        assert_eq!(&day[4..5], "-");
        assert_eq!(&day[7..8], "-");
    }

    #[test]
    fn cue_is_a_valid_pcm_wave() {
        let wav = tone_wav(&[(440.0, 0.01)]);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        let data = u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize;
        assert_eq!(wav.len(), 44 + data);
    }

    #[test]
    fn startup_names_are_validated() {
        assert!(set_launch_at_login("bad\\name", None).is_err());
        assert!(set_launch_at_login("", None).is_err());
    }
}
