-- Phase 2.5: offline filing records and the GST period lock.

-- A return filed outside the app (offline tool or portal), recorded with its
-- ARN. While a record is `filed`, documents of that GSTIN dated in the month
-- can't be issued, recorded or cancelled. Removing a record needs the owner
-- and a reason; the row stays as `removed` for the audit trail.
CREATE TABLE gst_filing (
    id                    TEXT PRIMARY KEY,
    organisation_id       TEXT NOT NULL REFERENCES organisation(id),
    gst_registration_id   TEXT NOT NULL REFERENCES gst_registration(id),
    return_type           TEXT NOT NULL CHECK (return_type IN ('gstr1', 'gstr3b')),
    period                TEXT NOT NULL,                 -- YYYY-MM
    status                TEXT NOT NULL CHECK (status IN ('filed', 'removed')),
    channel               TEXT NOT NULL CHECK (channel IN ('offline')),
    arn                   TEXT NOT NULL,
    filed_on              TEXT NOT NULL,
    note                  TEXT,
    return_pack_version   TEXT NOT NULL,
    rule_pack_version     TEXT NOT NULL,
    -- Headline figures as filed (JSON) and the SHA-256 of the full workpaper.
    summary_json          TEXT NOT NULL,
    workpaper_sha256      TEXT NOT NULL,
    warning_count         INTEGER NOT NULL,
    recorded_at           TEXT NOT NULL,
    recorded_by           TEXT NOT NULL,
    removed_reason        TEXT,
    removed_at            TEXT,
    removed_by            TEXT
);
CREATE UNIQUE INDEX ux_gst_filing_filed
    ON gst_filing (organisation_id, gst_registration_id, return_type, period) WHERE status = 'filed';
CREATE INDEX ix_gst_filing_reg ON gst_filing (organisation_id, gst_registration_id, period);

-- Files frozen with a filing: the portal file, the workpaper and the
-- acknowledgement. Content lives in attachment_blob, so backups carry it.
CREATE TABLE gst_filing_file (
    id                    TEXT PRIMARY KEY,
    filing_id             TEXT NOT NULL REFERENCES gst_filing(id),
    kind                  TEXT NOT NULL CHECK (kind IN ('portal_json', 'workpaper_json', 'workpaper_xlsx', 'acknowledgement')),
    file_name             TEXT NOT NULL,
    content_type          TEXT NOT NULL,
    size_bytes            INTEGER NOT NULL,
    sha256                TEXT NOT NULL REFERENCES attachment_blob(sha256)
);
CREATE INDEX ix_gst_filing_file ON gst_filing_file (filing_id);

-- ITC reviewed after the document's GSTR-3B was filed is claimed in a later
-- open month. NULL: the month of the document.
ALTER TABLE purchase_line ADD COLUMN itc_period TEXT;

-- The database refuses issuing, cancelling or re-dating sales documents in a
-- month whose GSTR-1 or GSTR-3B is filed, whatever the caller.
CREATE TRIGGER sales_document_gst_filed_insert BEFORE INSERT ON sales_document
WHEN NEW.status <> 'draft' AND NEW.gst_registration_id IS NOT NULL AND EXISTS (
    SELECT 1 FROM gst_filing f WHERE f.gst_registration_id = NEW.gst_registration_id
      AND f.period = substr(NEW.doc_date, 1, 7) AND f.status = 'filed')
BEGIN SELECT RAISE(ABORT, 'the GST return for this month is filed'); END;

CREATE TRIGGER sales_document_gst_filed_update BEFORE UPDATE OF status, doc_date, gst_registration_id ON sales_document
WHEN (NEW.status <> OLD.status OR (NEW.status <> 'draft' AND (NEW.doc_date <> OLD.doc_date
        OR IFNULL(NEW.gst_registration_id, '') <> IFNULL(OLD.gst_registration_id, ''))))
  AND EXISTS (
    SELECT 1 FROM gst_filing f WHERE f.status = 'filed' AND (
         (f.gst_registration_id = NEW.gst_registration_id AND f.period = substr(NEW.doc_date, 1, 7) AND NEW.status <> 'draft')
      OR (f.gst_registration_id = OLD.gst_registration_id AND f.period = substr(OLD.doc_date, 1, 7) AND OLD.status <> 'draft')))
BEGIN SELECT RAISE(ABORT, 'the GST return for this month is filed'); END;

-- Purchases are locked by a filed GSTR-3B.
CREATE TRIGGER purchase_document_gst_filed_insert BEFORE INSERT ON purchase_document
WHEN NEW.status <> 'draft' AND NEW.gst_registration_id IS NOT NULL AND EXISTS (
    SELECT 1 FROM gst_filing f WHERE f.gst_registration_id = NEW.gst_registration_id AND f.return_type = 'gstr3b'
      AND f.period = substr(NEW.doc_date, 1, 7) AND f.status = 'filed')
BEGIN SELECT RAISE(ABORT, 'the GST return for this month is filed'); END;

CREATE TRIGGER purchase_document_gst_filed_update BEFORE UPDATE OF status, doc_date, gst_registration_id ON purchase_document
WHEN (NEW.status <> OLD.status OR (NEW.status <> 'draft' AND (NEW.doc_date <> OLD.doc_date
        OR IFNULL(NEW.gst_registration_id, '') <> IFNULL(OLD.gst_registration_id, ''))))
  AND EXISTS (
    SELECT 1 FROM gst_filing f WHERE f.status = 'filed' AND f.return_type = 'gstr3b' AND (
         (f.gst_registration_id = NEW.gst_registration_id AND f.period = substr(NEW.doc_date, 1, 7) AND NEW.status <> 'draft')
      OR (f.gst_registration_id = OLD.gst_registration_id AND f.period = substr(OLD.doc_date, 1, 7) AND OLD.status <> 'draft')))
BEGIN SELECT RAISE(ABORT, 'the GST return for this month is filed'); END;
