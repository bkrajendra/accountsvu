-- Phase 2.6: manual tax override (spec §5.6). A line's tax may be entered by
-- hand by an owner or accountant, with a reason; the calculated tax is kept
-- next to it so the difference stays visible. Frozen once issued/recorded.

ALTER TABLE sales_document ADD COLUMN tax_override_reason TEXT;
ALTER TABLE sales_document ADD COLUMN tax_override_by TEXT;
ALTER TABLE sales_document ADD COLUMN tax_override_at TEXT;
ALTER TABLE sales_line ADD COLUMN computed_tax_paise INTEGER;

ALTER TABLE purchase_document ADD COLUMN tax_override_reason TEXT;
ALTER TABLE purchase_document ADD COLUMN tax_override_by TEXT;
ALTER TABLE purchase_document ADD COLUMN tax_override_at TEXT;
ALTER TABLE purchase_line ADD COLUMN computed_tax_paise INTEGER;

CREATE TRIGGER sales_document_override_frozen BEFORE UPDATE ON sales_document
WHEN OLD.status <> 'draft' AND (
       NEW.tax_override_reason IS NOT OLD.tax_override_reason OR NEW.tax_override_by IS NOT OLD.tax_override_by
    OR NEW.tax_override_at IS NOT OLD.tax_override_at)
BEGIN SELECT RAISE(ABORT, 'issued documents cannot be edited'); END;

CREATE TRIGGER purchase_document_override_frozen BEFORE UPDATE ON purchase_document
WHEN OLD.status <> 'draft' AND (
       NEW.tax_override_reason IS NOT OLD.tax_override_reason OR NEW.tax_override_by IS NOT OLD.tax_override_by
    OR NEW.tax_override_at IS NOT OLD.tax_override_at)
BEGIN SELECT RAISE(ABORT, 'recorded purchases cannot be edited'); END;

CREATE TRIGGER purchase_line_override_frozen BEFORE UPDATE ON purchase_line
WHEN (SELECT status FROM purchase_document WHERE id = OLD.document_id) <> 'draft'
 AND NEW.computed_tax_paise IS NOT OLD.computed_tax_paise
BEGIN SELECT RAISE(ABORT, 'lines of a recorded purchase cannot be edited'); END;
