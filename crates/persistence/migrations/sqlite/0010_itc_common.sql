-- Phase 2.3: common credit (rule 42). A line whose input tax is used for both
-- taxable and exempt supplies is marked common, so the GSTR-3B workpaper can
-- propose the rule 42 reversal. Part of the ITC review: editable on recorded
-- purchases (the frozen-line trigger doesn't list it).
ALTER TABLE purchase_line ADD COLUMN itc_common INTEGER NOT NULL DEFAULT 0 CHECK (itc_common IN (0, 1));
