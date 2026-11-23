-- Phase 2.6: advances adjusted against later documents, refunds, and GST on
-- advances received for services (GSTR-1 tables 11A and 11B).

-- A receipt or payment either settles the party's account (the default) or
-- refunds money: a payment back to a customer, or a receipt back from a
-- supplier. A refund may name the advance it gives back.
ALTER TABLE money_voucher ADD COLUMN purpose TEXT NOT NULL DEFAULT 'settlement'
    CHECK (purpose IN ('settlement', 'refund'));
ALTER TABLE money_voucher ADD COLUMN refund_of_id TEXT REFERENCES money_voucher(id);

CREATE TRIGGER money_voucher_purpose_frozen BEFORE UPDATE ON money_voucher
WHEN NEW.purpose IS NOT OLD.purpose OR NEW.refund_of_id IS NOT OLD.refund_of_id
BEGIN SELECT RAISE(ABORT, 'posted receipts and payments cannot be edited'); END;

-- Allocations made after the voucher (adjusting an advance against a later
-- invoice or bill) carry their own date and who made them. NULL: made with
-- the voucher itself.
ALTER TABLE voucher_allocation ADD COLUMN allocated_on TEXT;
ALTER TABLE voucher_allocation ADD COLUMN allocated_by TEXT;

-- Tax paid on an advance received before the supply (section 13: services;
-- goods are exempt by notification 66/2017-CT). The receipt's own journal
-- entry carries the tax: Dr GST paid on advances, Cr output tax.
CREATE TABLE voucher_advance_tax (
    voucher_id          TEXT PRIMARY KEY REFERENCES money_voucher(id),
    organisation_id     TEXT NOT NULL REFERENCES organisation(id),
    gst_registration_id TEXT NOT NULL REFERENCES gst_registration(id),
    place_of_supply     INTEGER NOT NULL,
    rate_bp             INTEGER NOT NULL CHECK (rate_bp > 0),
    -- The advance including tax; taxable + the heads always add up to it.
    gross_paise         INTEGER NOT NULL CHECK (gross_paise > 0),
    taxable_paise       INTEGER NOT NULL,
    igst_paise          INTEGER NOT NULL DEFAULT 0,
    cgst_paise          INTEGER NOT NULL DEFAULT 0,
    sgst_paise          INTEGER NOT NULL DEFAULT 0,
    utgst_paise         INTEGER NOT NULL DEFAULT 0,
    cess_paise          INTEGER NOT NULL DEFAULT 0,
    CHECK (taxable_paise + igst_paise + cgst_paise + sgst_paise + utgst_paise + cess_paise = gross_paise)
);
CREATE TRIGGER voucher_advance_tax_no_update BEFORE UPDATE ON voucher_advance_tax
BEGIN SELECT RAISE(ABORT, 'tax on an advance cannot be changed'); END;
CREATE TRIGGER voucher_advance_tax_no_delete BEFORE DELETE ON voucher_advance_tax
BEGIN SELECT RAISE(ABORT, 'tax on an advance cannot be deleted'); END;

-- Part of a taxed advance used up: adjusted against an invoice, or refunded.
-- Its entry takes the tax back (Dr output tax, Cr GST paid on advances), as
-- the invoice charges it again; reported in GSTR-1 table 11B.
CREATE TABLE advance_tax_adjustment (
    id                  TEXT PRIMARY KEY,
    organisation_id     TEXT NOT NULL REFERENCES organisation(id),
    voucher_id          TEXT NOT NULL REFERENCES money_voucher(id),
    reason              TEXT NOT NULL CHECK (reason IN ('invoice', 'refund')),
    allocation_id       TEXT REFERENCES voucher_allocation(id),
    refund_voucher_id   TEXT REFERENCES money_voucher(id),
    adjust_date         TEXT NOT NULL,
    gross_paise         INTEGER NOT NULL CHECK (gross_paise > 0),
    taxable_paise       INTEGER NOT NULL,
    igst_paise          INTEGER NOT NULL DEFAULT 0,
    cgst_paise          INTEGER NOT NULL DEFAULT 0,
    sgst_paise          INTEGER NOT NULL DEFAULT 0,
    utgst_paise         INTEGER NOT NULL DEFAULT 0,
    cess_paise          INTEGER NOT NULL DEFAULT 0,
    journal_entry_id    TEXT NOT NULL REFERENCES journal_entry(id),
    status              TEXT NOT NULL CHECK (status IN ('posted', 'cancelled')),
    cancel_entry_id     TEXT REFERENCES journal_entry(id),
    created_at          TEXT NOT NULL,
    CHECK ((reason = 'invoice') = (allocation_id IS NOT NULL)),
    CHECK ((reason = 'refund') = (refund_voucher_id IS NOT NULL)),
    CHECK ((status = 'cancelled') = (cancel_entry_id IS NOT NULL))
);
CREATE INDEX idx_advance_tax_adjustment_voucher ON advance_tax_adjustment (voucher_id);
CREATE INDEX idx_advance_tax_adjustment_date ON advance_tax_adjustment (organisation_id, adjust_date);
CREATE TRIGGER advance_tax_adjustment_frozen BEFORE UPDATE ON advance_tax_adjustment
WHEN OLD.status = 'cancelled' OR NEW.voucher_id IS NOT OLD.voucher_id OR NEW.adjust_date IS NOT OLD.adjust_date
  OR NEW.gross_paise IS NOT OLD.gross_paise OR NEW.journal_entry_id IS NOT OLD.journal_entry_id
BEGIN SELECT RAISE(ABORT, 'advance adjustments can only be cancelled'); END;
CREATE TRIGGER advance_tax_adjustment_no_delete BEFORE DELETE ON advance_tax_adjustment
BEGIN SELECT RAISE(ABORT, 'advance adjustments cannot be deleted'); END;

-- System account for the tax paid on advances, added to existing businesses
-- (new ones get it from the default chart). Code 1220 unless already taken.
INSERT INTO account (id, organisation_id, code, name, kind, system_role, created_at)
SELECT lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-7' || substr(lower(hex(randomblob(2))), 2)
         || '-' || substr('89ab', 1 + (abs(random()) % 4), 1) || substr(lower(hex(randomblob(2))), 2)
         || '-' || lower(hex(randomblob(6))),
       o.id,
       CASE WHEN EXISTS (SELECT 1 FROM account a WHERE a.organisation_id = o.id AND a.code = '1220')
            THEN '1229' ELSE '1220' END,
       'GST paid on advances', 'asset', 'gst_on_advances', o.created_at
  FROM organisation o
 WHERE NOT EXISTS (SELECT 1 FROM account a WHERE a.organisation_id = o.id AND a.system_role = 'gst_on_advances');
