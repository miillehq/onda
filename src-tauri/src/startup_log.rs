//! Content-free checkpoints distinguish service, webview and tray initialization delays.
use std::time::Instant;

pub(super) fn stage(started: Instant, name: &'static str) {
    tracing::info!(
        subsystem = "startup",
        pid = std::process::id(),
        setup_ms = started.elapsed().as_millis() as u64,
        "{name}"
    );
}

/// The final marker remains after successful native webview and tray construction.
pub(super) fn ready(started: Instant) {
    tracing::info!(
        subsystem = "application",
        version = onda_core::APP_VERSION,
        pid = std::process::id(),
        setup_ms = started.elapsed().as_millis() as u64,
        "application started"
    );
}
