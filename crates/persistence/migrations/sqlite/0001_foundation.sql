-- AccountsVU foundation schema (Phase 1.0)
-- Conventions:
--   * ids are UUID v7 stored as TEXT
--   * money is INTEGER paise (spec D10); rates are INTEGER basis points
--   * dates are ISO-8601 TEXT; timestamps are UTC RFC 3339 TEXT
--   * every business row carries organisation_id (cloud-ready, D2)

CREATE TABLE organisation (
    id                  TEXT PRIMARY KEY,
    legal_name          TEXT NOT NULL,
    trade_name          TEXT,
    pan                 TEXT,
    constitution        TEXT,
    address             TEXT,
    state_code          INTEGER NOT NULL,
    fy_start_month      INTEGER NOT NULL DEFAULT 4 CHECK (fy_start_month BETWEEN 1 AND 12),
    base_currency       TEXT NOT NULL DEFAULT 'INR',
    round_to_rupee      INTEGER NOT NULL DEFAULT 1,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

CREATE TABLE gst_registration (
    id                  TEXT PRIMARY KEY,
    organisation_id     TEXT NOT NULL REFERENCES organisation(id),
    gstin               TEXT NOT NULL,
    state_code          INTEGER NOT NULL,
    registration_type   TEXT NOT NULL DEFAULT 'regular',
    filing_frequency    TEXT NOT NULL DEFAULT 'monthly' CHECK (filing_frequency IN ('monthly', 'quarterly')),
    signatory_name      TEXT,
    effective_from      TEXT NOT NULL,
    effective_to        TEXT,
    status              TEXT NOT NULL DEFAULT 'active',
    created_at          TEXT NOT NULL,
    UNIQUE (organisation_id, gstin)
);

CREATE TABLE account (
    id                  TEXT PRIMARY KEY,
    organisation_id     TEXT NOT NULL REFERENCES organisation(id),
    code                TEXT NOT NULL,
    name                TEXT NOT NULL,
    kind                TEXT NOT NULL CHECK (kind IN ('asset', 'liability', 'equity', 'income', 'expense')),
    system_role         TEXT,           -- e.g. 'receivables', 'output_cgst', 'round_off'
    parent_id           TEXT REFERENCES account(id),
    is_active           INTEGER NOT NULL DEFAULT 1,
    created_at          TEXT NOT NULL,
    UNIQUE (organisation_id, code),
    UNIQUE (organisation_id, system_role)
);

CREATE TABLE journal_entry (
    id                  TEXT PRIMARY KEY,
    organisation_id     TEXT NOT NULL REFERENCES organisation(id),
    entry_date          TEXT NOT NULL,
    source_type         TEXT NOT NULL,  -- 'sales_invoice', 'purchase_bill', 'receipt', 'manual', ...
    source_id           TEXT,
    narration           TEXT,
    reverses_entry_id   TEXT REFERENCES journal_entry(id),
    idempotency_key     TEXT,
    posted_at           TEXT NOT NULL,
    posted_by           TEXT,
    UNIQUE (organisation_id, idempotency_key)
);
CREATE INDEX idx_journal_entry_date ON journal_entry (organisation_id, entry_date);
CREATE INDEX idx_journal_entry_source ON journal_entry (source_type, source_id);

CREATE TABLE journal_line (
    id                  TEXT PRIMARY KEY,
    journal_entry_id    TEXT NOT NULL REFERENCES journal_entry(id),
    line_no             INTEGER NOT NULL,
    account_id          TEXT NOT NULL REFERENCES account(id),
    debit_paise         INTEGER NOT NULL DEFAULT 0 CHECK (debit_paise >= 0),
    credit_paise        INTEGER NOT NULL DEFAULT 0 CHECK (credit_paise >= 0),
    memo                TEXT,
    CHECK ((debit_paise = 0) <> (credit_paise = 0)),
    UNIQUE (journal_entry_id, line_no)
);
CREATE INDEX idx_journal_line_account ON journal_line (account_id);

-- Posted journals are immutable (spec §5.5): corrections use reversal entries.
CREATE TRIGGER journal_line_no_update BEFORE UPDATE ON journal_line
BEGIN SELECT RAISE(ABORT, 'posted journal lines are immutable'); END;
CREATE TRIGGER journal_line_no_delete BEFORE DELETE ON journal_line
BEGIN SELECT RAISE(ABORT, 'posted journal lines are immutable'); END;
CREATE TRIGGER journal_entry_no_delete BEFORE DELETE ON journal_entry
BEGIN SELECT RAISE(ABORT, 'posted journal entries are immutable'); END;

CREATE TABLE audit_event (
    id                  TEXT PRIMARY KEY,
    organisation_id     TEXT,
    occurred_at         TEXT NOT NULL,
    actor               TEXT NOT NULL,
    action              TEXT NOT NULL,
    entity_type         TEXT NOT NULL,
    entity_id           TEXT,
    reason              TEXT,
    before_json         TEXT,
    after_json          TEXT
);
CREATE INDEX idx_audit_entity ON audit_event (entity_type, entity_id);

CREATE TABLE app_setting (
    key                 TEXT PRIMARY KEY,
    value               TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);
