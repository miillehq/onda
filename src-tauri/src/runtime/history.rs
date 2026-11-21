//! Retained dictations and edits. SQLite holds sealed bytes and memory holds the current revision.
use super::*;

const MAX_HISTORY_ENTRIES: usize = 2_000;
const DAY_MS: i64 = 86_400_000;
const IDLE_EXPIRY_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum HistoryStage {
    Result,
    CompletedPreview,
}

/// Usage credits live exactly as long as the in-memory entry and are shared with paste retries.
#[derive(Clone)]
pub(super) struct RetainedEntry {
    pub(super) value: Arc<HistoryEntry>,
    pub(super) usage: Option<DictationReceipt>,
}

pub(super) struct History {
    pub(super) days: u32,
    entries: VecDeque<RetainedEntry>,
    unreadable: HashMap<String, i64>,
    notify_on_read: bool,
    storage_unavailable: bool,
}

impl History {
    /// Reading n retained rows takes O(n + payload bytes) time and space.
    pub(super) fn load(
        store: &mut Store,
        days: u32,
        now: i64,
        unseal: impl Fn(&[u8]) -> Option<Vec<u8>>,
    ) -> Self {
        let mut history = Self {
            days,
            entries: VecDeque::new(),
            unreadable: HashMap::new(),
            notify_on_read: false,
            storage_unavailable: false,
        };
        let cutoff = history.cutoff(now);
        let records = if days == 0 {
            store.clear_history().map(|()| Vec::new())
        } else {
            store
                .prune_history(cutoff)
                .and_then(|()| store.history_since(cutoff))
        };
        let Ok(records) = records else {
            tracing::warn!(
                subsystem = "history",
                "saved history unavailable; using session history"
            );
            history.storage_unavailable = true;
            history.notify_on_read = true;
            return history;
        };
        for (id, timestamp, bytes) in records {
            let entry = unseal(&bytes)
                .and_then(|plain| serde_json::from_slice::<HistoryEntry>(&plain).ok())
                .filter(|entry| entry.id == id && entry.created_at == timestamp as f64);
            if let Some(entry) = entry {
                history.entries.push_back(RetainedEntry {
                    value: Arc::new(entry),
                    usage: None,
                });
            } else {
                // A locked keyring and a damaged entry both fail to open. Only an explicit
                // deletion or the retention policy may discard these encrypted bytes.
                history.unreadable.insert(id, timestamp);
            }
        }
        if !history.unreadable.is_empty() {
            history.notify_on_read = true;
            tracing::warn!(
                subsystem = "history",
                unreadable = history.unreadable.len(),
                "history entries could not be opened"
            );
        }
        history
    }

    fn cutoff(&self, now: i64) -> i64 {
        now.saturating_sub(i64::from(self.days) * DAY_MS)
    }

    fn retain_recent(&mut self, now: i64) {
        if self.days != 0 {
            let cutoff = self.cutoff(now) as f64;
            self.entries
                .retain(|entry| entry.value.created_at >= cutoff);
            self.unreadable
                .retain(|_, timestamp| *timestamp as f64 >= cutoff);
        }
        self.entries.truncate(MAX_HISTORY_ENTRIES);
    }

    fn record(&self, id: &str) -> Option<&RetainedEntry> {
        self.entries.iter().find(|entry| entry.value.id == id)
    }

    fn entry(&self, id: &str) -> Option<&Arc<HistoryEntry>> {
        self.record(id).map(|entry| &entry.value)
    }

    fn put(
        &mut self,
        entry: HistoryEntry,
        usage: Option<DictationReceipt>,
        now: i64,
    ) -> Arc<HistoryEntry> {
        let entry = Arc::new(entry);
        self.entries.retain(|prior| prior.value.id != entry.id);
        self.entries.push_front(RetainedEntry {
            value: Arc::clone(&entry),
            usage,
        });
        self.retain_recent(now);
        entry
    }

    fn expire(&mut self, store: &mut Store, now: i64) -> AppResult<bool> {
        let cutoff = self.cutoff(now);
        if self.days == 0
            || (self
                .entries
                .iter()
                .all(|entry| entry.value.created_at >= cutoff as f64)
                && self
                    .unreadable
                    .values()
                    .all(|timestamp| *timestamp >= cutoff))
        {
            return Ok(false);
        }
        if !self.storage_unavailable {
            store.prune_history(cutoff)?;
        }
        self.retain_recent(now);
        Ok(true)
    }

    /// Sealing can outlive a delete, edit or retention change. A revision is eligible to
    /// commit only while that exact allocation is still current under the history lock.
    fn save_if_current(
        &self,
        store: &mut Store,
        entry: &Arc<HistoryEntry>,
        sealed: &[u8],
        now: i64,
    ) -> AppResult<()> {
        if self.storage_unavailable
            || self.days == 0
            || entry.created_at < self.cutoff(now) as f64
            || !self
                .entry(&entry.id)
                .is_some_and(|current| Arc::ptr_eq(current, entry))
        {
            return Ok(());
        }
        store.save_history(&entry.id, entry.created_at as i64, sealed)?;
        store.prune_history(self.cutoff(now))
    }

    fn delete(&mut self, store: &mut Store, id: Option<&str>) -> AppResult<()> {
        if let Some(id) = id {
            store.delete_history(id)?;
            self.entries.retain(|entry| entry.value.id != id);
            self.unreadable.remove(id);
        } else {
            store.clear_history()?;
            self.entries.clear();
            self.unreadable.clear();
            self.storage_unavailable = false;
            self.notify_on_read = false;
        }
        Ok(())
    }
}

impl Runtime {
    /// Keep a visible History page and the disk store current while the app sits idle.
    pub(super) fn start_history_expiry(service: &Arc<Self>) {
        let service = Arc::downgrade(service);
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(IDLE_EXPIRY_INTERVAL).await;
                let Some(service) = service.upgrade() else {
                    return;
                };
                if service.operations.shutdown.load(Ordering::Acquire) {
                    return;
                }
                let expired = lock(&service.retention.history).and_then(|mut history| {
                    history.expire(&mut *lock(&service.data.store)?, now_ms())
                });
                match expired {
                    Ok(true) => service.event(RuntimeEvent::HistoryReset),
                    Err(_) => {
                        tracing::warn!(subsystem = "history", "history expiry could not be applied")
                    }
                    Ok(false) => {}
                }
            }
        });
    }
}

/// An unchanged selected-text draft is provisional. A completed preview has no selection
/// and is a result the user can revisit even when the provider kept their wording.
fn recordable(
    keep: bool,
    mode: &OperationMode,
    stage: HistoryStage,
    result: &OperationResult,
) -> bool {
    keep && !result.output.trim().is_empty()
        && (*mode != OperationMode::Transform
            || stage == HistoryStage::CompletedPreview
            || result.output != result.original
            || result.suggestion.is_some())
}

#[cfg(test)]
mod tests;

impl HistoryService {
    /// Keep a completed result findable after the next operation replaces the visible one.
    pub(super) fn record_history(
        &self,
        host: &Runtime,
        active: &ActiveOperation,
        result: &OperationResult,
        route: DataRoute,
        stage: HistoryStage,
    ) {
        if !recordable(active.history, &active.mode, stage, result) {
            return;
        }
        let (entry, persist) = {
            let Ok(mut history) = lock(&self.history) else {
                return;
            };
            let now = now_ms();
            let created_at = history
                .entry(&active.id)
                .map_or(now as f64, |entry| entry.created_at);
            let entry = history.put(
                HistoryEntry {
                    id: active.id.clone(),
                    created_at,
                    mode: active.mode.clone(),
                    route,
                    original: result.original.clone(),
                    output: result.output.clone(),
                    command_text: result.command_text.clone(),
                    inserted: result.inserted,
                    fallback_used: result.fallback_used,
                    error_code: None,
                    app: active
                        .target
                        .as_ref()
                        .map(|target| target.executable_name.clone())
                        .filter(|name| !name.is_empty()),
                    suggestion: result.suggestion.clone(),
                },
                active.usage.clone(),
                now,
            );
            host.event(RuntimeEvent::HistoryUpdated {
                entry: entry.as_ref().clone(),
            });
            (entry, history.days != 0 && !history.storage_unavailable)
        };
        if persist {
            self.persist_history(host, &entry);
        }
    }

    pub(super) fn update_history(
        &self,
        host: &Runtime,
        id: &str,
        change: impl FnOnce(&mut HistoryEntry),
    ) {
        let (entry, persist) = {
            let Ok(mut history) = lock(&self.history) else {
                return;
            };
            let Some(entry) = history
                .entries
                .iter_mut()
                .find(|entry| entry.value.id == id)
                .map(|entry| &mut entry.value)
            else {
                return;
            };
            change(Arc::make_mut(entry));
            host.event(RuntimeEvent::HistoryUpdated {
                entry: entry.as_ref().clone(),
            });
            (
                Arc::clone(entry),
                history.days != 0 && !history.storage_unavailable,
            )
        };
        if persist {
            self.persist_history(host, &entry);
        }
    }

    pub(super) fn mark_history_failed(&self, host: &Runtime, id: &str, code: ErrorCode) {
        self.update_history(host, id, |entry| {
            if !entry.inserted {
                entry.error_code = Some(code);
            }
        });
    }

    pub(super) fn persist_history(&self, host: &Runtime, entry: &Arc<HistoryEntry>) {
        let sealed = serde_json::to_vec(entry.as_ref())
            .ok()
            .and_then(|plain| onda_platform::protect_user_data(&plain).ok());
        let saved = sealed.is_some_and(|bytes| {
            lock(&self.history).is_ok_and(|history| {
                lock(&host.data.store).is_ok_and(|mut store| {
                    history
                        .save_if_current(&mut store, entry, &bytes, now_ms())
                        .is_ok()
                })
            })
        });
        if !saved {
            tracing::warn!(subsystem = "history", "history entry could not be saved");
        }
    }

    /// Hold history before the store wherever both are needed. Sealing happens outside both.
    pub(super) fn save_preferences(&self, host: &Runtime, settings: &Settings) -> AppResult<()> {
        let to_persist = {
            let mut history = lock(&self.history)?;
            let previous = history.days;
            lock(&host.data.store)?.save_settings(settings)?;
            if previous == settings.history_days {
                return Ok(());
            }
            history.days = settings.history_days;
            if history.days == 0 {
                history.unreadable.clear();
                history.storage_unavailable = false;
                history.notify_on_read = false;
            }
            history.retain_recent(now_ms());
            if previous == 0 {
                history.entries.clone()
            } else {
                VecDeque::new()
            }
        };
        for entry in to_persist.iter().rev() {
            self.persist_history(host, &entry.value);
        }
        host.event(RuntimeEvent::HistoryReset);
        Ok(())
    }

    pub(super) fn visible_history(&self, host: &Runtime) -> AppResult<MutexGuard<'_, History>> {
        let mut history = lock(&self.history)?;
        history.expire(&mut *lock(&host.data.store)?, now_ms())?;
        Ok(history)
    }

    pub(super) fn history_entries(&self, host: &Runtime) -> AppResult<Vec<HistoryEntry>> {
        let mut history = self.visible_history(host)?;
        if std::mem::take(&mut history.notify_on_read) {
            let message = if history.storage_unavailable {
                "Saved history could not be opened. New entries are kept for this session. Reopen Onda to try again."
            } else {
                "Some saved history could not be opened. Its encrypted records are retained. If your system key store was locked, unlock it and reopen Onda."
            };
            host.domain(AppEvent::Error(AppError::new(
                ErrorCode::Storage,
                message,
                true,
            )));
        }
        Ok(history
            .entries
            .iter()
            .map(|entry| entry.value.as_ref().clone())
            .collect())
    }

    pub(super) fn history_entry(&self, host: &Runtime, id: &str) -> AppResult<RetainedEntry> {
        self.visible_history(host)?
            .record(id)
            .cloned()
            .ok_or_else(|| AppError::invalid("That history entry is no longer kept."))
    }

    pub(super) fn delete_retained_history(
        &self,
        host: &Runtime,
        id: Option<&str>,
    ) -> AppResult<()> {
        lock(&self.history)?.delete(&mut *lock(&host.data.store)?, id)
    }
}
