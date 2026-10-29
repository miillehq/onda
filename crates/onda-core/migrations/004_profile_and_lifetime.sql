CREATE TABLE IF NOT EXISTS profile(id INTEGER PRIMARY KEY CHECK(id=1), name TEXT, picture BLOB);
CREATE TABLE IF NOT EXISTS usage_lifetime(id INTEGER PRIMARY KEY CHECK(id=1), since TEXT CHECK(since IS NULL OR length(since)=10), dictations INTEGER NOT NULL DEFAULT 0, words INTEGER NOT NULL DEFAULT 0, audio_ms INTEGER NOT NULL DEFAULT 0);
INSERT OR IGNORE INTO usage_lifetime(id,since,dictations,words,audio_ms) SELECT 1,MIN(day),COALESCE(SUM(dictations),0),COALESCE(SUM(words),0),COALESCE(SUM(audio_ms),0) FROM daily_usage;
PRAGMA user_version=4;
