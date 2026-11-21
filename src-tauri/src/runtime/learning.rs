//! Opt-in correction learning stops at the same deadline even when native reads queue.
use super::*;

const MAX_OBSERVATIONS: u32 = 3;
const OBSERVATION_WINDOW: Duration = Duration::from_secs(12);

#[derive(Clone, Copy)]
struct LearningWindow(Instant);

impl LearningWindow {
    fn deadline(self) -> Instant {
        self.0 + OBSERVATION_WINDOW
    }

    fn slot(self, index: u32) -> Instant {
        self.0 + (OBSERVATION_WINDOW / (MAX_OBSERVATIONS + 1)) * (index + 1)
    }

    fn allows(self, now: Instant) -> bool {
        now < self.deadline()
    }
}

impl OperationResults {
    pub(super) fn start_learning(
        &self,
        host: &Arc<Runtime>,
        active: &ActiveOperation,
        baseline: String,
        inserted: String,
    ) {
        let service = host.clone();
        let active = active.clone();
        let window = LearningWindow(Instant::now());
        tauri::async_runtime::spawn(async move {
            for slot in 0..MAX_OBSERVATIONS {
                tokio::time::sleep_until(window.slot(slot).into()).await;
                if !window.allows(Instant::now()) {
                    return;
                }
                if active.cancellation.is_cancelled()
                    || lock(&service.operations.state)
                        .map(|state| state.snapshot().operation_id.as_deref() != Some(&active.id))
                        .unwrap_or(true)
                {
                    return;
                }
                let Some(target) = active.target.clone() else {
                    return;
                };
                let cancellation = active.cancellation.clone();
                let reading = tokio::task::spawn_blocking(move || {
                    if cancellation.is_cancelled() || !window.allows(Instant::now()) {
                        return Err(onda_platform::PlatformError::Cancelled);
                    }
                    WindowsTargetService::new().field_text(&target, LEARNING_CHARACTERS)
                });
                let observed = tokio::time::timeout_at(window.deadline().into(), reading).await;
                let Ok(Ok(Ok(observed))) = observed else {
                    return;
                };
                let Some(scope) = observed_inserted_scope(&baseline, &observed, &inserted) else {
                    return;
                };
                if let Some(proposal) = onda_core::learning::propose_correction(&inserted, scope) {
                    let Ok(state) = lock(&service.operations.state) else {
                        return;
                    };
                    if !window.allows(Instant::now())
                        || active.cancellation.is_cancelled()
                        || state.snapshot().operation_id.as_deref() != Some(&active.id)
                    {
                        return;
                    }
                    if let Ok(mut saved) = lock(&service.results.proposal) {
                        *saved = Some(proposal.clone());
                    }
                    service.domain(AppEvent::LearningProposed(proposal));
                    drop(state);
                    return;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delayed_reads_cannot_extend_the_learning_window() {
        let start = Instant::now();
        let window = LearningWindow(start);
        assert_eq!(MAX_OBSERVATIONS, 3);
        for i in 0..MAX_OBSERVATIONS {
            assert!(window.allows(window.slot(i)));
        }
        assert!(!window.allows(start + Duration::from_secs(12)));
        assert!(!window.allows(start + Duration::from_secs(16)));
    }
}
