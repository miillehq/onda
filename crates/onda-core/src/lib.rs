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
