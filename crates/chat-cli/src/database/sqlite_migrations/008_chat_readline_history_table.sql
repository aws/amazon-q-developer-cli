CREATE TABLE chat_readline_history (
    id INTEGER PRIMARY KEY,
    input TEXT NOT NULL,
    cwd TEXT NOT NULL,
    timestamp INTEGER NOT NULL DEFAULT (unixepoch('now', 'subsec') * 1000)
);

CREATE INDEX idx_chat_readline_history_cwd_timestamp ON chat_readline_history(cwd, timestamp);
