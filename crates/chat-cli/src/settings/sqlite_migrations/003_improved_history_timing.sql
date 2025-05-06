ALTER TABLE history RENAME COLUMN time TO start_time;
ALTER TABLE history ADD COLUMN end_time INTEGER;
ALTER TABLE history ADD COLUMN duration INTEGER;
