-- Phase 2.4: GSTR-2B statements and reconciliation decisions.
--
-- One statement per GSTIN and month; importing again replaces its lines.
-- Decisions are keyed by a stable document key (supplier GSTIN, kind and
-- normalised number, or the purchase id for book-only rows), so they survive
-- a re-import.

CREATE TABLE gstr2b_statement (
    id                   TEXT PRIMARY KEY,
    organisation_id      TEXT NOT NULL REFERENCES organisation(id),
    gst_registration_id  TEXT NOT NULL REFERENCES gst_registration(id),
    period               TEXT NOT NULL,          -- YYYY-MM
    gstin                TEXT NOT NULL,
    generated_on         TEXT,
    format_version       TEXT,
    adapter_version      TEXT NOT NULL,
    file_name            TEXT NOT NULL,
    sha256               TEXT NOT NULL,
    skipped_summary      TEXT,                   -- e.g. "IMPG 2, ISD 1"
    imported_at          TEXT NOT NULL,
    imported_by          TEXT
);
CREATE UNIQUE INDEX ux_gstr2b_statement_period
    ON gstr2b_statement (organisation_id, gst_registration_id, period);

CREATE TABLE gstr2b_line (
    id                 TEXT PRIMARY KEY,
    statement_id       TEXT NOT NULL REFERENCES gstr2b_statement(id) ON DELETE CASCADE,
    line_no            INTEGER NOT NULL,
    section            TEXT NOT NULL CHECK (section IN ('b2b', 'b2ba', 'cdnr', 'cdnra')),
    supplier_gstin     TEXT NOT NULL,
    supplier_name      TEXT NOT NULL,
    doc_kind           TEXT NOT NULL CHECK (doc_kind IN ('invoice', 'credit_note', 'debit_note')),
    doc_number         TEXT NOT NULL,
    doc_date           TEXT NOT NULL,
    value_paise        INTEGER NOT NULL,
    taxable_paise      INTEGER NOT NULL,
    igst_paise         INTEGER NOT NULL,
    cgst_paise         INTEGER NOT NULL,
    sgst_paise         INTEGER NOT NULL,
    cess_paise         INTEGER NOT NULL,
    place_of_supply    TEXT,
    reverse_charge     INTEGER NOT NULL CHECK (reverse_charge IN (0, 1)),
    itc_available      INTEGER NOT NULL CHECK (itc_available IN (0, 1)),
    itc_reason         TEXT,
    supplier_filed_on  TEXT,
    supplier_period    TEXT,
    original_number    TEXT
);
CREATE INDEX idx_gstr2b_line_statement ON gstr2b_line (statement_id, line_no);

CREATE TABLE gstr2b_decision (
    id                   TEXT PRIMARY KEY,
    organisation_id      TEXT NOT NULL REFERENCES organisation(id),
    gst_registration_id  TEXT NOT NULL REFERENCES gst_registration(id),
    period               TEXT NOT NULL,          -- the statement month it was made in
    row_key              TEXT NOT NULL,          -- '2b:<gstin>|<kind>|<number>' or 'book:<purchase id>'
    purchase_id          TEXT REFERENCES purchase_document(id),
    decision             TEXT NOT NULL CHECK (decision IN ('accepted', 'deferred')),
    note                 TEXT,
    decided_at           TEXT NOT NULL,
    decided_by           TEXT
);
CREATE UNIQUE INDEX ux_gstr2b_decision
    ON gstr2b_decision (organisation_id, gst_registration_id, period, row_key);
