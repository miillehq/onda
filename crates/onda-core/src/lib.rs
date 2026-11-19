//! Application policy independent of Windows, inference engines and UI.
pub mod contracts;
pub mod dictionary;
pub mod error;
pub mod learning;
pub mod persistence;
pub mod profile;
pub mod state;
pub mod stats;
pub mod text;
pub mod transforms;

pub const APP_NAME: &str = env!("ONDA_APP_NAME");
pub const APP_IDENTIFIER: &str = env!("ONDA_APP_IDENTIFIER");
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MAX_TEXT_BYTES: usize = 128 * 1024;
pub const MAX_DICTIONARY_ENTRIES: usize = 20_000;
pub const MAX_DICTIONARY_TERM_CHARS: usize = 160;
pub const MAX_DICTIONARY_ID_BYTES: usize = 64;
// Two terms and the ID can expand to six JSON bytes per character. The fixed allowance covers
// all serialized keys, enum values, indentation and separators in Onda's own version 1 export.
const MAX_ARCHIVED_ENTRY_BYTES: usize =
    (2 * MAX_DICTIONARY_TERM_CHARS + MAX_DICTIONARY_ID_BYTES) * 6 + 256;
pub const MAX_DICTIONARY_ARCHIVE_BYTES: usize =
    256 + MAX_DICTIONARY_ENTRIES * MAX_ARCHIVED_ENTRY_BYTES;
