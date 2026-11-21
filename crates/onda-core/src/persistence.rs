//! Parameterized SQLite transactions; migrations never erase unknown/newer data.
mod archives;
mod history;
mod preferences;
mod usage;

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

const SCHEMA_VERSION: i64 = 6;
/// Applied in order; each script ends by setting its own user_version.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../migrations/001_initial.sql")),
    (2, include_str!("../migrations/002_daily_usage.sql")),
    (3, include_str!("../migrations/003_history.sql")),
    (
        4,
        include_str!("../migrations/004_profile_and_lifetime.sql"),
    ),
    (5, include_str!("../migrations/005_lifetime_days.sql")),
    (6, include_str!("../migrations/006_usage_window.sql")),
];
/// Bounds the encrypted history table regardless of retention.
const MAX_HISTORY_ROWS: i64 = 2000;
const MAX_HISTORY_PAYLOAD_BYTES: usize = 1024 * 1024;
/// Roughly a year of daily rows; older aggregates are pruned on write.
const MAX_USAGE_DAYS: i64 = 400;
const DATABASE_BUSY_TIMEOUT: Duration = Duration::from_secs(3);
const MILLIS_PER_DAY: i64 = 86_400_000;
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
        // Closing the last writable handle normally checkpoints WAL, even if we only read.
        let checkpoint = rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE;
        connection
            .set_db_config(checkpoint, true)
            .map_err(|_| AppError::storage())?;
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|_| AppError::storage())?;
        if version > SCHEMA_VERSION {
            return Err(AppError::invalid(&format!("The database belongs to a newer application version. Upgrade {} before opening it.", crate::APP_NAME)));
        }
        connection
            .set_db_config(checkpoint, false)
            .map_err(|_| AppError::storage())?;
        // Entering WAL mode changes the on-disk header even without a transaction.
        connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF; PRAGMA secure_delete=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;").map_err(|_| AppError::storage())?;
        for (target, migration) in MIGRATIONS {
            if version < *target {
                let transaction = connection.transaction().map_err(|_| AppError::storage())?;
                transaction
                    .execute_batch(migration)
                    .map_err(|_| AppError::storage())?;
                transaction.commit().map_err(|_| AppError::storage())?;
            }
        }
        usage::repair_retained_streaks(&connection)?;
        let mut store = Self { connection };
        if store.transforms()?.is_empty() {
            for transform in crate::transforms::builtin_transforms() {
                store.save_transform(&transform)?;
            }
        }
        Ok(store)
    }

    pub fn dictionary(&self) -> AppResult<Vec<DictionaryEntry>> {
        self.load_json_rows("SELECT payload FROM dictionary ORDER BY normalized_key,id")
    }

    pub fn save_dictionary_entry(&mut self, entry: &DictionaryEntry) -> AppResult<()> {
        validate_dictionary_entry(entry)?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| AppError::storage())?;
        let count: i64 = transaction
            .query_row("SELECT COUNT(*) FROM dictionary", [], |row| row.get(0))
            .map_err(|_| AppError::storage())?;
        let exists: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM dictionary WHERE id=?1)",
                [&entry.id],
                |row| row.get(0),
            )
            .map_err(|_| AppError::storage())?;
        if count >= MAX_DICTIONARY_ENTRIES as i64 && !exists {
            return Err(AppError::invalid("Dictionary entry limit reached."));
        }
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

pub fn validate_dictionary_entry(entry: &DictionaryEntry) -> AppResult<()> {
    if entry.id.is_empty()
        || entry.id.len() > crate::MAX_DICTIONARY_ID_BYTES
        || entry.phrase.trim().is_empty()
        || entry.phrase.chars().count() > crate::MAX_DICTIONARY_TERM_CHARS
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
                && replacement.chars().count() <= crate::MAX_DICTIONARY_TERM_CHARS
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
mod tests;
