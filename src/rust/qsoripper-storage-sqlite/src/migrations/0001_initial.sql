CREATE TABLE IF NOT EXISTS qsos (
    local_id TEXT PRIMARY KEY NOT NULL,
    qrz_logid TEXT,
    qrz_bookid TEXT,
    station_callsign TEXT NOT NULL,
    worked_callsign TEXT NOT NULL,
    utc_timestamp_ms INTEGER,
    band INTEGER NOT NULL,
    mode INTEGER NOT NULL,
    contest_id TEXT,
    created_at_ms INTEGER,
    updated_at_ms INTEGER,
    sync_status INTEGER NOT NULL,
    record BLOB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_qsos_station_callsign ON qsos (station_callsign);
CREATE INDEX IF NOT EXISTS idx_qsos_worked_callsign ON qsos (worked_callsign);
CREATE INDEX IF NOT EXISTS idx_qsos_utc_timestamp_ms ON qsos (utc_timestamp_ms);
CREATE INDEX IF NOT EXISTS idx_qsos_band ON qsos (band);
CREATE INDEX IF NOT EXISTS idx_qsos_mode ON qsos (mode);
CREATE INDEX IF NOT EXISTS idx_qsos_contest_id ON qsos (contest_id);
CREATE INDEX IF NOT EXISTS idx_qsos_sync_status ON qsos (sync_status);

CREATE TABLE IF NOT EXISTS sync_metadata (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    qrz_qso_count INTEGER NOT NULL DEFAULT 0,
    last_sync_ms INTEGER,
    qrz_logbook_owner TEXT
);

INSERT OR IGNORE INTO sync_metadata (id, qrz_qso_count) VALUES (1, 0);

CREATE TABLE IF NOT EXISTS lookup_snapshots (
    callsign TEXT PRIMARY KEY NOT NULL,
    result BLOB NOT NULL,
    stored_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER
);
