//! IPC commands exposed to the React UI. Each one only deserialises its
//! arguments, builds the request context and delegates to `AppCore`; all
//! validation and permission checks live in the Rust core.
//!
//! Personal mode (spec D19): every call acts as the local owner on the
//! install's organisation. The hosted API builds the context from the
//! authenticated session instead.

use av_application::reports::{
    AgeingView, BalanceSheetView, BooksControlView, JournalRegisterView, LedgerView, LogoContent,
    ProfitAndLossView, TrialBalanceView,
};
use av_application::{
    AppCore, AppError, AttachmentContent, BusinessProfile, DashboardSummary, FieldError,
    HealthInfo, ImportKind, ImportMapping, ImportPreview, ImportResult, ImportTable,
    ImportTemplate, ItcReviewInput, ItemInput, ItemOptions, MoneyAccountInput, MoneyAccountView,
    MoneyBookView, OpenDocumentView, OpeningBalanceInput, OpeningBalanceView, OrganisationInput,
    ParsedImport, PartyInput, PartyView, PurchaseCalculation, PurchaseDocumentInput,
    PurchaseDocumentView, PurchaseSummary, ReconciliationView, RegistrationInput, RequestContext,
    SalesCalculation, SalesDocumentInput, SalesDocumentView, SalesSummary, SeriesInput,
    SeriesUpdateInput, SeriesView, SettlementView, SetupBusinessInput, StatementFile,
    StatementImportInput, StatementImportResult, StatementPreview, UpdateRegistrationInput,
    VoucherInput, VoucherSummary, VoucherView,
};
use av_persistence::records::{AccountRecord, AttachmentRecord, AuditRecord, ItemRecord};
use serde::Serialize;
use tauri::State;

/// Error shape returned to the UI: a stable code, a human message and, for
/// validation failures, per-field messages to show inline.
#[derive(Debug, Serialize)]
pub struct CommandError {
    code: &'static str,
    message: String,
    fields: Vec<FieldError>,
}

impl From<AppError> for CommandError {
    fn from(e: AppError) -> Self {
        if let AppError::Db(db) = &e {
            tracing::error!(error = %db, "database error");
        }
        CommandError {
            code: e.code(),
            message: e.to_string(),
            fields: e.field_errors().to_vec(),
        }
    }
}

impl CommandError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        CommandError {
            code,
            message: message.into(),
            fields: Vec::new(),
        }
    }

    /// A validation error on one field.
    pub fn field(field: &str, message: &str) -> Self {
        CommandError {
            code: "validation_failed",
            message: "please correct the highlighted fields".into(),
            fields: vec![FieldError {
                field: field.into(),
                message: message.into(),
            }],
        }
    }

    pub fn rename_field(mut self, from: &str, to: &str) -> Self {
        for f in &mut self.fields {
            if f.field == from {
                f.field = to.into();
            }
        }
        self
    }

    pub fn locked() -> Self {
        CommandError::new(
            "locked",
            "AccountsVU is locked. Enter your PIN to continue.",
        )
    }
}

type CmdResult<T> = Result<T, CommandError>;

fn ctx() -> RequestContext {
    RequestContext::local_owner()
}

#[tauri::command]
pub async fn app_health(core: State<'_, AppCore>) -> CmdResult<HealthInfo> {
    Ok(core.health().await?)
}

#[tauri::command]
pub async fn dashboard_summary(
    core: State<'_, AppCore>,
    month_start: String,
    month_end: String,
) -> CmdResult<DashboardSummary> {
    Ok(core
        .dashboard_summary(&ctx(), &month_start, &month_end)
        .await?)
}

#[tauri::command]
pub async fn business_profile(core: State<'_, AppCore>) -> CmdResult<BusinessProfile> {
    Ok(core.business_profile(&ctx()).await?)
}

#[tauri::command]
pub async fn setup_business(
    core: State<'_, AppCore>,
    input: SetupBusinessInput,
) -> CmdResult<BusinessProfile> {
    Ok(core.setup_business(&ctx(), input).await?)
}

#[tauri::command]
pub async fn update_organisation(
    core: State<'_, AppCore>,
    input: OrganisationInput,
    expected_version: i64,
) -> CmdResult<BusinessProfile> {
    Ok(core
        .update_organisation(&ctx(), input, expected_version)
        .await?)
}

#[tauri::command]
pub async fn add_gst_registration(
    core: State<'_, AppCore>,
    input: RegistrationInput,
) -> CmdResult<BusinessProfile> {
    Ok(core.add_gst_registration(&ctx(), input).await?)
}

#[tauri::command]
pub async fn update_gst_registration(
    core: State<'_, AppCore>,
    id: String,
    input: UpdateRegistrationInput,
    expected_version: i64,
) -> CmdResult<BusinessProfile> {
    Ok(core
        .update_gst_registration(&ctx(), &id, input, expected_version)
        .await?)
}

#[tauri::command]
pub async fn list_accounts(core: State<'_, AppCore>) -> CmdResult<Vec<AccountRecord>> {
    Ok(core.list_accounts(&ctx()).await?)
}

#[tauri::command]
pub async fn list_series(core: State<'_, AppCore>) -> CmdResult<Vec<SeriesView>> {
    Ok(core.list_series(&ctx()).await?)
}

#[tauri::command]
pub async fn create_series(
    core: State<'_, AppCore>,
    input: SeriesInput,
) -> CmdResult<Vec<SeriesView>> {
    Ok(core.create_series(&ctx(), input).await?)
}

#[tauri::command]
pub async fn update_series(
    core: State<'_, AppCore>,
    id: String,
    input: SeriesUpdateInput,
    expected_version: i64,
) -> CmdResult<Vec<SeriesView>> {
    Ok(core
        .update_series(&ctx(), &id, input, expected_version)
        .await?)
}

#[tauri::command]
pub async fn list_audit_events(
    core: State<'_, AppCore>,
    limit: Option<i64>,
) -> CmdResult<Vec<AuditRecord>> {
    Ok(core.list_audit_events(&ctx(), limit.unwrap_or(100)).await?)
}

// ---------------------------------------------------------------------------
// Masters (Phase 1.2)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_parties(core: State<'_, AppCore>) -> CmdResult<Vec<PartyView>> {
    Ok(core.list_parties(&ctx()).await?)
}

#[tauri::command]
pub async fn create_party(core: State<'_, AppCore>, input: PartyInput) -> CmdResult<PartyView> {
    Ok(core.create_party(&ctx(), input).await?)
}

#[tauri::command]
pub async fn update_party(
    core: State<'_, AppCore>,
    id: String,
    input: PartyInput,
    expected_version: i64,
) -> CmdResult<PartyView> {
    Ok(core
        .update_party(&ctx(), &id, input, expected_version)
        .await?)
}

#[tauri::command]
pub fn item_options(core: State<'_, AppCore>) -> ItemOptions {
    core.item_options()
}

#[tauri::command]
pub fn hsn_lookup(
    core: State<'_, AppCore>,
    query: String,
    date: Option<String>,
) -> Vec<av_application::masters::HsnSuggestion> {
    let date = date.and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
    core.hsn_lookup(&query, date)
}

#[tauri::command]
pub fn rule_pack_info(core: State<'_, AppCore>) -> av_application::masters::RulePackInfo {
    core.rule_pack_info()
}

#[tauri::command]
pub async fn list_items(core: State<'_, AppCore>) -> CmdResult<Vec<ItemRecord>> {
    Ok(core.list_items(&ctx()).await?)
}

#[tauri::command]
pub async fn create_item(core: State<'_, AppCore>, input: ItemInput) -> CmdResult<ItemRecord> {
    Ok(core.create_item(&ctx(), input).await?)
}

#[tauri::command]
pub async fn update_item(
    core: State<'_, AppCore>,
    id: String,
    input: ItemInput,
    expected_version: i64,
) -> CmdResult<ItemRecord> {
    Ok(core
        .update_item(&ctx(), &id, input, expected_version)
        .await?)
}

// ---------------------------------------------------------------------------
// Import (Phase 1.2, D15)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn import_template(kind: ImportKind) -> ImportTemplate {
    av_application::import_template(kind)
}

/// The file's bytes arrive as a JSON array; files are capped at 5 MB in core.
#[tauri::command]
pub fn parse_import(
    kind: ImportKind,
    file_name: String,
    bytes: Vec<u8>,
) -> CmdResult<ParsedImport> {
    Ok(av_application::parse_import_file(kind, &file_name, &bytes)?)
}

#[tauri::command]
pub async fn preview_import(
    core: State<'_, AppCore>,
    kind: ImportKind,
    table: ImportTable,
    mapping: ImportMapping,
) -> CmdResult<ImportPreview> {
    Ok(core.preview_import(&ctx(), kind, &table, &mapping).await?)
}

#[tauri::command]
pub async fn commit_import(
    core: State<'_, AppCore>,
    kind: ImportKind,
    table: ImportTable,
    mapping: ImportMapping,
) -> CmdResult<ImportResult> {
    Ok(core.commit_import(&ctx(), kind, &table, &mapping).await?)
}

// ---------------------------------------------------------------------------
// Sales (Phase 1.3)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_sales(core: State<'_, AppCore>) -> CmdResult<Vec<SalesSummary>> {
    Ok(core.list_sales(&ctx()).await?)
}

#[tauri::command]
pub async fn get_sales_document(
    core: State<'_, AppCore>,
    id: String,
) -> CmdResult<SalesDocumentView> {
    Ok(core.get_sales_document(&ctx(), &id).await?)
}

#[tauri::command]
pub async fn calculate_sales(
    core: State<'_, AppCore>,
    input: SalesDocumentInput,
) -> CmdResult<SalesCalculation> {
    Ok(core.calculate_sales(&ctx(), input).await?)
}

#[tauri::command]
pub async fn save_sales_draft(
    core: State<'_, AppCore>,
    id: Option<String>,
    input: SalesDocumentInput,
    expected_version: Option<i64>,
) -> CmdResult<SalesDocumentView> {
    Ok(core
        .save_sales_draft(&ctx(), id.as_deref(), input, expected_version)
        .await?)
}

#[tauri::command]
pub async fn issue_sales_document(
    core: State<'_, AppCore>,
    id: Option<String>,
    input: SalesDocumentInput,
    expected_version: Option<i64>,
) -> CmdResult<SalesDocumentView> {
    Ok(core
        .issue_sales_document(&ctx(), id.as_deref(), input, expected_version)
        .await?)
}

#[tauri::command]
pub async fn cancel_sales_document(
    core: State<'_, AppCore>,
    id: String,
    reason: String,
    expected_version: i64,
) -> CmdResult<SalesDocumentView> {
    Ok(core
        .cancel_sales_document(&ctx(), &id, &reason, expected_version)
        .await?)
}

#[tauri::command]
pub async fn delete_sales_draft(
    core: State<'_, AppCore>,
    id: String,
    expected_version: i64,
) -> CmdResult<()> {
    Ok(core
        .delete_sales_draft(&ctx(), &id, expected_version)
        .await?)
}

// ---------------------------------------------------------------------------
// Purchases and attachments (Phase 1.4)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn purchase_accounts(core: State<'_, AppCore>) -> CmdResult<Vec<AccountRecord>> {
    Ok(core.purchase_accounts(&ctx()).await?)
}

#[tauri::command]
pub async fn list_purchases(core: State<'_, AppCore>) -> CmdResult<Vec<PurchaseSummary>> {
    Ok(core.list_purchases(&ctx()).await?)
}

#[tauri::command]
pub async fn get_purchase(core: State<'_, AppCore>, id: String) -> CmdResult<PurchaseDocumentView> {
    Ok(core.get_purchase(&ctx(), &id).await?)
}

#[tauri::command]
pub async fn calculate_purchase(
    core: State<'_, AppCore>,
    input: PurchaseDocumentInput,
) -> CmdResult<PurchaseCalculation> {
    Ok(core.calculate_purchase(&ctx(), input).await?)
}

#[tauri::command]
pub async fn save_purchase_draft(
    core: State<'_, AppCore>,
    id: Option<String>,
    input: PurchaseDocumentInput,
    expected_version: Option<i64>,
) -> CmdResult<PurchaseDocumentView> {
    Ok(core
        .save_purchase_draft(&ctx(), id.as_deref(), input, expected_version)
        .await?)
}

#[tauri::command]
pub async fn record_purchase(
    core: State<'_, AppCore>,
    id: Option<String>,
    input: PurchaseDocumentInput,
    expected_version: Option<i64>,
) -> CmdResult<PurchaseDocumentView> {
    Ok(core
        .record_purchase(&ctx(), id.as_deref(), input, expected_version)
        .await?)
}

#[tauri::command]
pub async fn review_itc(
    core: State<'_, AppCore>,
    document_id: String,
    line_id: String,
    input: ItcReviewInput,
    expected_version: i64,
) -> CmdResult<PurchaseDocumentView> {
    Ok(core
        .review_itc(&ctx(), &document_id, &line_id, input, expected_version)
        .await?)
}

#[tauri::command]
pub async fn cancel_purchase(
    core: State<'_, AppCore>,
    id: String,
    reason: String,
    expected_version: i64,
) -> CmdResult<PurchaseDocumentView> {
    Ok(core
        .cancel_purchase(&ctx(), &id, &reason, expected_version)
        .await?)
}

#[tauri::command]
pub async fn delete_purchase_draft(
    core: State<'_, AppCore>,
    id: String,
    expected_version: i64,
) -> CmdResult<()> {
    Ok(core
        .delete_purchase_draft(&ctx(), &id, expected_version)
        .await?)
}

/// The file's bytes arrive as a JSON array; the core caps attachments at 5 MB.
#[tauri::command]
pub async fn add_attachment(
    core: State<'_, AppCore>,
    document_id: String,
    file_name: String,
    bytes: Vec<u8>,
) -> CmdResult<AttachmentRecord> {
    Ok(core
        .add_attachment(&ctx(), &document_id, &file_name, &bytes)
        .await?)
}

#[tauri::command]
pub async fn attachment_content(
    core: State<'_, AppCore>,
    id: String,
) -> CmdResult<AttachmentContent> {
    Ok(core.attachment_content(&ctx(), &id).await?)
}

#[tauri::command]
pub async fn remove_attachment(core: State<'_, AppCore>, id: String) -> CmdResult<()> {
    Ok(core.remove_attachment(&ctx(), &id).await?)
}

// ---------------------------------------------------------------------------
// Banking (Phase 1.5)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_money_accounts(core: State<'_, AppCore>) -> CmdResult<Vec<MoneyAccountView>> {
    Ok(core.list_money_accounts(&ctx()).await?)
}

#[tauri::command]
pub async fn create_money_account(
    core: State<'_, AppCore>,
    input: MoneyAccountInput,
) -> CmdResult<MoneyAccountView> {
    Ok(core.create_money_account(&ctx(), &input).await?)
}

#[tauri::command]
pub async fn update_money_account(
    core: State<'_, AppCore>,
    id: String,
    input: MoneyAccountInput,
    expected_version: i64,
) -> CmdResult<MoneyAccountView> {
    Ok(core
        .update_money_account(&ctx(), &id, &input, expected_version)
        .await?)
}

#[tauri::command]
pub async fn list_opening_balances(core: State<'_, AppCore>) -> CmdResult<Vec<OpeningBalanceView>> {
    Ok(core.list_opening_balances(&ctx()).await?)
}

#[tauri::command]
pub async fn set_opening_balance(
    core: State<'_, AppCore>,
    input: OpeningBalanceInput,
) -> CmdResult<OpeningBalanceView> {
    Ok(core.set_opening_balance(&ctx(), &input).await?)
}

#[tauri::command]
pub async fn voucher_accounts(core: State<'_, AppCore>) -> CmdResult<Vec<AccountRecord>> {
    Ok(core.voucher_accounts(&ctx()).await?)
}

#[tauri::command]
pub async fn open_documents(
    core: State<'_, AppCore>,
    party_id: String,
    voucher_type: String,
) -> CmdResult<Vec<OpenDocumentView>> {
    Ok(core
        .open_documents(&ctx(), &party_id, &voucher_type)
        .await?)
}

#[tauri::command]
pub async fn list_vouchers(core: State<'_, AppCore>) -> CmdResult<Vec<VoucherSummary>> {
    Ok(core.list_vouchers(&ctx()).await?)
}

#[tauri::command]
pub async fn get_voucher(core: State<'_, AppCore>, id: String) -> CmdResult<VoucherView> {
    Ok(core.get_voucher(&ctx(), &id).await?)
}

#[tauri::command]
pub async fn document_settlements(
    core: State<'_, AppCore>,
    document_kind: String,
    document_id: String,
) -> CmdResult<Vec<SettlementView>> {
    Ok(core
        .document_settlements(&ctx(), &document_kind, &document_id)
        .await?)
}

#[tauri::command]
pub async fn create_voucher(
    core: State<'_, AppCore>,
    input: VoucherInput,
) -> CmdResult<VoucherView> {
    Ok(core.create_voucher(&ctx(), &input).await?)
}

#[tauri::command]
pub async fn cancel_voucher(
    core: State<'_, AppCore>,
    id: String,
    reason: String,
    expected_version: i64,
) -> CmdResult<VoucherView> {
    Ok(core
        .cancel_voucher(&ctx(), &id, &reason, expected_version)
        .await?)
}

#[tauri::command]
pub async fn money_book(
    core: State<'_, AppCore>,
    money_account_id: String,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<MoneyBookView> {
    Ok(core
        .money_book(&ctx(), &money_account_id, from.as_deref(), to.as_deref())
        .await?)
}

/// The file's bytes arrive as a JSON array; the core caps files at 5 MB.
#[tauri::command]
pub async fn read_statement_file(
    core: State<'_, AppCore>,
    money_account_id: String,
    file_name: String,
    bytes: Vec<u8>,
) -> CmdResult<StatementFile> {
    Ok(core
        .read_statement_file(&ctx(), &money_account_id, &file_name, &bytes)
        .await?)
}

#[tauri::command]
pub async fn preview_statement(
    core: State<'_, AppCore>,
    input: StatementImportInput,
) -> CmdResult<StatementPreview> {
    Ok(core.preview_statement(&ctx(), &input).await?)
}

#[tauri::command]
pub async fn import_statement(
    core: State<'_, AppCore>,
    input: StatementImportInput,
) -> CmdResult<StatementImportResult> {
    Ok(core.import_statement(&ctx(), &input).await?)
}

#[tauri::command]
pub async fn reconciliation(
    core: State<'_, AppCore>,
    money_account_id: String,
) -> CmdResult<ReconciliationView> {
    Ok(core.reconciliation(&ctx(), &money_account_id).await?)
}

#[tauri::command]
pub async fn match_statement_line(
    core: State<'_, AppCore>,
    line_id: String,
    entry_id: String,
) -> CmdResult<()> {
    Ok(core
        .match_statement_line(&ctx(), &line_id, &entry_id)
        .await?)
}

#[tauri::command]
pub async fn unmatch_statement_line(core: State<'_, AppCore>, line_id: String) -> CmdResult<()> {
    Ok(core.unmatch_statement_line(&ctx(), &line_id).await?)
}

#[tauri::command]
pub async fn ignore_statement_line(core: State<'_, AppCore>, line_id: String) -> CmdResult<()> {
    Ok(core.ignore_statement_line(&ctx(), &line_id).await?)
}

#[tauri::command]
pub async fn undo_statement_import(
    core: State<'_, AppCore>,
    money_account_id: String,
    batch: String,
) -> CmdResult<u64> {
    Ok(core
        .undo_statement_import(&ctx(), &money_account_id, &batch)
        .await?)
}

// ---------------------------------------------------------------------------
// Ledger and reports (Phase 1.6)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn trial_balance(
    core: State<'_, AppCore>,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<TrialBalanceView> {
    Ok(core
        .trial_balance(&ctx(), from.as_deref(), to.as_deref())
        .await?)
}

#[tauri::command]
pub async fn profit_and_loss(
    core: State<'_, AppCore>,
    from: String,
    to: String,
) -> CmdResult<ProfitAndLossView> {
    Ok(core.profit_and_loss(&ctx(), &from, &to).await?)
}

#[tauri::command]
pub async fn balance_sheet(core: State<'_, AppCore>, as_at: String) -> CmdResult<BalanceSheetView> {
    Ok(core.balance_sheet(&ctx(), &as_at).await?)
}

#[tauri::command]
pub async fn general_ledger(
    core: State<'_, AppCore>,
    account_id: String,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<LedgerView> {
    Ok(core
        .general_ledger(&ctx(), &account_id, from.as_deref(), to.as_deref())
        .await?)
}

#[tauri::command]
pub async fn party_ledger(
    core: State<'_, AppCore>,
    party_id: String,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<LedgerView> {
    Ok(core
        .party_ledger(&ctx(), &party_id, from.as_deref(), to.as_deref())
        .await?)
}

#[tauri::command]
pub async fn journal_register(
    core: State<'_, AppCore>,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<JournalRegisterView> {
    Ok(core
        .journal_register(&ctx(), from.as_deref(), to.as_deref())
        .await?)
}

#[tauri::command]
pub async fn ageing(
    core: State<'_, AppCore>,
    kind: String,
    as_of: String,
) -> CmdResult<AgeingView> {
    Ok(core.ageing(&ctx(), &kind, &as_of).await?)
}

#[tauri::command]
pub async fn return_centre(
    core: State<'_, AppCore>,
    registration_id: Option<String>,
    financial_year: Option<String>,
) -> CmdResult<av_application::returns::ReturnCentreView> {
    Ok(core
        .return_centre(
            &ctx(),
            registration_id.as_deref(),
            financial_year.as_deref(),
        )
        .await?)
}

#[tauri::command]
pub async fn gstr1(
    core: State<'_, AppCore>,
    registration_id: String,
    period: String,
) -> CmdResult<av_application::returns::Gstr1View> {
    Ok(core.gstr1(&ctx(), &registration_id, &period).await?)
}

#[tauri::command]
pub async fn gstr3b(
    core: State<'_, AppCore>,
    registration_id: String,
    period: String,
) -> CmdResult<av_application::gstr3b::Gstr3bView> {
    Ok(core.gstr3b(&ctx(), &registration_id, &period).await?)
}

#[tauri::command]
pub async fn gstr2b(
    core: State<'_, AppCore>,
    registration_id: String,
    period: String,
) -> CmdResult<av_application::gstr2b::Gstr2bView> {
    Ok(core.gstr2b(&ctx(), &registration_id, &period).await?)
}

/// The file's bytes arrive as a JSON array; files are capped at 20 MB in core.
#[tauri::command]
pub async fn import_gstr2b(
    core: State<'_, AppCore>,
    registration_id: String,
    file_name: String,
    bytes: Vec<u8>,
) -> CmdResult<av_application::gstr2b::Gstr2bImportResult> {
    Ok(core
        .import_gstr2b(&ctx(), &registration_id, &file_name, &bytes)
        .await?)
}

#[tauri::command]
pub async fn decide_gstr2b(
    core: State<'_, AppCore>,
    registration_id: String,
    period: String,
    input: av_application::gstr2b::Gstr2bDecisionInput,
) -> CmdResult<av_application::gstr2b::Gstr2bView> {
    Ok(core
        .decide_gstr2b(&ctx(), &registration_id, &period, input)
        .await?)
}

#[tauri::command]
pub async fn return_filing(
    core: State<'_, AppCore>,
    registration_id: String,
    period: String,
    return_type: String,
) -> CmdResult<av_application::filing::FilingView> {
    Ok(core
        .return_filing(&ctx(), &registration_id, &period, &return_type)
        .await?)
}

/// The acknowledgement's bytes arrive as a JSON array; capped at 5 MB in core.
#[tauri::command]
pub async fn record_filing(
    core: State<'_, AppCore>,
    registration_id: String,
    period: String,
    return_type: String,
    input: av_application::filing::FilingInput,
) -> CmdResult<av_application::filing::FilingView> {
    Ok(core
        .record_filing(&ctx(), &registration_id, &period, &return_type, input)
        .await?)
}

#[tauri::command]
pub async fn remove_filing(
    core: State<'_, AppCore>,
    registration_id: String,
    period: String,
    return_type: String,
    reason: String,
) -> CmdResult<av_application::filing::FilingView> {
    Ok(core
        .remove_filing(&ctx(), &registration_id, &period, &return_type, &reason)
        .await?)
}

#[tauri::command]
pub async fn books_control(core: State<'_, AppCore>) -> CmdResult<BooksControlView> {
    Ok(core.books_control(&ctx()).await?)
}

#[tauri::command]
pub async fn set_books_lock(
    core: State<'_, AppCore>,
    until: Option<String>,
    reason: Option<String>,
) -> CmdResult<BooksControlView> {
    Ok(core
        .set_books_lock(&ctx(), until.as_deref(), reason.as_deref())
        .await?)
}

#[tauri::command]
pub async fn close_financial_year(
    core: State<'_, AppCore>,
    code: String,
) -> CmdResult<BooksControlView> {
    Ok(core.close_financial_year(&ctx(), &code).await?)
}

#[tauri::command]
pub async fn reopen_financial_year(
    core: State<'_, AppCore>,
    code: String,
    reason: String,
) -> CmdResult<BooksControlView> {
    Ok(core.reopen_financial_year(&ctx(), &code, &reason).await?)
}

/// The image's bytes arrive as a JSON array; the core caps logos at 1 MB.
#[tauri::command]
pub async fn set_logo(
    core: State<'_, AppCore>,
    file_name: String,
    bytes: Vec<u8>,
) -> CmdResult<LogoContent> {
    Ok(core.set_logo(&ctx(), &file_name, &bytes).await?)
}

#[tauri::command]
pub async fn get_logo(core: State<'_, AppCore>) -> CmdResult<Option<LogoContent>> {
    Ok(core.logo(&ctx()).await?)
}

#[tauri::command]
pub async fn remove_logo(core: State<'_, AppCore>) -> CmdResult<()> {
    Ok(core.remove_logo(&ctx()).await?)
}
