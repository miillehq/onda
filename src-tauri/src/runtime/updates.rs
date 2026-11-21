//! Owns the update reservation and the checked update lifecycle.
use super::*;

#[derive(Default)]
pub(super) struct Updates {
    pub(super) updating: AtomicBool,
}

impl Updates {
    pub(super) fn ensure_not_updating(&self) -> AppResult<()> {
        if self.updating.load(Ordering::Acquire) {
            Err(AppError::new(
                ErrorCode::Busy,
                "A signed update is being installed. Wait for the application to restart.",
                true,
            ))
        } else {
            Ok(())
        }
    }
}
