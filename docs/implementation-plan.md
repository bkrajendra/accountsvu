# AccountsVU — Phase-wise Implementation Plan

Companion to [`accountsvu-spec.md`](accountsvu-spec.md) (v0.3). Decision IDs (D1–D23) refer to spec §12.
Last updated: 18 Nov 2026.

## Phase status

Status values: ⬜ Not started · 🟡 In progress · ✅ Done · ⛔ Blocked

| Phase | Title | Status | Decisions taken | Opens / attention needed | Comments |
|---|---|---|---|---|---|
| 0 | Product discovery | ✅ Done | D1–D18 recorded in spec §12 (Windows-only, local-first SQLite, Tauri-command transport, paise storage, per-line rounding + Round-off, Typst PDFs, passphrase backups, mock GSP first) | **GST practitioner reviewer not yet named (D6).** **No GSP selected (D3).** Both block Phase 2/3 exit, not their start. | Spec bumped to v0.2. |
| 1 | Accounting foundation | 🟡 In progress | Cargo workspace at root with `crates/`; runtime SQLx queries; Tailwind v4 + Inter (bundled) + light/dark/system theme. 1.1: composition registrations rejected for now; one organisation per install; series format locks after first number; optimistic concurrency via `row_version`. 1.2: party opening balances post to the ledger (reversal on change); masters editable by Owner and Accountant; import skips invalid rows rather than rejecting the file; account opening balances moved to 1.5. 1.3: invoice dates can't be in the future or out of order within a series; HSN/SAC required on GST lines; exports/SEZ choose IGST or zero-rated under LUT; outward reverse charge, e-invoice IRN and PDF not yet. 1.4: attachments stored inside SQLite (so backups include them); purchase tax is computed from rates — matching the supplier's printed paise and GSTR-2B comes in Phase 2; imports of goods (bill of entry) not yet | Bank statement CSV profiles: need 2–3 sample statements from the banks you use. Invoice PDF layout: need branding/sample invoice you like. Desktop launch (`pnpm tauri dev`) confirmed working on 19 Nov 2026. | **1.0 scaffold done. 1.1 business setup done:** onboarding wizard, business profile, GSTINs, number series, default chart of accounts, audit log (48 Rust tests). **1.1b architecture alignment done:** request context + roles, repository traits with a SQLite adapter, every query scoped by organisation. **1.2 masters done:** parties with ledger-posted opening balances, items, CSV/Excel import. **1.3 sales done:** invoices, bills of supply, credit notes, cancellation, live GST engine calculation, ledger posting. **1.4 purchases done:** bills, expenses, debit notes, reverse charge, per-line ITC review, SHA-256 attachments (81 Rust tests). Next: 1.5 receipts, payments, banking. |
| 2 | GST workbench | ⬜ Not started | Rule pack format = versioned JSON shipped with app (D12). Engine stays pure, GSP-free (D23). Offline filing channel + versioned format adapters (`gst-formats`) built here so ThingsVU can file without a GSP. | Portal JSON schemas for GSTR-1 / 2B sample files needed for golden tests. Reviewer for scenario catalogue. | |
| 3 | Filing integration | ⬜ Not started | Mock provider + export first (D3). `GstProvider` core trait + capability traits; submission and payment separate (D23). | GSP selection, sandbox access and commercial terms. D22: no hosted gateway; personal mode uses offline channel or taxpayer-held GSP credentials. | |
| 4 | Small-business polish | ⬜ Not started | Cloud backup target added here (D19) | Which convenience features matter most to pilot users. Cloud storage choice for backups. | |
| 5 | Commercial mode | ⬜ Not started | PostgreSQL adapter behind the same repository traits (D9, D20); online-only clients (D21) | Hosting target, OIDC provider, tenant onboarding/licensing model. | |
| 6 | Offline sync | ⬜ Not started | SQLite as offline store for commercial clients (D21) | Offline numbering rule: per-device number blocks vs no offline GST documents. | |

## Phase 0 — Product discovery ✅

- [x] Analyse spec, raise questions, record decisions D1–D18.
- [x] Adopt scope additions (FY close, Round-off ledger, bank CSV profiles, posting idempotency key, drill-down pattern, GSTR-2B fixtures).
- [ ] Name GST practitioner reviewer (carried forward; needed before Phase 2 exit).
- [ ] Shortlist GSPs (carried forward; needed before Phase 3 start of real adapter).

## Phase 1 — Accounting foundation 🟡

Exit condition: MVP bookkeeping acceptance criteria (spec §9.2 items 1–4, 9, 10) pass.

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

### 1.5 Receipts, payments, banking
- [ ] Receipts/payments with allocation to documents.
- [ ] Cashbook/bankbook; bank CSV import with per-bank mapping profile; basic reconciliation.

### 1.6 Ledger and reports
- [ ] Journal register, general ledger, trial balance, P&L, receivables/payables ageing.
- [ ] Period lock / reopen with owner reason; FY close and carry-forward.
- [ ] Drill-down from every figure to source.

### 1.7 Documents, backup, security
- [ ] Typst invoice PDF (D16) with golden-file tests.
- [ ] Encrypted backup/restore with passphrase (D17), restore verification, last-backup indicator.
- [ ] Optional PIN/password app lock (D14).
- [ ] Restrictive CSP and Tauri capabilities review.

## Phase 2 — GST workbench ⬜

- [ ] Versioned rule pack (JSON) + loader; effective-dated rates; override with reason.
- [ ] Engine coverage per spec §5.6: exempt/nil/non-GST, RCM liability and self-invoice data, ITC eligibility incl. s.17(5) blocked credits and rule 42/43 reversal workpaper, credit/debit note effects.
- [ ] Place-of-supply determination and warnings (missing GSTIN/HSN, unusual rate, ambiguity).
- [ ] Return centre per GSTIN/period: due dates, completeness, exceptions, estimated liability.
- [ ] GSTR-1 workpaper (B2B, B2CL, B2CS, CDNR, CDNUR, EXP, HSN summary, docs issued) + portal JSON export.
- [ ] GSTR-3B workpaper (3.1, 3.2, 4, 5, 6.1) with 1↔3B and 2B↔books cross-checks.
- [ ] GSTR-2B import (JSON/Excel) and reconciliation (exact/likely/missing/duplicate/mismatch/pending).
- [ ] Validation framework: blocking errors vs warnings.
- [ ] `gst-formats` crate: versioned format adapters with golden-file tests — GSTR-1 JSON export, GSTR-2B JSON/Excel import, purchase register Excel/CSV, 2B reconciliation Excel/PDF, GSTR-3B workpaper PDF/Excel, return summary PDF.
- [ ] `FilingChannel` with the **offline channel**: export pack, manual filing evidence (ARN, date, acknowledgement).
- [ ] Golden test set reviewed by GST practitioner (exit gate).

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
