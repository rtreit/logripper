-- Soft-delete columns for the qsos table. Applied conditionally by
-- the storage builder via pragma_table_info checks (SQLite has no
-- ALTER TABLE ADD COLUMN IF NOT EXISTS).
ALTER TABLE qsos ADD COLUMN deleted_at_ms INTEGER;
ALTER TABLE qsos ADD COLUMN pending_remote_delete INTEGER NOT NULL DEFAULT 0;
CREATE INDEX IF NOT EXISTS idx_qsos_deleted_at_ms ON qsos (deleted_at_ms);
