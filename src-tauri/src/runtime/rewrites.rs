//! Coordinates the selected provider for previews, selected-text rewrites and spoken edits.
use super::*;

pub(super) struct RewriteEngine {
    pub(super) accounts: ConnectedAccounts,
    pub(super) provider_access: ProviderAccess,
}

impl RewriteEngine {
    pub(super) async fn cloud_text(
        &self,
        host: &Arc<Runtime>,
        active: &ActiveOperation,
        source: &str,
        instructions: &str,
        preferred: Option<&str>,
    ) -> AppResult<String> {
        tokio::select! {_ = active.cancellation.cancelled()=>Err(AppError::new(ErrorCode::Cancelled,"Operation cancelled.",false)), result=self.cloud_text_inner(host, active,source,instructions,preferred)=>result}
    }

    pub(super) async fn cloud_text_inner(
        &self,
        host: &Arc<Runtime>,
        active: &ActiveOperation,
        source: &str,
        instructions: &str,
        preferred: Option<&str>,
    ) -> AppResult<String> {
        onda_core::text::validate_source(source, Some(&active.context))?;
        match active.settings.ai_provider {
            AiProvider::Chatgpt => {
                self.accounts
                    .chatgpt_text(host, active, source, instructions, preferred)
                    .await
            }
            // A rewrite's preferred model names a ChatGPT model; other providers use theirs.
            provider => {
                self.provider_access
                    .provider_text(host, active, provider, source, instructions)
                    .await
            }
        }
    }

    pub(super) async fn begin_selection_transform(
        &self,
        host: &Arc<Runtime>,
        transform_id: String,
    ) -> AppResult<()> {
        if host.microphone.microphone_testing.load(Ordering::Acquire) {
            return Err(AppError::new(
                ErrorCode::Busy,
                "Microphone test is active.",
                true,
            ));
        }
        let settings = lock(&host.data.store)?.settings()?;
        let transform = lock(&host.data.store)?
            .transforms()?
            .into_iter()
            .find(|transform| transform.id == transform_id)
            .ok_or_else(|| AppError::invalid("Transform was not found."))?;
        let id = host
            .operations
            .begin_operation(host, OperationMode::Transform)?;
        host.operations.emit_state(host);
        let native = tokio::task::spawn_blocking(|| {
            let service = WindowsTargetService::new();
            let target = service.snapshot().map_err(errors::platform)?;
            let selected = service
                .selected_text_for_rewrite(&target, onda_platform::MAX_TEXT_CHARACTERS)
                .map_err(errors::selection)?;
            Ok::<_, AppError>((target, selected))
        })
        .await
        .map_err(|_| {
            host.operations
                .worker_failure(host, &id, "Selection worker stopped.")
        })?;
        let (target, selection) = match native {
            Ok(value) => value,
            Err(error) => {
                host.operations.fail(host, &id, error.clone());
                return Err(error);
            }
        };
        validate_selected_source(&selection).inspect_err(|error| {
            host.operations.fail(host, &id, error.clone());
        })?;
        let active = ActiveOperation {
            id: id.clone(),
            target: Some(target),
            mode: OperationMode::Transform,
            selection: Some(selection.clone()),
            context: String::new(),
            settings,
            cancellation: CancellationToken::new(),
            history: true,
            usage: None,
        };
        host.operations.set_active(active.clone())?;
        host.results.save_result(
            host,
            &active,
            &selection,
            &selection,
            RecordedDelivery::Pending,
            false,
        )?;
        launch_selection_transform(host.clone(), id, active, selection, transform);
        Ok(())
    }
}

fn launch_selection_transform(
    service: Arc<Runtime>,
    id: String,
    active: ActiveOperation,
    selection: String,
    transform: Transform,
) {
    tauri::async_runtime::spawn(async move {
        let result = async {
            let instruction = transform_instruction(&transform.instruction);
            service.results.set_retry(
                &service,
                &id,
                TransformRetry {
                    route: RetryRoute::capture(&active.settings),
                    instructions: instruction.clone(),
                    preferred_model: transform.model_preference.clone(),
                    preserve_tokens: transform.preserve_protected_tokens,
                    command_text: None,
                },
            )?;
            let output = service
                .rewrites
                .cloud_text(
                    &service,
                    &active,
                    &selection,
                    &instruction,
                    transform.model_preference.as_deref(),
                )
                .await?;
            if transform.preserve_protected_tokens {
                service.results.check_enhancement(
                    &service,
                    &active,
                    &selection,
                    &output,
                    &[],
                    false,
                )?;
            }
            service.results.save_result(
                &service,
                &active,
                &selection,
                &output,
                RecordedDelivery::Pending,
                false,
            )?;
            service
                .operations
                .transition(&service, &id, Phase::Inserting)?;
            service
                .targets
                .insert(&service, &active, &output, false)
                .await?;
            service
                .operations
                .transition(&service, &id, Phase::Completed)
        }
        .await;
        if let Err(error) = result {
            if error.code != ErrorCode::Cancelled {
                service.operations.fail(&service, &id, error);
            }
        }
    });
}
