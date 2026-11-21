//! Owns the current recovery result and the user's pending correction proposal.
use super::*;

#[derive(Default)]
pub(super) struct OperationResults {
    pub(super) result: Mutex<Option<SavedResult>>,
    pub(super) proposal: Mutex<Option<LearningProposal>>,
}

pub(super) enum RecordedDelivery {
    Pending,
    Inserted(Option<String>),
}

impl OperationResults {
    pub(super) fn save_result(
        &self,
        host: &Runtime,
        active: &ActiveOperation,
        original: &str,
        output: &str,
        delivery: RecordedDelivery,
        fallback: bool,
    ) -> AppResult<()> {
        let (inserted, after_document) = match delivery {
            RecordedDelivery::Pending => (false, None),
            RecordedDelivery::Inserted(document) => (true, document),
        };
        let state = lock(&host.operations.state)?;
        if !state.matches(&active.id) {
            return Err(AppError::new(
                ErrorCode::Cancelled,
                "Operation cancelled.",
                false,
            ));
        }
        let copied = lock(&self.result)?.as_ref().is_some_and(|saved| {
            saved.result.operation_id == active.id && saved.result.output == output && saved.copied
        });
        let (previous_retry, suggestion) = lock(&self.result)?
            .as_ref()
            .filter(|saved| saved.result.operation_id == active.id)
            .map(|saved| (saved.retry.clone(), saved.result.suggestion.clone()))
            .unwrap_or_default();
        let result = OperationResult {
            command_text: previous_retry
                .as_ref()
                .and_then(|retry| retry.command_text.clone()),
            operation_id: active.id.clone(),
            original: original.into(),
            output: output.into(),
            inserted,
            fallback_used: fallback,
            diff: onda_core::transforms::diff(original, output),
            suggestion,
        };
        let shortcut = paste_shortcut(&active.settings, active.executable_name());
        *lock(&self.result)? = Some(SavedResult {
            usage: active.usage.clone(),
            copied,
            failed: false,
            retry: previous_retry,
            result: result.clone(),
            target: active.target.clone(),
            selection: active.selection.clone(),
            after_document,
            shortcut,
        });
        let route = state.snapshot().route;
        host.domain(AppEvent::ResultReady(result.clone()));
        drop(state);
        host.retention
            .record_history(host, active, &result, route, HistoryStage::Result);
        Ok(())
    }

    pub(super) fn set_retry(
        &self,
        host: &Runtime,
        id: &str,
        retry: TransformRetry,
    ) -> AppResult<()> {
        let state = lock(&host.operations.state)?;
        if !state.matches(id) {
            return Err(AppError::new(
                ErrorCode::Cancelled,
                "Operation cancelled.",
                false,
            ));
        }
        let mut saved = lock(&self.result)?;
        let saved = saved
            .as_mut()
            .filter(|saved| saved.result.operation_id == id)
            .ok_or_else(|| AppError::invalid("No transformation result is available."))?;
        saved.result.command_text = retry.command_text.clone();
        saved.retry = Some(retry);
        let result = saved.result.clone();
        host.domain(AppEvent::ResultReady(result.clone()));
        drop(state);
        host.retention.update_history(host, id, |entry| {
            entry.command_text = result.command_text.clone()
        });
        Ok(())
    }

    /// Check ChatGPT's text before it is used. A rejected version stays on the result and in
    /// History, so the user can still copy it.
    pub(super) fn check_enhancement(
        &self,
        host: &Runtime,
        active: &ActiveOperation,
        original: &str,
        candidate: &str,
        dictionary: &[String],
        strict_cleanup: bool,
    ) -> AppResult<()> {
        let checked =
            onda_core::text::validate_enhancement(original, candidate, dictionary, strict_cleanup);
        if checked.is_err() {
            self.keep_suggestion(host, active, candidate);
        }
        checked
    }

    pub(super) fn keep_suggestion(
        &self,
        host: &Runtime,
        active: &ActiveOperation,
        suggestion: &str,
    ) {
        let (result, route) = {
            let Ok(state) = lock(&host.operations.state) else {
                return;
            };
            if !state.matches(&active.id) {
                return;
            }
            let Ok(mut saved) = lock(&self.result) else {
                return;
            };
            let Some(saved) = saved
                .as_mut()
                .filter(|saved| saved.result.operation_id == active.id)
            else {
                return;
            };
            saved.result.suggestion = Some(suggestion.to_owned());
            (saved.result.clone(), state.snapshot().route)
        };
        host.domain(AppEvent::ResultReady(result.clone()));
        host.retention
            .record_history(host, active, &result, route, HistoryStage::Result);
    }

    pub(super) async fn retry_result(&self, host: &Arc<Runtime>) -> AppResult<()> {
        let saved = lock(&self.result)?
            .clone()
            .ok_or_else(|| AppError::invalid("There is no result to retry."))?;
        let target = saved
            .target
            .clone()
            .ok_or_else(|| AppError::invalid("Preview results have no insertion target."))?;
        let settings = lock(&host.data.store)?.settings()?;
        let id = host.operations.begin_recovery_operation(host)?;
        let active = ActiveOperation {
            id: id.clone(),
            target: Some(target),
            mode: OperationMode::Transform,
            selection: saved.selection,
            context: String::new(),
            settings,
            cancellation: CancellationToken::new(),
            history: false,
            usage: saved.usage.clone(),
        };
        host.operations.set_active(active.clone())?;
        host.operations.emit_state(host);
        let service = host.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(FOCUS_RETURN_DELAY).await;
            let result = async {
                service.results.save_result(
                    &service,
                    &active,
                    &saved.result.original,
                    &saved.result.output,
                    RecordedDelivery::Pending,
                    saved.result.fallback_used,
                )?;
                service
                    .operations
                    .transition(&service, &id, Phase::Inserting)?;
                service
                    .targets
                    .insert(
                        &service,
                        &active,
                        &saved.result.output,
                        saved.result.fallback_used,
                    )
                    .await?;
                service
                    .operations
                    .transition(&service, &id, Phase::Completed)
            }
            .await;
            if let Err(error) = result {
                service.operations.fail(&service, &id, error);
            }
        });
        Ok(())
    }

    pub(super) async fn undo_result(&self, host: &Arc<Runtime>) -> AppResult<()> {
        if lock(&host.operations.state)?.is_active() {
            return Err(AppError::new(
                ErrorCode::Busy,
                "Wait for the current operation.",
                true,
            ));
        }
        let saved = lock(&self.result)?
            .clone()
            .ok_or_else(|| AppError::invalid("No transform is available to undo."))?;
        if !saved.result.inserted || saved.selection.is_none() {
            return Err(AppError::invalid(
                "Undo applies only to inserted selection transforms.",
            ));
        }
        let target = saved
            .target
            .ok_or_else(|| AppError::invalid("Undo target is unavailable."))?;
        let document = saved.after_document.ok_or_else(|| {
            AppError::new(
                ErrorCode::UnsupportedControl,
                "The target did not expose a bounded document for safe undo.",
                false,
            )
        })?;
        let id = host
            .operations
            .begin_operation(host, OperationMode::Transform)?;
        host.operations.emit_state(host);
        let cancellation = CancellationToken::new();
        let active = ActiveOperation {
            id: id.clone(),
            target: Some(target.clone()),
            mode: OperationMode::Transform,
            selection: None,
            context: String::new(),
            settings: lock(&host.data.store)?.settings()?,
            cancellation: cancellation.clone(),
            history: false,
            usage: None,
        };
        host.operations.set_active(active)?;
        let service = host.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(FOCUS_RETURN_DELAY).await;
            if cancellation.is_cancelled() {
                return;
            }
            let result = tokio::task::spawn_blocking(move || {
                WindowsTargetService::new()
                    .replace_unique_text_guarded(
                        &target,
                        &document,
                        &saved.result.output,
                        &saved.result.original,
                        saved.shortcut,
                        || cancellation.is_cancelled(),
                    )
                    .map_err(errors::platform)
            })
            .await;
            match result {
                Ok(Ok(outcome)) if outcome.input_injected && outcome.clipboard_read_observed => {
                    finish_undo(&service, &id);
                }
                Ok(Err(error)) => service.operations.fail(&service, &id, error),
                _ => service.operations.fail(
                    &service,
                    &id,
                    AppError::new(ErrorCode::Insertion, "Undo could not be confirmed.", true),
                ),
            }
        });
        Ok(())
    }
}

fn finish_undo(service: &Arc<Runtime>, id: &str) {
    if let Ok(mut state) = lock(&service.operations.state) {
        if state.transition(id, Phase::Inserting).is_ok()
            && state.transition(id, Phase::Completed).is_ok()
        {
            if let Ok(mut result) = lock(&service.results.result) {
                *result = None;
            }
        }
    }
    service.operations.emit_state(service);
}
