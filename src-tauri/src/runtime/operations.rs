//! Owns the serialized user-action queue, active generation and operation state machine.
use super::*;

pub(super) struct OperationActor {
    pub(super) state: Mutex<StateMachine>,
    pub(super) active: Mutex<Option<ActiveOperation>>,
    pub(super) actions: mpsc::Sender<ActionMessage>,
    pub(super) shutdown: AtomicBool,
}

impl OperationActor {
    pub(super) fn emit_state(&self, host: &Runtime) {
        if let Ok(state) = lock(&self.state) {
            let current = state.snapshot();
            if let Some(tray) = host.app.tray_by_id("main") {
                let status = tray_status(&current.phase);
                let _ = tray.set_tooltip(Some(format!("{} · {}", onda_core::APP_NAME, status)));
                // GNOME and KDE show no tray tooltips; the label beside the icon shows the state.
                #[cfg(target_os = "linux")]
                let _ = tray.set_title((status != "Ready").then_some(status));
            }
            tracing::info!(subsystem="operation",operation_id=current.operation_id.as_deref().unwrap_or("none"),phase=?current.phase,route=?current.route,"state transition");
            drop(state);
            host.pill.on_state(&host.app, &current);
            host.domain(AppEvent::StateChanged(current));
        }
    }

    pub(super) fn fail(&self, host: &Runtime, id: &str, error: AppError) {
        if let Ok(mut state) = lock(&self.state) {
            if !state.matches(id) {
                return;
            }
            state.fail(id, error.clone());
        }
        if let Ok(mut saved) = lock(&host.results.result) {
            if let Some(saved) = saved
                .as_mut()
                .filter(|saved| saved.result.operation_id == id)
            {
                saved.failed = true;
            }
        }
        tracing::warn!(subsystem="operation",operation_id=id,code=?error.code,"operation failed");
        host.recognition.discard_warm_worker();
        if error.code != ErrorCode::Cancelled {
            host.retention
                .mark_history_failed(host, id, error.code.clone());
        }
        host.domain(AppEvent::Error(error));
        self.emit_state(host);
    }

    pub(super) fn worker_failure(&self, host: &Runtime, id: &str, message: &str) -> AppError {
        let error = AppError::new(ErrorCode::Internal, message, true);
        self.fail(host, id, error.clone());
        error
    }

    pub(super) fn set_active(&self, operation: ActiveOperation) -> AppResult<()> {
        let state = lock(&self.state)?;
        if !state.matches(&operation.id) {
            return Err(AppError::new(
                ErrorCode::Cancelled,
                "Operation cancelled.",
                false,
            ));
        }
        *lock(&self.active)? = Some(operation);
        drop(state);
        Ok(())
    }

    pub(super) fn begin_operation(&self, host: &Runtime, mode: OperationMode) -> AppResult<String> {
        let mut state = lock(&self.state)?;
        host.updates.ensure_not_updating()?;
        state.begin(mode)
    }

    /// History keeps failed text findable, so new operations never wait on it. Session-only
    /// history is lost on restart, so an update must not discard text never typed or copied,
    /// even after the error was dismissed.
    pub(super) fn ensure_recovery_resolved(&self, host: &Runtime) -> AppResult<()> {
        if lock(&host.retention.history)?.days == 0
            && lock(&host.results.result)?.as_ref().is_some_and(|saved| {
                saved.failed
                    && saved.target.is_some()
                    && saved.selection.is_none()
                    && !saved.result.inserted
                    && !saved.copied
                    && !saved.result.output.is_empty()
            })
        {
            return Err(AppError::new(
                ErrorCode::Busy,
                "Copy or discard the previous recovery text before starting another operation.",
                true,
            ));
        }
        Ok(())
    }

    pub(super) fn begin_recovery_operation(&self, host: &Runtime) -> AppResult<String> {
        let mut state = lock(&self.state)?;
        host.updates.ensure_not_updating()?;
        state.begin(OperationMode::Transform)
    }

    pub(super) fn queue_helper_action(&self, host: &Runtime, action: Action) {
        if self
            .actions
            .try_send(ActionMessage {
                action,
                reply: None,
            })
            .is_err()
        {
            self.cancel_immediate(host);
        }
    }

    pub(super) fn transition(&self, host: &Runtime, id: &str, phase: Phase) -> AppResult<()> {
        lock(&self.state)?.transition(id, phase)?;
        self.emit_state(host);
        Ok(())
    }

    pub(super) async fn send_action(&self, action: Action) -> AppResult<()> {
        let (sender, receiver) = oneshot::channel();
        self.actions
            .try_send(ActionMessage {
                action,
                reply: Some(sender),
            })
            .map_err(|_| AppError::new(ErrorCode::Busy, "Operation queue is busy.", true))?;
        receiver
            .await
            .map_err(|_| AppError::new(ErrorCode::Internal, "Operation service stopped.", true))?
    }

    pub(super) async fn action_loop(
        &self,
        host: &Arc<Runtime>,
        mut receiver: mpsc::Receiver<ActionMessage>,
    ) {
        while let Some(message) = receiver.recv().await {
            if self.shutdown.load(Ordering::Acquire) {
                break;
            }
            let result = match message.action {
                Action::Start { command_mode } => {
                    host.microphone
                        .begin_recording(host, command_mode, false)
                        .await
                }
                Action::Stop => host.microphone.finish_recording(host).await,
                Action::StopGeneration { id } => {
                    let should_finish = {
                        let state = lock(&self.state);
                        state
                            .map(|state| {
                                state.matches(&id) && state.snapshot().phase == Phase::Recording
                            })
                            .unwrap_or(false)
                    };
                    if should_finish {
                        host.microphone.finish_recording(host).await
                    } else {
                        Ok(())
                    }
                }
                Action::CancelGeneration { id } => {
                    host.microphone.cancel_generation(host, &id).await
                }
                Action::Transform { transform_id } => {
                    host.rewrites
                        .begin_selection_transform(host, transform_id)
                        .await
                }
                Action::Retry => host.results.retry_result(host).await,
                Action::Undo => host.results.undo_result(host).await,
                Action::Toggle => {
                    if lock(&self.state)
                        .map(|state| state.snapshot().phase == Phase::Recording)
                        .unwrap_or(false)
                    {
                        host.microphone.finish_recording(host).await
                    } else {
                        host.microphone.begin_recording(host, false, true).await
                    }
                }
            };
            if let Some(reply) = message.reply {
                let _ = reply.send(result);
            } else if let Err(error) = result {
                host.domain(AppEvent::Error(error));
            }
        }
    }

    pub(super) fn cancel_immediate(&self, host: &Runtime) {
        if let Ok(Some(id)) = self.cancel_owned_operation(host) {
            let _ = self.actions.try_send(ActionMessage {
                action: Action::CancelGeneration { id },
                reply: None,
            });
        }
        self.emit_state(host);
    }

    pub(super) fn cancel_owned_operation(&self, host: &Runtime) -> AppResult<Option<String>> {
        host.recognition.discard_warm_worker();
        let mut state = lock(&self.state)?;
        let id = state.snapshot().operation_id;
        {
            let active = lock(&self.active)?;
            if let Some(operation) = active.as_ref() {
                if id.as_deref() == Some(&operation.id) {
                    operation.cancellation.cancel();
                }
            }
        }
        if !state.is_active() {
            return Ok(None);
        }
        state.cancel();
        Ok(id)
    }

    pub(super) async fn cancel(&self, host: &Arc<Runtime>) -> AppResult<()> {
        let cancelled = self.cancel_owned_operation(host)?;
        self.emit_state(host);
        if let Some(id) = cancelled {
            host.microphone.cancel_generation(host, &id).await?;
        }
        Ok(())
    }

    /// `onda --dictate`, for Linux desktops without a shortcuts portal: it starts or finishes
    /// hands-free dictation like the hands-free shortcut does.
    pub(super) fn toggle_dictation(&self, host: &Runtime) {
        self.queue_helper_action(host, Action::Toggle);
    }

    pub(super) fn shutdown(&self, host: &Runtime) {
        self.shutdown.store(true, Ordering::Release);
        host.recognition.discard_warm_worker();
        self.cancel_immediate(host);
        if let Ok(mut capture) = lock(&host.microphone.capture) {
            let _ = capture.cancel();
        }
        if let Ok(downloads) = lock(&host.model_library.downloads) {
            for cancellation in downloads.values() {
                cancellation.cancel();
            }
        }
        if let Ok(oauth) = lock(&host.rewrites.accounts.oauth) {
            if let Some(cancellation) = oauth.as_ref() {
                cancellation.cancel();
            }
        }
        if let Ok(mut helper) = lock(&host.shortcuts.helper_join) {
            if let Some(helper) = helper.take() {
                let _ = helper.join();
            }
        }
    }
}

impl OperationActor {
    pub(super) fn new() -> (Self, mpsc::Receiver<ActionMessage>) {
        let (actions, receiver) = mpsc::channel(ACTION_QUEUE_CAPACITY);
        (
            Self {
                state: Mutex::default(),
                active: Mutex::default(),
                shutdown: AtomicBool::new(false),
                actions,
            },
            receiver,
        )
    }
}
