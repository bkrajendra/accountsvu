-- Phase 1.3: sales documents (tax invoices, bills of supply, credit notes).
-- A document is editable only while it is a draft. Once issued, its number,
-- party, tax and lines are frozen (also enforced by triggers below);
-- corrections are made with a credit note or a cancellation.

CREATE TABLE sales_document (
    id                    TEXT PRIMARY KEY,
    organisation_id       TEXT NOT NULL REFERENCES organisation(id),
    doc_type              TEXT NOT NULL CHECK (doc_type IN ('tax_invoice', 'bill_of_supply', 'credit_note')),
    status                TEXT NOT NULL CHECK (status IN ('draft', 'issued', 'cancelled')),
    gst_registration_id   TEXT REFERENCES gst_registration(id),
    series_id             TEXT REFERENCES invoice_series(id),
    financial_year        TEXT,
    number                TEXT,
    doc_date              TEXT NOT NULL,
    due_date              TEXT,
    -- Party snapshot at the time of the document (spec §5.4 tax snapshot).
    party_id              TEXT NOT NULL REFERENCES party(id),
    party_name            TEXT NOT NULL,
    party_gstin           TEXT,
    party_gst_type        TEXT NOT NULL,
    party_address         TEXT,
    place_of_supply       INTEGER NOT NULL,
    supplier_state        INTEGER NOT NULL,
    supply_kind           TEXT NOT NULL CHECK (supply_kind IN ('intra_state', 'intra_union_territory', 'inter_state')),
    zero_rated            INTEGER NOT NULL DEFAULT 0,
    reverse_charge        INTEGER NOT NULL DEFAULT 0,
    prices_include_tax    INTEGER NOT NULL DEFAULT 0,
    original_document_id  TEXT REFERENCES sales_document(id),
    reason                TEXT,
    notes                 TEXT,
    taxable_paise         INTEGER NOT NULL DEFAULT 0,
    cgst_paise            INTEGER NOT NULL DEFAULT 0,
    sgst_paise            INTEGER NOT NULL DEFAULT 0,
    utgst_paise           INTEGER NOT NULL DEFAULT 0,
    igst_paise            INTEGER NOT NULL DEFAULT 0,
    cess_paise            INTEGER NOT NULL DEFAULT 0,
    round_off_paise       INTEGER NOT NULL DEFAULT 0,
    total_paise           INTEGER NOT NULL DEFAULT 0,
    paid_paise            INTEGER NOT NULL DEFAULT 0,   -- receipts arrive in Phase 1.5
    journal_entry_id      TEXT REFERENCES journal_entry(id),
    cancel_entry_id       TEXT REFERENCES journal_entry(id),
    cancel_reason         TEXT,
    row_version           INTEGER NOT NULL DEFAULT 1,
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL,
    issued_at             TEXT,
    cancelled_at          TEXT,
    CHECK (status = 'draft' OR (number IS NOT NULL AND journal_entry_id IS NOT NULL)),
    CHECK (status <> 'cancelled' OR (cancel_reason IS NOT NULL AND cancel_entry_id IS NOT NULL))
);
-- A number is used once per GSTIN and financial year (Rule 46).
CREATE UNIQUE INDEX ux_sales_number
    ON sales_document (organisation_id, IFNULL(gst_registration_id, ''), financial_year, number)
    WHERE number IS NOT NULL;
CREATE INDEX idx_sales_party ON sales_document (organisation_id, party_id);
CREATE INDEX idx_sales_date ON sales_document (organisation_id, doc_date);
CREATE INDEX idx_sales_original ON sales_document (original_document_id) WHERE original_document_id IS NOT NULL;

CREATE TABLE sales_line (
    id                    TEXT PRIMARY KEY,
    document_id           TEXT NOT NULL REFERENCES sales_document(id),
    line_no               INTEGER NOT NULL,
    item_id               TEXT REFERENCES item(id),
    description           TEXT NOT NULL,
    hsn_sac               TEXT,
    unit                  TEXT,
    quantity              TEXT NOT NULL,              -- exact decimal, e.g. '2.5'
    unit_price_paise      INTEGER NOT NULL,
    discount_paise        INTEGER NOT NULL DEFAULT 0,
    tax_category          TEXT NOT NULL,
    gst_rate_bp           INTEGER NOT NULL DEFAULT 0,
    cess_rate_bp          INTEGER NOT NULL DEFAULT 0,
    amount_paise          INTEGER NOT NULL,
    taxable_paise         INTEGER NOT NULL,
    cgst_paise            INTEGER NOT NULL DEFAULT 0,
    sgst_paise            INTEGER NOT NULL DEFAULT 0,
    utgst_paise           INTEGER NOT NULL DEFAULT 0,
    igst_paise            INTEGER NOT NULL DEFAULT 0,
    cess_paise            INTEGER NOT NULL DEFAULT 0,
    total_paise           INTEGER NOT NULL,
    UNIQUE (document_id, line_no)
);

-- No silent edits: once issued, only status, cancellation and payment fields
-- may change.
CREATE TRIGGER sales_document_frozen BEFORE UPDATE ON sales_document
WHEN OLD.status <> 'draft' AND (
       NEW.doc_type IS NOT OLD.doc_type OR NEW.number IS NOT OLD.number OR NEW.doc_date IS NOT OLD.doc_date
    OR NEW.party_id IS NOT OLD.party_id OR NEW.party_name IS NOT OLD.party_name OR NEW.party_gstin IS NOT OLD.party_gstin
    OR NEW.place_of_supply IS NOT OLD.place_of_supply OR NEW.supply_kind IS NOT OLD.supply_kind
    OR NEW.taxable_paise IS NOT OLD.taxable_paise OR NEW.total_paise IS NOT OLD.total_paise
    OR NEW.cgst_paise IS NOT OLD.cgst_paise OR NEW.sgst_paise IS NOT OLD.sgst_paise OR NEW.utgst_paise IS NOT OLD.utgst_paise
    OR NEW.igst_paise IS NOT OLD.igst_paise OR NEW.cess_paise IS NOT OLD.cess_paise OR NEW.round_off_paise IS NOT OLD.round_off_paise
    OR NEW.journal_entry_id IS NOT OLD.journal_entry_id OR NEW.original_document_id IS NOT OLD.original_document_id
    OR NEW.status = 'draft')
BEGIN SELECT RAISE(ABORT, 'issued documents cannot be edited'); END;

CREATE TRIGGER sales_document_no_delete BEFORE DELETE ON sales_document
WHEN OLD.status <> 'draft'
BEGIN SELECT RAISE(ABORT, 'issued documents cannot be deleted'); END;

CREATE TRIGGER sales_line_frozen_update BEFORE UPDATE ON sales_line
WHEN (SELECT status FROM sales_document WHERE id = OLD.document_id) <> 'draft'
BEGIN SELECT RAISE(ABORT, 'lines of an issued document cannot be edited'); END;

CREATE TRIGGER sales_line_frozen_delete BEFORE DELETE ON sales_line
WHEN (SELECT status FROM sales_document WHERE id = OLD.document_id) <> 'draft'
BEGIN SELECT RAISE(ABORT, 'lines of an issued document cannot be deleted'); END;

CREATE TRIGGER sales_line_frozen_insert BEFORE INSERT ON sales_line
WHEN (SELECT status FROM sales_document WHERE id = NEW.document_id) <> 'draft'
BEGIN SELECT RAISE(ABORT, 'lines cannot be added to an issued document'); END;
