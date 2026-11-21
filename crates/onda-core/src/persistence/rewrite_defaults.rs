//! Keep the default shortcut attached to a stored rewrite across deletes and older profiles.
use super::*;
use crate::transforms::{builtin_transforms, DEFAULT_TRANSFORM_ID};

pub(super) fn exists(connection: &Connection, id: &str) -> AppResult<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM transforms WHERE id=?1)",
            [id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::storage())
}

fn repair_reference(connection: &Connection) -> AppResult<()> {
    // Update only the orphaned preference. Older databases can carry fields introduced by a
    // different build and the user's unrelated settings and log retention must survive.
    connection.execute(
        "UPDATE settings SET value=json_set(value,'$.defaultTransformId',?1)
         WHERE key='application' AND json_type(value,'$.defaultTransformId')='text'
         AND NOT EXISTS(SELECT 1 FROM transforms WHERE id=json_extract(settings.value,'$.defaultTransformId'))",
        [DEFAULT_TRANSFORM_ID],
    ).map_err(|_| AppError::storage())?;
    Ok(())
}

pub(super) fn restore(connection: &mut Connection) -> AppResult<()> {
    let transaction = connection.transaction().map_err(|_| AppError::storage())?;
    let stored_ids = {
        let mut statement = transaction
            .prepare("SELECT payload FROM transforms")
            .map_err(|_| AppError::storage())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|_| AppError::storage())?;
        let mut ids = std::collections::HashSet::new();
        for row in rows {
            let decoded: Transform = serde_json::from_str(&row.map_err(|_| AppError::storage())?)
                .map_err(|_| AppError::storage())?;
            ids.insert(decoded.id);
        }
        ids
    };
    for transform in builtin_transforms() {
        if !stored_ids.contains(&transform.id) {
            let payload = serde_json::to_string(&transform).map_err(|_| AppError::storage())?;
            transaction
                .execute(
                    "INSERT INTO transforms(id,payload) VALUES(?1,?2)",
                    params![transform.id, payload],
                )
                .map_err(|_| AppError::storage())?;
        }
    }
    repair_reference(&transaction)?;
    transaction.commit().map_err(|_| AppError::storage())
}

pub(super) fn remove(connection: &mut Connection, id: &str) -> AppResult<()> {
    if builtin_transforms().iter().any(|item| item.id == id) {
        return Err(AppError::invalid(
            "Built-in rewrites can be edited but cannot be removed.",
        ));
    }
    let transaction = connection.transaction().map_err(|_| AppError::storage())?;
    transaction
        .execute("DELETE FROM transforms WHERE id=?1", [id])
        .map_err(|_| AppError::storage())?;
    repair_reference(&transaction)?;
    transaction.commit().map_err(|_| AppError::storage())
}
