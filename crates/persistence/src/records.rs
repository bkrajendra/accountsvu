//! Row types shared by every storage adapter. They are plain data: the
//! `FromRow` derive works for any SQLx database, so SQLite and PostgreSQL
//! adapters can both produce them.

use serde::Serialize;
use sqlx::FromRow;

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganisationRecord {
    pub id: String,
    pub legal_name: String,
    pub trade_name: Option<String>,
    pub pan: Option<String>,
    pub constitution: Option<String>,
    pub address_line1: Option<String>,
    pub address_line2: Option<String>,
    pub city: Option<String>,
    pub pincode: Option<String>,
    pub state_code: i64,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub fy_start_month: i64,
    pub books_start_date: String,
    pub round_to_rupee: bool,
    pub row_version: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrationRecord {
    pub id: String,
    pub organisation_id: String,
    pub gstin: String,
    pub state_code: i64,
    pub registration_type: String,
    pub filing_frequency: String,
    pub signatory_name: Option<String>,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub status: String,
    pub row_version: i64,
    pub created_at: String,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountRecord {
    pub id: String,
    pub organisation_id: String,
    pub code: String,
    pub name: String,
    pub kind: String,
    pub system_role: Option<String>,
    pub is_active: bool,
    pub created_at: String,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesRecord {
    pub id: String,
    pub organisation_id: String,
    pub gst_registration_id: Option<String>,
    pub document_type: String,
    pub financial_year: String,
    pub prefix: String,
    pub suffix: String,
    pub padding: i64,
    pub next_number: i64,
    pub is_default: bool,
    pub row_version: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRecord {
    pub id: String,
    pub organisation_id: Option<String>,
    pub occurred_at: String,
    pub actor: String,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub reason: Option<String>,
    pub before_json: Option<String>,
    pub after_json: Option<String>,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartyRecord {
    pub id: String,
    pub organisation_id: String,
    pub kind: String,
    pub name: String,
    pub legal_name: Option<String>,
    pub gst_type: String,
    pub gstin: Option<String>,
    pub pan: Option<String>,
    pub state_code: i64,
    pub address_line1: Option<String>,
    pub address_line2: Option<String>,
    pub city: Option<String>,
    pub pincode: Option<String>,
    pub shipping_same: bool,
    pub shipping_line1: Option<String>,
    pub shipping_line2: Option<String>,
    pub shipping_city: Option<String>,
    pub shipping_pincode: Option<String>,
    pub shipping_state_code: Option<i64>,
    pub contact_person: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub payment_terms_days: Option<i64>,
    pub credit_limit_paise: Option<i64>,
    pub opening_balance_paise: i64,
    pub opening_entry_id: Option<String>,
    pub notes: Option<String>,
    pub is_active: bool,
    pub row_version: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemRecord {
    pub id: String,
    pub organisation_id: String,
    pub kind: String,
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub hsn_sac: Option<String>,
    pub unit: String,
    pub tax_category: String,
    pub gst_rate_bp: i64,
    pub cess_rate_bp: i64,
    pub sales_price_paise: Option<i64>,
    pub purchase_price_paise: Option<i64>,
    pub price_includes_tax: bool,
    pub track_stock: bool,
    pub is_active: bool,
    pub row_version: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// A posted journal entry header. Entries are append-only.
#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntryRecord {
    pub id: String,
    pub organisation_id: String,
    pub entry_date: String,
    pub source_type: String,
    pub source_id: Option<String>,
    pub narration: Option<String>,
    pub reverses_entry_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub posted_at: String,
    pub posted_by: Option<String>,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalLineRecord {
    pub id: String,
    pub journal_entry_id: String,
    pub line_no: i64,
    pub account_id: String,
    pub debit_paise: i64,
    pub credit_paise: i64,
    pub memo: Option<String>,
    pub party_id: Option<String>,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesDocumentRecord {
    pub id: String,
    pub organisation_id: String,
    pub doc_type: String,
    pub status: String,
    pub gst_registration_id: Option<String>,
    pub series_id: Option<String>,
    pub financial_year: Option<String>,
    pub number: Option<String>,
    pub doc_date: String,
    pub due_date: Option<String>,
    pub party_id: String,
    pub party_name: String,
    pub party_gstin: Option<String>,
    pub party_gst_type: String,
    pub party_address: Option<String>,
    pub place_of_supply: i64,
    pub supplier_state: i64,
    pub supply_kind: String,
    pub zero_rated: bool,
    pub reverse_charge: bool,
    pub prices_include_tax: bool,
    pub original_document_id: Option<String>,
    pub reason: Option<String>,
    pub notes: Option<String>,
    pub taxable_paise: i64,
    pub cgst_paise: i64,
    pub sgst_paise: i64,
    pub utgst_paise: i64,
    pub igst_paise: i64,
    pub cess_paise: i64,
    pub round_off_paise: i64,
    pub total_paise: i64,
    pub paid_paise: i64,
    pub journal_entry_id: Option<String>,
    pub cancel_entry_id: Option<String>,
    pub cancel_reason: Option<String>,
    pub row_version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub issued_at: Option<String>,
    pub cancelled_at: Option<String>,
    /// GST rule pack the tax was computed with (Phase 2.1); `None` before.
    pub rule_pack_version: Option<String>,
    /// Manual tax override (spec §5.6): why, who and when. Set when any line's
    /// tax was entered by hand.
    pub tax_override_reason: Option<String>,
    pub tax_override_by: Option<String>,
    pub tax_override_at: Option<String>,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesLineRecord {
    pub id: String,
    pub document_id: String,
    pub line_no: i64,
    pub item_id: Option<String>,
    pub description: String,
    pub hsn_sac: Option<String>,
    pub unit: Option<String>,
    pub quantity: String,
    pub unit_price_paise: i64,
    pub discount_paise: i64,
    pub tax_category: String,
    pub gst_rate_bp: i64,
    pub cess_rate_bp: i64,
    pub amount_paise: i64,
    pub taxable_paise: i64,
    pub cgst_paise: i64,
    pub sgst_paise: i64,
    pub utgst_paise: i64,
    pub igst_paise: i64,
    pub cess_paise: i64,
    pub total_paise: i64,
    /// Set when the line's tax was entered by hand: the tax as calculated.
    pub computed_tax_paise: Option<i64>,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseDocumentRecord {
    pub id: String,
    pub organisation_id: String,
    pub doc_type: String,
    pub status: String,
    pub gst_registration_id: Option<String>,
    pub series_id: Option<String>,
    pub number: Option<String>,
    pub financial_year: Option<String>,
    pub supplier_invoice_number: Option<String>,
    pub supplier_invoice_date: Option<String>,
    pub doc_date: String,
    pub due_date: Option<String>,
    pub party_id: Option<String>,
    pub party_name: Option<String>,
    pub party_gstin: Option<String>,
    pub party_gst_type: Option<String>,
    pub place_of_supply: i64,
    pub supplier_state: i64,
    pub supply_kind: String,
    pub reverse_charge: bool,
    pub prices_include_tax: bool,
    pub payment_mode: String,
    pub original_document_id: Option<String>,
    pub reason: Option<String>,
    pub notes: Option<String>,
    pub taxable_paise: i64,
    pub cgst_paise: i64,
    pub sgst_paise: i64,
    pub utgst_paise: i64,
    pub igst_paise: i64,
    pub cess_paise: i64,
    pub round_off_paise: i64,
    pub total_paise: i64,
    pub paid_paise: i64,
    pub journal_entry_id: Option<String>,
    pub cancel_entry_id: Option<String>,
    pub cancel_reason: Option<String>,
    pub row_version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub recorded_at: Option<String>,
    pub cancelled_at: Option<String>,
    /// GST rule pack the tax was computed with (Phase 2.1); `None` before.
    pub rule_pack_version: Option<String>,
    /// Manual tax override (spec §5.6): why, who and when. Set when any line's
    /// tax was entered by hand.
    pub tax_override_reason: Option<String>,
    pub tax_override_by: Option<String>,
    pub tax_override_at: Option<String>,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseLineRecord {
    pub id: String,
    pub document_id: String,
    pub line_no: i64,
    pub item_id: Option<String>,
    pub account_id: String,
    pub description: String,
    pub hsn_sac: Option<String>,
    pub unit: Option<String>,
    pub quantity: String,
    pub unit_price_paise: i64,
    pub discount_paise: i64,
    pub tax_category: String,
    pub gst_rate_bp: i64,
    pub cess_rate_bp: i64,
    pub amount_paise: i64,
    pub taxable_paise: i64,
    pub cgst_paise: i64,
    pub sgst_paise: i64,
    pub utgst_paise: i64,
    pub igst_paise: i64,
    pub cess_paise: i64,
    pub total_paise: i64,
    pub itc_status: String,
    pub itc_reason: Option<String>,
    pub itc_eligible_paise: i64,
    pub itc_reviewed_at: Option<String>,
    pub itc_reviewed_by: Option<String>,
    /// Used for both taxable and exempt supplies (rule 42), Phase 2.3.
    pub itc_common: bool,
    /// `YYYY-MM` the ITC is claimed in when reviewed after the document's
    /// GSTR-3B was filed (Phase 2.5). `None`: the document's month.
    pub itc_period: Option<String>,
    /// Set when the line's tax was entered by hand: the tax as calculated.
    pub computed_tax_paise: Option<i64>,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRecord {
    pub id: String,
    pub organisation_id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub uploaded_at: String,
    pub uploaded_by: String,
}

// ---------------------------------------------------------------------------
// Banking (Phase 1.5)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoneyAccountRecord {
    pub id: String,
    pub organisation_id: String,
    pub account_id: String,
    pub kind: String,
    pub name: String,
    pub bank_name: Option<String>,
    pub account_number: Option<String>,
    pub ifsc: Option<String>,
    pub statement_mapping: Option<String>,
    pub is_active: bool,
    pub row_version: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountOpeningRecord {
    pub account_id: String,
    pub organisation_id: String,
    /// Debit balances positive, credit balances negative.
    pub amount_paise: i64,
    pub entry_id: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoneyVoucherRecord {
    pub id: String,
    pub organisation_id: String,
    pub voucher_type: String,
    pub status: String,
    pub number: String,
    pub financial_year: String,
    pub seq: i64,
    pub voucher_date: String,
    pub money_account_id: String,
    pub party_id: Option<String>,
    pub counter_account_id: Option<String>,
    pub amount_paise: i64,
    pub mode: String,
    pub reference: Option<String>,
    pub narration: Option<String>,
    pub journal_entry_id: String,
    pub cancel_entry_id: Option<String>,
    pub cancel_reason: Option<String>,
    pub row_version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub cancelled_at: Option<String>,
    /// `settlement` (the party's account) or `refund` (money given back).
    pub purpose: String,
    /// The advance a refund gives back, when named.
    pub refund_of_id: Option<String>,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoucherAllocationRecord {
    pub id: String,
    pub voucher_id: String,
    pub document_kind: String,
    pub document_id: String,
    pub amount_paise: i64,
    /// Set when the allocation was made after the voucher (an advance
    /// adjusted against a later document).
    pub allocated_on: Option<String>,
    pub allocated_by: Option<String>,
}

/// GST paid on an advance received before the supply (Phase 2.6).
#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvanceTaxRecord {
    pub voucher_id: String,
    pub organisation_id: String,
    pub gst_registration_id: String,
    pub place_of_supply: i64,
    pub rate_bp: i64,
    pub gross_paise: i64,
    pub taxable_paise: i64,
    pub igst_paise: i64,
    pub cgst_paise: i64,
    pub sgst_paise: i64,
    pub utgst_paise: i64,
    pub cess_paise: i64,
}

/// Part of a taxed advance adjusted against an invoice or refunded.
#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvanceAdjustmentRecord {
    pub id: String,
    pub organisation_id: String,
    pub voucher_id: String,
    /// `invoice` or `refund`.
    pub reason: String,
    pub allocation_id: Option<String>,
    pub refund_voucher_id: Option<String>,
    pub adjust_date: String,
    pub gross_paise: i64,
    pub taxable_paise: i64,
    pub igst_paise: i64,
    pub cgst_paise: i64,
    pub sgst_paise: i64,
    pub utgst_paise: i64,
    pub cess_paise: i64,
    pub journal_entry_id: String,
    pub status: String,
    pub cancel_entry_id: Option<String>,
    pub created_at: String,
}

/// An invoice or bill of a party with something still to settle.
#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenDocumentRecord {
    pub document_kind: String,
    pub id: String,
    pub doc_type: String,
    pub number: Option<String>,
    pub doc_date: String,
    pub due_date: Option<String>,
    pub total_paise: i64,
    pub paid_paise: i64,
    /// Reduced by credit notes (sales) or debit notes (purchases).
    pub adjusted_paise: i64,
}

/// One journal entry's net effect on one ledger account.
#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookLineRecord {
    pub entry_id: String,
    pub entry_date: String,
    pub source_type: String,
    pub source_id: Option<String>,
    pub narration: Option<String>,
    pub debit_paise: i64,
    pub credit_paise: i64,
    pub party_name: Option<String>,
    pub posted_at: String,
    /// The statement line this entry is reconciled with, if any.
    pub statement_line_id: Option<String>,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementLineRecord {
    pub id: String,
    pub organisation_id: String,
    pub money_account_id: String,
    pub line_date: String,
    pub description: String,
    pub reference: Option<String>,
    pub withdrawal_paise: i64,
    pub deposit_paise: i64,
    pub balance_paise: Option<i64>,
    pub fingerprint: String,
    pub import_batch: String,
    pub source_file: Option<String>,
    pub status: String,
    pub journal_entry_id: Option<String>,
    pub matched_at: Option<String>,
    pub matched_by: Option<String>,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Ledger and reports (Phase 1.6)
// ---------------------------------------------------------------------------

/// One account's movement: balance before the period, and debits/credits in it.
#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountTotalsRecord {
    pub account_id: String,
    pub code: String,
    pub name: String,
    pub kind: String,
    pub system_role: Option<String>,
    pub opening_paise: i64,
    pub debit_paise: i64,
    pub credit_paise: i64,
}

/// One journal line with its entry, for the journal register.
#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalRowRecord {
    pub entry_id: String,
    pub entry_date: String,
    pub source_type: String,
    pub source_id: Option<String>,
    pub narration: Option<String>,
    pub reverses_entry_id: Option<String>,
    pub posted_at: String,
    pub posted_by: Option<String>,
    pub line_no: i64,
    pub account_id: String,
    pub account_code: String,
    pub account_name: String,
    pub debit_paise: i64,
    pub credit_paise: i64,
    pub party_name: Option<String>,
}

/// An invoice or bill with something outstanding, with its party, for ageing.
#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgeingDocRecord {
    pub document_kind: String,
    pub id: String,
    pub party_id: String,
    pub party_name: String,
    pub number: Option<String>,
    pub doc_date: String,
    pub due_date: Option<String>,
    pub total_paise: i64,
    pub outstanding_paise: i64,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BooksLockRecord {
    pub organisation_id: String,
    pub locked_until: String,
    pub reason: Option<String>,
    pub updated_at: String,
    pub updated_by: String,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FyCloseRecord {
    pub id: String,
    pub organisation_id: String,
    pub financial_year: String,
    pub start_date: String,
    pub end_date: String,
    pub status: String,
    pub entry_id: Option<String>,
    pub profit_paise: i64,
    pub closed_at: String,
    pub closed_by: String,
    pub reopen_entry_id: Option<String>,
    pub reopen_reason: Option<String>,
    pub reopened_at: Option<String>,
    pub reopened_by: Option<String>,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogoRecord {
    pub organisation_id: String,
    pub sha256: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub updated_at: String,
    pub updated_by: String,
}

#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupLogRecord {
    pub id: String,
    pub organisation_id: String,
    /// 'backup' or 'restore'.
    pub kind: String,
    pub happened_at: String,
    pub file_name: String,
    pub size_bytes: i64,
    pub sha256: Option<String>,
    pub actor: Option<String>,
}

/// What a database file contains and whether it is sound — used to check a
/// backup before it replaces the live data.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseCheck {
    /// "ok", or SQLite's first integrity problem.
    pub integrity: String,
    pub foreign_key_problems: i64,
    pub schema_version: i64,
    pub business_name: Option<String>,
    pub parties: i64,
    pub items: i64,
    pub sales_documents: i64,
    pub purchase_documents: i64,
    pub vouchers: i64,
    pub journal_entries: i64,
    /// Journal entries whose debits and credits differ (must be 0).
    pub unbalanced_entries: i64,
    pub attachments: i64,
    pub last_entry_date: Option<String>,
}

impl DatabaseCheck {
    pub fn is_sound(&self) -> bool {
        self.integrity == "ok" && self.foreign_key_problems == 0 && self.unbalanced_entries == 0
    }
}

/// An imported GSTR-2B statement (Phase 2.4), one per GSTIN and month.
#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gstr2bStatementRecord {
    pub id: String,
    pub organisation_id: String,
    pub gst_registration_id: String,
    /// `YYYY-MM`.
    pub period: String,
    pub gstin: String,
    pub generated_on: Option<String>,
    pub format_version: Option<String>,
    pub adapter_version: String,
    pub file_name: String,
    pub sha256: String,
    pub skipped_summary: Option<String>,
    pub imported_at: String,
    pub imported_by: Option<String>,
}

/// One supplier document from a GSTR-2B statement.
#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gstr2bLineRecord {
    pub id: String,
    pub statement_id: String,
    pub line_no: i64,
    pub section: String,
    pub supplier_gstin: String,
    pub supplier_name: String,
    pub doc_kind: String,
    pub doc_number: String,
    pub doc_date: String,
    pub value_paise: i64,
    pub taxable_paise: i64,
    pub igst_paise: i64,
    pub cgst_paise: i64,
    pub sgst_paise: i64,
    pub cess_paise: i64,
    pub place_of_supply: Option<String>,
    pub reverse_charge: bool,
    pub itc_available: bool,
    pub itc_reason: Option<String>,
    pub supplier_filed_on: Option<String>,
    pub supplier_period: Option<String>,
    pub original_number: Option<String>,
}

/// A reviewer's decision on a reconciliation row.
#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gstr2bDecisionRecord {
    pub id: String,
    pub organisation_id: String,
    pub gst_registration_id: String,
    pub period: String,
    pub row_key: String,
    pub purchase_id: Option<String>,
    /// 'accepted' or 'deferred'.
    pub decision: String,
    pub note: Option<String>,
    pub decided_at: String,
    pub decided_by: Option<String>,
}

/// A return filed outside the app, recorded with its ARN (Phase 2.5).
#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GstFilingRecord {
    pub id: String,
    pub organisation_id: String,
    pub gst_registration_id: String,
    /// 'gstr1' or 'gstr3b'.
    pub return_type: String,
    pub period: String,
    /// 'filed' or 'removed'.
    pub status: String,
    pub channel: String,
    pub arn: String,
    pub filed_on: String,
    pub note: Option<String>,
    pub return_pack_version: String,
    pub rule_pack_version: String,
    pub summary_json: String,
    pub workpaper_sha256: String,
    pub warning_count: i64,
    pub recorded_at: String,
    pub recorded_by: String,
    pub removed_reason: Option<String>,
    pub removed_at: Option<String>,
    pub removed_by: Option<String>,
}

/// A file frozen with a filing; content in `attachment_blob`.
#[derive(Clone, Debug, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GstFilingFileRecord {
    pub id: String,
    pub filing_id: String,
    /// 'portal_json', 'workpaper_json', 'workpaper_xlsx' or 'acknowledgement'.
    pub kind: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub sha256: String,
}
