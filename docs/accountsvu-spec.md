# AccountsVU-Small Business Accounting & GST Desktop Application

**Product and Technical Specification**  
Version 0.3 | Decisions recorded (see §12) | India GST | 18 November 2026

## 1. Executive summary

Build a focused accounting application for Indian micro and small businesses. It should make day-to-day sales, purchases, payments and GST compliance understandable to a proprietor who is not an accountant. The first release should be desktop-first, local-first and usable without a permanent internet connection for ordinary bookkeeping.

**Recommended stack:** Rust domain/application core, Axum REST API, SQLx with SQLite for local mode and PostgreSQL for optional hosted multi-user mode, Tauri 2 desktop shell, and React + TypeScript UI.

**Two deployment modes, one core (D19).** *Personal* mode is the Tauri desktop app with a local SQLite database and optional encrypted cloud backup. *Commercial* mode is Tauri and web clients talking to a hosted Rust API backed by PostgreSQL. The business logic (accounting, GST engine, use cases) is identical in both modes; only the persistence, transport and deployment adapters change (§7.2).

**We own the GST engine; GST connectivity is pluggable (D23).** All tax calculation, classification, return preparation and reconciliation live in our own engine with no GSP dependency. Getting data to and from GSTN is a separate, replaceable *filing channel*: an offline channel that produces portal-compatible files, or an online channel backed by a GSP provider adapter (§5.8). This lets the first user (ThingsVU) run the product without paying for a GSP, lets us sell to other businesses later, and lets us add or change GSPs without touching the accounting or GST modules.

The product should prepare GST returns and automate the exchange of data with GST systems through an authorised GST Suvidha Provider (GSP). Do not design around direct, unrestricted calls to GSTN production APIs. GSTN describes the ASP model as a software provider connecting through a GSP, and states that an ASP needs a GSP tie-up. [S1][S2]

The phrase **auto submission** must be implemented carefully. The application can automatically assemble, validate, reconcile and transmit return data where the chosen provider supports it. Final filing still requires the taxpayer's authorised signatory and applicable EVC/DSC verification. GSTR-3B also involves tax-liability offset and, where necessary, a challan and payment. The product must not silently file returns, invent an authorisation, store an OTP, or make a bank payment without an explicit user action. [S3][S4]

## 2. Product goals and principles

1. A proprietor can issue an invoice, record a purchase and understand outstanding customer/supplier balances without accounting training.
2. Every business transaction creates a balanced, traceable accounting entry.
3. GST calculations are rule-driven, date-effective and reviewable, not scattered hard-coded formulas.
4. A GST period can be closed through a guided workflow with clear exceptions, tax estimates, sign-off and filing evidence.
5. Local bookkeeping continues during an internet outage. GST sync and filing require connectivity.
6. The user remains in control of tax classification, ITC decisions, payment and final legal filing.

### 2.1 Initial target and assumptions

- First user: **ThingsVU**, in personal mode, without a GSP (offline filing channel). The product is then offered to other businesses in commercial mode.
- India-based micro/small business; desktop-first on Windows, with macOS/Linux support designed in.
- Start with **regular GST taxpayers**. Support one business entity with one or more GST registrations (GSTINs); keep the data model ready for more than one legal entity.
- Goods and services sales, purchases, expenses, customer/vendor balances, basic cash/bank tracking and GST returns.
- Default mode: one user on one computer, SQLite database. Optional hosted deployment enables multiple users and PostgreSQL.
- Composition taxpayers, specialised returns, payroll, manufacturing, job work and complex inventory are not in MVP; leave extension points.
- The application is an accounting and compliance aid, not a replacement for a qualified tax professional. Return applicability and tax treatment must be confirmed for each taxpayer.

## 3. Scope and release boundaries

### 3.1 MVP (must have)

- Business/GST profile, GSTINs, financial year and invoice numbering.
- Customer, supplier, item/service and HSN/SAC masters.
- Sales invoices, credit notes, purchase bills, debit notes and expense entries.
- Receipts, supplier payments, cashbook and basic bank statement import/reconciliation.
- Double-entry ledger, chart of accounts, trial balance, profit and loss, receivables and payables.
- GST tax engine for common intra-state/inter-state supplies and tax components.
- GSTR-1 and GSTR-3B workpapers, validation, preview and filing status tracking.
- GSTR-2B import and purchase-to-2B reconciliation for ITC review.
- CSV/Excel import/export, invoice PDF, data backup/restore and audit log.
- GST filing centre with manual/export mode first and a provider adapter for a GSP integration.

### 3.2 Release 1.1 / 2 (recommended)

- GSP-based GST return submission and status retrieval, once a commercial/technical provider agreement is in place.
- QRMP/IFF support, IMS workflow support where exposed by the selected provider, recurring invoices/expenses, payment reminders, purchase document attachments and richer bank reconciliation.
- E-invoice IRN generation and e-way bill generation where applicable and supported by the selected IRP/GSP integration.
- Simple inventory: opening stock, stock movements, low-stock alerts and item-wise margin.

### 3.3 Later / optional

- Composition scheme and CMP-08/GSTR-4; annual returns GSTR-9/9C where applicable; TDS/TCS, ISD, export-specific workflows and advanced reverse-charge cases.
- Multi-branch, multi-company, accountant portal, cloud sync, mobile companion, OCR-assisted bill capture and payment-gateway integration.
- Payroll, fixed-asset depreciation, manufacturing/BOM and advanced inventory should be separate modules, not MVP blockers.

## 4. User roles and permissions

| Role | Main permissions |
|---|---|
| Owner / Admin | Business setup, user access, masters, all transactions, GST review, filing approval, backup and restore |
| Accountant | Transactions, ledgers, reconciliation, reports and GST workpapers; filing only if explicitly delegated |
| Operator | Create/edit draft invoices and purchases, record receipts/payments; cannot file returns or alter locked periods |
| Read-only / Auditor | View transactions, reports, return packs and audit history; no edits |

A single-user local installation can default to Owner. The permission model should still exist in the core so hosted mode does not require a redesign.

## 5. Functional requirements

### 5.1 Business setup

- Capture legal/trade name, PAN, constitution, address, state code, contact details, financial year start and currency (INR initially).
- Store each GSTIN separately with registration type, state, effective dates, filing frequency, authorised signatory display name and status.
- Capture invoice series by document type, GSTIN and financial year. Prevent duplicate invoice numbers within the relevant series and enforce chronological auditability.
- Maintain a configurable chart of accounts, opening balances, payment modes, banks and tax defaults.
- Import customer/vendor/product masters from CSV/Excel with a preview and field-mapping step.

### 5.2 Sales

- Create tax invoices, bills of supply (where enabled), quotations/estimates, credit notes and export invoices.
- Invoice fields: document number/date, supplier GSTIN, recipient name/GSTIN, billing/shipping address, place of supply, line items, HSN/SAC, quantity/unit, taxable value, discount, tax rate, CGST/SGST/UTGST/IGST/cess, reverse-charge indicator, supply category, notes and payment terms.
- Support B2B and B2C classification, intra/inter-state tax selection, exempt/nil/non-GST items and tax-inclusive pricing.
- Generate printable PDF with business branding, payment QR/UPI details (optional), bank information and terms.
- Track draft/issued/part-paid/paid/overdue/cancelled states. Issued documents must not be silently edited; use cancellation or credit/debit notes with an audit trail.
- Allocate customer receipts to invoices and show overdue ageing.

### 5.3 Purchases and expenses

- Record supplier bills, debit notes, cash purchases and operating expenses.
- Capture supplier GSTIN, supplier invoice number/date, receipt date, place of supply, HSN/SAC, taxable amount, tax breakup, reverse charge and ITC eligibility/reason.
- Attach original bill files or images. Store their hash and metadata for traceability.
- Track payment due dates, part-payments, supplier balances and overdue ageing.
- Distinguish capital assets, inventory, direct expenses and general expenses.
- Never infer ITC eligibility solely from the presence of a GSTIN or tax amount; require a reviewable eligibility status.

### 5.4 Items and services

- Maintain item/service name, code, description, HSN/SAC, unit of measure, default tax treatment, sales price, purchase cost and active dates.
- Tax rates and classification are effective-dated and configurable. A transaction stores a snapshot of the rate/classification used when it was issued.
- Allow non-stock services and stock-tracked goods. Inventory quantities are optional in MVP but the item model must support them.

### 5.5 Accounting ledger

Use **double-entry accounting** as the system of record. Every posted document generates balanced journal entries in one database transaction.

Examples:
- Sales invoice: Dr Accounts Receivable; Cr Sales; Cr Output CGST/SGST or IGST.
- Supplier bill: Dr Expense/Inventory/Asset; Dr Eligible Input Tax; Cr Accounts Payable.
- Receipt: Dr Bank/Cash; Cr Accounts Receivable.
- Supplier payment: Dr Accounts Payable; Cr Bank/Cash.
- GST payment: Dr GST payable/clearing; Cr Bank/Cash, linked to challan and payment evidence.

Requirements:
- Configurable chart of accounts, journal register, general ledger and trial balance.
- No floating-point arithmetic for money. Use fixed-precision decimal types and currency-safe rounding rules.
- Posted entries are immutable. Corrections use reversal/adjustment entries.
- Period lock after review/filing; reopening requires owner permission and an audit reason.

### 5.6 GST engine

The GST engine is entirely ours. It lives in the `gst-engine` crate, depends only on `domain` (no database, network or GSP code) and is a set of pure, deterministic functions: transaction facts and a rule-pack version in, tax results and return figures out. It is tested on its own with scenario and golden tests. All money uses `rust_decimal`.

Responsibilities:

| Area | Engine responsibility |
|---|---|
| Tax computation | Taxable value; CGST/SGST/UTGST/IGST/cess per line; document totals and rupee round-off (D11) |
| Supply classification | Intra-state vs inter-state from place of supply; B2B/B2C/B2CL; exports and SEZ (with/without payment); deemed exports |
| HSN/SAC | Lookup and validation against the rule pack; rate suggestion by HSN/SAC and date; HSN summary for returns |
| Exempt, nil-rated, non-GST | Separate treatment and reporting for each; never mixed with taxable value |
| Reverse charge (RCM) | Inward RCM liability (incl. purchases from unregistered persons where notified), self-invoice data, cash-only discharge; outward RCM flag |
| Input Tax Credit | Eligibility status per line (pending/eligible/ineligible/partial) with reason; blocked credits (s.17(5)); common-credit reversal (rules 42/43) as a workpaper; ITC reversal and reclaim tracking |
| Credit/debit notes | Tax adjustment linked to the original document; time-limit warnings; effect on returns and ITC |
| GSTR-1 aggregation | Table mapping (B2B, B2CL, B2CS, CDNR, CDNUR, EXP, NIL, HSN, DOCS) from saved source snapshots |
| GSTR-3B preparation | Sections 3.1, 3.2, 4, 5 and 6.1 working; liability vs ITC offset proposal (cross-utilisation order) |
| 2B reconciliation | Matching rules and scoring between the purchase register and GSTR-2B lines |
| Rule versioning | Versioned rule pack with effective-from/to dates and source references (D12); every result records the rule-pack version used |

Requirements:

- Determine tax treatment from effective-dated rules plus transaction facts: supplier/recipient registration, place of supply, supply type, HSN/SAC, exemption, export/SEZ status, reverse charge and applicable notification-driven conditions.
- Calculate taxable value and CGST/SGST/UTGST/IGST/cess separately. Preserve per-line and document totals.
- Provide tax calculation explanation and warnings for missing GSTIN, invalid state code, missing HSN/SAC, unusual rate, place-of-supply ambiguity and incomplete mandatory data.
- Maintain versioned rate/rule tables with effective-from/effective-to dates, source/notification reference and release identifier.
- Tax rule updates must be deployable independently from the desktop UI. Never overwrite historic transaction tax snapshots.
- Include a manual override path with reason, user and timestamp. High-risk overrides should require owner/accountant review.

### 5.7 GST return preparation and filing

**Return centre:** one workspace per GSTIN and tax period, showing due date, filing frequency, source data freshness, completeness, exceptions, estimated liability and filing status.

**GSTR-1:** aggregate outward supplies into the applicable tables from invoices, credit/debit notes and export/zero-rated supplies. Support amendment tracking and period-to-period carry-forward checks. Generate a draft summary and provider-specific payload/export.

**GSTR-3B:** prepare outward tax liability, reverse charge, eligible/ineligible ITC, exempt/nil/non-GST inward supplies, interest/late-fee fields and payment/offset working. Reconcile the 3B outward summary with GSTR-1 and compare ITC with GSTR-2B and books.

**GSTR-2B and ITC reconciliation:** import the statement through a supported GSP/API or portal download. Match by supplier GSTIN, document number/date, taxable value and tax components. Show exact match, likely match, missing in 2B, missing in books, duplicate, mismatch and pending review. GSTR-2B is an auto-drafted ITC statement and is not itself a return to file. Eligibility still requires taxpayer review. [S5]

**QRMP:** store the taxpayer's actual scheme/frequency as returned by GSTN/provider. Support quarterly GSTR-1/GSTR-3B and monthly payment/IFF workflow where applicable. Eligibility is generally linked to aggregate PAN-level turnover up to ₹5 crore and other conditions; do not infer eligibility from one GSTIN's sales alone. [S6]

**Validation:** run structural, arithmetic, master-data, duplicate-document, tax-period and return-rule validations. Show errors that block submission separately from warnings requiring review. Never silently drop an invoice.

**Filing flow:**
1. Close transaction capture for the period or mark late entries explicitly.
2. Refresh available GST data, GSTR-2B and provider status.
3. Reconcile sales and purchase data; resolve or explicitly acknowledge exceptions.
4. Generate GSTR-1 and GSTR-3B draft summaries and cross-check totals.
5. Validate, preview and save a versioned return pack.
6. Send data through the selected GSP/ASP integration or export the provider/portal-compatible file.
7. Require an authorised user to approve filing and complete EVC/DSC or other official verification required by the applicable workflow.
8. For 3B, show tax payable, available ITC/cash-ledger information (if available), challan/payment requirement and offset result. Payment must be explicitly confirmed by the user.
9. Retrieve filing status and ARN; save filed return, acknowledgements, challan references, API request IDs and provider response metadata.
10. Lock the period and create a read-only compliance pack.

**Auto-submission policy:**
- Default to human approval before final filing. Support automation of draft generation, validation, data transfer and status polling.
- A provider may support delegated authentication or a streamlined filing flow, but the product must follow the provider's current agreement and GSTN's current authentication/signing rules. Do not assume a reusable OTP or unattended EVC/DSC is permitted.
- Never store GST portal passwords, OTPs, DSC private keys or bank credentials in application settings. Use provider-authorised token flows and OS-backed secret storage only where contractually permitted.
- Never automate browser scraping or simulate clicks on the GST portal as a production integration.

### 5.8 GST integration strategy

Treat GST connectivity as a replaceable integration layer, not as core accounting logic. The return centre never talks to a GSP directly; it hands a prepared, validated return pack to a **filing channel** (D23).

```text
Return centre ──► FilingChannel
                    ├── Offline channel ──► gst-formats (versioned portal file adapters) ──► files the user uploads/downloads
                    └── Online channel  ──► dyn GstProvider (Mock, GSP A, GSP B, ...) ──► GSP ──► GSTN
```

- GSTN states that ASPs are software providers that route client GST data through GSPs and that there is no separate ASP empanelment; the software provider needs a tie-up with a GSP. [S1][S2]
- The offline channel is a first-class, permanent option, not a fallback. It needs no GSP and no credentials.

#### 5.8.1 Offline channel (portal-compatible files)

Generates files that work with the GST portal's offline workflows, and imports the files the portal produces. GSTN supports uploading suitably structured GSTR-1 JSON prepared by third-party software. [S3]

| Operation | Output / input |
|---|---|
| Generate GSTR-1 | Portal-compatible GSTR-1 JSON (for upload or the offline tool) |
| Import GSTR-2B | Portal GSTR-2B JSON/Excel download |
| Export purchase register | Excel / CSV |
| GSTR-2B reconciliation | Excel and PDF report (matched, mismatched, missing on each side) |
| GSTR-3B workpaper | PDF and Excel (section working, ITC, offset proposal) |
| Return summary | PDF, saved in the compliance pack |
| Manual filing evidence | User records ARN, filing date and acknowledgement upload |

**Format adapters are versioned.** Each portal schema (for example GSTR-1 JSON) has its own adapter, tagged with the portal schema version it implements and validated by golden-file tests. A schema change adds a new adapter version; old return packs keep the version they were produced with. Adapters live in the `gst-formats` crate and are pure functions from engine output to bytes (and bytes to engine input).

#### 5.8.2 Online channel (GSP provider)

A provider abstraction behind which each GSP gets its own adapter. It is object-safe (`dyn GstProvider`, via `async_trait`) so the provider is chosen at runtime from settings. The core trait covers what every GSP offers; further operations are separate capability traits that an adapter implements only if its GSP supports them.

```rust
#[async_trait]
pub trait GstProvider: Send + Sync {
    fn capabilities(&self) -> ProviderCapabilities;
    async fn authenticate(&self, request: AuthRequest) -> Result<AuthSession, GstError>;
    async fn fetch_gstr2b(&self, request: Gstr2bRequest) -> Result<Gstr2bData, GstError>;
    async fn upload_gstr1(&self, request: Gstr1Request) -> Result<UploadResult, GstError>;
    async fn get_return_status(&self, request: ReturnStatusRequest) -> Result<ReturnStatus, GstError>;
}

/// Submission and filing are separate steps from upload (spec §5.7).
#[async_trait] pub trait ReturnFiling: GstProvider { /* submit (freeze) return; file with EVC/DSC; fetch ARN */ }
#[async_trait] pub trait Gstr3bWorkflow: GstProvider { /* save 3B; fetch liability and ledger balances; offset liability */ }
#[async_trait] pub trait TaxPayment: GstProvider { /* create challan; fetch payment status */ }
#[async_trait] pub trait EInvoicing: GstProvider { /* IRN generate/cancel; e-way bill (Phase 4) */ }
```

Rules:

- **Return submission and tax payment are always separate operations,** each with its own explicit user approval. Uploading data never implies submitting, filing or paying.
- `ProviderCapabilities` flags tell the UI which steps the selected GSP can do online; anything else falls back to the offline channel for that step.
- `MockGstProvider` implements every trait, with scripted responses, for development and contract tests.
- Add a production GSP adapter only after selecting a provider and reviewing its API specifications, security terms, data-retention rules, commercial terms and sandbox.
- Integration contract tests per adapter; correlation IDs stored for every request; idempotency keys for operations that create or submit documents.
- **Where online calls run (D22):** our own GSP client secrets are never shipped in the Tauri binary. Focus is personal (local, single-business) mode; **no hosted gateway is built at this stage.** In personal mode the online channel is used only with GSP credentials issued to the taxpayer, stored in the OS credential store; without them the offline channel is used. In commercial mode the provider runs in the hosted API. A hosted gateway can be reconsidered if personal-mode customers need our GSP contract.

### 5.9 E-invoice and e-way bill readiness

- Keep the invoice schema ready for IRN, acknowledgement number/date, signed QR code, IRP response and cancellation metadata.
- The current notified e-invoicing threshold is aggregate annual turnover of ₹5 crore or more, based on PAN-level turnover in a preceding financial year, subject to notified exemptions. Reconfirm applicability and exemptions for each GSTIN. [S7]
- From 1 April 2025, taxpayers with AATO of ₹10 crore or more are subject to a 30-day reporting limit for covered e-invoices; implement configurable deadline warnings rather than hard-coding an assumption for all businesses. [S8]
- E-way bill rules generally require an e-way bill for movement of consignments above ₹50,000, subject to rule-specific and state-specific conditions. [S9]
- Keep IRP/GSP integration optional for businesses not covered by the mandate.

### 5.10 Reports and small-business features

**Must-have reports:** sales register, purchase register, expense register, cashbook, bankbook, customer outstanding, supplier outstanding, invoice ageing, GST tax summary, ITC reconciliation, GSTR-1 preview, GSTR-3B preview, general ledger, trial balance and profit & loss.

**Recommended convenience features:**
- Payment reminders and overdue alerts by email/WhatsApp link (user-triggered initially).
- Recurring invoices and recurring expenses.
- Document attachments and searchable notes.
- Import/export from common spreadsheet formats.
- Barcode/QR scanning for item lookup, optional.
- Simple inventory and low-stock notification after MVP.
- Multi-GSTIN dashboard and consolidated turnover view while preserving GSTIN-wise returns.
- Accountant export pack with ledgers, registers, source documents and return acknowledgements.
- Backup reminders, restore verification and a visible last-backup timestamp.

## 6. UX and visual design direction

Use modern Apple-like product principles without copying Apple's proprietary interface: restrained surfaces, strong hierarchy, direct manipulation, generous spacing, predictable navigation, crisp typography and low visual noise.


### 6.1 Application shell

- Desktop window with a persistent left sidebar (collapsible), top command/search area and contextual page actions.
- Sidebar: Overview, Sales, Purchases, Parties, Items, Banking, Reports, GST Filing, Settings.
- Keep the current GSTIN and financial year visible in the top context selector.
- Use a calm neutral background, white/near-white content surfaces, a single blue accent, subtle separators and restrained shadows. Use translucency only for navigation or floating overlays where it improves depth.
- Use system fonts and accessible typography. Avoid tiny labels, dense grids and excessive gradients.

### 6.2 Dashboard

Show five useful cards: Sales this month, Purchases this month, Cash/bank balance, Customer dues and Estimated GST payable. Add a prominent “Next actions” list: invoices due, bills to review, GSTR-1/3B due, GSTR-2B mismatches and backup status. Do not show a tax estimate as a filed/final liability.

### 6.3 Key screens

- **New invoice:** single-page form with customer selection, line-item grid, live tax summary and clear Save Draft / Issue Invoice actions. Advanced fields stay collapsed until needed.
- **Purchases:** bill list with supplier, bill date, gross amount, payment status and ITC review status.
- **GST Filing:** stepper with Prepare → Reconcile → Validate → Review → Authorise & File → Filed. Show errors and warnings inline, not only in a modal.
- **Reports:** date/GSTIN filters, clear totals, export action and drill-down to source transactions.
- **Settings:** GST profile, numbering, tax defaults, users, provider connection, backups and audit export.

### 6.4 Interaction rules

- Prefer inline editing and keyboard-friendly data entry for invoice lines.
- Destructive actions require confirmation and a reason where they affect posted accounting data.
- Statuses use text and icons as well as colour. Meet WCAG AA contrast for text and focus indicators.
- Every tax number should be drillable to the invoices or ledger entries that produced it.
- Avoid dashboard overload; show exceptions and next actions rather than a wall of charts.

### 6.5 Theme, typography and motion

- **Themes:** Light, Dark and System (follows the OS), switchable from the top bar and Settings. The choice is remembered per install. Colours are defined once as CSS tokens per theme. Theme changes cross-fade gently instead of flashing.
- **Typeface:** Inter (variable), bundled with the app so it works offline. Tabular numerals (`font-variant-numeric: tabular-nums`) for every amount column. Tracking is size-specific: tighter on large headings, neutral on body text.
- **Materials:** the sidebar and top bar are translucent layers (`backdrop-filter`) over a calm neutral canvas. Content cards are solid surfaces with hairline borders and soft shadows.
- **Motion:** feedback on pointer-down (a 100–160 ms press scale); popovers and menus scale from their trigger; 150–250 ms with a strong ease-out curve; no animation on keyboard-driven actions or high-frequency navigation. `prefers-reduced-motion`, `prefers-reduced-transparency` and `prefers-contrast` are honoured.

## 7. Technical architecture

### 7.1 Recommended components

| Layer | Recommendation | Responsibility |
|---|---|---|
| Desktop shell | Tauri 2 | App lifecycle, secure IPC, native menus, auto-update, OS integration |
| UI | React + TypeScript + Vite | Screens, forms, tables, state and accessibility |
| UI toolkit | Tailwind CSS v4 + design tokens (shadcn/ui-style components as needed) | Consistent components and design tokens; TanStack Query/Table, React Hook Form + Zod and Zustand are suitable supporting libraries |
| Transport | Tauri commands (desktop, D8); Rust + Axum + Tokio REST (commercial) | Both are thin adapters over the same `application` use cases; authentication and request context are resolved here |
| Domain core | Rust workspace crates | Accounting, tax calculation, validation and GST return mapping |
| Persistence | SQLx + SQLite (local); PostgreSQL (hosted) | Transactions, migrations, query layer |
| Money/date | rust_decimal; chrono/time | Exact monetary arithmetic and tax-period handling |
| Documents | Rust PDF/XLSX libraries or controlled HTML-to-PDF renderer | Invoice and report output |
| Secrets | OS keychain/keyring; server-side secret manager in hosted mode | Provider tokens and encryption keys |

### 7.2 Deployment modes

```text
        Client applications:   Desktop (Tauri + React)        Web (React + TypeScript)
                                       │ typed Tauri IPC              │ HTTPS REST
                                       ▼                              ▼
Transport adapters:            src-tauri commands            apps/api-server (Axum, auth)
                                       └──────────┬───────────────────┘
                                                  ▼  RequestContext (actor, organisation, role)
Rust application core:         application (use cases)  ── same code in both modes
                                 ├── accounting (ledger, invoicing, payments)
                                 └── gst-engine (calculations, returns, reconciliation)
                                                  │
GST integration layer:         gst-providers: FilingChannel ── Offline (gst-formats) | Online (dyn GstProvider)
                                                  │
Persistence adapters:          persistence: repository traits ── SQLite (personal) | PostgreSQL (commercial)
```

| Mode | Clients | Core runs in | Authoritative database | Notes |
|---|---|---|---|---|
| **Personal** (MVP) | Tauri desktop | The Tauri process | Local SQLite | Fully offline for bookkeeping; optional encrypted cloud backup (D17, D20); GST filing via the offline channel, or online with the taxpayer's own GSP credentials (D22) |
| **Commercial** | Tauri desktop and web | Hosted Axum API | PostgreSQL | Multi-tenant, OIDC, roles enforced, server-side GSP secrets. v1 clients are online-only (D21) |
| **Commercial + offline** (later) | Tauri desktop | Hosted API + local core | PostgreSQL; SQLite is a local cache and offline working store | Needs sync (D21) |

**Business logic is identical in every mode.** Only these adapters change: the persistence implementation behind the repository traits (§7.3), the transport (Tauri commands or REST), how the request context is built (fixed local owner or authenticated user), backup target, and where the GST provider runs.

**Personal mode:** Tauri hosts the Rust application core in-process. React talks to it only through validated Tauri commands (D8). No local HTTP listener ships. SQLite is the authoritative database. All bookkeeping works offline; online GST operations need internet.

**Commercial mode:** the same `application` crate runs inside an Axum server with PostgreSQL, organisation-level tenant isolation, OIDC authentication, role-based access, encrypted backups and server-side GSP credentials. The React UI is the same codebase for desktop and web; it has a transport layer with a Tauri implementation and an HTTP implementation. Do not expose the local API to the LAN by default.

**Commercial with offline working (later phase, D21):** PostgreSQL stays authoritative; the desktop keeps a SQLite copy for offline work. This needs an outbox of local changes, a sync protocol, conflict handling via `row_version`, and a rule for document numbers issued offline (per-device pre-allocated number blocks, or no offline issue of GST documents). The current data model already supports it: UUID v7 ids generated by clients, `organisation_id` on every row, `row_version` on editable rows, and append-only journals.

### 7.3 Suggested Rust workspace

Adopted layout (decision D7). It keeps the standard Tauri project shape and adds a Cargo workspace at the root:

```text
Cargo.toml              # workspace root
src-tauri/              # Tauri 2 desktop host: thin IPC command layer only
crates/
  domain/               # entities, value objects (Money, Gstin, StateCode, ...)
  accounting/           # journals, ledgers, posting and period locks
  gst-engine/           # tax calculation, rule versions, return workpapers
  persistence/          # repository traits (ports) + record types; sqlite/ adapter now, postgres/ adapter in Phase 5
  application/          # use cases and orchestration; RequestContext; the only API transports call
  gst-formats/          # (Phase 2) versioned portal file adapters: GSTR-1 JSON, 2B import, Excel/CSV/PDF workpapers
  gst-providers/        # (Phase 2–3) FilingChannel, GstProvider + capability traits, mock provider, GSP adapters
  document-render/      # (Phase 1) Typst invoice/report PDF generation
  api-contract/         # (Phase 5) DTOs and OpenAPI schema
apps/api-server/        # (Phase 5) Axum binary for commercial mode
src/                    # React + TypeScript UI (desktop and web)
```

**Persistence ports (D20).** The `application` crate depends only on repository traits (`OrganisationRepo`, `RegistrationRepo`, `AccountRepo`, `SeriesRepo`, `AuditRepo`, `LedgerQueries`, …), grouped into a `UnitOfWork` that commits or rolls back as one transaction, and a `Store` that opens units of work. Each database engine implements the traits with its own SQL and its own migrations. Engine-specific features (SQLite triggers, `IFNULL`, `?n` placeholders) stay inside the adapter. The engine's job is storage and integrity constraints; business rules stay in Rust.

### 7.4 Core data model

| Entity | Purpose / key fields |
|---|---|
| Organisation | Legal entity, PAN, base currency, settings |
| GstRegistration | GSTIN, state, registration type, effective dates, filing frequency |
| User / Role | Identity, permissions, organisation membership |
| Party | Customer/supplier identity, GSTIN, addresses, terms |
| Item | HSN/SAC, unit, default tax treatment, stock flag |
| TaxRuleVersion | Code, rate, components, conditions, effective dates, source reference |
| SalesDocument / SalesLine | Invoice, credit note, line snapshots, tax breakup, status |
| PurchaseDocument / PurchaseLine | Supplier bill, debit note, ITC status/reason and tax breakup |
| Receipt / Payment | Cash/bank movement and allocation to documents |
| JournalEntry / JournalLine | Immutable double-entry accounting source of truth |
| GstPeriod / ReturnWorkpaper | GSTIN, period, return type, source snapshot, validations, totals |
| Gst2bDocument / Reconciliation | Supplier statement line, matching status, review decision |
| FilingSubmission | Provider, return type, payload hash, state, ARN, correlation IDs |
| Attachment / AuditEvent | Source-file metadata, actor, timestamp, before/after reference |

Amounts use fixed precision. Store monetary values in integer paise or a decimal type with explicit scale; never use binary floating-point for tax calculations. Use database transactions for posting and unique constraints for GSTIN/document-number/financial-year series.

### 7.5 API design

**Request context (D20).** Every use case receives a `RequestContext`: the acting user (id, display name), the organisation (tenant) being acted on, and the user's role. The transport builds it. In personal mode it is a fixed local Owner, and the organisation is the single one on the install. In commercial mode it comes from the authenticated OIDC session and the selected organisation. Use cases check the role's permission before doing anything, scope every repository call by `organisation_id`, and write the actor into the audit log. No use case reads "the" organisation without going through the context.

The Tauri commands and the REST endpoints below are two transports over the same use cases and return the same DTOs and error codes.

- REST under `/api/v1`; JSON request/response DTOs; OpenAPI specification generated from the Rust API.
- Use pagination, filtering, stable identifiers, idempotency keys and optimistic concurrency for edits.
- All writes return a resource ID and a clear validation result. Use consistent machine-readable error codes.

Example endpoint groups:

| Method / path | Purpose |
|---|---|
| `POST /api/v1/sales-invoices` | Create a draft invoice |
| `POST /api/v1/sales-invoices/{id}/issue` | Validate, post journal and issue |
| `POST /api/v1/purchase-bills` | Record a supplier bill |
| `POST /api/v1/receipts` / `POST /api/v1/payments` | Record cash/bank movement |
| `GET /api/v1/reports/trial-balance` | Trial balance |
| `GET /api/v1/gst/periods/{period}` | GST period status and checklist |
| `POST /api/v1/gst/periods/{period}/gstr1/preview` | Build GSTR-1 draft |
| `POST /api/v1/gst/periods/{period}/gstr3b/preview` | Build GSTR-3B draft |
| `POST /api/v1/gst/periods/{period}/reconcile-2b` | Reconcile imported GSTR-2B |
| `POST /api/v1/gst/submissions/{id}/approve` | Record authorised user approval |
| `POST /api/v1/gst/submissions/{id}/file` | Trigger provider filing after approval |
| `GET /api/v1/gst/submissions/{id}` | Return state, ARN and provider result |

The API must distinguish `draft`, `validated`, `ready_for_approval`, `submitted_to_provider`, `pending_authorisation`, `filed`, `failed` and `rejected`. `filed` requires authoritative confirmation and ARN/reference evidence, not merely an HTTP 200 response.

### 7.6 Security and privacy

- Use OS-backed secure storage for local secrets; never put provider secrets in React bundles, Tauri assets or source control.
- No local HTTP listener in personal mode (D8). If one is ever added, bind it to loopback only and protect it with a per-install session token and strict origin/CORS policy.
- Tenant isolation is enforced in two places: every repository query filters by the context's `organisation_id`, and (in PostgreSQL) row-level security as a second line of defence.
- Use Tauri allowlists/capabilities, a restrictive Content Security Policy and validated IPC commands. Do not expose arbitrary shell execution to the UI.
- Encrypt backups. Use full-disk encryption where available and protect app-level secrets with the OS keychain.
- Provide audit trails for master changes, invoice issue/cancel, journal corrections, ITC decisions, period unlock and filing approval.
- Minimise personal data, provide export/delete workflows where legally permitted, and document data retention.
- In hosted mode use TLS, OIDC/MFA, tenant isolation, server-side secrets, rate limiting, signed releases and tested restore procedures.

## 8. Non-functional requirements

| Area | Requirement |
|---|---|
| Correctness | Balanced journals; deterministic GST calculations; repeatable return generation from a saved source snapshot |
| Reliability | ACID transaction posting; crash-safe recovery; automated backup reminders; restore test |
| Performance | Responsive invoice entry; target under 2 seconds for ordinary list/filter operations on a typical small-business dataset |
| Scale | Initial target 1–5 GSTINs and up to 50,000 documents per organisation without redesign |
| Availability | Offline for local bookkeeping; provider/GST operations clearly show connectivity status |
| Maintainability | Versioned tax rules, provider adapter contract, database migrations and documented API |
| Accessibility | Keyboard operability, visible focus, scalable text and WCAG AA contrast |
| Portability | Windows first; avoid OS-specific domain logic; package macOS/Linux after MVP |
| Observability | Local diagnostic bundle with redaction; structured logs and correlation IDs for provider calls |

## 9. Quality strategy and acceptance criteria

### 9.1 Automated tests

- Unit tests for decimal rounding, tax component split, place-of-supply scenarios, credit/debit notes, reverse-charge cases and tax-rule effective dates.
- Property tests: journal debits equal credits; invoice totals equal line totals; return aggregation reconciles to source documents.
- Integration tests for SQLite/PostgreSQL repositories, migrations, invoice posting, period lock and backup restore.
- Provider contract tests with mocked GSTN/GSP responses, timeout/retry, idempotency and duplicate submission handling.
- UI tests for invoice entry, purchase entry, period checklist, validation errors and filing approval.
- Golden-file tests for generated PDF invoices, return summaries and export payloads.

### 9.2 MVP acceptance criteria

1. A user can configure a regular GST business and create a customer, supplier and taxable item.
2. A sales invoice produces a correct tax breakup and balanced journal entry; the issued invoice cannot be silently altered.
3. A purchase bill records ITC as pending review, eligible, ineligible or partially eligible with a reason.
4. Receipts/payments update party balances and cash/bank ledgers without unbalanced journals.
5. The app can import a purchase register and GSTR-2B file, match records and export mismatch reports.
6. GSTR-1 and GSTR-3B previews can be generated for a tax period from saved source data, with an explainable tie-out to transactions.
7. The filing centre can export a provider-specific pack, record manual filing evidence, or call a mock provider without production credentials.
8. Filing status is not marked `filed` without ARN/authoritative confirmation.
9. A backup can be restored on a clean installation and data integrity checks pass.
10. All important edits and filing decisions are auditable.

## 10. Delivery roadmap

| Phase | Deliverable | Exit condition |
|---|---|---|
| 0. Product discovery | Confirm taxpayer profile, CA review, GSP shortlist, data model and compliance assumptions | Written scope and selected GSP integration path |
| 1. Accounting foundation | Rust core, SQLite, masters, sales, purchases, receipts/payments, ledger and PDF invoices | MVP bookkeeping acceptance criteria pass |
| 2. GST workbench | Versioned tax engine, GSTR-1/3B workpapers, GSTR-2B import/reconciliation, offline filing channel with versioned format adapters | Tax scenarios reviewed by GST practitioner; golden test set passes; ThingsVU can file using exported files |
| 3. Filing integration | Provider traits, mock provider, GSP adapter, sandbox, auth/approval, filing status, ARN and audit pack | Provider sandbox sign-off and controlled production pilot |
| 4. Small-business polish | Recurring entries, bank reconciliation, inventory, e-invoice/e-way bill where applicable, cloud backup target | User testing with real small-business workflows |
| 5. Commercial mode | PostgreSQL adapter, Axum API, web client transport, OIDC, roles enforced, tenant isolation, tenant onboarding | Security review and restore/tenant isolation tests pass |
| 6. Offline sync | SQLite as offline store for commercial clients, outbox and sync, conflict handling, offline numbering | Sync conflict and offline-numbering tests pass |

## 11. Key risks and mitigations

- **GST rule changes:** maintain effective-dated rule packs, source references and controlled releases; obtain CA review before production release.
- **GSP dependency/cost:** provider adapter interface, capability matrix, export fallback and documented commercial dependency.
- **Incorrect ITC claim:** 2B reconciliation plus explicit eligibility/reason workflow; do not auto-claim merely because an invoice matches.
- **Duplicate filing:** idempotency keys, payload hashes, provider correlation IDs and authoritative status check before retry.
- **Unattended filing risk:** approval gate, explicit authorised-signatory action and no storage of OTP/DSC/bank credentials.
- **Desktop data loss:** encrypted scheduled backups, restore wizard and backup health indicator.
- **Scope creep:** keep payroll, complex inventory, multi-company consolidation and specialised return types outside MVP.

## 12. Decisions (taken 17 Nov 2026)

The open decisions from the v0.1 draft were resolved during Phase 0. The implementation plan and phase status live in `docs/implementation-plan.md`.

| # | Topic | Decision |
|---|---|---|
| D1 | First-release OS | **Windows-only MVP.** Domain code stays OS-neutral; macOS/Linux packaging after MVP. |
| D2 | Local vs hosted | **Local-first, single install, SQLite.** Schema is cloud-ready (UUID keys, `organisation_id` on every row); hosted mode is Phase 5. |
| D3 | GST integration | **No GSP selected yet.** MVP ships `MockGstProvider` plus portal-compatible JSON/Excel export and manual filing evidence. A production `GspProvider` is added in Phase 3 only after a GSP agreement. |
| D4 | Launch taxpayer profile | Return model supports **monthly and QRMP**; the pilot and golden tests target **regular monthly filers** first. |
| D5 | Inventory | Item master supports a stock flag; **no stock accounting in MVP**. |
| D6 | Tax rule validation | A practising GST professional reviews the scenario catalogue and sample returns. This is an exit gate for Phase 2 and Phase 3. Reviewer still to be named. |
| D7 | Repository layout | Keep the Tauri layout (`src/` UI, `src-tauri/` desktop host) and add a root Cargo workspace with domain crates under `crates/`. See §7.3. |
| D8 | UI ↔ core transport | **The MVP UI uses Tauri commands only.** They call the `application` crate directly. The Axum REST API (`api-server`) is built later on the same use cases, for integrations and hosted mode. No local HTTP listener ships in the MVP. |
| D9 | Database engines | **SQLite only through Phase 4.** Persistence sits behind repository traits (D20); PostgreSQL arrives in Phase 5. Queries use runtime-checked SQLx (no `DATABASE_URL` needed at build time). |
| D10 | Money | `rust_decimal::Decimal` in the domain; stored as **INTEGER paise** in the database. Tax rates are stored as basis points (`1800` = 18%). |
| D11 | Rounding | Tax is computed **per line** and rounded to the paisa (half-up). The invoice grand total is rounded to the **nearest rupee**, with the difference posted to a **Round-off** ledger. Both settings are configurable per organisation. |
| D12 | Tax rate data | The app ships a small, versioned **rule pack** with GST slabs (including the September 2025 rationalisation to 0/5/18/40% plus legacy 12/28% for historic periods) and a curated HSN/SAC list. Users can add more. The rule pack is updatable independently of the app. |
| D13 | Export invoices | The foreign currency code and exchange rate are stored on the document for printing; **the books stay in INR**. |
| D14 | Local login | Single user opens as **Owner**, with an optional app **PIN/password lock**. Roles exist in the core but are not enforced in the UI until hosted mode. |
| D15 | Data migration | **CSV/Excel templates** for masters and opening balances. No Tally/Vyapar importers in MVP. |
| D16 | PDF generation | **Typst** (native Rust, deterministic, golden-file testable). |
| D17 | Backup encryption | **User passphrase** (Argon2id-derived key, AES-256-GCM), with a clear warning that a lost passphrase means unrecoverable backups. |
| D18 | UI stack | React 19 + TypeScript + Vite, Tailwind CSS v4 with CSS-variable design tokens, **Inter** typeface bundled locally (offline-safe), light/dark/system theme switcher, lucide icons. Motion follows Apple-style restraint (see §6.5). |

Decisions added on 18 November 2026 (architecture review):

| # | Topic | Decision |
|---|---|---|
| D19 | Deployment modes | **Personal** (Tauri + local SQLite + optional cloud backup) and **Commercial** (Tauri/web clients + hosted Rust API + PostgreSQL). Business logic is identical; only persistence, transport, context and deployment adapters differ (§7.2). |
| D20 | Ports and context | The `application` crate depends on **repository traits**, not on a database engine, and every use case takes a **`RequestContext`** (actor, organisation, role). Every query is scoped by `organisation_id`. Done in Phase 1.1b, before more tables are added. |
| D21 | Commercial offline | Commercial v1 clients are **online-only**. Using SQLite as an offline working store for commercial clients (outbox, sync, offline numbering) is a separate later phase (Phase 6). |
| D22 | Where GSP calls run | **No hosted gateway for now; personal single-business mode comes first.** Personal mode files through the offline channel; the online channel is optional and uses GSP credentials issued to the taxpayer, kept in the OS credential store. Our own GSP secrets never ship in the desktop binary. Commercial: provider runs in the hosted API. |
| D23 | GST integration layer | Return centre → **FilingChannel** → *Offline* (versioned portal format adapters in `gst-formats`) or *Online* (`dyn GstProvider` with separate capability traits for filing, 3B workflow, tax payment and e-invoicing). Submission and payment are always separate operations (§5.8). |

Additions adopted into scope: financial-year close and balance carry-forward; Round-off ledger; per-bank statement CSV mapping profiles; a posting idempotency key on documents; a consistent drill-down pattern from every tax figure to source documents; a sample GSTR-2B fixture set for tests.

## 13. Regulatory and integration source notes

The following official/primary materials informed the GST integration and workflow requirements. Rules and portal behaviour can change; verify current notifications, provider contracts and applicability immediately before implementation and every filing release.

- **[S1] GSTN, GSP ecosystem and design/implementation framework:** https://www.gstn.org.in/gsp-ecosystem
- **[S2] GSTN FAQs, ASP definition and GSP tie-up requirement:** https://www.gstn.org.in/faqs-category-details
- **[S3] GST Portal, GSTR-1 creation, submission and DSC/EVC filing workflow:** https://tutorial.gst.gov.in/userguide/returns/Creation_of_Outward_Supplies_Return_in_GSTR-1.htm
- **[S4] GST Portal, GSTR-3B creation, liability offset, challan/payment and DSC/EVC filing workflow:** https://tutorial.gst.gov.in/userguide/returns/Create_and_Submit_GSTR3B.htm
- **[S5] GST Portal, GSTR-2B purpose and ITC FAQ:** https://tutorial.gst.gov.in/userguide/returns/FAQ_gstr2b.htm
- **[S6] GST Portal, QRMP eligibility and working:** https://tutorial.gst.gov.in/userguide/returns/FAQs_change_profile.htm
- **[S7] IRP, e-invoice mandate and ₹5 crore threshold timeline:** https://einvoice6.gst.gov.in/content/einvoice-mandate/
- **[S8] IRP, 30-day e-invoice reporting limit for AATO ₹10 crore and above from 1 April 2025:** https://einvoice6.gst.gov.in/content/revised-time-limit-for-e-invoice-reporting-for-businesses-with-aato-of-%E2%82%B910-crores-above/
- **[S9] CBIC, E-way Bill Rules, Rule 138:** https://cbic-gst.gov.in/ewaybill-rules.html

---

**Specification status:** Working draft for product discovery and implementation planning. GST outputs and filing behaviour require validation against the current GST portal, applicable notifications, and the selected GSP before production use.
