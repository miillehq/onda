use onda_core::{contracts::Settings, persistence::Store};
use rusqlite::Connection;
use std::fs;

#[test]
fn opening_a_newer_profile_preserves_its_bytes_and_journal_mode() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("future.db");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "PRAGMA journal_mode=DELETE; CREATE TABLE future_data(value); PRAGMA user_version=999;",
        )
        .unwrap();
    drop(connection);
    let original = fs::read(&path).unwrap();
    assert!(Store::open(&path).is_err());
    assert!(
        fs::read(&path).unwrap() == original,
        "opening an unsupported profile changed its bytes"
    );
    assert!(!directory.path().join("future.db-wal").exists());
}

#[test]
fn refusing_a_newer_wal_profile_preserves_uncheckpointed_pages() {
    use rusqlite::config::DbConfig;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("future.db");
    let wal = directory.path().join("future.db-wal");
    let writer = Connection::open(&path).unwrap();
    writer
        .set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
        .unwrap();
    writer
        .execute_batch(
            "PRAGMA journal_mode=WAL; CREATE TABLE future(value); PRAGMA user_version=999;",
        )
        .unwrap();
    drop(writer);
    let original = fs::read(&path).unwrap();
    let pending = fs::read(&wal).unwrap();
    assert!(Store::open(&path).is_err());
    assert!(
        fs::read(&path).unwrap() == original,
        "refusal checkpointed a newer database"
    );
    assert!(fs::read(wal).unwrap() == pending);
}

#[test]
fn a_failed_history_clear_rolls_back_the_retention_preference() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("profile.db");
    let mut store = Store::open(&path).unwrap();
    let previous = store.settings().unwrap();
    let session_only = Settings {
        history_days: 0,
        ..previous.clone()
    };
    store
        .save_history("existing", i64::MAX, b"sealed test record")
        .unwrap();
    let database = Connection::open(path).unwrap();
    database.execute_batch("CREATE TRIGGER simulate_full_disk BEFORE DELETE ON history BEGIN SELECT RAISE(ABORT, 'simulated write failure'); END;").unwrap();
    assert!(store.save_settings(&session_only).is_err());
    assert_eq!(store.settings().unwrap(), previous);
    assert_eq!(store.history_since(0).unwrap().len(), 1);
    database
        .execute_batch("DROP TRIGGER simulate_full_disk")
        .unwrap();
    store.save_settings(&session_only).unwrap();
    assert_eq!(store.settings().unwrap().history_days, 0);
    assert!(store.history_since(0).unwrap().is_empty());
}
