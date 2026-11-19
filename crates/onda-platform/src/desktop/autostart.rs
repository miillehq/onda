use crate::PlatformError;
use std::path::PathBuf;

pub fn path(name: &str) -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(
        config
            .join("autostart")
            .join(format!("{}.desktop", name.to_lowercase())),
    )
}

pub fn write_or_delete(name: &str, command: Option<&str>) -> Result<(), PlatformError> {
    let path = path(name).ok_or(PlatformError::InvalidParameter("home folder".into()))?;
    let Some(command) = command else {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                Err(PlatformError::InvalidParameter("autostart entry".into()))
            }
            _ => Ok(()),
        };
    };
    if command.contains(['\n', '\r']) {
        return Err(PlatformError::InvalidParameter("startup command".into()));
    }
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName={name}\nExec={command}\nTerminal=false\nNoDisplay=true\nX-GNOME-Autostart-enabled=true\n"
    );
    let directory = path
        .parent()
        .ok_or(PlatformError::InvalidParameter("autostart folder".into()))?;
    std::fs::create_dir_all(directory)
        .map_err(|_| PlatformError::InvalidParameter("autostart folder".into()))?;
    let temporary = directory.join(format!(".{}.tmp", std::process::id()));
    std::fs::write(&temporary, entry)
        .and_then(|()| std::fs::rename(&temporary, &path))
        .map_err(|_| {
            let _ = std::fs::remove_file(&temporary);
            PlatformError::InvalidParameter("autostart entry".into())
        })
}
