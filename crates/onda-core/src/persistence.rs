//! Parameterized SQLite transactions; migrations never erase unknown/newer data.
use crate::{
    contracts::{
        DailyUsage, DictionaryEntry, DictionaryKind, EntrySource, Settings, Snippet, Transform,
    },
    dictionary::lookup_key,
    error::{AppError, AppResult},
    stats::LifetimeUsage,
    MAX_DICTIONARY_ENTRIES, MAX_TEXT_BYTES,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::{path::Path, time::Duration};

const SCHEMA_VERSION: i64 = 4;
/// Applied in order; each script ends by setting its own user_version.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../migrations/001_initial.sql")),
    (2, include_str!("../migrations/002_daily_usage.sql")),
    (3, include_str!("../migrations/003_history.sql")),
    (
        4,
        include_str!("../migrations/004_profile_and_lifetime.sql"),
    ),
];
/// Bounds the encrypted history table regardless of retention.
const MAX_HISTORY_ROWS: i64 = 2000;
const MAX_HISTORY_PAYLOAD_BYTES: usize = 1024 * 1024;
/// Roughly a year of daily rows; older aggregates are pruned on write.
const MAX_USAGE_DAYS: i64 = 400;
const DATABASE_BUSY_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_IMPORT_BYTES: usize = 4 * 1024 * 1024;
const MAX_SNIPPETS: usize = 2000;
const MAX_TRANSFORMS: usize = 256;

/// Each store is owned behind the runtime's short-lived persistence mutex.
pub struct Store {
    connection: Connection,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DictionaryArchive {
    version: u32,
    entries: Vec<DictionaryEntry>,
}

impl Store {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| AppError::storage())?;
        }
        let connection = Connection::open(path).map_err(|_| AppError::storage())?;
        Self::initialize(connection)
    }

    pub fn in_memory() -> AppResult<Self> {
        Self::initialize(Connection::open_in_memory().map_err(|_| AppError::storage())?)
    }

    fn initialize(mut connection: Connection) -> AppResult<Self> {
        connection
            .busy_timeout(DATABASE_BUSY_TIMEOUT)
            .map_err(|_| AppError::storage())?;
        connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF; PRAGMA secure_delete=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;").map_err(|_| AppError::storage())?;
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|_| AppError::storage())?;
        if version > SCHEMA_VERSION {
            return Err(AppError::invalid(&format!("The database belongs to a newer application version. Upgrade {} before opening it.", crate::APP_NAME)));
        }
        for (target, migration) in MIGRATIONS {
            if version < *target {
                let transaction = connection.transaction().map_err(|_| AppError::storage())?;
                transaction
                    .execute_batch(migration)
                    .map_err(|_| AppError::storage())?;
                transaction.commit().map_err(|_| AppError::storage())?;
            }
        }
        let mut store = Self { connection };
        if store.transforms()?.is_empty() {
            for transform in crate::transforms::builtin_transforms() {
                store.save_transform(&transform)?;
            }
        }
        Ok(store)
    }

    pub fn settings(&self) -> AppResult<Settings> {
        let raw: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM settings WHERE key='application'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| AppError::storage())?;
        let settings: Settings = raw
            .map(|json| serde_json::from_str(&json).map_err(|_| AppError::storage()))
            .transpose()?
            .unwrap_or_default();
        settings.validate()?;
        Ok(settings)
    }

    pub fn save_settings(&mut self, settings: &Settings) -> AppResult<()> {
        settings.validate()?;
        let json = serde_json::to_string(settings).map_err(|_| AppError::storage())?;
        self.connection.execute("INSERT INTO settings(key,value) VALUES('application',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[json]).map_err(|_|AppError::storage())?;
        Ok(())
    }

    pub fn dictionary(&self) -> AppResult<Vec<DictionaryEntry>> {
        self.load_json_rows("SELECT payload FROM dictionary ORDER BY normalized_key,id")
    }

    pub fn save_dictionary_entry(&mut self, entry: &DictionaryEntry) -> AppResult<()> {
        validate_dictionary_entry(entry)?;
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM dictionary", [], |row| row.get(0))
            .map_err(|_| AppError::storage())?;
        let exists: bool = self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM dictionary WHERE id=?1)",
                [&entry.id],
                |row| row.get(0),
            )
            .map_err(|_| AppError::storage())?;
        if count >= MAX_DICTIONARY_ENTRIES as i64 && !exists {
            return Err(AppError::invalid("Dictionary entry limit reached."));
        }
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| AppError::storage())?;
        upsert_dictionary(&transaction, entry)?;
        transaction.commit().map_err(|_| AppError::storage())?;
        Ok(())
    }

    pub fn delete_dictionary_entry(&mut self, id: &str) -> AppResult<()> {
        self.connection
            .execute("DELETE FROM dictionary WHERE id=?1", [id])
            .map_err(|_| AppError::storage())?;
        Ok(())
    }

    /// Export is explicit; the renderer receives dictionary data only on request.
    pub fn export_dictionary(&self) -> AppResult<String> {
        serde_json::to_string_pretty(&DictionaryArchive {
            version: 1,
            entries: self.dictionary()?,
        })
        .map_err(|_| AppError::storage())
    }

    /// Entire import validates before a transaction; duplicates merge by normalized trigger.
    pub fn import_dictionary(&mut self, source: &str) -> AppResult<usize> {
        if source.len() > MAX_IMPORT_BYTES {
            return Err(AppError::invalid("Dictionary import exceeds 4 MiB."));
        }
        let archive: DictionaryArchive = serde_json::from_str(source)
            .map_err(|_| AppError::invalid("Dictionary archive is malformed."))?;
        if archive.version != 1 || archive.entries.len() > MAX_DICTIONARY_ENTRIES {
            return Err(AppError::invalid(
                "Unsupported dictionary archive version or entry count.",
            ));
        }
        let mut merged: std::collections::BTreeMap<String, DictionaryEntry> = self
            .dictionary()?
            .into_iter()
            .map(|entry| (lookup_key(&entry.phrase), entry))
            .collect();
        let mut added = 0;
        for mut entry in archive.entries {
            validate_dictionary_entry(&entry)?;
            let key = lookup_key(&entry.phrase);
            if merged.contains_key(&key) {
                continue;
            }
            entry.id = uuid::Uuid::new_v4().to_string();
            entry.source = EntrySource::Imported;
            merged.insert(key, entry);
            added += 1;
        }
        if merged.len() > MAX_DICTIONARY_ENTRIES {
            return Err(AppError::invalid(
                "Import would exceed the dictionary entry limit.",
            ));
        }
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| AppError::storage())?;
        for entry in merged.values() {
            upsert_dictionary(&transaction, entry)?;
        }
        transaction.commit().map_err(|_| AppError::storage())?;
        Ok(added)
    }

    pub fn snippets(&self) -> AppResult<Vec<Snippet>> {
        self.load_json_rows("SELECT payload FROM snippets ORDER BY normalized_trigger,id")
    }

    pub fn save_snippet(&mut self, snippet: &Snippet) -> AppResult<()> {
        if snippet.id.is_empty()
            || snippet.id.len() > 64
            || snippet.trigger.trim().is_empty()
            || snippet.trigger.chars().count() > 160
            || snippet.trigger.contains('\0')
            || snippet.expansion.is_empty()
            || snippet.expansion.len() > MAX_TEXT_BYTES
            || snippet.expansion.contains('\0')
        {
            return Err(AppError::invalid(
                "Snippet trigger or expansion is invalid.",
            ));
        }
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM snippets", [], |row| row.get(0))
            .map_err(|_| AppError::storage())?;
        if count >= MAX_SNIPPETS as i64
            && !self.snippets()?.iter().any(|saved| saved.id == snippet.id)
        {
            return Err(AppError::invalid("Snippet limit reached."));
        }
        let payload = serde_json::to_string(snippet).map_err(|_| AppError::storage())?;
        self.connection.execute("INSERT INTO snippets(id,normalized_trigger,payload) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET normalized_trigger=excluded.normalized_trigger,payload=excluded.payload",params![snippet.id,lookup_key(&snippet.trigger),payload]).map_err(|error| map_write_error(&error))?;
        Ok(())
    }

    pub fn delete_snippet(&mut self, id: &str) -> AppResult<()> {
        self.connection
            .execute("DELETE FROM snippets WHERE id=?1", [id])
            .map_err(|_| AppError::storage())?;
        Ok(())
    }
    pub fn transforms(&self) -> AppResult<Vec<Transform>> {
        self.load_json_rows("SELECT payload FROM transforms ORDER BY id")
    }

    pub fn save_transform(&mut self, transform: &Transform) -> AppResult<()> {
        if transform.id.is_empty()
            || transform.id.len() > 64
            || transform.name.trim().is_empty()
            || transform.name.len() > 160
            || transform.instruction.trim().is_empty()
            || transform.instruction.len() > 8192
        {
            return Err(AppError::invalid(
                "Transform name or instruction is invalid.",
            ));
        }
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM transforms", [], |row| row.get(0))
            .map_err(|_| AppError::storage())?;
        if count >= MAX_TRANSFORMS as i64
            && !self
                .transforms()?
                .iter()
                .any(|saved| saved.id == transform.id)
        {
            return Err(AppError::invalid("Transform limit reached."));
        }
        let payload = serde_json::to_string(transform).map_err(|_| AppError::storage())?;
        self.connection.execute("INSERT INTO transforms(id,payload) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",params![transform.id,payload]).map_err(|_|AppError::storage())?;
        Ok(())
    }

    pub fn delete_transform(&mut self, id: &str) -> AppResult<()> {
        self.connection
            .execute("DELETE FROM transforms WHERE id=?1", [id])
            .map_err(|_| AppError::storage())?;
        Ok(())
    }

    /// Aggregate counters are content-free and do not persist operation identifiers.
    /// `words` is a count only; `day` is the caller's local calendar day.
    pub fn record_usage(
        &mut self,
        day: &str,
        words: u64,
        audio_ms: u64,
        inference_ms: u64,
        fallback: bool,
    ) -> AppResult<()> {
        if !is_day_key(day) {
            return Err(AppError::invalid("Usage day must be YYYY-MM-DD."));
        }
        let audio_ms = i64::try_from(audio_ms)
            .map_err(|_| AppError::invalid("Usage duration exceeds the supported range."))?;
        let inference_ms = i64::try_from(inference_ms)
            .map_err(|_| AppError::invalid("Usage duration exceeds the supported range."))?;
        let words = i64::try_from(words)
            .map_err(|_| AppError::invalid("Usage count exceeds the supported range."))?;
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| AppError::storage())?;
        transaction.execute("UPDATE usage SET operations=operations+1,audio_ms=audio_ms+?1,inference_ms=inference_ms+?2,fallbacks=fallbacks+?3 WHERE id=1", params![audio_ms,inference_ms,u32::from(fallback)]).map_err(|_|AppError::storage())?;
        transaction.execute("INSERT INTO daily_usage(day,dictations,words,audio_ms) VALUES(?1,1,?2,?3) ON CONFLICT(day) DO UPDATE SET dictations=dictations+1,words=words+excluded.words,audio_ms=audio_ms+excluded.audio_ms", params![day,words,audio_ms]).map_err(|_|AppError::storage())?;
        // Lifetime totals outlive the pruned daily rows.
        transaction.execute("UPDATE usage_lifetime SET since=COALESCE(since,?1),dictations=dictations+1,words=words+?2,audio_ms=audio_ms+?3 WHERE id=1", params![day,words,audio_ms]).map_err(|_|AppError::storage())?;
        transaction.execute("DELETE FROM daily_usage WHERE day NOT IN (SELECT day FROM daily_usage ORDER BY day DESC LIMIT ?1)", params![MAX_USAGE_DAYS]).map_err(|_|AppError::storage())?;
        transaction.commit().map_err(|_| AppError::storage())?;
        Ok(())
    }

    /// Upsert one history entry. `payload` is already encrypted by the caller; the store never
    /// sees plaintext.
    pub fn save_history(&mut self, id: &str, created_at: i64, payload: &[u8]) -> AppResult<()> {
        if id.is_empty()
            || id.len() > 64
            || payload.is_empty()
            || payload.len() > MAX_HISTORY_PAYLOAD_BYTES
        {
            return Err(AppError::invalid("History entry is invalid."));
        }
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| AppError::storage())?;
        transaction
            .execute(
                "INSERT INTO history(id,created_at,payload) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",
                params![id, created_at, payload],
            )
            .map_err(|_| AppError::storage())?;
        transaction
            .execute(
                "DELETE FROM history WHERE id NOT IN (SELECT id FROM history ORDER BY created_at DESC LIMIT ?1)",
                params![MAX_HISTORY_ROWS],
            )
            .map_err(|_| AppError::storage())?;
        transaction.commit().map_err(|_| AppError::storage())?;
        Ok(())
    }

    /// Newest first: (id, created_at, encrypted payload) at or after `since`.
    pub fn history_since(&self, since: i64) -> AppResult<Vec<(String, i64, Vec<u8>)>> {
        let mut statement = self
            .connection
            .prepare("SELECT id,created_at,payload FROM history WHERE created_at>=?1 ORDER BY created_at DESC LIMIT ?2")
            .map_err(|_| AppError::storage())?;
        let rows = statement
            .query_map(params![since, MAX_HISTORY_ROWS], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|_| AppError::storage())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::storage())
    }

    /// Remove entries created before `before`; `i64::MAX` removes everything.
    pub fn prune_history(&mut self, before: i64) -> AppResult<()> {
        self.connection
            .execute("DELETE FROM history WHERE created_at<?1", params![before])
            .map_err(|_| AppError::storage())?;
        Ok(())
    }

    pub fn delete_history(&mut self, id: &str) -> AppResult<()> {
        self.connection
            .execute("DELETE FROM history WHERE id=?1", params![id])
            .map_err(|_| AppError::storage())?;
        Ok(())
    }

    /// Most recent days first, at most `limit` rows.
    pub fn daily_usage(&self, limit: u32) -> AppResult<Vec<DailyUsage>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT day,dictations,words,audio_ms FROM daily_usage ORDER BY day DESC LIMIT ?1",
            )
            .map_err(|_| AppError::storage())?;
        let rows = statement
            .query_map(params![limit], |row| {
                Ok(DailyUsage {
                    day: row.get(0)?,
                    dictations: saturating_u32(row.get(1)?),
                    words: saturating_u32(row.get(2)?),
                    audio_ms: saturating_u32(row.get(3)?),
                })
            })
            .map_err(|_| AppError::storage())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::storage())
    }

    pub fn lifetime_usage(&self) -> AppResult<LifetimeUsage> {
        self.connection
            .query_row(
                "SELECT since,dictations,words,audio_ms FROM usage_lifetime WHERE id=1",
                [],
                |row| {
                    Ok(LifetimeUsage {
                        since: row.get(0)?,
                        dictations: saturating_u32(row.get(1)?),
                        words: saturating_u32(row.get(2)?),
                        audio_ms: row.get::<_, i64>(3)?.max(0) as f64,
                    })
                },
            )
            .map_err(|_| AppError::storage())
    }

    /// The name and the stored, already normalized PNG.
    pub fn profile(&self) -> AppResult<(Option<String>, Option<Vec<u8>>)> {
        self.connection
            .query_row("SELECT name,picture FROM profile WHERE id=1", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()
            .map(Option::unwrap_or_default)
            .map_err(|_| AppError::storage())
    }

    /// `name` must already be validated by `profile::normalize_name`.
    pub fn set_profile_name(&mut self, name: Option<&str>) -> AppResult<()> {
        self.connection
            .execute(
                "INSERT INTO profile(id,name) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET name=excluded.name",
                params![name],
            )
            .map_err(|_| AppError::storage())?;
        Ok(())
    }

    /// `picture` must already be re-encoded by `profile::normalize_picture`.
    pub fn set_profile_picture(&mut self, picture: Option<&[u8]>) -> AppResult<()> {
        if picture.is_some_and(|bytes| bytes.len() > crate::profile::MAX_STORED_PICTURE_BYTES) {
            return Err(AppError::invalid("The picture is too large."));
        }
        self.connection
            .execute(
                "INSERT INTO profile(id,picture) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET picture=excluded.picture",
                params![picture],
            )
            .map_err(|_| AppError::storage())?;
        Ok(())
    }

    fn load_json_rows<T: serde::de::DeserializeOwned>(&self, query: &str) -> AppResult<Vec<T>> {
        let mut statement = self
            .connection
            .prepare(query)
            .map_err(|_| AppError::storage())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|_| AppError::storage())?;
        rows.map(|row| {
            row.map_err(|_| AppError::storage())
                .and_then(|raw| serde_json::from_str(&raw).map_err(|_| AppError::storage()))
        })
        .collect()
    }
}

fn is_day_key(day: &str) -> bool {
    let bytes = day.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

fn saturating_u32(value: i64) -> u32 {
    u32::try_from(value.max(0)).unwrap_or(u32::MAX)
}

pub fn validate_dictionary_entry(entry: &DictionaryEntry) -> AppResult<()> {
    if entry.id.is_empty()
        || entry.id.len() > 64
        || entry.phrase.trim().is_empty()
        || entry.phrase.chars().count() > 160
        || !entry.phrase.chars().any(char::is_alphanumeric)
        || entry.phrase.contains('\0')
    {
        return Err(AppError::invalid(
            "Dictionary phrase must contain a word and at most 160 characters.",
        ));
    }
    match (&entry.kind, &entry.replacement) {
        (DictionaryKind::Vocabulary, None) => Ok(()),
        (DictionaryKind::Replacement, Some(replacement))
            if !replacement.trim().is_empty()
                && replacement.chars().count() <= 160
                && !replacement.contains('\0') =>
        {
            Ok(())
        }
        _ => Err(AppError::invalid(
            "Vocabulary has no replacement; a correction needs an output phrase.",
        )),
    }
}

fn upsert_dictionary(transaction: &Transaction<'_>, entry: &DictionaryEntry) -> AppResult<()> {
    let payload = serde_json::to_string(entry).map_err(|_| AppError::storage())?;
    transaction.execute("INSERT INTO dictionary(id,normalized_key,payload) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET normalized_key=excluded.normalized_key,payload=excluded.payload",params![entry.id,lookup_key(&entry.phrase),payload]).map_err(|error|map_write_error(&error))?;
    Ok(())
}

fn map_write_error(error: &rusqlite::Error) -> AppError {
    if let rusqlite::Error::SqliteFailure(code, _) = error {
        if code.code == rusqlite::ErrorCode::ConstraintViolation {
            return AppError::invalid("This trigger already exists. Edit the existing entry.");
        }
    }
    AppError::storage()
}

#[cfg(test)]
mod tests {
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
    fn version_one_database_gains_daily_usage() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("v1.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(include_str!("../migrations/001_initial.sql"))
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
                "INSERT INTO daily_usage VALUES('2026-10-23',2,30,9000),('2026-10-24',1,12,4000);",
            )
            .unwrap();
        drop(connection);
        let mut store = Store::open(&path).unwrap();
        let lifetime = store.lifetime_usage().unwrap();
        assert_eq!(lifetime.since.as_deref(), Some("2026-10-23"));
        assert_eq!((lifetime.dictations, lifetime.words), (3, 42));
        assert_eq!(lifetime.audio_ms, 13_000.0);
        store
            .record_usage("2026-10-25", 8, 1000, 10, false)
            .unwrap();
        let lifetime = store.lifetime_usage().unwrap();
        assert_eq!((lifetime.dictations, lifetime.words), (4, 50));
        assert_eq!(lifetime.since.as_deref(), Some("2026-10-23"));
    }

    #[test]
    fn lifetime_totals_start_on_the_first_dictation_and_outlive_pruned_days() {
        let mut store = Store::in_memory().unwrap();
        assert_eq!(store.lifetime_usage().unwrap(), LifetimeUsage::default());
        for offset in 0..(MAX_USAGE_DAYS + 5) {
            let day = format!("2025-{:02}-{:02}", offset / 28 + 1, offset % 28 + 1);
            store.record_usage(&day, 2, 1000, 1, false).unwrap();
        }
        assert_eq!(
            store.daily_usage(1000).unwrap().len(),
            MAX_USAGE_DAYS as usize
        );
        let lifetime = store.lifetime_usage().unwrap();
        assert_eq!(lifetime.since.as_deref(), Some("2025-01-01"));
        assert_eq!(lifetime.words, 2 * (MAX_USAGE_DAYS as u32 + 5));
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
    }
}
