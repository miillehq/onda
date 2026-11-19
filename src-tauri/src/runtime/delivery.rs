//! Deliver a result only while its operation still owns the native handoff.
use super::*;

/// The paste chord the user presses when Onda leaves the words on the clipboard.
const PASTE_KEYS: &str = if cfg!(target_os = "macos") {
    "\u{2318}V"
} else {
    "Ctrl+V"
};
const PRECEDING_CHARACTERS: usize = 2;

impl Runtime {
    pub(super) async fn insert(
        self: &Arc<Self>,
        active: &ActiveOperation,
        output: &str,
        fallback: bool,
    ) -> AppResult<()> {
        if cfg!(not(windows)) {
            let target = active.insertion_target()?.clone();
            let typeable =
                tokio::task::spawn_blocking(move || WindowsTargetService::new().can_type(&target))
                    .await
                    .unwrap_or(false);
            if !typeable {
                return self.hand_off_to_clipboard(active, output, fallback).await;
            }
        }
        let target = active.insertion_target()?.clone();
        let text = output.to_owned();
        let selection = active.selection.clone();
        let observe_document = selection.is_some() || active.settings.learning_enabled;
        let shortcut = paste_shortcut(&active.settings, &target.executable_name);
        let cancellation = active.cancellation.clone();
        let started = Instant::now();
        let native = tokio::task::spawn_blocking(move || {
            if cancellation.is_cancelled() {
                return Err((
                    AppError::new(ErrorCode::Cancelled, "Operation cancelled.", false),
                    false,
                ));
            }
            let targets = WindowsTargetService::new();
            let typed = (|| {
                let outcome = if let Some(original) = &selection {
                    targets.replace_selection_guarded(&target, original, &text, shortcut, || {
                        cancellation.is_cancelled()
                    })
                } else {
                    // Two characters tell a closing quote from an opening one; a field that
                    // exposes no caret simply gets the text as dictated.
                    let before = targets
                        .preceding_text(&target, PRECEDING_CHARACTERS)
                        .unwrap_or_default();
                    let spaced = onda_core::text::space_after_previous_text(&before, &text);
                    targets
                        .paste_guarded(&target, &spaced, shortcut, || cancellation.is_cancelled())
                }
                .map_err(errors::platform)?;
                if !outcome.input_injected || !outcome.clipboard_read_observed {
                    return Err(AppError::new(
                        ErrorCode::Insertion,
                        "The target did not acknowledge the paste. Copy your result or retry.",
                        true,
                    ));
                }
                Ok(())
            })();
            typed.map_err(|error| copy_untyped(error, &text, &cancellation))?;
            let document = if observe_document {
                targets.field_text(&target, LEARNING_CHARACTERS).ok()
            } else {
                None
            };
            Ok(document)
        })
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Insertion worker stopped.", true))?;
        self.metric(&active.id, "paste", started, native.is_ok());
        let document = match native {
            Ok(document) => document,
            Err((error, copied)) => {
                if copied {
                    self.credit_delivery(active.usage.as_ref(), fallback);
                    if let Ok(mut saved) = lock(&self.result) {
                        if let Some(saved) = saved
                            .as_mut()
                            .filter(|saved| saved.result.operation_id == active.id)
                        {
                            saved.copied = true;
                        }
                    }
                }
                return Err(error);
            }
        };
        self.credit_delivery(active.usage.as_ref(), fallback);
        let original = lock(&self.result)?
            .as_ref()
            .map(|result| result.result.original.clone())
            .unwrap_or_else(|| output.into());
        self.save_result(active, &original, output, true, fallback, document.clone())?;
        if active.settings.learning_enabled && active.mode == OperationMode::Dictation {
            if let Some(document) = document {
                self.start_learning(active, document, output.to_owned());
            }
        }
        Ok(())
    }

    /// Where Onda cannot type into the focused app (Linux, without the paste permission or a
    /// field it can edit), a dictation completes by putting the words on the clipboard with the
    /// privacy markers; the pill and Home say to press Ctrl+V.
    async fn hand_off_to_clipboard(
        self: &Arc<Self>,
        active: &ActiveOperation,
        output: &str,
        fallback: bool,
    ) -> AppResult<()> {
        if active.selection.is_some() {
            return Err(errors::platform(
                onda_platform::PlatformError::UnsupportedPlatform,
            ));
        }
        let text = output.to_owned();
        let started = Instant::now();
        let cancellation = active.cancellation.clone();
        let copied = tokio::task::spawn_blocking(move || {
            copy_while_active(&cancellation, || {
                onda_platform::copy_text_to_clipboard(&text)
            })
        })
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Clipboard worker stopped.", true))?;
        self.metric(&active.id, "clipboard", started, copied.is_ok());
        copied.map_err(errors::platform)?;
        self.credit_delivery(active.usage.as_ref(), fallback);
        let original = lock(&self.result)?
            .as_ref()
            .map(|result| result.result.original.clone())
            .unwrap_or_else(|| output.into());
        self.save_result(active, &original, output, false, fallback, None)?;
        if let Some(saved) = lock(&self.result)?
            .as_mut()
            .filter(|saved| saved.result.operation_id == active.id)
        {
            saved.copied = true;
        }
        Ok(())
    }
}

/// When Onda could not type, put the words on the clipboard so they paste wherever the user is
/// now, instead of leaving whatever was copied before; the message says which happened. Password
/// fields and clipboards Onda must not overwrite are left alone.
fn copy_untyped(
    mut error: AppError,
    text: &str,
    cancellation: &CancellationToken,
) -> (AppError, bool) {
    let reason = match error.code {
        ErrorCode::FocusChanged => "Onda only types where you started dictating.",
        ErrorCode::TargetExited => "Nothing was typed.",
        ErrorCode::ElevatedTarget => {
            "Windows doesn't let Onda type into apps run as administrator."
        }
        ErrorCode::UnsupportedControl => "Onda can't type into this kind of field.",
        ErrorCode::ClipboardBusy => "Another app was holding the clipboard.",
        ErrorCode::Insertion => "The app didn't accept the paste.",
        _ => return (error, false),
    };
    let copied = if text.trim().is_empty() {
        false
    } else {
        match copy_while_active(cancellation, || onda_platform::copy_text_to_clipboard(text)) {
            Ok(()) => true,
            Err(onda_platform::PlatformError::Cancelled) => {
                return (
                    errors::platform(onda_platform::PlatformError::Cancelled),
                    false,
                )
            }
            Err(_) => false,
        }
    };
    error.message = if copied {
        format!("{reason} Your words are on the clipboard, so press {PASTE_KEYS} to paste them.")
    } else {
        format!("{reason} Your words are saved in History.")
    };
    (error, copied)
}

/// Check inside the blocking worker, after any queue or platform-capability wait.
fn copy_while_active(
    token: &CancellationToken,
    copy: impl FnOnce() -> Result<(), onda_platform::PlatformError>,
) -> Result<(), onda_platform::PlatformError> {
    if token.is_cancelled() {
        return Err(onda_platform::PlatformError::Cancelled);
    }
    copy()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_during_the_handoff_wait_never_reaches_the_clipboard_writer() {
        let token = CancellationToken::new();
        assert!(copy_while_active(&token, || Ok(())).is_ok());
        token.cancel();
        assert!(matches!(
            copy_while_active(&token, || panic!("late clipboard write")),
            Err(onda_platform::PlatformError::Cancelled)
        ));
    }
    #[test]
    fn untyped_words_are_copied_only_after_failures_that_left_them_untyped() {
        // Neither case reaches the clipboard: one is excluded, the other has no words.
        let (password, copied) = copy_untyped(
            AppError::new(ErrorCode::PasswordField, "secure field", false),
            "words",
            &CancellationToken::new(),
        );
        assert!(!copied);
        assert_eq!(password.message, "secure field");
        let (moved, copied) = copy_untyped(
            AppError::new(ErrorCode::FocusChanged, "moved", true),
            "  ",
            &CancellationToken::new(),
        );
        assert!(!copied);
        assert_eq!(
            moved.message,
            "Onda only types where you started dictating. Your words are saved in History."
        );
    }
}
