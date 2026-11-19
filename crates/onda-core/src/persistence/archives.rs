//! Read versioned dictionary backups without retaining more than one dictionary's worth of rows.
use super::*;
use serde::de::{IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use std::{collections::HashSet, fmt};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputArchive<'a> {
    version: u32,
    #[serde(borrow, deserialize_with = "read_records")]
    entries: Records<'a>,
}

struct Records<'a> {
    values: Vec<&'a RawValue>,
    count: usize,
}

/// Keep borrowed spans for supported records. Beyond the limit, only count the remaining rows.
fn read_records<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Records<'de>, D::Error> {
    struct RecordsVisitor;
    impl<'de> Visitor<'de> for RecordsVisitor {
        type Value = Records<'de>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an array of dictionary entries")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            let mut values = Vec::new();
            while values.len() < MAX_DICTIONARY_ENTRIES {
                let Some(entry) = sequence.next_element()? else {
                    break;
                };
                values.push(entry);
            }
            let mut count = values.len();
            if count == MAX_DICTIONARY_ENTRIES {
                while sequence.next_element::<IgnoredAny>()?.is_some() {
                    count += 1;
                }
            }
            Ok(Records { values, count })
        }
    }
    deserializer.deserialize_seq(RecordsVisitor)
}

impl Store {
    /// Import atomically, preserving existing spellings when normalized triggers collide.
    /// Parsing and lookup take expected O(bytes + existing vocabulary bytes) time, plus indexed
    /// inserts. The record index and normalized keys use O(maximum dictionary size) space.
    pub fn import_dictionary(&mut self, source: &str) -> AppResult<usize> {
        if source.len() > crate::MAX_DICTIONARY_ARCHIVE_BYTES {
            return Err(AppError::invalid(
                "Dictionary archive exceeds the maximum backup size.",
            ));
        }
        if !source.trim_start().starts_with('{') {
            return Err(AppError::invalid(
                "Dictionary archive must be a versioned JSON object.",
            ));
        }
        let archive: InputArchive<'_> = serde_json::from_str(source).map_err(|error| {
            AppError::invalid(&format!(
                "Dictionary archive is malformed at line {}.",
                error.line()
            ))
        })?;
        if archive.version != 1 {
            return Err(AppError::invalid("Unsupported dictionary archive version."));
        }
        if archive.entries.count > MAX_DICTIONARY_ENTRIES {
            return Err(AppError::invalid(&format!(
                "Dictionary import rejected all {} entries. The limit is {MAX_DICTIONARY_ENTRIES}.",
                archive.entries.count
            )));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| AppError::storage())?;
        let mut keys = existing_keys(&transaction)?;
        let mut added = 0;
        for (index, raw) in archive.entries.values.into_iter().enumerate() {
            let malformed = || {
                AppError::invalid(&format!(
                    "Dictionary entry {} is malformed at line {}.",
                    index + 1,
                    record_line(source, raw)
                ))
            };
            if !raw.get().starts_with('{') {
                return Err(malformed());
            }
            let mut entry: DictionaryEntry =
                serde_json::from_str(raw.get()).map_err(|_| malformed())?;
            validate_dictionary_entry(&entry).map_err(|error| {
                AppError::invalid(&format!(
                    "Dictionary entry {} at line {}. {}",
                    index + 1,
                    record_line(source, raw),
                    error.message
                ))
            })?;
            if !keys.insert(lookup_key(&entry.phrase)) {
                continue;
            }
            if keys.len() > MAX_DICTIONARY_ENTRIES {
                return Err(AppError::invalid(
                    "Import would exceed the dictionary entry limit.",
                ));
            }
            entry.id = uuid::Uuid::new_v4().to_string();
            entry.source = EntrySource::Imported;
            upsert_dictionary(&transaction, &entry)?;
            added += 1;
        }
        transaction.commit().map_err(|_| AppError::storage())?;
        Ok(added)
    }
}

/// The borrowed raw record points into this source. Count lines only on the first error,
/// including when identical records occur on different lines.
fn record_line(source: &str, record: &RawValue) -> usize {
    let offset = record.get().as_ptr() as usize - source.as_ptr() as usize;
    1 + source.as_bytes()[..offset]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
}

fn existing_keys(transaction: &Transaction<'_>) -> AppResult<HashSet<String>> {
    let mut statement = transaction
        .prepare("SELECT payload FROM dictionary")
        .map_err(|_| AppError::storage())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|_| AppError::storage())?;
    rows.map(|row| {
        let raw = row.map_err(|_| AppError::storage())?;
        let entry: DictionaryEntry = serde_json::from_str(&raw).map_err(|_| AppError::storage())?;
        Ok(lookup_key(&entry.phrase))
    })
    .collect()
}
