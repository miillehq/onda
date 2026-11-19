use super::*;

impl Store {
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
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| AppError::storage())?;
        if !rewrite_defaults::exists(&transaction, &settings.default_transform_id)? {
            return Err(AppError::invalid("Choose an existing default rewrite."));
        }
        transaction.execute("INSERT INTO settings(key,value) VALUES('application',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[json]).map_err(|_|AppError::storage())?;
        // A crash must never leave the new retention preference beside the previous disk history.
        if settings.history_days == 0 {
            transaction.execute("DELETE FROM history", [])
        } else {
            transaction.execute(
                "DELETE FROM history WHERE created_at < CAST(unixepoch('subsec') * 1000 AS INTEGER) - ?1",
                [i64::from(settings.history_days) * MILLIS_PER_DAY],
            )
        }.map_err(|_| AppError::storage())?;
        transaction.commit().map_err(|_| AppError::storage())
    }
}
