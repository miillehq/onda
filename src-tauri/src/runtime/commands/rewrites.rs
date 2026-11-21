use super::*;

impl RewriteEngine {
    pub(in crate::runtime) async fn preview_transform(
        &self,
        host: &Arc<Runtime>,
        source: String,
        transform_id: String,
    ) -> AppResult<OperationResult> {
        onda_core::text::validate_source(&source, None)?;
        let transform = lock(&host.data.store)?
            .transforms()?
            .into_iter()
            .find(|transform| transform.id == transform_id)
            .ok_or_else(|| AppError::invalid("Unknown transform."))?;
        let settings = lock(&host.data.store)?.settings()?;
        let id = host
            .operations
            .begin_operation(host, OperationMode::Transform)?;
        let cancellation = CancellationToken::new();
        let active = ActiveOperation {
            id: id.clone(),
            target: None,
            mode: OperationMode::Transform,
            selection: None,
            context: String::new(),
            settings,
            cancellation,
            // Home shows a tried rewrite as the last result, so History keeps it too.
            history: true,
            usage: None,
        };
        host.operations.set_active(active.clone())?;
        store_preview(host, &active, &source, &source, false)?;
        host.operations.emit_state(host);
        let retry = TransformRetry {
            route: RetryRoute::capture(&active.settings),
            instructions: transform_instruction(&transform.instruction),
            preferred_model: transform.model_preference.clone(),
            preserve_tokens: transform.preserve_protected_tokens,
            command_text: None,
        };
        host.results.set_retry(host, &id, retry.clone())?;
        let result: AppResult<OperationResult> = async {
            let output = checked_transform(host, &active, &source, &retry).await?;
            let result = store_preview(host, &active, &source, &output, true)?;
            Ok(result)
        }
        .await;
        if let Err(error) = &result {
            host.operations.fail(host, &id, error.clone());
        }
        result
    }

    pub(in crate::runtime) async fn retry_transformation(
        &self,
        host: &Arc<Runtime>,
    ) -> AppResult<OperationResult> {
        let saved = lock(&host.results.result)?
            .clone()
            .ok_or_else(|| AppError::invalid("No transform is available to retry."))?;
        if saved.result.inserted {
            return Err(AppError::invalid(
                "This transform was already inserted. Select the text and trigger a new transform.",
            ));
        }
        let retry = saved.retry.ok_or_else(|| {
            AppError::invalid("This result has no cloud transformation to retry.")
        })?;
        let settings = retry.route.apply(lock(&host.data.store)?.settings()?)?;
        let id = host
            .operations
            .begin_operation(host, OperationMode::Transform)?;
        let target = saved.target.clone();
        let active = ActiveOperation {
            id: id.clone(),
            target,
            mode: OperationMode::Transform,
            selection: saved.selection,
            context: String::new(),
            settings,
            cancellation: CancellationToken::new(),
            // A retry asks ChatGPT again, so its new text is kept like the first attempt's.
            history: true,
            usage: saved.usage.clone(),
        };
        host.operations.set_active(active.clone())?;
        host.results.save_result(
            host,
            &active,
            &saved.result.original,
            &saved.result.original,
            RecordedDelivery::Pending,
            false,
        )?;
        host.results.set_retry(host, &id, retry.clone())?;
        host.operations.emit_state(host);
        let started = Instant::now();
        let result: AppResult<OperationResult> = async {
            let output = checked_transform(host, &active, &saved.result.original, &retry).await?;
            host.results.save_result(
                host,
                &active,
                &saved.result.original,
                &output,
                RecordedDelivery::Pending,
                false,
            )?;
            if saved.target.is_some() {
                let remaining = FOCUS_RETURN_DELAY.saturating_sub(started.elapsed());
                tokio::time::sleep(remaining).await;
                host.operations.transition(host, &id, Phase::Inserting)?;
                host.targets.insert(host, &active, &output, false).await?;
            }
            let completed = finish_retry(host, &active)?;
            Ok(completed)
        }
        .await;
        if let Err(error) = &result {
            host.operations.fail(host, &id, error.clone());
        }
        result
    }
}

/// Store a provisional result for recovery, or complete it and publish its History entry.
fn store_preview(
    host: &Arc<Runtime>,
    active: &ActiveOperation,
    source: &str,
    output: &str,
    completed: bool,
) -> AppResult<OperationResult> {
    let result = OperationResult {
        command_text: None,
        operation_id: active.id.clone(),
        original: source.to_owned(),
        output: output.to_owned(),
        inserted: false,
        fallback_used: false,
        diff: onda_core::transforms::diff(source, output),
        suggestion: None,
    };
    let route = {
        let mut state = lock(&host.operations.state)?;
        if !state.matches(&active.id) {
            return Err(AppError::new(
                ErrorCode::Cancelled,
                "Operation cancelled.",
                false,
            ));
        }
        let mut slot = lock(&host.results.result)?;
        let retry = slot
            .as_ref()
            .filter(|saved| saved.result.operation_id == active.id)
            .and_then(|saved| saved.retry.clone());
        *slot = Some(SavedResult {
            usage: None,
            copied: false,
            failed: false,
            retry,
            result: result.clone(),
            target: None,
            selection: None,
            after_document: None,
            shortcut: onda_platform::PasteShortcut::ControlV,
        });
        drop(slot);
        if !completed {
            return Ok(result);
        }
        let route = state.snapshot().route;
        state.transition(&active.id, Phase::Completed)?;
        host.domain(AppEvent::ResultReady(result.clone()));
        route
    };
    host.retention
        .record_history(host, active, &result, route, HistoryStage::CompletedPreview);
    host.operations.emit_state(host);
    Ok(result)
}

fn finish_retry(host: &Arc<Runtime>, active: &ActiveOperation) -> AppResult<OperationResult> {
    let (completed, route) = {
        let mut state = lock(&host.operations.state)?;
        if !state.matches(&active.id) {
            return Err(AppError::new(
                ErrorCode::Cancelled,
                "Operation cancelled.",
                false,
            ));
        }
        let result = lock(&host.results.result)?
            .as_ref()
            .filter(|saved| saved.result.operation_id == active.id)
            .map(|saved| saved.result.clone())
            .ok_or_else(|| AppError::invalid("Transformation result expired."))?;
        state.transition(&active.id, Phase::Completed)?;
        (result, state.snapshot().route)
    };
    if active.target.is_none() {
        host.retention.record_history(
            host,
            active,
            &completed,
            route,
            HistoryStage::CompletedPreview,
        );
    }
    host.operations.emit_state(host);
    Ok(completed)
}

async fn checked_transform(
    host: &Arc<Runtime>,
    active: &ActiveOperation,
    source: &str,
    retry: &TransformRetry,
) -> AppResult<String> {
    let output = host
        .rewrites
        .cloud_text(
            host,
            active,
            source,
            &retry.instructions,
            retry.preferred_model.as_deref(),
        )
        .await?;
    if retry.preserve_tokens {
        host.results
            .check_enhancement(host, active, source, &output, &[], false)?;
    }
    Ok(output)
}
