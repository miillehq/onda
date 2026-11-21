ALTER TABLE usage_lifetime ADD COLUMN days_before_window INTEGER NOT NULL DEFAULT 0;
-- A v5 current streak can extend before every retained date. Keep its provable prefix
-- without inventing dates for historical streaks whose detail has already been pruned.
UPDATE usage_lifetime
SET days_before_window = max(0, current_run - (SELECT count(*) FROM daily_usage))
WHERE id = 1 AND last_day = (SELECT max(day) FROM daily_usage)
  AND (SELECT julianday(max(day)) - julianday(min(day)) + 1 = count(*) FROM daily_usage);
PRAGMA user_version=6;
