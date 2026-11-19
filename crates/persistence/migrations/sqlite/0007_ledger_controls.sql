-- Phase 1.6: period lock, financial-year close and the business logo.

-- Books are locked up to and including `locked_until`: nothing may be posted
-- on or before that date. Reopening needs the owner and a reason (audited).
CREATE TABLE books_lock (
    organisation_id     TEXT PRIMARY KEY REFERENCES organisation(id),
    locked_until        TEXT NOT NULL,
    reason              TEXT,
    updated_at          TEXT NOT NULL,
    updated_by          TEXT NOT NULL
);

-- The database refuses postings into a locked period, whatever the caller.
CREATE TRIGGER journal_entry_period_locked BEFORE INSERT ON journal_entry
WHEN NEW.entry_date <= IFNULL((SELECT locked_until FROM books_lock WHERE organisation_id = NEW.organisation_id), '')
BEGIN SELECT RAISE(ABORT, 'the period is locked'); END;

-- Year-end close: income and expense balances moved to retained earnings by
-- one entry dated the last day of the year. Reopening reverses that entry.
CREATE TABLE fy_close (
    id                  TEXT PRIMARY KEY,
    organisation_id     TEXT NOT NULL REFERENCES organisation(id),
    financial_year      TEXT NOT NULL,
    start_date          TEXT NOT NULL,
    end_date            TEXT NOT NULL,
    status              TEXT NOT NULL CHECK (status IN ('closed', 'reopened')),
    entry_id            TEXT REFERENCES journal_entry(id),
    profit_paise        INTEGER NOT NULL,
    closed_at           TEXT NOT NULL,
    closed_by           TEXT NOT NULL,
    reopen_entry_id     TEXT REFERENCES journal_entry(id),
    reopen_reason       TEXT,
    reopened_at         TEXT,
    reopened_by         TEXT
);
CREATE UNIQUE INDEX ux_fy_close_open ON fy_close (organisation_id, financial_year) WHERE status = 'closed';

-- Logo printed on invoices. Content lives in attachment_blob (by SHA-256),
-- so backups carry it like any attachment.
CREATE TABLE organisation_logo (
    organisation_id     TEXT PRIMARY KEY REFERENCES organisation(id),
    sha256              TEXT NOT NULL REFERENCES attachment_blob(sha256),
    file_name           TEXT NOT NULL,
    content_type        TEXT NOT NULL,
    size_bytes          INTEGER NOT NULL,
    updated_at          TEXT NOT NULL,
    updated_by          TEXT NOT NULL
);
