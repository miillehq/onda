use super::*;

#[test]
fn only_a_successful_unchanged_preview_is_kept() {
    let result = OperationResult {
        operation_id: "preview".into(),
        original: "Test".into(),
        output: "Test".into(),
        command_text: None,
        inserted: false,
        fallback_used: false,
        suggestion: None,
        diff: vec![],
    };
    assert!(recordable(
        true,
        &OperationMode::Transform,
        HistoryStage::CompletedPreview,
        &result
    ));
    assert!(!recordable(
        true,
        &OperationMode::Transform,
        HistoryStage::Result,
        &result
    ));
    assert!(!recordable(
        false,
        &OperationMode::Transform,
        HistoryStage::CompletedPreview,
        &result
    ));
}

fn fixture(now: i64) -> HistoryEntry {
    HistoryEntry {
        id: "synthetic".into(),
        created_at: now as f64,
        mode: OperationMode::Dictation,
        route: DataRoute::Local,
        original: "Test entry".into(),
        output: "Test entry".into(),
        command_text: None,
        inserted: false,
        fallback_used: false,
        error_code: None,
        app: None,
        suggestion: None,
    }
}

#[test]
fn unavailable_protection_preserves_unexpired_history_bytes() {
    let mut store = Store::in_memory().unwrap();
    let now = now_ms();
    let entry = fixture(now);
    let sealed = b"synthetic encrypted record";
    store.save_history(&entry.id, now, sealed).unwrap();
    assert!(History::load(&mut store, 3, now, |_| None)
        .entries
        .is_empty());
    assert_eq!(
        store.history_since(0).unwrap(),
        vec![(entry.id.clone(), now, sealed.to_vec())],
        "a temporary unlock failure removed stored history"
    );
    let opened = History::load(&mut store, 3, now, |_| serde_json::to_vec(&entry).ok());
    assert_eq!(opened.entry(&entry.id).unwrap().output, entry.output);
}

#[test]
fn late_writes_cannot_restore_deleted_replaced_or_session_only_history() {
    let mut store = Store::in_memory().unwrap();
    let now = now_ms();
    let mut history = History::load(&mut store, 3, now, |_| None);
    let old = history.put(fixture(now), now);
    let current = history.put(fixture(now), now);
    history
        .save_if_current(&mut store, &current, b"current", now)
        .unwrap();
    history
        .save_if_current(&mut store, &old, b"stale", now)
        .unwrap();
    assert_eq!(store.history_since(0).unwrap()[0].2, b"current");
    history.delete(&mut store, Some(&current.id)).unwrap();
    history
        .save_if_current(&mut store, &current, b"deleted", now)
        .unwrap();
    assert!(store.history_since(0).unwrap().is_empty());
    let current = history.put(fixture(now), now);
    history.delete(&mut store, None).unwrap();
    history
        .save_if_current(&mut store, &current, b"cleared", now)
        .unwrap();
    assert!(store.history_since(0).unwrap().is_empty());
    let current = history.put(fixture(now), now);
    history.days = 0;
    history
        .save_if_current(&mut store, &current, b"session only", now)
        .unwrap();
    assert!(store.history_since(0).unwrap().is_empty());
}

#[test]
fn reading_history_after_an_idle_period_expires_memory_and_disk_together() {
    let mut store = Store::in_memory().unwrap();
    let now = now_ms();
    let mut history = History::load(&mut store, 1, now, |_| None);
    let current = history.put(fixture(now), now);
    history
        .save_if_current(&mut store, &current, b"retained", now)
        .unwrap();
    history.expire(&mut store, now + DAY_MS).unwrap();
    assert!(history.entry(&current.id).is_some());
    history.expire(&mut store, now + DAY_MS + 1).unwrap();
    assert!(history.entry(&current.id).is_none());
    assert!(store.history_since(0).unwrap().is_empty());
}

#[test]
fn a_history_read_failure_leaves_dictation_available_and_durable_rows_intact() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("profile.db");
    let mut store = Store::open(&path).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let now = now_ms();
    db.execute(
        "INSERT INTO history (id,created_at,payload) VALUES ('legacy',?1,'invalid storage type')",
        [now],
    )
    .unwrap();
    let mut history = History::load(&mut store, 3, now, |_| None);
    let entry = history.put(fixture(now), now);
    assert_eq!(history.entry(&entry.id).unwrap().output, "Test entry");
    history
        .save_if_current(&mut store, &entry, b"sealed", now)
        .unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM history", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(history.notify_on_read);
}
