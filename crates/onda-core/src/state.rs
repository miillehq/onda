use crate::{
    contracts::{DataRoute, OperationMode, OperationState, Phase},
    error::{AppError, AppResult, ErrorCode},
};

/// The operation generation rejects late callbacks from a cancelled/replaced job.
#[derive(Debug)]
pub struct StateMachine {
    current: OperationState,
}

impl Default for StateMachine {
    fn default() -> Self {
        Self {
            current: OperationState {
                operation_id: None,
                phase: Phase::Idle,
                mode: OperationMode::Dictation,
                route: DataRoute::Local,
                error: None,
                fallback_used: false,
                hands_free: false,
            },
        }
    }
}

impl StateMachine {
    /// A new model choice resolves configuration failure, without dismissing other errors.
    pub fn clear_no_model_error(&mut self) {
        if self.current.phase == Phase::Error
            && self
                .current
                .error
                .as_ref()
                .is_some_and(|error| error.code == ErrorCode::NoModel)
        {
            *self = Self::default();
        }
    }
    /// The user acknowledged this failure; a newer operation's state is left alone. The
    /// operation keeps its id and mode, so its result still reads as a dictation or a rewrite.
    pub fn dismiss_error(&mut self, operation_id: &str) -> bool {
        let dismissed = self.current.phase == Phase::Error
            && self.current.operation_id.as_deref() == Some(operation_id);
        if dismissed {
            self.current.phase = Phase::Idle;
            self.current.error = None;
        }
        dismissed
    }
    pub fn snapshot(&self) -> OperationState {
        self.current.clone()
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.current.phase,
            Phase::Recording
                | Phase::FinalizingAudio
                | Phase::Transcribing
                | Phase::Enhancing
                | Phase::Inserting
        )
    }

    /// Begin only from a terminal state; processing is never overlapped.
    pub fn begin(&mut self, mode: OperationMode) -> AppResult<String> {
        if self.is_active() {
            return Err(AppError::new(
                ErrorCode::Busy,
                "An operation is already running.",
                true,
            ));
        }
        let operation_id = uuid::Uuid::new_v4().to_string();
        self.current = OperationState {
            operation_id: Some(operation_id.clone()),
            phase: if mode == OperationMode::Transform {
                Phase::Enhancing
            } else {
                Phase::Recording
            },
            mode,
            route: DataRoute::Local,
            error: None,
            fallback_used: false,
            hands_free: false,
        };
        Ok(operation_id)
    }

    pub fn matches(&self, operation_id: &str) -> bool {
        self.current.operation_id.as_deref() == Some(operation_id) && self.is_active()
    }

    pub fn transition(&mut self, operation_id: &str, next: Phase) -> AppResult<()> {
        if !self.matches(operation_id) {
            return Err(AppError::new(
                ErrorCode::Cancelled,
                "Operation no longer active.",
                false,
            ));
        }
        let allowed = matches!(
            (&self.current.phase, &next),
            (Phase::Recording, Phase::FinalizingAudio)
                | (Phase::FinalizingAudio, Phase::Transcribing)
                | (Phase::Transcribing, Phase::Enhancing)
                | (Phase::Transcribing, Phase::Inserting)
                | (Phase::Enhancing, Phase::Inserting)
                | (Phase::Enhancing, Phase::Completed)
                | (Phase::Inserting, Phase::Completed)
        ) || matches!(next, Phase::Error | Phase::Cancelled);
        if !allowed {
            return Err(AppError::new(
                ErrorCode::Internal,
                "Invalid operation state transition.",
                false,
            ));
        }
        self.current.phase = next;
        Ok(())
    }

    /// Record where the text is going; the interface names it while the model works.
    pub fn mark_cloud(&mut self, operation_id: &str, route: DataRoute) {
        if self.matches(operation_id) {
            self.current.route = route;
        }
    }
    pub fn mark_hands_free(&mut self, operation_id: &str) {
        if self.matches(operation_id) {
            self.current.hands_free = true;
        }
    }
    pub fn mark_fallback(&mut self, operation_id: &str) {
        if self.matches(operation_id) {
            self.current.fallback_used = true;
            self.current.route = DataRoute::Local;
        }
    }
    pub fn fail(&mut self, operation_id: &str, error: AppError) {
        if self.matches(operation_id) {
            self.current.phase = Phase::Error;
            self.current.error = Some(error);
        }
    }
    pub fn cancel(&mut self) {
        if self.is_active() {
            self.current.phase = Phase::Cancelled;
            self.current.route = DataRoute::Local;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_selection_clears_only_the_resolved_no_model_error() {
        let mut state = StateMachine::default();
        let id = state.begin(OperationMode::Dictation).unwrap();
        state.fail(
            &id,
            AppError::new(ErrorCode::NoModel, "missing model", true),
        );
        state.clear_no_model_error();
        assert_eq!(state.snapshot().phase, Phase::Idle);
        assert!(state.snapshot().error.is_none());
        let id = state.begin(OperationMode::Dictation).unwrap();
        state.fail(
            &id,
            AppError::new(ErrorCode::FocusChanged, "keep recovery result", true),
        );
        state.clear_no_model_error();
        assert_eq!(state.snapshot().phase, Phase::Error);
    }
    #[test]
    fn dismissing_clears_only_that_operations_error() {
        let mut state = StateMachine::default();
        let first = state.begin(OperationMode::Transform).unwrap();
        state.fail(
            &first,
            AppError::new(ErrorCode::InvalidEnhancement, "kept original", true),
        );
        assert!(!state.dismiss_error("another operation"));
        assert_eq!(state.snapshot().phase, Phase::Error);
        assert!(state.dismiss_error(&first));
        assert_eq!(state.snapshot().phase, Phase::Idle);
        let second = state.begin(OperationMode::Dictation).unwrap();
        assert!(!state.dismiss_error(&second));
        assert_eq!(state.snapshot().phase, Phase::Recording);
    }
    #[test]
    fn rejects_overlap_and_late_completion() {
        let mut state = StateMachine::default();
        let first = state.begin(OperationMode::Dictation).unwrap();
        assert_eq!(
            state.begin(OperationMode::Dictation).unwrap_err().code,
            ErrorCode::Busy
        );
        state.cancel();
        let second = state.begin(OperationMode::Dictation).unwrap();
        assert!(!state.matches(&first));
        assert!(state.matches(&second));
        assert_eq!(
            state.transition(&first, Phase::Completed).unwrap_err().code,
            ErrorCode::Cancelled
        );
    }
    #[test]
    fn requires_audio_finalization_and_processing_before_paste() {
        let mut state = StateMachine::default();
        let id = state.begin(OperationMode::Dictation).unwrap();
        assert!(state.transition(&id, Phase::Inserting).is_err());
        for phase in [
            Phase::FinalizingAudio,
            Phase::Transcribing,
            Phase::Enhancing,
            Phase::Inserting,
            Phase::Completed,
        ] {
            state.transition(&id, phase).unwrap();
        }
        assert!(!state.is_active());
    }
}
