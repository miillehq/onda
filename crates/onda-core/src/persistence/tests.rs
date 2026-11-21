use super::*;
fn entry(id: &str, phrase: &str) -> DictionaryEntry {
    DictionaryEntry {
        id: id.into(),
        kind: DictionaryKind::Vocabulary,
        phrase: phrase.into(),
        replacement: None,
        enabled: true,
        starred: false,
        source: EntrySource::Manual,
    }
}
#[test]
fn persistence_restart_and_idempotent_migrations() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("app.db");
    {
        let mut store = Store::open(&path).unwrap();
        store
            .save_dictionary_entry(&entry("one", "OpenAI"))
            .unwrap();
        store.save_settings(&Settings::default()).unwrap();
    }
    let store = Store::open(&path).unwrap();
    assert_eq!(store.dictionary().unwrap()[0].phrase, "OpenAI");
    assert_eq!(store.transforms().unwrap().len(), 9);
}
#[test]
fn normalized_duplicates_fail_and_invalid_import_is_atomic() {
    let mut store = Store::in_memory().unwrap();
    store.save_dictionary_entry(&entry("one", "Café")).unwrap();
    assert!(store
        .save_dictionary_entry(&entry("two", "CAFE\u{301}"))
        .is_err());
    assert!(store
        .import_dictionary("{\"version\":1,\"entries\":[{}]}")
        .is_err());
    assert_eq!(store.dictionary().unwrap().len(), 1);
    let archive = store.export_dictionary().unwrap();
    assert_eq!(store.import_dictionary(&archive).unwrap(), 0);
    // Well-formed JSON whose first entry is new and valid and whose second fails validation:
    // the import is refused and the valid entry is not written either.
    let mixed = serde_json::to_string(&DictionaryArchive {
        version: 1,
        entries: vec![entry("new", "Kubernetes"), entry("bad", "!!!")],
    })
    .unwrap();
    let error = store.import_dictionary(&mixed).unwrap_err();
    assert!(
        error
            .message
            .contains("Dictionary phrase must contain a word"),
        "{}",
        error.message
    );
    let phrases: Vec<String> = store
        .dictionary()
        .unwrap()
        .into_iter()
        .map(|saved| saved.phrase)
        .collect();
    assert_eq!(phrases, ["Café"], "a refused import wrote some entries");
}
#[test]
fn daily_usage_aggregates_counts_per_day_without_content() {
    let mut store = Store::in_memory().unwrap();
    store
        .record_usage("2026-10-22", 12, 4000, 300, false)
        .unwrap();
    store
        .record_usage("2026-10-23", 5, 2000, 100, false)
        .unwrap();
    store
        .record_usage("2026-10-23", 7, 3000, 100, true)
        .unwrap();
    assert!(store.record_usage("23/10/2026", 1, 1, 1, false).is_err());
    let days = store.daily_usage(7).unwrap();
    assert_eq!(days.len(), 2);
    assert_eq!(days[0].day, "2026-10-23");
    assert_eq!(
        (days[0].dictations, days[0].words, days[0].audio_ms),
        (2, 12, 5000)
    );
    assert_eq!(days[1].words, 12);
}

#[test]
fn history_rows_upsert_prune_and_delete() {
    let mut store = Store::in_memory().unwrap();
    store.save_history("a", 1_000, b"sealed-a").unwrap();
    store.save_history("b", 2_000, b"sealed-b").unwrap();
    store.save_history("a", 1_000, b"sealed-a2").unwrap();
    let rows = store.history_since(0).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "b");
    assert_eq!(rows[1].2, b"sealed-a2");
    assert_eq!(store.history_since(1_500).unwrap().len(), 1);
    store.prune_history(1_500).unwrap();
    assert_eq!(store.history_since(0).unwrap().len(), 1);
    store.delete_history("b").unwrap();
    assert!(store.history_since(0).unwrap().is_empty());
    assert!(store.save_history("", 1, b"x").is_err());
}

#[test]
fn the_history_cap_keeps_the_newest_dictations() {
    let mut store = Store::in_memory().unwrap();
    let newest = MAX_HISTORY_ROWS;
    for created_at in 0..=newest {
        store
            .save_history(&format!("h{created_at:04}"), created_at, b"sealed")
            .unwrap();
    }
    let stored: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(stored, MAX_HISTORY_ROWS);
    let rows = store.history_since(0).unwrap();
    assert_eq!(
        rows.first().map(|row| row.0.clone()),
        Some(format!("h{newest:04}")),
        "the dictation just saved was dropped"
    );
    assert_eq!(rows.last().map(|row| row.0.as_str()), Some("h0001"));
    let oldest_kept: bool = store
        .connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM history WHERE id='h0000')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!oldest_kept, "the oldest dictation outlived the cap");
}

#[test]
fn version_one_database_gains_daily_usage() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("v1.db");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(include_str!("../../migrations/001_initial.sql"))
        .unwrap();
    drop(connection);
    let mut store = Store::open(&path).unwrap();
    store
        .record_usage("2026-01-02", 3, 1000, 10, false)
        .unwrap();
    assert_eq!(store.daily_usage(1).unwrap()[0].words, 3);
}

#[test]
fn version_three_database_backfills_lifetime_totals_from_daily_rows() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("v3.db");
    let connection = Connection::open(&path).unwrap();
    for (_, migration) in &MIGRATIONS[..3] {
        connection.execute_batch(migration).unwrap();
    }
    connection
        .execute_batch(
            "INSERT INTO daily_usage VALUES('2026-10-19',1,5,1000),('2026-10-23',2,30,9000),('2026-10-24',1,12,4000),('2026-10-26',0,0,0);",
        )
        .unwrap();
    drop(connection);
    let mut store = Store::open(&path).unwrap();
    let lifetime = store.lifetime_usage().unwrap();
    assert_eq!(lifetime.since.as_deref(), Some("2026-10-19"));
    assert_eq!((lifetime.dictations, lifetime.words), (4, 47));
    assert_eq!(lifetime.audio_ms, 14_000.0);
    assert_eq!(lifetime.active_days, 3);
    assert_eq!(lifetime.last_day.as_deref(), Some("2026-10-24"));
    assert_eq!((lifetime.current_run, lifetime.longest_run), (2, 2));
    store
        .record_usage("2026-10-25", 8, 1000, 10, false)
        .unwrap();
    store
        .record_usage("2026-10-25", 2, 1000, 10, false)
        .unwrap();
    let lifetime = store.lifetime_usage().unwrap();
    assert_eq!((lifetime.dictations, lifetime.words), (6, 57));
    assert_eq!(lifetime.since.as_deref(), Some("2026-10-19"));
    assert_eq!(lifetime.active_days, 4);
    assert_eq!((lifetime.current_run, lifetime.longest_run), (3, 3));
}

#[test]
fn version_four_database_backfills_days_and_streaks() {
    let connection = Connection::open_in_memory().unwrap();
    for (_, migration) in &MIGRATIONS[..4] {
        connection.execute_batch(migration).unwrap();
    }
    connection
        .execute_batch(
            "INSERT INTO daily_usage VALUES('2026-10-01',1,1,1),('2026-10-02',1,1,1),('2026-10-03',1,1,1),('2026-10-10',1,1,1);",
        )
        .unwrap();
    let store = Store::initialize(connection).unwrap();
    let lifetime = store.lifetime_usage().unwrap();
    assert_eq!(lifetime.active_days, 4);
    assert_eq!(lifetime.last_day.as_deref(), Some("2026-10-10"));
    assert_eq!((lifetime.current_run, lifetime.longest_run), (1, 3));
}

#[test]
fn lifetime_totals_start_on_the_first_dictation_and_outlive_pruned_days() {
    let mut store = Store::in_memory().unwrap();
    assert_eq!(store.lifetime_usage().unwrap(), LifetimeUsage::default());
    // Consecutive real calendar days from 2025-01-01, oldest first.
    let days: Vec<String> = (2025..=2026)
        .flat_map(|year| {
            (1..=12).flat_map(move |month| {
                (1..=31).map(move |date| format!("{year}-{month:02}-{date:02}"))
            })
        })
        .filter(|day| crate::stats::day_number(day).is_some())
        .take(MAX_USAGE_DAYS as usize + 5)
        .collect();
    assert_eq!(days.len(), MAX_USAGE_DAYS as usize + 5);
    for day in &days {
        store.record_usage(day, 2, 1000, 1, false).unwrap();
    }
    assert_eq!(
        store.daily_usage(1000).unwrap().len(),
        MAX_USAGE_DAYS as usize
    );
    let lifetime = store.lifetime_usage().unwrap();
    assert_eq!(lifetime.since.as_deref(), Some("2025-01-01"));
    assert_eq!(lifetime.words, 2 * (MAX_USAGE_DAYS as u32 + 5));
    // The prune drops the five oldest days and keeps every newer one, newest first.
    let kept: Vec<String> = store
        .daily_usage(1000)
        .unwrap()
        .into_iter()
        .map(|usage| usage.day)
        .collect();
    assert_eq!(kept.first(), days.last(), "the day just written was pruned");
    let expected: Vec<String> = days[5..].iter().rev().cloned().collect();
    assert_eq!(kept, expected);
}

#[test]
fn profile_name_and_picture_are_stored_independently() {
    let mut store = Store::in_memory().unwrap();
    assert_eq!(store.profile().unwrap(), (None, None));
    store.set_profile_name(Some("Daniel")).unwrap();
    store.set_profile_picture(Some(&[1, 2, 3])).unwrap();
    store.set_profile_name(None).unwrap();
    assert_eq!(store.profile().unwrap(), (None, Some(vec![1, 2, 3])));
    store.set_profile_picture(None).unwrap();
    assert_eq!(store.profile().unwrap(), (None, None));
    let oversized = vec![0; crate::profile::MAX_STORED_PICTURE_BYTES + 1];
    assert!(store.set_profile_picture(Some(&oversized)).is_err());
}

#[test]
fn newer_schema_is_not_destroyed() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("PRAGMA user_version=99;").unwrap();
    assert!(Store::initialize(connection).is_err());

    // A newer database that has every table this version knows plus one of its own: it is
    // refused by name, and nothing in it is migrated, seeded or renumbered.
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("v99.db");
    let connection = Connection::open(&path).unwrap();
    for (_, migration) in MIGRATIONS {
        connection.execute_batch(migration).unwrap();
    }
    connection
        .execute_batch(
            "CREATE TABLE future(value TEXT NOT NULL); INSERT INTO future VALUES('kept'); PRAGMA user_version=99;",
        )
        .unwrap();
    drop(connection);
    let refused = Store::open(&path).err();
    let connection = Connection::open(&path).unwrap();
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    let transforms: i64 = connection
        .query_row("SELECT COUNT(*) FROM transforms", [], |row| row.get(0))
        .unwrap();
    let future: String = connection
        .query_row("SELECT value FROM future", [], |row| row.get(0))
        .unwrap();
    drop(connection);
    assert_eq!(version, 99, "the newer schema version was rewritten");
    assert_eq!(
        transforms, 0,
        "an older version wrote into a newer database"
    );
    assert_eq!(future, "kept");
    let error = refused.expect("a newer database must be refused");
    assert!(
        error.message.contains("newer application version"),
        "refused without naming the newer version: {}",
        error.message
    );
}
