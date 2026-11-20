-- Phase 2.1: every document records the GST rule pack version its tax was
-- computed with, so later rule changes never alter historic documents.
-- Documents from before this migration keep NULL (computed before packs).
ALTER TABLE sales_document ADD COLUMN rule_pack_version TEXT;
ALTER TABLE purchase_document ADD COLUMN rule_pack_version TEXT;

CREATE TRIGGER sales_document_rule_pack_frozen BEFORE UPDATE OF rule_pack_version ON sales_document
WHEN OLD.status <> 'draft' AND NEW.rule_pack_version IS NOT OLD.rule_pack_version
BEGIN SELECT RAISE(ABORT, 'issued documents cannot be edited'); END;

CREATE TRIGGER purchase_document_rule_pack_frozen BEFORE UPDATE OF rule_pack_version ON purchase_document
WHEN OLD.status <> 'draft' AND NEW.rule_pack_version IS NOT OLD.rule_pack_version
BEGIN SELECT RAISE(ABORT, 'recorded purchases cannot be edited'); END;
