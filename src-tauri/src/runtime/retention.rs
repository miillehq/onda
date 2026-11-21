//! Coordinates the in-memory history revisions with the durable retention transaction.
use super::*;

pub(super) struct HistoryService {
    /// Newest first. Decrypted only in memory; the store holds DPAPI-sealed copies.
    pub(super) history: Mutex<History>,
}
