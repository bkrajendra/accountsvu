-- Phase 1.4: purchases (supplier bills, debit notes, expenses), input tax
-- credit review, and attachments.

CREATE TABLE purchase_document (
    id                       TEXT PRIMARY KEY,
    organisation_id          TEXT NOT NULL REFERENCES organisation(id),
    doc_type                 TEXT NOT NULL CHECK (doc_type IN ('bill', 'debit_note', 'expense')),
    status                   TEXT NOT NULL CHECK (status IN ('draft', 'recorded', 'cancelled')),
    gst_registration_id      TEXT REFERENCES gst_registration(id),
    -- Our own number: debit notes only (we issue them).
    series_id                TEXT REFERENCES invoice_series(id),
    number                   TEXT,
    financial_year           TEXT,
    supplier_invoice_number  TEXT,
    supplier_invoice_date    TEXT,
    doc_date                 TEXT NOT NULL,              -- date recorded / goods received
    due_date                 TEXT,
    party_id                 TEXT REFERENCES party(id),  -- optional for expenses paid on the spot
    party_name               TEXT,
    party_gstin              TEXT,
    party_gst_type           TEXT,
    place_of_supply          INTEGER NOT NULL,
    supplier_state           INTEGER NOT NULL,
    supply_kind              TEXT NOT NULL CHECK (supply_kind IN ('intra_state', 'intra_union_territory', 'inter_state')),
    reverse_charge           INTEGER NOT NULL DEFAULT 0,
    prices_include_tax       INTEGER NOT NULL DEFAULT 0,
    payment_mode             TEXT NOT NULL CHECK (payment_mode IN ('credit', 'cash', 'bank')),
    original_document_id     TEXT REFERENCES purchase_document(id),
    reason                   TEXT,
    notes                    TEXT,
    taxable_paise            INTEGER NOT NULL DEFAULT 0,
    cgst_paise               INTEGER NOT NULL DEFAULT 0,
    sgst_paise               INTEGER NOT NULL DEFAULT 0,
    utgst_paise              INTEGER NOT NULL DEFAULT 0,
    igst_paise               INTEGER NOT NULL DEFAULT 0,
    cess_paise               INTEGER NOT NULL DEFAULT 0,
    round_off_paise          INTEGER NOT NULL DEFAULT 0,
    -- What the supplier is owed (excludes tax under reverse charge).
    total_paise              INTEGER NOT NULL DEFAULT 0,
    paid_paise               INTEGER NOT NULL DEFAULT 0,
    journal_entry_id         TEXT REFERENCES journal_entry(id),
    cancel_entry_id          TEXT REFERENCES journal_entry(id),
    cancel_reason            TEXT,
    row_version              INTEGER NOT NULL DEFAULT 1,
    created_at               TEXT NOT NULL,
    updated_at               TEXT NOT NULL,
    recorded_at              TEXT,
    cancelled_at             TEXT,
    CHECK (status = 'draft' OR journal_entry_id IS NOT NULL),
    CHECK (status <> 'cancelled' OR (cancel_reason IS NOT NULL AND cancel_entry_id IS NOT NULL)),
    CHECK (payment_mode <> 'credit' OR party_id IS NOT NULL)
);
-- The same supplier invoice can't be recorded twice in a financial year.
CREATE UNIQUE INDEX ux_purchase_supplier_invoice
    ON purchase_document (organisation_id, party_id, UPPER(supplier_invoice_number), financial_year)
    WHERE status = 'recorded' AND supplier_invoice_number IS NOT NULL AND doc_type <> 'debit_note';
CREATE UNIQUE INDEX ux_purchase_number
    ON purchase_document (organisation_id, IFNULL(gst_registration_id, ''), financial_year, number)
    WHERE number IS NOT NULL;
CREATE INDEX idx_purchase_party ON purchase_document (organisation_id, party_id);
CREATE INDEX idx_purchase_date ON purchase_document (organisation_id, doc_date);

CREATE TABLE purchase_line (
    id                    TEXT PRIMARY KEY,
    document_id           TEXT NOT NULL REFERENCES purchase_document(id),
    line_no               INTEGER NOT NULL,
    item_id               TEXT REFERENCES item(id),
    account_id            TEXT NOT NULL REFERENCES account(id),  -- what it's booked to
    description           TEXT NOT NULL,
    hsn_sac               TEXT,
    unit                  TEXT,
    quantity              TEXT NOT NULL,
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
    -- Input tax credit is never inferred: every taxed line carries a status.
    itc_status            TEXT NOT NULL CHECK (itc_status IN ('na', 'pending', 'eligible', 'ineligible', 'partial')),
    itc_reason            TEXT,
    itc_eligible_paise    INTEGER NOT NULL DEFAULT 0,
    itc_reviewed_at       TEXT,
    itc_reviewed_by       TEXT,
    UNIQUE (document_id, line_no)
);

CREATE TRIGGER purchase_document_frozen BEFORE UPDATE ON purchase_document
WHEN OLD.status <> 'draft' AND (
       NEW.doc_type IS NOT OLD.doc_type OR NEW.number IS NOT OLD.number OR NEW.doc_date IS NOT OLD.doc_date
    OR NEW.supplier_invoice_number IS NOT OLD.supplier_invoice_number OR NEW.supplier_invoice_date IS NOT OLD.supplier_invoice_date
    OR NEW.party_id IS NOT OLD.party_id OR NEW.payment_mode IS NOT OLD.payment_mode OR NEW.reverse_charge IS NOT OLD.reverse_charge
    OR NEW.supply_kind IS NOT OLD.supply_kind OR NEW.taxable_paise IS NOT OLD.taxable_paise OR NEW.total_paise IS NOT OLD.total_paise
    OR NEW.cgst_paise IS NOT OLD.cgst_paise OR NEW.sgst_paise IS NOT OLD.sgst_paise OR NEW.utgst_paise IS NOT OLD.utgst_paise
    OR NEW.igst_paise IS NOT OLD.igst_paise OR NEW.cess_paise IS NOT OLD.cess_paise OR NEW.round_off_paise IS NOT OLD.round_off_paise
    OR NEW.journal_entry_id IS NOT OLD.journal_entry_id OR NEW.original_document_id IS NOT OLD.original_document_id
    OR NEW.status = 'draft')
BEGIN SELECT RAISE(ABORT, 'recorded purchases cannot be edited'); END;

CREATE TRIGGER purchase_document_no_delete BEFORE DELETE ON purchase_document
WHEN OLD.status <> 'draft'
BEGIN SELECT RAISE(ABORT, 'recorded purchases cannot be deleted'); END;

-- Recorded lines: only the ITC review fields may change.
CREATE TRIGGER purchase_line_frozen_update BEFORE UPDATE ON purchase_line
WHEN (SELECT status FROM purchase_document WHERE id = OLD.document_id) <> 'draft' AND (
       NEW.account_id IS NOT OLD.account_id OR NEW.description IS NOT OLD.description OR NEW.quantity IS NOT OLD.quantity
    OR NEW.unit_price_paise IS NOT OLD.unit_price_paise OR NEW.discount_paise IS NOT OLD.discount_paise
    OR NEW.taxable_paise IS NOT OLD.taxable_paise OR NEW.total_paise IS NOT OLD.total_paise OR NEW.gst_rate_bp IS NOT OLD.gst_rate_bp
    OR NEW.cgst_paise IS NOT OLD.cgst_paise OR NEW.sgst_paise IS NOT OLD.sgst_paise OR NEW.utgst_paise IS NOT OLD.utgst_paise
    OR NEW.igst_paise IS NOT OLD.igst_paise OR NEW.cess_paise IS NOT OLD.cess_paise OR NEW.line_no IS NOT OLD.line_no)
BEGIN SELECT RAISE(ABORT, 'lines of a recorded purchase cannot be edited'); END;

CREATE TRIGGER purchase_line_frozen_delete BEFORE DELETE ON purchase_line
WHEN (SELECT status FROM purchase_document WHERE id = OLD.document_id) <> 'draft'
BEGIN SELECT RAISE(ABORT, 'lines of a recorded purchase cannot be deleted'); END;

CREATE TRIGGER purchase_line_frozen_insert BEFORE INSERT ON purchase_line
WHEN (SELECT status FROM purchase_document WHERE id = NEW.document_id) <> 'draft'
BEGIN SELECT RAISE(ABORT, 'lines cannot be added to a recorded purchase'); END;

-- Attachments: file content is stored once per SHA-256 and kept in the
-- database so backups carry it. Metadata rows are append-only.
CREATE TABLE attachment_blob (
    sha256                TEXT PRIMARY KEY,
    content               BLOB NOT NULL,
    size_bytes            INTEGER NOT NULL
);

CREATE TABLE attachment (
    id                    TEXT PRIMARY KEY,
    organisation_id       TEXT NOT NULL REFERENCES organisation(id),
    entity_type           TEXT NOT NULL,
    entity_id             TEXT NOT NULL,
    file_name             TEXT NOT NULL,
    content_type          TEXT NOT NULL,
    size_bytes            INTEGER NOT NULL,
    sha256                TEXT NOT NULL REFERENCES attachment_blob(sha256),
    uploaded_at           TEXT NOT NULL,
    uploaded_by           TEXT NOT NULL
);
CREATE INDEX idx_attachment_entity ON attachment (organisation_id, entity_type, entity_id);

CREATE TRIGGER attachment_no_update BEFORE UPDATE ON attachment
BEGIN SELECT RAISE(ABORT, 'attachments cannot be changed'); END;
CREATE TRIGGER attachment_blob_no_update BEFORE UPDATE ON attachment_blob
BEGIN SELECT RAISE(ABORT, 'attachment content cannot be changed'); END;
