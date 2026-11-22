# AccountsVU — Phase-wise Implementation Plan

Companion to [`accountsvu-spec.md`](accountsvu-spec.md) (v0.3). Decision IDs (D1–D23) refer to spec §12.
Last updated: 19 Nov 2026.

## Phase status

Status values: ⬜ Not started · 🟡 In progress · ✅ Done · ⛔ Blocked

| Phase | Title | Status | Decisions taken | Opens / attention needed | Comments |
|---|---|---|---|---|---|
| 0 | Product discovery | ✅ Done | D1–D18 recorded in spec §12 (Windows-only, local-first SQLite, Tauri-command transport, paise storage, per-line rounding + Round-off, Typst PDFs, passphrase backups, mock GSP first) | **GST practitioner reviewer not yet named (D6).** **No GSP selected (D3).** Both block Phase 2/3 exit, not their start. | Spec bumped to v0.2. |
| 1 | Accounting foundation | ✅ Done | Cargo workspace at root with `crates/`; runtime SQLx queries; Tailwind v4 + Inter (bundled) + light/dark/system theme. 1.1: composition registrations rejected for now; one organisation per install; series format locks after first number; optimistic concurrency via `row_version`. 1.2: party opening balances post to the ledger (reversal on change); masters editable by Owner and Accountant; import skips invalid rows rather than rejecting the file; account opening balances moved to 1.5. 1.3: invoice dates can't be in the future or out of order within a series; HSN/SAC required on GST lines; exports/SEZ choose IGST or zero-rated under LUT; outward reverse charge, e-invoice IRN and PDF not yet. 1.4: attachments stored inside SQLite (so backups include them); purchase tax is computed from rates — matching the supplier's printed paise and GSTR-2B comes in Phase 2; imports of goods (bill of entry) not yet. 1.5: receipts and payments post immediately (no drafts; cancel posts a reversal and reopens what they settled); numbered RCT/PMT per financial year, separate from GST receipt vouchers; each cash/bank account has its own ledger; allocation only to the same party's open documents, remainder kept as an advance; credit/debit notes count towards "paid"; statement lines never post by themselves — they are matched, recorded as a receipt/payment, or ignored; column mapping remembered per bank account; re-imports skip lines by fingerprint; operators may record receipts/payments (spec §2), cancelling and reconciling need Owner/Accountant. 1.6: period lock enforced by a database trigger as well as the core; lock, reopen, FY close and logo are Owner-only; reopening needs a reason; a year-end close posts one entry moving income/expense to retained earnings (P&L leaves it out, so closed years still report); years close in order and reopen newest first; invoice previews render as light "paper" in both themes. 1.7: invoice PDFs rendered in-process by Typst (Inter bundled as static TTFs, no system fonts, no clock, data passed as JSON so it can't inject markup); backups are one `.avbackup` file (readable header + AES-256-GCM over a `VACUUM INTO` snapshot, Argon2id 64 MiB/3 passes, header authenticated); restore is checked (integrity, foreign keys, balanced journals, schema not newer) and swapped in at the next start, keeping the replaced data as `accountsvu.before-restore-<time>.sqlite3`; app lock stored in `app-lock.json` outside the database (a restore never changes it), PIN 4–12 digits or 6+ character password, one-time recovery code, throttling after 5 wrong tries, enforced by the host for every command; the window capability grants no core or plugin permissions and file dialogs run from Rust | PDF bank statements not supported (the ICICI sample PDF uses embedded CID fonts, so text needs a PDF/OCR library) — import the XLS/CSV version. Adjusting an advance against a later invoice, refunds and GST on advances (receipt vouchers) are Phase 2 items. Invoice layout follows the app branding (brand blue, Inter/SF Pro, business logo uploaded in Settings › Business). Closing stock is not valued, so P&L expenses purchases as booked — inventory valuation needs a decision before Phase 2. Desktop launch (`pnpm tauri dev`) confirmed working on 19 Nov 2026. 1.7 desktop paths not yet run in the desktop app: native save/open dialogs, restore-and-restart, and the tightened capability (the browser preview simulates them). Golden PDF hashes assume identical Typst output on every OS the release workflow tests on. | **1.0 scaffold done. 1.1 business setup done:** onboarding wizard, business profile, GSTINs, number series, default chart of accounts, audit log (48 Rust tests). **1.1b architecture alignment done:** request context + roles, repository traits with a SQLite adapter, every query scoped by organisation. **1.2 masters done:** parties with ledger-posted opening balances, items, CSV/Excel import. **1.3 sales done:** invoices, bills of supply, credit notes, cancellation, live GST engine calculation, ledger posting. **1.4 purchases done:** bills, expenses, debit notes, reverse charge, per-line ITC review, SHA-256 attachments (81 Rust tests). **1.5 banking done:** cash/bank accounts with opening balances, account opening balances, receipts/payments with allocation, cash/bank books, statement import (CSV/XLS/XLSX) and reconciliation (91 Rust tests). **1.6 ledger and reports done:** trial balance, P&L, balance sheet, general ledger, party statement, journal register, receivables/payables ageing with drill-down; period lock (DB-enforced) and FY close/reopen; business logo on invoices (98 Rust tests). **1.7 documents, backup, security done:** Typst invoice PDF with golden files, encrypted backup/restore with verification and last-backup indicator, optional PIN lock, CSP and capabilities tightened (119 Rust tests). Phase 1 exit check passed on the installed macOS build (20 Nov 2026). |
| 2 | GST workbench | 🟡 In progress | Rule pack format = versioned JSON shipped with app (D12). Engine stays pure, GSP-free (D23). Offline filing channel + versioned format adapters (`gst-formats`) built here so ThingsVU can file without a GSP. | Portal JSON schemas for GSTR-1 / 2B sample files needed for golden tests. Reviewer for scenario catalogue. | **2.1 rule pack and tax checks done:** bundled versioned rule pack, date-aware rates, non-blocking warnings, GSTR-1 placement on invoices, HSN/SAC help (132 Rust tests). **2.2 return centre and GSTR-1 done:** monthly periods per GSTIN with status and output tax, GSTR-1 workpaper (tables 4–13) with drill-down and exceptions, portal JSON export via the new `gst-formats` crate (136 Rust tests). **2.3 GSTR-3B done:** tables 3.1, 3.2, 4, 5 and 6.1 from saved documents, ITC from each line's review incl. 17(5) and rule 42 common credit, rule 88A payment proposal with credit carried forward, GSTR-1 cross-check, Excel workpaper (142 Rust tests). **2.4 GSTR-2B done:** portal JSON import, matching (exact/likely/mismatch/missing/duplicate) with accept/defer carried across months, ITC comparison, Excel report with purchase register (149 Rust tests). **2.5 offline filing and period close done:** versioned return pack with blocking errors and warnings, filing pack, ARN-backed filing records with frozen files, GST month lock (core and database), late ITC moved to the next open month, compliance pack (154 Rust tests). Next: 2.6 advances, refunds and review. The starter pack's HSN rates need the practitioner review before Phase 2 exit. |
| 3 | Filing integration | ⬜ Not started | Mock provider + export first (D3). `GstProvider` core trait + capability traits; submission and payment separate (D23). | GSP selection, sandbox access and commercial terms. D22: no hosted gateway; personal mode uses offline channel or taxpayer-held GSP credentials. | |
| 4 | Small-business polish | ⬜ Not started | Cloud backup target added here (D19) | Which convenience features matter most to pilot users. Cloud storage choice for backups. | |
| 5 | Commercial mode | ⬜ Not started | PostgreSQL adapter behind the same repository traits (D9, D20); online-only clients (D21) | Hosting target, OIDC provider, tenant onboarding/licensing model. | |
| 6 | Offline sync | ⬜ Not started | SQLite as offline store for commercial clients (D21) | Offline numbering rule: per-device number blocks vs no offline GST documents. | |

## Phase 0 — Product discovery ✅

- [x] Analyse spec, raise questions, record decisions D1–D18.
- [x] Adopt scope additions (FY close, Round-off ledger, bank CSV profiles, posting idempotency key, drill-down pattern, GSTR-2B fixtures).
- [ ] Name GST practitioner reviewer (carried forward; needed before Phase 2 exit).
- [ ] Shortlist GSPs (carried forward; needed before Phase 3 start of real adapter).

## Phase 1 — Accounting foundation ✅

Exit condition: MVP bookkeeping acceptance criteria (spec §9.2 items 1–4, 9, 10) pass.

Exit check (20 Nov 2026): the product owner ran the desktop-only paths in the installed macOS build — native save/open dialogs, restore-and-restart, and the update check against the live release feed. Installing an update end to end is tested with v0.2.1 during Phase 2.

### 1.0 Project scaffold ✅
- [x] Root Cargo workspace: `src-tauri`, `crates/{domain,accounting,gst-engine,persistence,application}`.
- [x] `domain`: `Money` (Decimal, paise conversion, half-up rounding), `Gstin` (format + checksum), `StateCode`, ids.
- [x] `accounting`: `JournalEntry` with balanced-entry invariant; `AccountKind`.
- [x] `gst-engine`: intra/inter-state tax split per line (CGST/SGST vs IGST), basis-point rates.
- [x] `persistence`: SQLite pool in app data dir, embedded migrations (organisation, gst_registration, account, journal tables).
- [x] `application`: `AppCore` service with health/dashboard use cases; Tauri commands call it.
- [x] UI shell: sidebar (spec §6.1 sections), top bar with GSTIN/FY context and theme switcher, dashboard (§6.2 cards + Next actions), placeholder pages.
- [x] Design tokens for light/dark, Inter bundled, reduced-motion/transparency/contrast support.

### 1.1 Business setup ✅
- [x] Organisation profile (legal/trade name, PAN, constitution, address, contact, FY start, books start date, rupee rounding). PAN ↔ constitution and PAN ↔ GSTIN checks.
- [x] GST registrations (GSTIN check digit, state from GSTIN, type, monthly/QRMP, effective dates, status). Multiple GSTINs per PAN.
- [x] Number series per document type / GSTIN / FY: Rule 46 limits (≤16 chars, `A-Z 0-9 - /`), DB-enforced uniqueness, one default per scope, atomic `reserve_next_number`, format lock after first issue, next number can only move forward.
- [x] Default chart of accounts (42 accounts, system roles for posting: AR/AP, cash/bank, input/output CGST/SGST/UTGST/IGST/cess, ITC pending, RCM, Round-off).
- [x] First-run onboarding wizard (GSTIN first → prefills PAN/state/constitution; review step; server errors jump back to the right step).
- [x] Settings: Business, GST registrations, Numbering, Chart of accounts (read-only), Activity log, Appearance & about.
- [x] Audit events written in the same transaction as every change, with before/after snapshots.
- [x] Field-level validation errors from Rust shown inline; optimistic concurrency (`row_version`) on all edits.

### 1.1b Architecture alignment (D19–D23) ✅
Done before 1.2 so every new table and use case is written the new way.
- [x] `RequestContext` (actor, organisation, role) passed to every use case; permission check per use case; actor recorded in the audit log.
- [x] Repository traits (`Store`, `UnitOfWork`, per-aggregate repos) in `persistence`; SQLite implementation moved to `persistence::sqlite`; `application` no longer touches SQLx types.
- [x] Every query and update scoped by `organisation_id`.
- [x] Tauri commands build a local-owner context; same DTOs and error codes (new `forbidden` code).

### 1.2 Masters ✅
- [x] Parties: customer/supplier/both; GST status (regular, composition, unregistered, consumer, overseas, SEZ); GSTIN with state and PAN taken from it; billing and shipping address; contact, payment terms, credit limit; active flag. Names and GSTINs unique per business.
- [x] Party opening balances posted to the ledger (Accounts Receivable/Payable against Opening Balance, tagged with the party) as at the books start date; changes post a reversal plus a new entry, never an edit. Journal lines now carry `party_id` (party sub-ledger) and the party list shows the running balance.
- [x] Items/services: goods/service, HSN (4/6/8 digits) or SAC (6 digits, 99…), GST UQC unit, tax category (taxable/exempt/nil-rated/non-GST), GST rate from the known-rates list (legacy pre-Sept-2025 rates labelled), cess, sales/purchase price, price-includes-GST, stock flag. Names and codes unique.
- [x] CSV/Excel import (.csv, .xlsx, .xls, .ods; 5 MB / 5,000 rows) for parties and items: column auto-matching, mapping step, row-by-row preview using the same rules as the forms (incl. duplicates in the file and against existing records), import of valid rows in one transaction, skipped rows reported with spreadsheet row numbers, downloadable template, `import.run` audit entry.
- [ ] Account-level opening balances (cash, bank, capital, other ledgers) — moved to 1.5 with bank accounts.

### 1.3 Sales ✅
- [x] Tax invoices, bills of supply and credit notes. States draft → issued → cancelled; shown as draft / issued / overdue (past due date) / part-paid / paid / cancelled — paid amounts arrive with receipts in 1.5.
- [x] Editor: customer picker, keyboard-friendly line grid (item picker or free-text line, Enter adds the next line), place of supply, GSTIN, prices-include-GST, zero-rated (export/SEZ only), notes; live tax summary calculated by the Rust GST engine (CGST/SGST, UTGST or IGST, cess, round-off, amount in words); number preview.
- [x] Issue = full validation + next number from the default series for the GSTIN/type/FY (dates must run in order within a series) + party and tax snapshot + balanced posting (Dr Receivable/party, Cr Sales, Cr output taxes, ± Round-off), all in one transaction. Posting idempotency key `sales:issue:<id>` (unique in the database) blocks double posting.
- [x] Credit notes against an issued invoice: party, GSTIN, place of supply and GST treatment come from the invoice; reason required; total can't exceed what's left on the invoice; mirror-image posting.
- [x] Cancellation with reason posts a reversal entry; the number is never reused; an invoice with live credit notes can't be cancelled. Drafts can be deleted.
- [x] No silent edits: SQLite triggers refuse any change to an issued document's number, party, tax, totals or lines, and refuse deleting it.
- [x] Permissions: Operators prepare drafts; Owner/Accountant issue and cancel (spec §4).
- [x] Engine fix: intra-state split of fractional rates (0.25% → 0.125% + 0.125%) no longer rounds the half-rate down.

### 1.4 Purchases and expenses ✅
- [x] Supplier bills (supplier's invoice number and date, duplicate check per supplier and FY), expenses (supplier optional when paid on the spot), cash/bank purchases (paid immediately) and debit notes against a bill (own DN number series, can't exceed the bill).
- [x] Each line is booked to a chosen expense or asset account (purchases, inventory, fixed assets, rent, …); round-off and tax/control accounts are excluded.
- [x] Reverse charge: supplier is owed the taxable value; the tax is credited to "GST payable under reverse charge".
- [x] ITC status per taxed line — pending (default, never inferred) / eligible / ineligible / partial with reason and claimable amount. Eligible → input CGST/SGST/UTGST/IGST/cess; pending → "Input tax pending review"; ineligible and the unclaimed part of partial → the line's cost. ITC can't be claimed without a registered supplier's GSTIN (unless reverse charge) or when the business isn't registered. Reviewing a recorded line posts an adjusting entry; cancelling reverses the bill as it stands, including reviews.
- [x] Recording = validation + tax snapshot + one balanced posting with idempotency key `purchase:record:<id>`; recorded purchases are frozen by database triggers except the ITC review fields.
- [x] Attachments (PDF, images, Excel/CSV; up to 5 MB) stored inside the database, content-addressed by SHA-256 (deduplicated, verified on read), append-only once the purchase is recorded; metadata records who added it and when.
- [x] Screens: purchases list (filters incl. "ITC to review"), editor (supplier picker, payment mode, reverse charge, per-line account and ITC), document view (ITC review dialog, attachments, debit notes, cancellation).

### 1.5 Receipts, payments, banking ✅
- [x] Cash and bank accounts: each backed by its own ledger account (cash 1001–1009, banks 1011–1099; the default "Cash in hand" and "Bank account" become money accounts, backfilled for existing businesses by migration 0006). Bank name, account number, IFSC; closing requires a zero balance.
- [x] Opening balances for balance-sheet ledgers (cash, bank, capital, loans, assets) as at the books start date, against "Opening balance adjustment"; changing one reverses the old entry. Receivables/payables stay on parties. Settings › Chart of accounts lists and edits them.
- [x] Receipts and payments (RCT/PMT numbering per FY): from/to a party or another ledger (bank charges, capital, drawings, GST, transfer between cash and bank). Party receipts credit receivables, payments debit payables. Allocation to the party's open invoices or credit bills (oldest first by default, editable, never above what is outstanding); the rest is an advance. Posted with idempotency key `voucher:post:<id>`; frozen by triggers; cancelling posts a reversal and releases the allocations.
- [x] Invoice and bill status now count receipts/payments and credit/debit notes (paid, part-paid); paid documents can't be cancelled; documents show their settlements and a "Record receipt/payment" shortcut.
- [x] Cash book / bank book with date range, opening, running and closing balance; every line drills down to its source document.
- [x] Bank statement import (CSV, XLS, XLSX): header row detection (title rows above the table), withdrawal/deposit columns or one amount with a Dr/Cr column, Indian date formats, Excel serial dates, footer rows skipped; preview of new/duplicate/skipped lines; mapping remembered per account; fingerprints make overlapping re-imports safe; undo an import (unreconciled lines only). Tested against the ICICI "detailed statement" layout.
- [x] Reconciliation: suggestions by amount, direction and ±7 days; one-click "match sure matches"; record a receipt/payment from a line (matched in the same step); ignore/restore; unmatch; book entries not on any statement; books vs statement balance. A reconciled voucher can't be cancelled until unmatched.
- [ ] PDF statement import (deferred; needs a PDF text/OCR library).
- [ ] Adjusting advances against later invoices, refunds, GST on advances — Phase 2.

### 1.6 Ledger and reports ✅
- [x] Trial balance (opening, debit, credit, closing for any period; balanced check), profit & loss (sales, cost of sales, gross profit, other income, operating expenses, net profit), balance sheet as on a date (unclosed profit shown separately; balanced check).
- [x] General ledger per account, party statement per customer/supplier, journal register (every entry with its lines; capped at 1,000 entries per view).
- [x] Receivables and payables ageing as on a date: not due, 1–30, 31–60, 61–90, over 90 days from the due date; by party (expandable) or by document; "unallocated" column for opening balances and advances.
- [x] Drill-down: report rows → ledger for the same period → source document (invoice, bill, receipt/payment, party). Period kept in the URL.
- [x] Period lock: Owner sets the last locked date (not in the future); postings on or before it are refused by the core and by a database trigger; moving it back needs a reason; all changes audited.
- [x] Financial-year close: once the year has ended, one entry on its last day moves income and expenses to retained earnings and the year is locked; balances carry forward. Reopen (newest first, with a reason) reverses that entry and unlocks the year.
- [x] Business logo (PNG/JPEG/SVG ≤ 1 MB, type checked by content, scripts in SVG refused) stored with attachments, shown on the invoice preview; the invoice renders as white paper with brand-blue accents in both themes, ready for the 1.7 PDF.

### 1.7 Documents, backup, security ✅
- [x] Typst invoice PDF (D16): new `document-render` crate compiles a fixed template in memory (Inter fonts bundled, no file system, packages or clock), so output is deterministic. A4 layout follows the app branding: logo, seller block, brand-blue title and table header, bill-to and place of supply, reverse-charge statement, line table with HSN/SAC, rate-wise tax breakup (only the taxes charged), totals, amount in words, notes and signatory. Drafts print with a DRAFT watermark, cancelled documents with CANCELLED; long invoices repeat the table header on each page. Golden tests check the print data (JSON) and the SHA-256 of the PDF bytes for four cases (intra-state with PNG logo, inter-state credit note with SVG logo, 60-line draft, cancelled bill of supply); `UPDATE_GOLDEN=1` rewrites them and `target/golden/` holds the PDFs to look at. Invoice page: Print (opens in the default PDF viewer) and Save PDF (native save dialog, then "Show in folder").
- [x] Encrypted backup/restore (D17): Owner-only "Back up now" with passphrase (8+ characters, typed twice, loss warning) → native save dialog → file read back and compared → logged (`backup_log`, migration 0008) and audited. Restore: choose file (header shows business and date without the passphrase) → passphrase → decrypted to a staging file and checked (SQLite integrity, foreign keys, every journal entry balanced, schema not newer than the app, counts shown) → "Replace data and restart"; the swap happens before the database opens, keeping the old data. Also offered on the first-run setup screen (clean install, §9.2 item 9). Last-backup indicator in Settings and on the Overview (due after 7 days).
- [x] Optional PIN/password app lock (D14): Settings › Backup & security turns it on (PIN or password, typed twice, idle timeout: start only / 1 / 5 / 15 / 30 / 60 minutes) and shows a one-time recovery code; change PIN and turn off need the current PIN. Locks at start, after idle time, or from the top-bar lock button; the host refuses every other command while locked, and the UI drops its cached data behind the lock screen. Wrong tries are slowed after 5 (30 s doubling to 15 min). The recovery code unlocks and turns the lock off.
- [x] CSP and capabilities review: `img-src` gains `blob:` (logo and attachment previews), the unused `asset:` source goes, and `base-uri`, `form-action`, `frame-src`, `worker-src` and `media-src` are closed. The main window's capability now grants nothing beyond the app's own commands (was `core:default` + `opener:allow-open-url`). Dialogs and opening files run from Rust, so the web view never supplies a path; only files saved in this session can be shown again.

## Phase 2 — GST workbench 🟡

Exit condition: tax scenarios reviewed by a GST practitioner; golden test set passes; a regular monthly filer can file GSTR-1 and GSTR-3B from the exported files.

### 2.1 Rule pack and tax checks ✅
- [x] Versioned rule pack (D12): `crates/gst-engine/rules/rule-pack.json` (format 1, version `2026.11.1`), bundled into the engine. Rate slabs with effective dates and sources (12%/28%/6% to 21 Sep 2025, 40% from 22 Sep 2025), 46 curated HSN/SAC headings with dated rates (longest-prefix match; "conditional" headings whose rate depends on value, packaging or buyer suggest but never warn), and the B2CL threshold (₹2.5 lakh to 31 Jul 2024, ₹1 lakh from 1 Aug 2024). `RulePack::from_json` validates format, sources, slabs, codes and overlapping dates, so a newer pack can be loaded later without changing the engine. Migration 0009 adds `rule_pack_version` to sales and purchase documents (frozen once issued/recorded); documents from before keep NULL.
- [x] Rates against the document date: the line and item rate pickers label slabs not in force ("28% (until 21 Sep 2025)", "40% (from 22 Sep 2025)"); credit notes are checked on their invoice's date, purchases on the supplier's invoice date. The hard-coded rate list is gone; unknown slabs are still a blocking error.
- [x] Warnings (never blocking, shown amber under the line or in the summary): rate not in force on the date (with the pack's rate for the HSN when known), rate different from the pack's rate for the HSN/SAC, HSN/SAC the pack treats as exempt/nil but the line is taxable (and the reverse), place of supply different from a registered customer's GSTIN state.
- [x] GSTR-1 placement (`checks::gstr1_placement`): B2B, B2C large/small by inter-state and invoice value against the dated threshold, export and SEZ with/without payment, CDNR/CDNUR (credit notes against B2C small invoices net into table 7), and table 8 for exempt/nil/non-GST lines. Shown live in the invoice editor and on the issued document with the rule pack version.
- [x] HSN/SAC help in the item editor: typing a code shows the pack's entry and rate with a "Use 5%" action; with no code, the item name suggests up to three codes. Settings › About shows the rule pack version.

### 2.2 Return centre and GSTR-1 ✅
- [x] Return centre (GST Filing in the sidebar): per GSTIN and financial year, one row per month with GSTR-1 status (in progress / due on the 11th / was due), issued and draft counts, taxable value, output tax (invoices less credit notes, before ITC) and checks to fix or review. Quarterly (QRMP) GSTINs are shown monthly with a note until QRMP support; composition GSTINs are not listed.
- [x] GSTR-1 workpaper (`application::returns`), built only from issued documents of the GSTIN dated in the month and placed with the same rule-pack logic as the invoice editor: B2B incl. SEZ (4A, 4B, 6B, 6C), B2CL (5), exports (6A), B2CS (7, by place of supply, rate and intra/inter, net of credit notes against B2CS invoices), nil/exempt/non-GST (8), CDNR/CDNUR (9B), HSN summary split B2B/B2C (12; services in UQC `NA`, units outside the UQC list in `OTH`) and documents issued per series with cancellations (13). Every row drills down to its documents.
- [x] Exceptions: errors (missing HSN/SAC, registered-party table without a valid GSTIN, HSN tax not matching the tables) and warnings (drafts in the month, rule-pack tax checks per document, `OTH` units, export shipping-bill details to add on the portal).
- [x] `gst-formats` crate: GSTR-1 portal JSON adapter (`gstr1-json/1`, header version `GST3.2.2`), exact decimal numbers (no floats), golden-file tests. Export JSON saves through the save dialog and is audit-logged (`gstr1.exported` with period, rule pack and SHA-256).
- [ ] To confirm with the practitioner / offline tool before Phase 2 exit: the portal header `version`, and an upload of an exported file through the offline tool.

### 2.3 GSTR-3B ✅
- [x] GSTR-3B workpaper (`application::gstr3b`) from issued sales and recorded purchases of the GSTIN dated in the month: 3.1 (a)–(e) with zero-rated supplies (exports, SEZ; credit notes follow their invoice) and reverse-charge inward supplies, 3.2 inter-state supplies to unregistered and composition persons by place of supply, 4A (2) import of services, (3) reverse charge, (5) all other ITC, 4B(1) reversals, 4C net ITC, and table 5 exempt/nil/non-GST inward supplies. 4A(1) imports of goods, 4A(4) ISD and 4D stay zero until those flows exist.
- [x] ITC per line from its review: eligible in full, partial by the claimable amount (split across heads as the posting does), not eligible reported in 4A and reversed in 4B(1) (section 17(5) treatment per Circular 170/02/2022); pending lines are left out and listed; lines from suppliers without a GSTIN (not reverse charge) are left out. Debit notes reduce ITC.
- [x] Rule 42 common credit: migration 0010 adds `itc_common` to purchase lines, set in the ITC review ("Common credit (rule 42)", eligible or partial only). Monthly D1 = C2 × E ÷ F from the month's exempt/nil/non-GST and total outward turnover goes to 4B(1). Rule 43 (capital goods), D2 and the annual true-up are not done.
- [x] 6.1 payment proposal (`gst_engine::offset`): rule 88A order (IGST credit first to IGST, then CGST/SGST where it saves cash; CGST/SGST credit to IGST only after IGST credit is used; cess to cess; reverse charge in cash), credit carried forward chained month by month from the books start (an estimate of the electronic credit ledger); a negative head moves to liability.
- [x] GSTR-1 ↔ 3B cross-check (taxable value and each head against 3.1 a+b; table 8 against 3.1 c+e) with mismatches as exceptions; Excel workpaper (`rust_xlsxwriter`: GSTR-3B tables and payment, ITC lines, cross-check) saved through the dialog. PDF workpaper not done.
- [x] Return centre shows GSTR-3B status (due on the 20th), net ITC and cash to pay per month; GSTR-1/GSTR-3B switch on both pages; ITC review shows the common-credit choice and badge.
- [x] ITC reviewed after the month's GSTR-3B is filed moves to the next open month (done in 2.5).
- [ ] Later: interest and late fee are not computed.

### 2.4 GSTR-2B reconciliation ✅
- [x] GSTR-2B import from the portal JSON download (`gst_formats::gstr2b`, adapter `gstr2b-json/1`): B2B, B2BA, CDNR and CDNRA documents become lines (amounts converted to paise from decimal text; invoice totals or summed item rows); ISD and import sections are counted and listed as not reconciled. Migration 0011 adds `gstr2b_statement` (one per GSTIN and month, replaced on re-import, SHA-256 kept), `gstr2b_line` and `gstr2b_decision`. The statement's GSTIN must match the registration. Excel/ZIP downloads are refused with a hint to use the JSON.
- [x] Matching (`gst_engine::recon`): duplicate in 2B → same GSTIN, kind and number (ignoring case, punctuation and leading zeros) → exact (date and amounts within ₹1) or mismatch (differences listed) → likely (same GSTIN and kind, amounts within ₹1, same date or same trailing digits) → missing in 2B / missing in books. Our debit notes are matched as the supplier's credit notes.
- [x] Which purchases take part: recorded purchases from GSTIN suppliers against the registration, dated (supplier invoice date) in the month, plus earlier ones not found in another month's 2B (they can match a late filer; they are listed as missing only when deferred earlier); a purchase found in another month's 2B shows "In 2B for YYYY-MM".
- [x] Accept / defer / undo per row (`decide_gstr2b`), keyed by a stable document key so decisions survive a re-import; deferring a purchase missing in 2B carries it into the next month. Decisions and imports are audit-logged. Matching never claims credit — ITC stays with each line's review; the page shows lines with ITC review pending.
- [x] ITC comparison: 2B available / not available against ITC claimable in the books, and claimed ITC not backed by this 2B. Return centre shows each month's 2B status (not imported / N to review / reconciled); GSTR-2B joins the GSTR-1/GSTR-3B switch.
- [x] Excel report (`save_gstr2b_xlsx`): summary by status with ITC comparison, every row side by side with differences and decisions, and the purchase register with 2B status. PDF report not done.
- [ ] Later: portal Excel import; manual pairing of a 2B line with a chosen purchase; PDF mismatch report; feeding 2B figures into the GSTR-3B workpaper (table 4 against 2B).


### 2.5 Offline filing and period close ✅
- [x] Versioned return pack (`gst_engine::returns`, `rules/return-pack.json`, version `2026.11.1`): the returns (GSTR-1 due on the 11th, GSTR-3B on the 20th, which document families each locks) and every check a return can raise with its severity. Errors block the filing pack and recording the return as filed; warnings don't. Severity is looked up in the pack, and a check the pack doesn't know counts as an error. The return centre's due dates come from the pack.
- [x] Validation (`application::filing`): the GSTR-1/GSTR-3B exceptions plus filing checks: month not over (error), previous month's return not recorded as filed, GSTR-1 not filed before GSTR-3B, GSTR-2B not imported or rows still to review (warnings). The nil-rated GSTR-1 ↔ 3B difference now has its own code (`gstr1_nil_mismatch`, a warning).
- [x] Offline channel: the filing pack (zip: portal JSON for GSTR-1 or the Excel workpaper for GSTR-3B, the workpaper JSON, the open warnings and a manifest with SHA-256s) is refused while errors remain; recording needs the 15-character ARN, a filing date after the month ends and not in the future, and takes an optional acknowledgement (PDF/PNG/JPG, 5 MB) and note. Recording is refused while errors remain or when the ARN is already used. Migration 0012 adds `gst_filing` (one filed record per GSTIN, return and month) and `gst_filing_file`; the portal file, workpaper and acknowledgement are frozen in `attachment_blob`, so backups carry them. Headline figures are stored, and the page warns when the books no longer match them.
- [x] Period lock after filing: a filed GSTR-1 locks the month's sales of that GSTIN; a filed GSTR-3B locks sales and purchases. Issuing, recording and cancelling in a locked month are refused by the core (with a message pointing to a credit/debit note in an open month) and by database triggers. Drafts stay editable.
- [x] ITC after filing: a line still pending when the month's GSTR-3B was filed can be reviewed; its ITC moves to the first month whose GSTR-3B isn't filed (`purchase_line.itc_period`) and shows there as "From October 2026". A line whose ITC was already filed can't be re-reviewed.
- [x] Compliance pack (zip, read-only): the frozen files, the acknowledgement, a manifest with the ARN, filing date, pack versions and figures as filed, and the filing's audit trail. Removing a filing record needs the owner and a reason; the record stays as removed and the month opens again. Recording, removing and pack exports are audit-logged.
- [x] UI: a filing panel on the GSTR-1 and GSTR-3B pages (status, filing checks, Export filing pack, Record as filed, Compliance pack, Remove record); the return centre shows "Filed <date>" or "Not filed · was due <date>", and the next-due banner skips filed returns. GSTR-1's own JSON export is hidden once filed.
- [ ] Later: correcting an already-claimed ITC in a later return (4B(2)/4D(1) adjustments); QRMP quarterly filing; GSTR-3B portal JSON; filing through a GSP (Phase 3).

### 2.6 Advances, refunds and review
- [ ] Carried from 1.5: adjust advances against later invoices, refunds, GST on advances (receipt vouchers).
- [ ] Manual tax override with reason (owner/accountant).
- [ ] Golden test set reviewed by a GST practitioner (exit gate).

## Phase 3 — Filing integration ⬜

- [ ] `GstProvider` core trait (`authenticate`, `fetch_gstr2b`, `upload_gstr1`, `get_return_status`) via `async_trait`, plus capability traits `ReturnFiling`, `Gstr3bWorkflow`, `TaxPayment`, `EInvoicing`; capability flags; `MockGstProvider` implementing all.
- [ ] Online channel in `FilingChannel`; per-step fallback to offline when the GSP lacks a capability.
- [ ] Taxpayer-held GSP credentials stored in the Windows credential store for personal mode (D22). No hosted gateway.
- [ ] Submission state machine (`draft` … `filed` requires ARN), idempotency keys, payload hashes, correlation IDs.
- [ ] Approval gate and manual filing evidence capture; read-only compliance pack; period lock.
- [ ] Real `GspProvider` behind trusted backend (after GSP agreement).

## Phase 4 — Small-business polish ⬜

- [ ] Recurring invoices/expenses, payment reminders (user-triggered email/WhatsApp link).
- [ ] Richer bank reconciliation; simple inventory.
- [ ] Pluggable backup target: encrypted backup to a cloud folder/storage (personal mode).
- [ ] E-invoice IRN / e-way bill (where applicable via provider).

## Phase 5 — Commercial mode ⬜

- [ ] `persistence::postgres` implementing the same repository traits; PostgreSQL migrations; row-level security as second line of tenant isolation.
- [ ] Axum `api-server` + OpenAPI over the same use cases; context from OIDC session.
- [ ] Web client: HTTP transport in the UI's API layer (same React codebase).
- [ ] Roles enforced in UI and API, tenant onboarding, server-side GSP secrets, managed backups.

## Phase 6 — Offline sync ⬜

- [ ] Desktop keeps a SQLite working copy for commercial tenants; outbox of local changes and sync protocol.
- [ ] Conflict handling via `row_version`; append-only journals sync without conflicts.
- [ ] Offline document numbering (per-device number blocks, or block offline issue of GST documents).

## Change log

| Date | Change |
|---|---|
| 2026-11-17 | Plan created; Phase 0 closed; Phase 1.0 scaffold delivered. |
| 2026-11-17 | Phase 1.1 business setup delivered (migration 0002, setup use cases, wizard, Settings tabs). |
| 2026-11-18 | Architecture review: spec v0.3 adds D19–D23 (two deployment modes, ports + request context, online-only commercial v1, GSP gateway, filing channels). Added 1.1b, Phase 6; Phase 5 renamed Commercial mode. |
| 2026-11-18 | Phase 1.1b delivered: `RequestContext`/roles, persistence ports + `persistence::sqlite` adapter (migrations moved to `migrations/sqlite/`), organisation-scoped queries, tenant-isolation and role tests. |
| 2026-11-18 | D22 decided: no hosted gateway; personal single-business mode first. Phase 1.2 delivered: migration 0003 (party, item, `journal_line.party_id`), masters use cases, ledger-posted opening balances, CSV/Excel import, Parties and Items screens. |
| 2026-11-18 | Phase 1.3 delivered: migration 0004 (sales_document, sales_line, freeze triggers), GST engine document calculation, sales use cases and screens (list, editor, document view, credit notes, cancellation). |
| 2026-11-19 | Phase 1.4 delivered: migration 0005 (purchase_document, purchase_line, attachment, attachment_blob, freeze triggers), purchases/ITC/attachments use cases and screens; posting lines can target a chosen account; engine document calc reused. |
| 2026-11-19 | First desktop launch with `pnpm tauri dev` confirmed by the product owner (migrations 0001–0005 applied to the local database). |
| 2026-11-19 | Phase 1.5 delivered: migration 0006 (money_account, account_opening, money_voucher, voucher_allocation, bank_statement_line, freeze triggers, backfill of cash/bank accounts), banking use cases, 21 Tauri commands, Banking screens (accounts, receipts & payments, editor, books, statement import, reconciliation), opening balances in Settings, settlements on invoices and bills. Dashboard cash & bank now sums every money account. |
| 2026-11-19 | Brand kit applied (docs/brand-kit): theme tokens from the brand palette (brand blue #0A84FF; text-on-light shades deepened for WCAG AA), SF Pro → Inter font stack, 10px control radius, folded-A mark + wordmark in sidebar and setup, app icons (`pnpm tauri icon docs/brand-kit/app-icon-1024.png`) and web favicons. Release workflow `.github/workflows/release.yml`: tests, then Windows/macOS (arm64 + x64)/Linux builds on every push to main, published to release `v<Cargo version>`; version lives only in Cargo.toml. |
| 2026-11-19 | Phase 1.6 delivered: migration 0007 (books_lock + posting trigger, fy_close, organisation_logo), report queries (account totals, journal rows, party book, outstanding documents), reports/lock/close/logo use cases, 14 Tauri commands, Reports section (8 reports with drill-down), Settings › Books & periods, logo upload in Settings › Business and on the invoice preview. |
| 2026-11-19 | Phase 1.7 delivered: `document-render` crate (Typst 0.15, bundled Inter, golden-file tests), invoice Print/Save PDF, migration 0008 (`backup_log`), encrypted backup/restore with verification and restart-time swap, restore on first run, last-backup indicator, optional PIN app lock with recovery code and host-side command gate, CSP and capability tightening, `tauri-plugin-dialog` (Rust-side only). |
| 2026-11-19 | Release workflow fix: `bundle.targets` was limited to MSI/NSIS, so the macOS and Linux jobs built but produced no installers ("No artifacts were found"). Now `all`: Windows MSI + NSIS, macOS .app + .dmg (Apple silicon and Intel), Linux .deb + .rpm + AppImage. |
| 2026-11-19 | In-app updates: `tauri-plugin-updater` driven from Rust (`check_update`, `install_update` with a progress channel, `open_releases_page`); feed is `releases/latest/download/latest.json`, update bundles signed with the project's updater key (public key in `tauri.conf.json`, private key as the `TAURI_SIGNING_PRIVATE_KEY` repo secret) and verified before install. The app checks once a few seconds after start and offers the update in a toast; Settings › Appearance & about shows installed and latest version with Update / Check again. Windows installs with the per-user NSIS installer (passive), macOS replaces the .app, Linux self-updates only as AppImage (.deb/.rpm get the download page). An update is only offered when the version in Cargo.toml goes up. |
| 2026-11-20 | Phase 1 closed after the desktop exit check on macOS. Phase 2 split into 2.1–2.6. Phase 2.1 delivered: GST rule pack (JSON, versioned, validated) in `gst-engine` with `rules` and `checks` modules, migration 0009 (`rule_pack_version` on sales/purchase documents), date-aware rate checks, non-blocking tax warnings in the sales and purchase editors, GSTR-1 placement on invoices, HSN/SAC lookup (`hsn_lookup`, `rule_pack_info` commands) in the item editor (132 Rust tests). Version 0.2.1. |
| 2026-11-21 | Phase 2.2 delivered: return centre and GSTR-1 workpaper (`application::returns`, commands `return_centre`, `gstr1`, `save_gstr1_json`), new `gst-formats` crate with the GSTR-1 portal JSON adapter and golden files, GST Filing pages (`/gst`, `/gst/gstr1/:period`) replacing the placeholder (136 Rust tests). Version 0.2.2. |
| 2026-11-21 | Phase 2.3 delivered: GSTR-3B workpaper (`application::gstr3b`, command `gstr3b`, `save_gstr3b_xlsx`), payment proposal in `gst_engine::offset` (rule 88A), rule 42 common credit with migration 0010 (`purchase_line.itc_common`), GSTR-1 cross-check, Excel workpaper, GSTR-3B page and return-centre columns (142 Rust tests). Version 0.2.3. |
| 2026-11-21 | Fixes: Overview "New invoice" and "Record bill" open their editors; import templates are saved through a Save dialog (`save_import_template`) with a confirmation. Version 0.2.4. |
| 2026-11-22 | Phase 2.4 delivered: GSTR-2B JSON import (`gst_formats::gstr2b`), reconciliation engine (`gst_engine::recon`), migration 0011 (statements, lines, decisions), use cases `gstr2b`, `import_gstr2b`, `decide_gstr2b`, Excel report, GSTR-2B page and return-centre column (149 Rust tests). Version 0.2.5. |
| 2026-11-22 | Phase 2.5 delivered: return pack (`gst_engine::returns`), migration 0012 (`gst_filing`, `gst_filing_file`, `purchase_line.itc_period`, lock triggers), `application::filing` (validation, filing pack, record/remove filing, compliance pack), GST month lock in sales and purchases, late ITC moved to the next open month, filing panel and return-centre filed status (154 Rust tests). Future plan for mobile layouts written to `docs/mobile-ready-design.md`. Version 0.2.6. |
