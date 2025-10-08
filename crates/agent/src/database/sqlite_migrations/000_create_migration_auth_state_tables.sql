CREATE TABLE IF NOT EXISTS migrations (
    version INTEGER PRIMARY KEY,
    migration_time INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS auth_kv (
    key TEXT PRIMARY KEY,
    value TEXT
);

CREATE TABLE IF NOT EXISTS state (
    key TEXT PRIMARY KEY,
    value TEXT
);
