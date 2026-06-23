CREATE TABLE extracted_kas_versions (
    version TEXT PRIMARY KEY,
    last_used_at INTEGER NOT NULL -- Unix timestamp in milliseconds
);
