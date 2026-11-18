-- Phase 1.5: cash and bank accounts, opening balances, receipts and payments
-- with allocation to documents, bank statement import and reconciliation.

-- A cash drawer or bank account. Each is backed by exactly one ledger
-- account, so books and statements always agree on what "the bank" is.
CREATE TABLE money_account (
    id                  TEXT PRIMARY KEY,
    organisation_id     TEXT NOT NULL REFERENCES organisation(id),
    account_id          TEXT NOT NULL UNIQUE REFERENCES account(id),
    kind                TEXT NOT NULL CHECK (kind IN ('cash', 'bank')),
    name                TEXT NOT NULL,
    bank_name           TEXT,
    account_number      TEXT,
    ifsc                TEXT,
    -- How this bank's statement files are laid out (JSON), remembered from
    -- the last import.
    statement_mapping   TEXT,
    is_active           INTEGER NOT NULL DEFAULT 1,
    row_version         INTEGER NOT NULL DEFAULT 1,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);
CREATE UNIQUE INDEX ux_money_account_name ON money_account (organisation_id, UPPER(name));

-- Existing businesses: their default cash and bank ledgers become money accounts.
INSERT INTO money_account (id, organisation_id, account_id, kind, name, created_at, updated_at)
SELECT lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-7' || substr(lower(hex(randomblob(2))), 2)
         || '-' || substr('89ab', 1 + (abs(random()) % 4), 1) || substr(lower(hex(randomblob(2))), 2)
         || '-' || lower(hex(randomblob(6))),
       a.organisation_id, a.id, a.system_role, a.name, a.created_at, a.created_at
  FROM account a
 WHERE a.system_role IN ('cash', 'bank');

-- Opening balance of a ledger account as at the books start date. Parties
-- carry their own opening balances; this is for everything else (cash, bank,
-- capital, loans, assets). Debit balances are positive.
CREATE TABLE account_opening (
    account_id          TEXT PRIMARY KEY REFERENCES account(id),
    organisation_id     TEXT NOT NULL REFERENCES organisation(id),
    amount_paise        INTEGER NOT NULL,
    entry_id            TEXT REFERENCES journal_entry(id),
    updated_at          TEXT NOT NULL
);

-- Money received (receipt) or paid out (payment). Posted immediately; a
-- mistake is corrected by cancelling, which posts a reversal.
CREATE TABLE money_voucher (
    id                  TEXT PRIMARY KEY,
    organisation_id     TEXT NOT NULL REFERENCES organisation(id),
    voucher_type        TEXT NOT NULL CHECK (voucher_type IN ('receipt', 'payment')),
    status              TEXT NOT NULL CHECK (status IN ('posted', 'cancelled')),
    number              TEXT NOT NULL,
    financial_year      TEXT NOT NULL,
    seq                 INTEGER NOT NULL,
    voucher_date        TEXT NOT NULL,
    money_account_id    TEXT NOT NULL REFERENCES money_account(id),
    -- Either a party (customer/supplier) or another ledger account (bank
    -- charges, capital, a transfer to another bank) — never both.
    party_id            TEXT REFERENCES party(id),
    counter_account_id  TEXT REFERENCES account(id),
    amount_paise        INTEGER NOT NULL CHECK (amount_paise > 0),
    mode                TEXT NOT NULL,
    reference           TEXT,
    narration           TEXT,
    journal_entry_id    TEXT NOT NULL REFERENCES journal_entry(id),
    cancel_entry_id     TEXT REFERENCES journal_entry(id),
    cancel_reason       TEXT,
    row_version         INTEGER NOT NULL DEFAULT 1,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    cancelled_at        TEXT,
    CHECK ((party_id IS NULL) <> (counter_account_id IS NULL)),
    CHECK (status <> 'cancelled' OR (cancel_reason IS NOT NULL AND cancel_entry_id IS NOT NULL)),
    UNIQUE (organisation_id, voucher_type, financial_year, seq)
);
CREATE INDEX idx_money_voucher_date ON money_voucher (organisation_id, voucher_date);
CREATE INDEX idx_money_voucher_party ON money_voucher (organisation_id, party_id);

-- Which invoices or bills a receipt or payment settles. Whatever is not
-- allocated stays on the party's account as an advance.
CREATE TABLE voucher_allocation (
    id                  TEXT PRIMARY KEY,
    voucher_id          TEXT NOT NULL REFERENCES money_voucher(id),
    document_kind       TEXT NOT NULL CHECK (document_kind IN ('sales', 'purchase')),
    document_id         TEXT NOT NULL,
    amount_paise        INTEGER NOT NULL CHECK (amount_paise > 0),
    UNIQUE (voucher_id, document_kind, document_id)
);
CREATE INDEX idx_voucher_allocation_doc ON voucher_allocation (document_kind, document_id);

CREATE TRIGGER money_voucher_frozen BEFORE UPDATE ON money_voucher
WHEN NEW.voucher_type IS NOT OLD.voucher_type OR NEW.number IS NOT OLD.number OR NEW.voucher_date IS NOT OLD.voucher_date
  OR NEW.money_account_id IS NOT OLD.money_account_id OR NEW.party_id IS NOT OLD.party_id
  OR NEW.counter_account_id IS NOT OLD.counter_account_id OR NEW.amount_paise IS NOT OLD.amount_paise
  OR NEW.journal_entry_id IS NOT OLD.journal_entry_id OR OLD.status = 'cancelled'
BEGIN SELECT RAISE(ABORT, 'posted receipts and payments cannot be edited'); END;
CREATE TRIGGER money_voucher_no_delete BEFORE DELETE ON money_voucher
BEGIN SELECT RAISE(ABORT, 'receipts and payments cannot be deleted; cancel them'); END;
CREATE TRIGGER voucher_allocation_no_update BEFORE UPDATE ON voucher_allocation
BEGIN SELECT RAISE(ABORT, 'allocations cannot be changed'); END;
CREATE TRIGGER voucher_allocation_no_delete BEFORE DELETE ON voucher_allocation
BEGIN SELECT RAISE(ABORT, 'allocations cannot be deleted'); END;

-- One line of an imported bank statement. `fingerprint` makes re-importing
-- an overlapping statement safe: lines already present are skipped.
CREATE TABLE bank_statement_line (
    id                  TEXT PRIMARY KEY,
    organisation_id     TEXT NOT NULL REFERENCES organisation(id),
    money_account_id    TEXT NOT NULL REFERENCES money_account(id),
    line_date           TEXT NOT NULL,
    description         TEXT NOT NULL,
    reference           TEXT,
    withdrawal_paise    INTEGER NOT NULL DEFAULT 0 CHECK (withdrawal_paise >= 0),
    deposit_paise       INTEGER NOT NULL DEFAULT 0 CHECK (deposit_paise >= 0),
    balance_paise       INTEGER,
    fingerprint         TEXT NOT NULL,
    import_batch        TEXT NOT NULL,
    source_file         TEXT,
    status              TEXT NOT NULL CHECK (status IN ('unmatched', 'matched', 'ignored')),
    journal_entry_id    TEXT REFERENCES journal_entry(id),
    matched_at          TEXT,
    matched_by          TEXT,
    created_at          TEXT NOT NULL,
    CHECK ((withdrawal_paise = 0) <> (deposit_paise = 0)),
    CHECK ((status = 'matched') = (journal_entry_id IS NOT NULL)),
    UNIQUE (money_account_id, fingerprint)
);
CREATE INDEX idx_statement_line_date ON bank_statement_line (money_account_id, line_date);
-- A ledger entry reconciles against at most one statement line.
CREATE UNIQUE INDEX ux_statement_line_entry
    ON bank_statement_line (money_account_id, journal_entry_id) WHERE journal_entry_id IS NOT NULL;
