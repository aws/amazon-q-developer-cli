ALTER TABLE state RENAME TO state_old;
CREATE TABLE state (
    key TEXT PRIMARY KEY,
    value BLOB
);
INSERT INTO state SELECT key, value FROM state_old;
DROP TABLE state_old;