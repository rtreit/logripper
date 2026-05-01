CREATE INDEX IF NOT EXISTS idx_qsos_worked_callsign_upper
    ON qsos (UPPER(worked_callsign));
