use super::*;

impl Store {
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

    /// Remove entries created strictly before the retention boundary.
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

    /// Explicit removal applies to every row, regardless of its timestamp or whether it opens.
    pub fn clear_history(&mut self) -> AppResult<()> {
        self.connection
            .execute("DELETE FROM history", [])
            .map_err(|_| AppError::storage())?;
        Ok(())
    }
}
