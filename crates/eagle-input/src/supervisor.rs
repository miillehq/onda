use crate::{HelperConfig, HelperEvent, InputError, EVENT_QUEUE_CAPACITY, MAX_PROTOCOL_LINE_BYTES};
use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_RESTARTS_PER_WINDOW: u32 = 3;
const RESTART_WINDOW: Duration = Duration::from_secs(60);

/// Owns one helper. Consume events off the UI thread and cancel recording on Reset/Error.
pub struct HelperSupervisor {
    executable: PathBuf,
    config: HelperConfig,
    child: Child,
    events: Receiver<Result<HelperEvent, InputError>>,
    last_heartbeat: Instant,
    restart_window: Instant,
    restart_count: u32,
    operation_active: bool,
}

impl HelperSupervisor {
    /// Spawn an explicit sibling executable; never resolve helper paths through PATH.
    pub fn start(executable: impl AsRef<Path>, config: HelperConfig) -> Result<Self, InputError> {
        config.validate()?;
        let executable = executable.as_ref().canonicalize()?;
        let (child, events) = spawn(&executable, &config)?;
        Ok(Self {
            executable,
            config,
            child,
            events,
            last_heartbeat: Instant::now(),
            restart_window: Instant::now(),
            restart_count: 0,
            operation_active: false,
        })
    }

    /// Writes a bounded control message only when the application's active state changes.
    pub fn set_operation_active(&mut self, active: bool) -> Result<(), InputError> {
        if self.operation_active == active {
            return Ok(());
        }
        let input = self
            .child
            .stdin
            .as_mut()
            .ok_or_else(|| InputError::Helper("helper stdin missing".into()))?;
        serde_json::to_writer(
            &mut *input,
            &crate::HelperCommand::SetOperationActive { active },
        )
        .map_err(|error| InputError::Protocol(error.to_string()))?;
        input.write_all(b"\n")?;
        input.flush()?;
        self.operation_active = active;
        Ok(())
    }

    /// Wait briefly for an event, checking process death and stalled message-loop heartbeat.
    /// A replacement always emits Reset, so a lost release cannot leave recording latched.
    pub fn poll(&mut self, timeout: Duration) -> Result<Option<HelperEvent>, InputError> {
        if self.child.try_wait()?.is_some() || self.last_heartbeat.elapsed() > HEARTBEAT_TIMEOUT {
            return self.restart().map(Some);
        }
        match self
            .events
            .recv_timeout(timeout.min(Duration::from_millis(500)))
        {
            Ok(Ok(event)) => {
                if matches!(event, HelperEvent::Reset { .. }) {
                    self.operation_active = false;
                }
                if matches!(event, HelperEvent::Ready { protocol_version } if protocol_version != crate::PROTOCOL_VERSION)
                {
                    return Err(InputError::Protocol("helper version mismatch".into()));
                }
                if matches!(
                    event,
                    HelperEvent::Heartbeat { .. } | HelperEvent::Ready { .. }
                ) {
                    self.last_heartbeat = Instant::now();
                }
                Ok(Some(event))
            }
            Ok(Err(error)) => Err(error),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => self.restart().map(Some),
        }
    }

    fn restart(&mut self) -> Result<HelperEvent, InputError> {
        if self.restart_window.elapsed() > RESTART_WINDOW {
            self.restart_window = Instant::now();
            self.restart_count = 0;
        }
        if self.restart_count >= MAX_RESTARTS_PER_WINDOW {
            return Err(InputError::Helper(
                "helper restart limit reached; shortcuts unavailable".into(),
            ));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        let (child, events) = spawn(&self.executable, &self.config)?;
        self.child = child;
        self.events = events;
        self.last_heartbeat = Instant::now();
        self.restart_count += 1;
        self.operation_active = false;
        Ok(HelperEvent::Reset {
            reason: "helperRestarted".into(),
        })
    }
}

impl Drop for HelperSupervisor {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn(
    executable: &Path,
    config: &HelperConfig,
) -> Result<(Child, Receiver<Result<HelperEvent, InputError>>), InputError> {
    let mut command = Command::new(executable);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn()?;
    let result = (|| {
        let input = child
            .stdin
            .as_mut()
            .ok_or_else(|| InputError::Helper("helper stdin missing".into()))?;
        serde_json::to_writer(&mut *input, config)
            .map_err(|error| InputError::Protocol(error.to_string()))?;
        input.write_all(b"\n")?;
        input.flush()?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| InputError::Helper("helper stdout missing".into()))?;
        let (sender, receiver) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        thread::Builder::new()
            .name("eagle-shortcut-protocol".into())
            .spawn(move || {
                let mut reader = BufReader::new(output);
                loop {
                    let event = read_event(&mut reader);
                    let terminal = event.is_err();
                    if sender.try_send(event).is_err() || terminal {
                        break;
                    }
                }
            })?;
        Ok(receiver)
    })();
    match result {
        Ok(receiver) => Ok((child, receiver)),
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(error)
        }
    }
}

/// The byte limit applies before UTF-8/JSON allocation, including hostile helper output.
fn read_event(reader: &mut impl BufRead) -> Result<HelperEvent, InputError> {
    let mut bytes = Vec::with_capacity(256);
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Err(InputError::Protocol("helper stream closed".into()));
        }
        let length = available
            .iter()
            .position(|&byte| byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        if bytes.len() + length > MAX_PROTOCOL_LINE_BYTES {
            return Err(InputError::Protocol("oversized helper message".into()));
        }
        bytes.extend_from_slice(&available[..length]);
        let complete = available[length - 1] == b'\n';
        reader.consume(length);
        if complete {
            break;
        }
    }
    serde_json::from_slice(&bytes).map_err(|error| InputError::Protocol(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_oversized_and_unknown_messages() {
        assert!(read_event(&mut std::io::Cursor::new(vec![
            b'x';
            MAX_PROTOCOL_LINE_BYTES + 1
        ]))
        .is_err());
        assert!(read_event(&mut std::io::Cursor::new(b"{\"type\":\"keyLog\"}\n")).is_err());
    }
}
