CREATE TABLE conversations_v2 (
    key TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    value TEXT NOT NULL,
    created_at INTEGER NOT NULL,  -- Unix timestamp in milliseconds
    updated_at INTEGER NOT NULL,  -- Unix timestamp in milliseconds
    PRIMARY KEY (key, conversation_id)
);

CREATE INDEX idx_conversations_v2_key_updated ON conversations_v2(key, updated_at DESC);
CREATE INDEX idx_conversations_v2_updated_at ON conversations_v2(updated_at DESC);

INSERT INTO conversations_v2 (key, conversation_id, value, created_at, updated_at)
SELECT 
    key,
    json_extract(value, '$.conversation_id') as conversation_id,
    value,
    CAST(strftime('%s', 'now') AS INTEGER) * 1000 as created_at,
    CAST(strftime('%s', 'now') AS INTEGER) * 1000 as updated_at
FROM conversations;
