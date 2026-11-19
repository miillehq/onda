use super::*;
use crate::contracts::UsageSummary;

impl Store {
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
        if crate::stats::day_number(day).is_none() {
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
        transaction.execute("UPDATE usage_lifetime SET since=CASE WHEN since IS NULL OR ?1 < since THEN ?1 ELSE since END,dictations=dictations+1,words=words+?2,audio_ms=audio_ms+?3 WHERE id=1", params![day,words,audio_ms]).map_err(|_|AppError::storage())?;
        let dictations_today: i64 = transaction
            .query_row(
                "SELECT dictations FROM daily_usage WHERE day=?1",
                params![day],
                |row| row.get(0),
            )
            .map_err(|_| AppError::storage())?;
        if dictations_today == 1 {
            let mut lifetime = lifetime_usage(&transaction)?;
            let late = lifetime.last_day.as_deref().is_some_and(|last| day < last);
            let outside_retained_days = late
                && lifetime.active_days >= MAX_USAGE_DAYS as u32
                && transaction
                    .query_row(
                        "SELECT COUNT(*) >= ?2 FROM daily_usage WHERE day > ?1",
                        params![day, MAX_USAGE_DAYS],
                        |row| row.get::<_, bool>(0),
                    )
                    .map_err(|_| AppError::storage())?;
            // Once the detailed days have been pruned, a repeated old date cannot be
            // distinguished from another visit to it. Keep the measured lifetime totals.
            if !outside_retained_days {
                lifetime.count_active_day(day);
                if late {
                    let runs =
                        retained_runs(&transaction, lifetime.last_day.as_deref().unwrap_or(day))?;
                    lifetime.current_run = lifetime.current_run.max(runs.latest);
                    lifetime.longest_run = lifetime.longest_run.max(runs.longest);
                }
            }
            transaction.execute("UPDATE usage_lifetime SET active_days=?1,last_day=?2,current_run=?3,longest_run=?4 WHERE id=1", params![lifetime.active_days,lifetime.last_day,lifetime.current_run,lifetime.longest_run]).map_err(|_|AppError::storage())?;
            if !outside_retained_days {
                advance_window_prefix(&transaction)?;
            }
            transaction.execute("DELETE FROM daily_usage WHERE day NOT IN (SELECT day FROM daily_usage ORDER BY day DESC LIMIT ?1)", params![MAX_USAGE_DAYS]).map_err(|_|AppError::storage())?;
        }

        transaction.commit().map_err(|_| AppError::storage())?;
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
        lifetime_usage(&self.connection)
    }

    /// A clock change may leave a recorded date ahead of today. Base the visible current
    /// streak on the latest retained day up to today in that case.
    pub fn usage_summary(&self, today: &str) -> AppResult<UsageSummary> {
        let mut lifetime = self.lifetime_usage()?;
        if lifetime
            .last_day
            .as_deref()
            .is_some_and(|last| last > today)
        {
            let runs = retained_runs(&self.connection, today)?;
            lifetime.last_day = runs.day;
            lifetime.current_run = runs.latest;
        }
        Ok(crate::stats::summarize(&lifetime, today))
    }
}

struct RetainedRuns {
    day: Option<String>,
    latest: u32,
    longest: u32,
}

/// At most 401 daily rows enter the indexed date scan. Equal differences between the
/// Julian day and row number identify consecutive dates, even across months and years.
fn retained_runs(connection: &Connection, through: &str) -> AppResult<RetainedRuns> {
    connection
        .query_row(
            "WITH ranked AS (
            SELECT day, julianday(day) - row_number() OVER (ORDER BY day) AS series
            FROM daily_usage WHERE day <= ?1 AND dictations > 0
         ), runs AS (
            SELECT max(day) AS ending, count(*) + CASE WHEN min(day) = (SELECT min(day) FROM daily_usage)
                   THEN (SELECT days_before_window FROM usage_lifetime WHERE id=1) ELSE 0 END AS length
            FROM ranked GROUP BY series
         )
         SELECT (SELECT max(ending) FROM runs),
                coalesce((SELECT length FROM runs ORDER BY ending DESC LIMIT 1), 0),
                coalesce((SELECT max(length) FROM runs), 0)",
            [through],
            |row| {
                Ok(RetainedRuns {
                    day: row.get(0)?,
                    latest: row.get(1)?,
                    longest: row.get(2)?,
                })
            },
        )
        .map_err(|_| AppError::storage())
}

/// Before pruning the oldest daily row, carry only the length of its contiguous prefix.
fn advance_window_prefix(transaction: &Transaction<'_>) -> AppResult<()> {
    transaction.execute(
        "WITH boundary AS (
             SELECT min(day) AS first, (SELECT day FROM daily_usage ORDER BY day LIMIT 1 OFFSET 1) AS second
             FROM daily_usage HAVING count(*) > ?1
         )
         UPDATE usage_lifetime SET days_before_window =
             CASE WHEN (SELECT julianday(second) - julianday(first) FROM boundary) = 1
                  THEN days_before_window + 1 ELSE 0 END
         WHERE id=1 AND EXISTS (SELECT 1 FROM boundary)",
        [MAX_USAGE_DAYS],
    ).map_err(|_| AppError::storage())?;
    Ok(())
}

fn lifetime_usage(connection: &Connection) -> AppResult<LifetimeUsage> {
    connection
        .query_row(
            "SELECT since,dictations,words,audio_ms,active_days,last_day,current_run,longest_run FROM usage_lifetime WHERE id=1",
            [],
            |row| {
                Ok(LifetimeUsage {
                    since: row.get(0)?,
                    dictations: saturating_u32(row.get(1)?),
                    words: saturating_u32(row.get(2)?),
                    audio_ms: row.get::<_, i64>(3)?.max(0) as f64,
                    active_days: saturating_u32(row.get(4)?),
                    last_day: row.get(5)?,
                    current_run: saturating_u32(row.get(6)?),
                    longest_run: saturating_u32(row.get(7)?),
                })
            },
        )
        .map_err(|_| AppError::storage())
}

fn saturating_u32(value: i64) -> u32 {
    u32::try_from(value.max(0)).unwrap_or(u32::MAX)
}
