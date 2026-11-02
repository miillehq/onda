ALTER TABLE usage_lifetime ADD COLUMN active_days INTEGER NOT NULL DEFAULT 0;
ALTER TABLE usage_lifetime ADD COLUMN last_day TEXT CHECK(last_day IS NULL OR length(last_day)=10);
ALTER TABLE usage_lifetime ADD COLUMN current_run INTEGER NOT NULL DEFAULT 0;
ALTER TABLE usage_lifetime ADD COLUMN longest_run INTEGER NOT NULL DEFAULT 0;
WITH active AS (SELECT day, julianday(day)-ROW_NUMBER() OVER (ORDER BY day) AS run FROM daily_usage WHERE dictations>0)
UPDATE usage_lifetime SET
  active_days=(SELECT COUNT(*) FROM active),
  last_day=(SELECT MAX(day) FROM active),
  current_run=(SELECT COUNT(*) FROM active WHERE run=(SELECT run FROM active ORDER BY day DESC LIMIT 1)),
  longest_run=(SELECT COALESCE(MAX(length),0) FROM (SELECT COUNT(*) AS length FROM active GROUP BY run))
WHERE id=1;
PRAGMA user_version=5;
