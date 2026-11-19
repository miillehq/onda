//! Content-free accounting shared by an original dictation and its delivery retries.
use super::*;

struct Counts {
    words: u64,
    audio_ms: u64,
    inference_ms: u64,
}

#[derive(Clone)]
pub(super) struct DictationReceipt(Arc<Mutex<Option<Counts>>>);

impl DictationReceipt {
    pub(super) fn for_transcript(
        mode: &OperationMode,
        keep: bool,
        local: &str,
        audio_ms: u64,
        inference_ms: u64,
    ) -> Option<Self> {
        (keep && *mode == OperationMode::Dictation).then(|| {
            Self(Arc::new(Mutex::new(Some(Counts {
                words: onda_core::text::word_count(local),
                audio_ms,
                inference_ms,
            }))))
        })
    }

    fn credit(&self, write: impl FnOnce(&Counts) -> AppResult<()>) -> AppResult<()> {
        let mut pending = lock(&self.0)?;
        if let Some(counts) = pending.as_ref() {
            write(counts)?;
            pending.take();
        }
        Ok(())
    }
}

impl Runtime {
    pub(super) fn credit_delivery(&self, receipt: Option<&DictationReceipt>, fallback: bool) {
        let Some(receipt) = receipt else { return };
        let result = receipt.credit(|counts| {
            lock(&self.store)?.record_usage(
                &onda_platform::local_date(),
                counts.words,
                counts.audio_ms,
                counts.inference_ms,
                fallback,
            )
        });
        if result.is_err() {
            tracing::warn!(subsystem = "metrics", "delivered usage could not be saved");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_delivered_dictations_count_and_all_recovery_paths_share_one_credit() {
        for mode in [OperationMode::Transform, OperationMode::SpokenCommand] {
            assert!(
                DictationReceipt::for_transcript(&mode, true, "test words", 1000, 10).is_none()
            );
        }
        assert!(DictationReceipt::for_transcript(
            &OperationMode::Dictation,
            false,
            "test words",
            1000,
            10
        )
        .is_none());
        let mut store = Store::in_memory().unwrap();
        let receipt = DictationReceipt::for_transcript(
            &OperationMode::Dictation,
            true,
            "test words",
            1000,
            10,
        )
        .unwrap();
        let retry = receipt.clone();
        assert_eq!(store.lifetime_usage().unwrap().dictations, 0);
        assert!(receipt.credit(|_| Err(AppError::storage())).is_err());
        retry
            .credit(|counts| {
                store.record_usage(
                    "2026-11-19",
                    counts.words,
                    counts.audio_ms,
                    counts.inference_ms,
                    true,
                )
            })
            .unwrap();
        receipt
            .credit(|_| panic!("a second delivery was counted"))
            .unwrap();
        let usage = store.lifetime_usage().unwrap();
        assert_eq!(
            (usage.dictations, usage.words, usage.audio_ms),
            (1, 2, 1000.0)
        );
    }
}
