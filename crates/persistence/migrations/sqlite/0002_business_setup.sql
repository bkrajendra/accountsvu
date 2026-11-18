-- Phase 1.1: business setup, GST registrations, document series.
-- row_version supports optimistic concurrency on edits (spec §7.5).

ALTER TABLE organisation RENAME COLUMN address TO address_line1;
ALTER TABLE organisation ADD COLUMN address_line2 TEXT;
ALTER TABLE organisation ADD COLUMN city TEXT;
ALTER TABLE organisation ADD COLUMN pincode TEXT;
ALTER TABLE organisation ADD COLUMN email TEXT;
ALTER TABLE organisation ADD COLUMN phone TEXT;
ALTER TABLE organisation ADD COLUMN books_start_date TEXT NOT NULL DEFAULT '1970-01-01';
ALTER TABLE organisation ADD COLUMN row_version INTEGER NOT NULL DEFAULT 1;

ALTER TABLE gst_registration ADD COLUMN row_version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE gst_registration ADD COLUMN updated_at TEXT;

CREATE TABLE invoice_series (
    id                  TEXT PRIMARY KEY,
    organisation_id     TEXT NOT NULL REFERENCES organisation(id),
    gst_registration_id TEXT REFERENCES gst_registration(id),  -- NULL for unregistered businesses
    document_type       TEXT NOT NULL,
    financial_year      TEXT NOT NULL,                         -- e.g. '2026-27'
    prefix              TEXT NOT NULL DEFAULT '',
    suffix              TEXT NOT NULL DEFAULT '',
    padding             INTEGER NOT NULL DEFAULT 4 CHECK (padding BETWEEN 1 AND 8),
    next_number         INTEGER NOT NULL DEFAULT 1 CHECK (next_number >= 1),
    is_default          INTEGER NOT NULL DEFAULT 0,
    row_version         INTEGER NOT NULL DEFAULT 1,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

-- Two series in the same GSTIN + FY must never be able to produce the same
-- number, whatever their document type (GST Rule 46: unique per FY).
CREATE UNIQUE INDEX ux_invoice_series_scope
    ON invoice_series (organisation_id, IFNULL(gst_registration_id, ''), financial_year, prefix, suffix);

-- At most one default series per GSTIN, document type and FY.
CREATE UNIQUE INDEX ux_invoice_series_default
    ON invoice_series (organisation_id, IFNULL(gst_registration_id, ''), document_type, financial_year)
    WHERE is_default = 1;

CREATE INDEX idx_audit_occurred ON audit_event (occurred_at DESC);
