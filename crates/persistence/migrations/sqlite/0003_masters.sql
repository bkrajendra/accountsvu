-- Phase 1.2: masters — parties (customers/suppliers) and items/services.

CREATE TABLE party (
    id                    TEXT PRIMARY KEY,
    organisation_id       TEXT NOT NULL REFERENCES organisation(id),
    kind                  TEXT NOT NULL CHECK (kind IN ('customer', 'supplier', 'both')),
    name                  TEXT NOT NULL,              -- display name used on documents
    legal_name            TEXT,
    gst_type              TEXT NOT NULL CHECK (gst_type IN ('regular', 'composition', 'unregistered', 'consumer', 'overseas', 'sez')),
    gstin                 TEXT,
    pan                   TEXT,
    state_code            INTEGER NOT NULL,           -- default place of supply (96 = other country)
    address_line1         TEXT,
    address_line2         TEXT,
    city                  TEXT,
    pincode               TEXT,
    shipping_same         INTEGER NOT NULL DEFAULT 1,
    shipping_line1        TEXT,
    shipping_line2        TEXT,
    shipping_city         TEXT,
    shipping_pincode      TEXT,
    shipping_state_code   INTEGER,
    contact_person        TEXT,
    email                 TEXT,
    phone                 TEXT,
    payment_terms_days    INTEGER CHECK (payment_terms_days IS NULL OR payment_terms_days BETWEEN 0 AND 365),
    credit_limit_paise    INTEGER CHECK (credit_limit_paise IS NULL OR credit_limit_paise >= 0),
    -- Signed: positive = they owe us (debit), negative = we owe them (credit).
    opening_balance_paise INTEGER NOT NULL DEFAULT 0,
    opening_entry_id      TEXT REFERENCES journal_entry(id),
    notes                 TEXT,
    is_active             INTEGER NOT NULL DEFAULT 1,
    row_version           INTEGER NOT NULL DEFAULT 1,
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL
);
CREATE UNIQUE INDEX ux_party_name ON party (organisation_id, name COLLATE NOCASE);
CREATE UNIQUE INDEX ux_party_gstin ON party (organisation_id, gstin) WHERE gstin IS NOT NULL;

CREATE TABLE item (
    id                    TEXT PRIMARY KEY,
    organisation_id       TEXT NOT NULL REFERENCES organisation(id),
    kind                  TEXT NOT NULL CHECK (kind IN ('goods', 'service')),
    name                  TEXT NOT NULL,
    code                  TEXT,                       -- SKU / item code
    description           TEXT,
    hsn_sac               TEXT,
    unit                  TEXT NOT NULL,              -- GST UQC, e.g. NOS, KGS
    tax_category          TEXT NOT NULL CHECK (tax_category IN ('taxable', 'exempt', 'nil_rated', 'non_gst')),
    gst_rate_bp           INTEGER NOT NULL DEFAULT 0 CHECK (gst_rate_bp BETWEEN 0 AND 10000),
    cess_rate_bp          INTEGER NOT NULL DEFAULT 0 CHECK (cess_rate_bp BETWEEN 0 AND 50000),
    sales_price_paise     INTEGER CHECK (sales_price_paise IS NULL OR sales_price_paise >= 0),
    purchase_price_paise  INTEGER CHECK (purchase_price_paise IS NULL OR purchase_price_paise >= 0),
    price_includes_tax    INTEGER NOT NULL DEFAULT 0,
    track_stock           INTEGER NOT NULL DEFAULT 0,
    is_active             INTEGER NOT NULL DEFAULT 1,
    row_version           INTEGER NOT NULL DEFAULT 1,
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL
);
CREATE UNIQUE INDEX ux_item_name ON item (organisation_id, name COLLATE NOCASE);
CREATE UNIQUE INDEX ux_item_code ON item (organisation_id, code COLLATE NOCASE) WHERE code IS NOT NULL;

-- Party sub-ledger: receivable/payable lines name the party they belong to.
ALTER TABLE journal_line ADD COLUMN party_id TEXT REFERENCES party(id);
CREATE INDEX idx_journal_line_party ON journal_line (party_id) WHERE party_id IS NOT NULL;
CREATE INDEX idx_journal_entry_org_source ON journal_entry (organisation_id, source_type, source_id);
