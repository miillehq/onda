CREATE TABLE IF NOT EXISTS history(id TEXT PRIMARY KEY NOT NULL, created_at INTEGER NOT NULL, payload BLOB NOT NULL);
CREATE INDEX IF NOT EXISTS history_created_at ON history(created_at);
PRAGMA user_version=3;
