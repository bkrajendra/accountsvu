//! Repository traits (ports, spec D20). The application layer depends only on
//! these; each database engine provides an adapter (`sqlite` now, `postgres`
//! in Phase 5).
//!
//! Rules every adapter follows:
//! * Every read and write is scoped by `organisation_id` — a record of another
//!   organisation behaves as if it does not exist.
//! * `update_*` methods apply only when `row_version` equals the expected
//!   version, bump it, and return `false` on a concurrent-edit conflict.
//! * Methods return `Send` futures so use cases can run on any executor.

use std::future::Future;

use crate::DbError;
use crate::records::{
    AccountOpeningRecord, AccountRecord, AttachmentRecord, AuditRecord, BookLineRecord, ItemRecord,
    JournalEntryRecord, JournalLineRecord, MoneyAccountRecord, MoneyVoucherRecord,
    OpenDocumentRecord, OrganisationRecord, PartyRecord, PurchaseDocumentRecord,
    PurchaseLineRecord, RegistrationRecord, SalesDocumentRecord, SalesLineRecord, SeriesRecord,
    StatementLineRecord, VoucherAllocationRecord,
};
use crate::records::{
    AccountTotalsRecord, AgeingDocRecord, BackupLogRecord, BooksLockRecord, FyCloseRecord,
    JournalRowRecord, LogoRecord,
};

pub type DbResult<T> = Result<T, DbError>;

/// A database the application can open units of work against.
pub trait Store: Send + Sync + 'static {
    type Uow: UnitOfWork;

    /// Start a unit of work (one database transaction). Dropping it without
    /// calling [`UnitOfWork::commit`] rolls everything back.
    fn begin(&self) -> impl Future<Output = DbResult<Self::Uow>> + Send;

    /// Human-readable location, for diagnostics.
    fn location(&self) -> &str;

    /// Highest applied schema migration.
    fn schema_version(&self) -> impl Future<Output = DbResult<i64>> + Send;

    /// Write a consistent copy of the whole database to `dest` (which must
    /// not exist yet), without blocking other readers. Used for backups.
    fn snapshot(&self, dest: &std::path::Path) -> impl Future<Output = DbResult<()>> + Send;
}

/// One transaction spanning every repository.
pub trait UnitOfWork:
    OrganisationRepo
    + RegistrationRepo
    + AccountRepo
    + SeriesRepo
    + AuditRepo
    + LedgerQueries
    + PartyRepo
    + ItemRepo
    + JournalRepo
    + SalesRepo
    + PurchaseRepo
    + AttachmentRepo
    + BankingRepo
    + ReportRepo
    + BackupRepo
    + Send
{
    fn commit(self) -> impl Future<Output = DbResult<()>> + Send;
}

pub trait OrganisationRepo {
    fn get_organisation(
        &mut self,
        org_id: &str,
    ) -> impl Future<Output = DbResult<Option<OrganisationRecord>>> + Send;
    /// Personal mode only: the single organisation on this install, if any.
    fn install_organisation(
        &mut self,
    ) -> impl Future<Output = DbResult<Option<OrganisationRecord>>> + Send;
    fn organisation_count(&mut self) -> impl Future<Output = DbResult<i64>> + Send;
    fn insert_organisation(
        &mut self,
        o: &OrganisationRecord,
    ) -> impl Future<Output = DbResult<()>> + Send;
    fn update_organisation(
        &mut self,
        o: &OrganisationRecord,
        expected_version: i64,
    ) -> impl Future<Output = DbResult<bool>> + Send;
}

pub trait RegistrationRepo {
    fn list_registrations(
        &mut self,
        org_id: &str,
    ) -> impl Future<Output = DbResult<Vec<RegistrationRecord>>> + Send;
    fn get_registration(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> impl Future<Output = DbResult<Option<RegistrationRecord>>> + Send;
    fn gstin_exists(
        &mut self,
        org_id: &str,
        gstin: &str,
    ) -> impl Future<Output = DbResult<bool>> + Send;
    fn insert_registration(
        &mut self,
        r: &RegistrationRecord,
    ) -> impl Future<Output = DbResult<()>> + Send;
    /// GSTIN and state are identity and never change; everything else may.
    fn update_registration(
        &mut self,
        r: &RegistrationRecord,
        expected_version: i64,
    ) -> impl Future<Output = DbResult<bool>> + Send;
}

pub trait AccountRepo {
    fn insert_account(&mut self, a: &AccountRecord) -> impl Future<Output = DbResult<()>> + Send;
    fn list_accounts(
        &mut self,
        org_id: &str,
    ) -> impl Future<Output = DbResult<Vec<AccountRecord>>> + Send;
    fn get_account(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> impl Future<Output = DbResult<Option<AccountRecord>>> + Send;
}

pub trait SeriesRepo {
    fn list_series(
        &mut self,
        org_id: &str,
    ) -> impl Future<Output = DbResult<Vec<SeriesRecord>>> + Send;
    fn get_series(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> impl Future<Output = DbResult<Option<SeriesRecord>>> + Send;
    /// Whether another series in the same GSTIN + FY already uses this prefix/suffix.
    fn series_scope_taken(
        &mut self,
        org_id: &str,
        gst_registration_id: Option<&str>,
        financial_year: &str,
        prefix: &str,
        suffix: &str,
        except_id: Option<&str>,
    ) -> impl Future<Output = DbResult<bool>> + Send;
    /// The default series for a GSTIN (or none), document type and FY.
    fn default_series(
        &mut self,
        org_id: &str,
        gst_registration_id: Option<&str>,
        document_type: &str,
        financial_year: &str,
    ) -> impl Future<Output = DbResult<Option<SeriesRecord>>> + Send;
    fn insert_series(&mut self, s: &SeriesRecord) -> impl Future<Output = DbResult<()>> + Send;
    fn update_series(
        &mut self,
        s: &SeriesRecord,
        expected_version: i64,
    ) -> impl Future<Output = DbResult<bool>> + Send;
    /// Clear the default flag on sibling series so `s` can become the default.
    fn clear_default_series(
        &mut self,
        s: &SeriesRecord,
        now: &str,
    ) -> impl Future<Output = DbResult<()>> + Send;
    /// Atomically take the next number. Call inside the unit of work that
    /// persists the document, so a rollback returns the number.
    fn reserve_next_number(
        &mut self,
        org_id: &str,
        series_id: &str,
        now: &str,
    ) -> impl Future<Output = DbResult<i64>> + Send;
}

pub trait AuditRepo {
    fn insert_audit(&mut self, a: &AuditRecord) -> impl Future<Output = DbResult<()>> + Send;
    fn list_audit(
        &mut self,
        org_id: &str,
        limit: i64,
    ) -> impl Future<Output = DbResult<Vec<AuditRecord>>> + Send;
}

pub trait LedgerQueries {
    fn journal_entry_count(&mut self, org_id: &str) -> impl Future<Output = DbResult<i64>> + Send;
    /// Net balance (debit − credit, paise) of the account with a system role,
    /// for entries dated within `[from, to]` (inclusive ISO dates).
    fn role_balance_paise(
        &mut self,
        org_id: &str,
        role: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> impl Future<Output = DbResult<i64>> + Send;
    /// Net balance (debit − credit, paise) of one account for entries dated
    /// within `[from, to]`.
    fn account_balance_paise(
        &mut self,
        org_id: &str,
        account_id: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> impl Future<Output = DbResult<i64>> + Send;
    /// Each entry's net effect on one account within `[from, to]`, oldest first.
    fn account_book(
        &mut self,
        org_id: &str,
        account_id: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> impl Future<Output = DbResult<Vec<BookLineRecord>>> + Send;
}

pub trait PartyRepo {
    /// All parties, active first, then by name.
    fn list_parties(
        &mut self,
        org_id: &str,
    ) -> impl Future<Output = DbResult<Vec<PartyRecord>>> + Send;
    fn get_party(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> impl Future<Output = DbResult<Option<PartyRecord>>> + Send;
    /// Case-insensitive name clash with another party.
    fn party_name_taken(
        &mut self,
        org_id: &str,
        name: &str,
        except_id: Option<&str>,
    ) -> impl Future<Output = DbResult<bool>> + Send;
    fn party_gstin_taken(
        &mut self,
        org_id: &str,
        gstin: &str,
        except_id: Option<&str>,
    ) -> impl Future<Output = DbResult<bool>> + Send;
    fn insert_party(&mut self, p: &PartyRecord) -> impl Future<Output = DbResult<()>> + Send;
    fn update_party(
        &mut self,
        p: &PartyRecord,
        expected_version: i64,
    ) -> impl Future<Output = DbResult<bool>> + Send;
}

pub trait ItemRepo {
    fn list_items(
        &mut self,
        org_id: &str,
    ) -> impl Future<Output = DbResult<Vec<ItemRecord>>> + Send;
    fn get_item(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> impl Future<Output = DbResult<Option<ItemRecord>>> + Send;
    fn item_name_taken(
        &mut self,
        org_id: &str,
        name: &str,
        except_id: Option<&str>,
    ) -> impl Future<Output = DbResult<bool>> + Send;
    fn item_code_taken(
        &mut self,
        org_id: &str,
        code: &str,
        except_id: Option<&str>,
    ) -> impl Future<Output = DbResult<bool>> + Send;
    fn insert_item(&mut self, i: &ItemRecord) -> impl Future<Output = DbResult<()>> + Send;
    fn update_item(
        &mut self,
        i: &ItemRecord,
        expected_version: i64,
    ) -> impl Future<Output = DbResult<bool>> + Send;
}

/// Posting. Journals are append-only: corrections are new reversal entries.
pub trait JournalRepo {
    /// The id of the account carrying a system role (e.g. `receivables`).
    fn account_id_by_role(
        &mut self,
        org_id: &str,
        role: &str,
    ) -> impl Future<Output = DbResult<Option<String>>> + Send;
    /// Insert a balanced entry and its lines. The caller has validated balance
    /// with `av_accounting::JournalEntry`.
    fn insert_journal(
        &mut self,
        entry: &JournalEntryRecord,
        lines: &[JournalLineRecord],
    ) -> impl Future<Output = DbResult<()>> + Send;
    /// The last locked date, if the books are locked: nothing may be posted
    /// on or before it.
    fn books_locked_until(
        &mut self,
        org_id: &str,
    ) -> impl Future<Output = DbResult<Option<String>>> + Send;
    /// Net balance (debit − credit, paise) per party across all postings.
    fn party_balances(
        &mut self,
        org_id: &str,
    ) -> impl Future<Output = DbResult<Vec<(String, i64)>>> + Send;
}

/// Sales documents. Issued documents are frozen by the database; only drafts
/// can have their header or lines replaced or be deleted.
pub trait SalesRepo {
    fn list_sales(
        &mut self,
        org_id: &str,
    ) -> impl Future<Output = DbResult<Vec<SalesDocumentRecord>>> + Send;
    fn get_sales(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> impl Future<Output = DbResult<Option<SalesDocumentRecord>>> + Send;
    /// Lines of a document already loaded through `get_sales` (so already
    /// scoped to the organisation).
    fn sales_lines(
        &mut self,
        document_id: &str,
    ) -> impl Future<Output = DbResult<Vec<SalesLineRecord>>> + Send;
    fn insert_sales(
        &mut self,
        d: &SalesDocumentRecord,
    ) -> impl Future<Output = DbResult<()>> + Send;
    fn update_sales(
        &mut self,
        d: &SalesDocumentRecord,
        expected_version: i64,
    ) -> impl Future<Output = DbResult<bool>> + Send;
    fn replace_sales_lines(
        &mut self,
        document_id: &str,
        lines: &[SalesLineRecord],
    ) -> impl Future<Output = DbResult<()>> + Send;
    /// Delete a draft and its lines; returns false if it isn't a draft of this organisation.
    fn delete_sales_draft(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> impl Future<Output = DbResult<bool>> + Send;
    /// Latest document date issued from a series, for chronological numbering.
    fn latest_issued_date(
        &mut self,
        org_id: &str,
        series_id: &str,
    ) -> impl Future<Output = DbResult<Option<String>>> + Send;
    /// Credit notes (any status) raised against a document.
    fn credit_notes_for(
        &mut self,
        org_id: &str,
        original_id: &str,
    ) -> impl Future<Output = DbResult<Vec<SalesDocumentRecord>>> + Send;
}

/// Purchases. Recorded documents are frozen by the database, except the ITC
/// review fields on their lines.
pub trait PurchaseRepo {
    fn list_purchases(
        &mut self,
        org_id: &str,
    ) -> impl Future<Output = DbResult<Vec<PurchaseDocumentRecord>>> + Send;
    fn get_purchase(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> impl Future<Output = DbResult<Option<PurchaseDocumentRecord>>> + Send;
    fn purchase_lines(
        &mut self,
        document_id: &str,
    ) -> impl Future<Output = DbResult<Vec<PurchaseLineRecord>>> + Send;
    fn insert_purchase(
        &mut self,
        d: &PurchaseDocumentRecord,
    ) -> impl Future<Output = DbResult<()>> + Send;
    fn update_purchase(
        &mut self,
        d: &PurchaseDocumentRecord,
        expected_version: i64,
    ) -> impl Future<Output = DbResult<bool>> + Send;
    fn replace_purchase_lines(
        &mut self,
        document_id: &str,
        lines: &[PurchaseLineRecord],
    ) -> impl Future<Output = DbResult<()>> + Send;
    /// Update only the ITC review fields of a line.
    fn update_line_itc(
        &mut self,
        line: &PurchaseLineRecord,
    ) -> impl Future<Output = DbResult<()>> + Send;
    fn delete_purchase_draft(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> impl Future<Output = DbResult<bool>> + Send;
    /// A recorded bill from this supplier with this invoice number in the FY.
    fn find_supplier_invoice(
        &mut self,
        org_id: &str,
        party_id: &str,
        supplier_invoice_number: &str,
        financial_year: &str,
        except_id: Option<&str>,
    ) -> impl Future<Output = DbResult<Option<PurchaseDocumentRecord>>> + Send;
    fn debit_notes_for(
        &mut self,
        org_id: &str,
        original_id: &str,
    ) -> impl Future<Output = DbResult<Vec<PurchaseDocumentRecord>>> + Send;
    fn latest_purchase_number_date(
        &mut self,
        org_id: &str,
        series_id: &str,
    ) -> impl Future<Output = DbResult<Option<String>>> + Send;
}

/// File attachments, content-addressed by SHA-256.
pub trait AttachmentRepo {
    /// Store content once per hash (no-op if already stored).
    fn put_blob(
        &mut self,
        sha256: &str,
        content: &[u8],
    ) -> impl Future<Output = DbResult<()>> + Send;
    fn insert_attachment(
        &mut self,
        a: &AttachmentRecord,
    ) -> impl Future<Output = DbResult<()>> + Send;
    fn list_attachments(
        &mut self,
        org_id: &str,
        entity_type: &str,
        entity_id: &str,
    ) -> impl Future<Output = DbResult<Vec<AttachmentRecord>>> + Send;
    fn get_attachment(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> impl Future<Output = DbResult<Option<AttachmentRecord>>> + Send;
    fn blob(&mut self, sha256: &str) -> impl Future<Output = DbResult<Option<Vec<u8>>>> + Send;
    fn delete_attachment(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> impl Future<Output = DbResult<()>> + Send;
}

/// Cash and bank accounts, opening balances, receipts and payments, and bank
/// statement lines (Phase 1.5).
pub trait BankingRepo {
    fn list_money_accounts(
        &mut self,
        org_id: &str,
    ) -> impl Future<Output = DbResult<Vec<MoneyAccountRecord>>> + Send;
    fn get_money_account(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> impl Future<Output = DbResult<Option<MoneyAccountRecord>>> + Send;
    fn money_account_name_taken(
        &mut self,
        org_id: &str,
        name: &str,
        except_id: Option<&str>,
    ) -> impl Future<Output = DbResult<bool>> + Send;
    fn insert_money_account(
        &mut self,
        m: &MoneyAccountRecord,
    ) -> impl Future<Output = DbResult<()>> + Send;
    fn update_money_account(
        &mut self,
        m: &MoneyAccountRecord,
        expected_version: i64,
    ) -> impl Future<Output = DbResult<bool>> + Send;
    /// Rename the ledger account behind a money account.
    fn rename_account(
        &mut self,
        org_id: &str,
        account_id: &str,
        name: &str,
    ) -> impl Future<Output = DbResult<()>> + Send;

    fn list_openings(
        &mut self,
        org_id: &str,
    ) -> impl Future<Output = DbResult<Vec<AccountOpeningRecord>>> + Send;
    fn get_opening(
        &mut self,
        org_id: &str,
        account_id: &str,
    ) -> impl Future<Output = DbResult<Option<AccountOpeningRecord>>> + Send;
    fn upsert_opening(
        &mut self,
        o: &AccountOpeningRecord,
    ) -> impl Future<Output = DbResult<()>> + Send;

    /// The next sequence number for a voucher type in a financial year.
    fn next_voucher_seq(
        &mut self,
        org_id: &str,
        voucher_type: &str,
        financial_year: &str,
    ) -> impl Future<Output = DbResult<i64>> + Send;
    fn insert_voucher(
        &mut self,
        v: &MoneyVoucherRecord,
    ) -> impl Future<Output = DbResult<()>> + Send;
    /// Only the status and cancellation fields may change.
    fn update_voucher(
        &mut self,
        v: &MoneyVoucherRecord,
        expected_version: i64,
    ) -> impl Future<Output = DbResult<bool>> + Send;
    fn get_voucher(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> impl Future<Output = DbResult<Option<MoneyVoucherRecord>>> + Send;
    fn list_vouchers(
        &mut self,
        org_id: &str,
    ) -> impl Future<Output = DbResult<Vec<MoneyVoucherRecord>>> + Send;
    fn insert_allocation(
        &mut self,
        a: &VoucherAllocationRecord,
    ) -> impl Future<Output = DbResult<()>> + Send;
    fn voucher_allocations(
        &mut self,
        voucher_id: &str,
    ) -> impl Future<Output = DbResult<Vec<VoucherAllocationRecord>>> + Send;
    /// Allocations of posted (not cancelled) vouchers to a document, with the
    /// voucher each came from.
    fn document_allocations(
        &mut self,
        org_id: &str,
        document_kind: &str,
        document_id: &str,
    ) -> impl Future<Output = DbResult<Vec<(MoneyVoucherRecord, i64)>>> + Send;
    /// Add `delta_paise` to a sales or purchase document's paid amount.
    fn adjust_paid(
        &mut self,
        org_id: &str,
        document_kind: &str,
        document_id: &str,
        delta_paise: i64,
    ) -> impl Future<Output = DbResult<bool>> + Send;
    /// Issued invoices (`sales`) or recorded credit bills (`purchase`) of a
    /// party that still have something outstanding, oldest first.
    fn open_documents(
        &mut self,
        org_id: &str,
        party_id: &str,
        document_kind: &str,
    ) -> impl Future<Output = DbResult<Vec<OpenDocumentRecord>>> + Send;

    /// Insert unless a line with the same fingerprint already exists for the
    /// account; returns whether it was inserted.
    fn insert_statement_line(
        &mut self,
        l: &StatementLineRecord,
    ) -> impl Future<Output = DbResult<bool>> + Send;
    fn list_statement_lines(
        &mut self,
        org_id: &str,
        money_account_id: &str,
    ) -> impl Future<Output = DbResult<Vec<StatementLineRecord>>> + Send;
    fn get_statement_line(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> impl Future<Output = DbResult<Option<StatementLineRecord>>> + Send;
    /// Update the match fields (status, entry, matched at/by).
    fn update_statement_match(
        &mut self,
        l: &StatementLineRecord,
    ) -> impl Future<Output = DbResult<()>> + Send;
    /// The statement line reconciled with a journal entry, if any.
    fn statement_line_for_entry(
        &mut self,
        org_id: &str,
        entry_id: &str,
    ) -> impl Future<Output = DbResult<Option<StatementLineRecord>>> + Send;
    /// Delete the unmatched lines of one import; returns how many went.
    fn delete_statement_batch(
        &mut self,
        org_id: &str,
        money_account_id: &str,
        batch: &str,
    ) -> impl Future<Output = DbResult<u64>> + Send;
}

/// Reports, period lock, year-end close and the business logo (Phase 1.6).
pub trait ReportRepo {
    /// Every account with its balance before `from` and its debits/credits
    /// within `[from, to]`. Entries whose source type is `exclude_source`
    /// (e.g. the year-end close) are left out.
    fn account_totals(
        &mut self,
        org_id: &str,
        from: Option<&str>,
        to: Option<&str>,
        exclude_source: Option<&str>,
    ) -> impl Future<Output = DbResult<Vec<AccountTotalsRecord>>> + Send;
    /// Journal lines dated within `[from, to]`, oldest first, at most `limit`.
    fn journal_rows(
        &mut self,
        org_id: &str,
        from: Option<&str>,
        to: Option<&str>,
        limit: i64,
    ) -> impl Future<Output = DbResult<Vec<JournalRowRecord>>> + Send;
    /// Each entry's net effect on one party within `[from, to]`.
    fn party_book(
        &mut self,
        org_id: &str,
        party_id: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> impl Future<Output = DbResult<Vec<BookLineRecord>>> + Send;
    /// A party's net balance (debit − credit) for entries dated up to `to`.
    fn party_balance_until(
        &mut self,
        org_id: &str,
        party_id: &str,
        to: Option<&str>,
    ) -> impl Future<Output = DbResult<i64>> + Send;
    /// Issued invoices (`sales`) or recorded credit bills (`purchase`) dated
    /// on or before `as_of` with something outstanding.
    fn outstanding_documents(
        &mut self,
        org_id: &str,
        document_kind: &str,
        as_of: &str,
    ) -> impl Future<Output = DbResult<Vec<AgeingDocRecord>>> + Send;

    fn get_books_lock(
        &mut self,
        org_id: &str,
    ) -> impl Future<Output = DbResult<Option<BooksLockRecord>>> + Send;
    fn set_books_lock(&mut self, l: &BooksLockRecord) -> impl Future<Output = DbResult<()>> + Send;
    fn clear_books_lock(&mut self, org_id: &str) -> impl Future<Output = DbResult<()>> + Send;

    fn list_fy_closes(
        &mut self,
        org_id: &str,
    ) -> impl Future<Output = DbResult<Vec<FyCloseRecord>>> + Send;
    fn insert_fy_close(&mut self, c: &FyCloseRecord) -> impl Future<Output = DbResult<()>> + Send;
    /// Record a reopening (status and reopen fields only).
    fn update_fy_close(&mut self, c: &FyCloseRecord) -> impl Future<Output = DbResult<()>> + Send;

    fn get_logo(
        &mut self,
        org_id: &str,
    ) -> impl Future<Output = DbResult<Option<LogoRecord>>> + Send;
    fn set_logo(&mut self, l: &LogoRecord) -> impl Future<Output = DbResult<()>> + Send;
    fn delete_logo(&mut self, org_id: &str) -> impl Future<Output = DbResult<()>> + Send;
}

/// Backup history (Phase 1.7).
pub trait BackupRepo {
    fn insert_backup_log(
        &mut self,
        r: &BackupLogRecord,
    ) -> impl Future<Output = DbResult<()>> + Send;
    /// Most recent entry of `kind` ('backup' or 'restore').
    fn last_backup_log(
        &mut self,
        org_id: &str,
        kind: &str,
    ) -> impl Future<Output = DbResult<Option<BackupLogRecord>>> + Send;
}
