-- We create a separate auth_kv to ensure the data is not available in all the same
-- places that the state is available in
CREATE TABLE auth_kv (
    key TEXT PRIMARY KEY,
    value TEXT
);
