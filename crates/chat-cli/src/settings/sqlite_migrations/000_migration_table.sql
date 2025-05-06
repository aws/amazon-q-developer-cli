CREATE TABLE IF NOT EXISTS migrations (
    id INTEGER PRIMARY KEY,
    version INTEGER NOT NULL,
    migration_time INTEGER NOT NULL
);