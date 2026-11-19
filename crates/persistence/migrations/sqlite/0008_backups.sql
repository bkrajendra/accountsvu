-- Phase 1.7: backup history (last-backup indicator, restore record).
-- Backups themselves live outside the database, encrypted with a passphrase.
CREATE TABLE backup_log (
    id              TEXT PRIMARY KEY,
    organisation_id TEXT NOT NULL REFERENCES organisation(id),
    kind            TEXT NOT NULL CHECK (kind IN ('backup', 'restore')),
    happened_at     TEXT NOT NULL,
    file_name       TEXT NOT NULL,
    size_bytes      INTEGER NOT NULL CHECK (size_bytes >= 0),
    sha256          TEXT,
    actor           TEXT
);
CREATE INDEX idx_backup_log_org ON backup_log (organisation_id, kind, happened_at);
