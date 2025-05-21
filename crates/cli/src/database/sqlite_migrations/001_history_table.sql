CREATE TABLE IF NOT EXISTS history (
    id INTEGER PRIMARY KEY,
    command TEXT,
    shell TEXT,
    pid INTEGER,
    session_id TEXT,
    cwd TEXT,
    time INTEGER,
    in_ssh INTEGER,
    in_docker INTEGER,
    hostname TEXT,
    exit_code INTEGER
);
