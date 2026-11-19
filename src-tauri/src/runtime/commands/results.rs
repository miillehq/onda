use super::*;
use ApiResponse as O;

pub(super) async fn start_dictation(
    service: &Arc<Runtime>,
    command_mode: bool,
) -> AppResult<ApiResponse> {
    service
        .operations
        .send_action(Action::Start { command_mode })
        .await?;
    Ok(O::Unit)
}

pub(super) async fn stop_dictation(service: &Arc<Runtime>) -> AppResult<ApiResponse> {
    service.operations.send_action(Action::Stop).await?;
    Ok(O::Unit)
}

pub(super) fn cancel_operation(service: &Arc<Runtime>) -> AppResult<ApiResponse> {
    service.operations.cancel_immediate(service);
    Ok(O::Unit)
}

pub(super) async fn transform_selection(
    service: &Arc<Runtime>,
    transform_id: String,
) -> AppResult<ApiResponse> {
    service
        .operations
        .send_action(Action::Transform { transform_id })
        .await?;
    Ok(O::Unit)
}

pub(super) async fn retry_result(service: &Arc<Runtime>) -> AppResult<ApiResponse> {
    service.operations.send_action(Action::Retry).await?;
    Ok(O::Unit)
}

pub(super) async fn undo_last_transform(service: &Arc<Runtime>) -> AppResult<ApiResponse> {
    service.operations.send_action(Action::Undo).await?;
    Ok(O::Unit)
}

pub(super) async fn copy_result(service: &Arc<Runtime>) -> AppResult<ApiResponse> {
    let (saved, usage) = lock(&service.results.result)?
        .as_ref()
        .map(|result| (result.result.clone(), result.usage.clone()))
        .ok_or_else(|| AppError::invalid("No result is available to copy."))?;
    copy_text(saved.output.clone()).await?;
    service.credit_delivery(usage.as_ref(), saved.fallback_used);
    if let Some(current) = lock(&service.results.result)?.as_mut() {
        if current.result.operation_id == saved.operation_id
            && current.result.output == saved.output
        {
            current.copied = true;
        }
    }
    Ok(O::Unit)
}

pub(super) fn clear_result(service: &Arc<Runtime>) -> AppResult<ApiResponse> {
    service.operations.cancel_immediate(service);
    *lock(&service.results.result)? = None;
    *lock(&service.results.proposal)? = None;
    Ok(O::Unit)
}

pub(super) async fn copy_suggestion(service: &Arc<Runtime>) -> AppResult<ApiResponse> {
    let text = lock(&service.results.result)?
        .as_ref()
        .and_then(|saved| saved.result.suggestion.clone())
        .ok_or_else(|| AppError::invalid("The AI version is no longer available."))?;
    copy_text(text).await?;
    Ok(O::Unit)
}

pub(super) fn dismiss_error(
    service: &Arc<Runtime>,
    operation_id: String,
) -> AppResult<ApiResponse> {
    if lock(&service.operations.state)?.dismiss_error(&operation_id) {
        service.pill.hide_now(&service.app);
        service.operations.emit_state(service);
    }
    Ok(O::Unit)
}

pub(super) async fn copy_history_suggestion(
    service: &Arc<Runtime>,
    id: String,
) -> AppResult<ApiResponse> {
    let text = service
        .retention
        .history_entry(service, &id)?
        .suggestion
        .clone()
        .ok_or_else(|| AppError::invalid("That history entry is no longer kept."))?;
    copy_text(text).await?;
    Ok(O::Unit)
}

pub(super) async fn copy_history_entry(
    service: &Arc<Runtime>,
    id: String,
) -> AppResult<ApiResponse> {
    let text = service
        .retention
        .history_entry(service, &id)?
        .output
        .clone();
    let usage = lock(&service.results.result)?
        .as_ref()
        .filter(|saved| saved.result.operation_id == id && saved.result.output == text)
        .and_then(|saved| {
            saved
                .usage
                .clone()
                .map(|usage| (usage, saved.result.fallback_used))
        });
    copy_text(text.clone()).await?;
    if let Some((receipt, fallback)) = usage {
        service.credit_delivery(Some(&receipt), fallback);
    }
    if let Some(current) = lock(&service.results.result)?.as_mut() {
        if current.result.operation_id == id && current.result.output == text {
            current.copied = true;
        }
    }
    Ok(O::Unit)
}

pub(super) fn delete_history_entry(service: &Arc<Runtime>, id: String) -> AppResult<ApiResponse> {
    service
        .retention
        .delete_retained_history(service, Some(&id))?;
    service.results.forget_result(service, Some(&id))?;
    service.event(RuntimeEvent::HistoryReset);
    Ok(O::Unit)
}

pub(super) fn clear_history(service: &Arc<Runtime>) -> AppResult<ApiResponse> {
    service.retention.delete_retained_history(service, None)?;
    service.results.forget_result(service, None)?;
    service.event(RuntimeEvent::HistoryReset);
    Ok(O::Unit)
}

impl OperationResults {
    /// Deleted history should not linger as the visible result; an operation in flight keeps it.
    pub(in crate::runtime) fn forget_result(
        &self,
        host: &Runtime,
        id: Option<&str>,
    ) -> AppResult<()> {
        let state = lock(&host.operations.state)?;
        if state.is_active() {
            return Ok(());
        }
        let mut result = lock(&self.result)?;
        if result
            .as_ref()
            .is_some_and(|saved| id.is_none_or(|id| saved.result.operation_id == id))
        {
            *result = None;
        }
        Ok(())
    }
}
