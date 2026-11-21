//! Owns SQLite, the live dictionary index and their serialized configuration changes.
use super::*;

pub(super) struct LocalData {
    pub(super) store: Mutex<Store>,
    pub(super) index: Mutex<DictionaryIndex>,
    pub(super) configuration_gate: tokio::sync::Mutex<()>,
    pub(super) dictionary_gate: tokio::sync::Mutex<()>,
}

impl LocalData {
    pub(super) fn settings(&self) -> AppResult<Settings> {
        lock(&self.store)?.settings()
    }
}

impl LocalData {
    pub(super) fn new(store: Store, index: DictionaryIndex) -> Self {
        Self {
            store: Mutex::new(store),
            index: Mutex::new(index),
            configuration_gate: tokio::sync::Mutex::new(()),
            dictionary_gate: tokio::sync::Mutex::new(()),
        }
    }
}
