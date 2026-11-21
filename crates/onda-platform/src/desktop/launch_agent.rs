use crate::PlatformError;
use std::path::PathBuf;

pub fn label(name: &str) -> String {
    format!("com.miille.{}", name.to_lowercase())
}

pub fn path(name: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())?;
    Some(
        home.join("Library/LaunchAgents")
            .join(format!("{}.plist", label(name))),
    )
}

/// Splits `"program path" argument ...` (the form the app passes on every OS) into arguments.
pub fn arguments(command: &str) -> Option<Vec<String>> {
    let command = command.trim();
    let (program, rest) = match command.strip_prefix('"') {
        Some(quoted) => {
            let end = quoted.find('"')?;
            (quoted[..end].to_string(), &quoted[end + 1..])
        }
        None => {
            let end = command.find(char::is_whitespace).unwrap_or(command.len());
            (command[..end].to_string(), &command[end..])
        }
    };
    if program.is_empty() {
        return None;
    }
    Some(
        std::iter::once(program)
            .chain(rest.split_whitespace().map(str::to_string))
            .collect(),
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn plist(name: &str, arguments: &[String]) -> String {
    let arguments: String = arguments
        .iter()
        .map(|argument| format!("    <string>{}</string>\n", escape(argument)))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n  <key>Label</key>\n  <string>{}</string>\n  \
         <key>ProgramArguments</key>\n  <array>\n{arguments}  </array>\n  \
         <key>RunAtLoad</key>\n  <true/>\n  <key>ProcessType</key>\n  <string>Interactive</string>\n\
         </dict>\n</plist>\n",
        escape(&label(name))
    )
}

pub fn write_or_delete(name: &str, command: Option<&str>) -> Result<(), PlatformError> {
    let path = path(name).ok_or(PlatformError::InvalidParameter("home folder".into()))?;
    let Some(command) = command else {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                Err(PlatformError::InvalidParameter("launch agent".into()))
            }
            _ => Ok(()),
        };
    };
    if command.contains(['\n', '\r']) {
        return Err(PlatformError::InvalidParameter("startup command".into()));
    }
    let arguments =
        arguments(command).ok_or(PlatformError::InvalidParameter("startup command".into()))?;
    let directory = path.parent().ok_or(PlatformError::InvalidParameter(
        "launch agent folder".into(),
    ))?;
    std::fs::create_dir_all(directory)
        .map_err(|_| PlatformError::InvalidParameter("launch agent folder".into()))?;
    let temporary = directory.join(format!(".{}.tmp", std::process::id()));
    std::fs::write(&temporary, plist(name, &arguments))
        .and_then(|()| std::fs::rename(&temporary, &path))
        .map_err(|_| {
            let _ = std::fs::remove_file(&temporary);
            PlatformError::InvalidParameter("launch agent".into())
        })
}
