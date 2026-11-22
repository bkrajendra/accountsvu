//! Application use cases. Every transport — the Tauri host now, the Axum API
//! server later — calls only this crate (spec D8, D19). Use cases depend on
//! the persistence *traits*, never on a database engine (D20), and take a
//! [`RequestContext`] saying who is acting on which organisation.

pub mod backup;
pub mod banking;
mod context;
pub mod crypto;
pub mod documents;
mod error;
pub mod filing;
pub mod gstr2b;
pub mod gstr3b;
pub mod hsn_directory;
pub mod import;
pub mod masters;
mod posting;
pub mod purchases;
pub mod reports;
pub mod returns;
pub mod sales;
pub mod security;
pub mod setup;
mod workpaper_xlsx;

pub use banking::{
    AllocationInput, MoneyAccountInput, MoneyAccountView, MoneyBookView, OpenDocumentView,
    OpeningBalanceInput, OpeningBalanceView, ReconciliationView, SettlementView, StatementFile,
    StatementImportInput, StatementImportResult, StatementMapping, StatementPreview, VoucherInput,
    VoucherSummary, VoucherView,
};
pub use context::{Actor, OrgScope, Permission, RequestContext, Role};
pub use error::{AppError, AppResult, FieldError};
pub use import::{
    ImportKind, ImportMapping, ImportPreview, ImportResult, ImportTable, ImportTemplate,
    ParsedImport, import_template, parse_import_file,
};
pub use masters::{ItemInput, ItemOptions, PartyInput, PartyView};
pub use purchases::{
    AttachmentContent, ItcReviewInput, PurchaseCalculation, PurchaseDocumentInput,
    PurchaseDocumentView, PurchaseLineInput, PurchaseSummary,
};
pub use sales::{
    SalesCalculation, SalesDocumentInput, SalesDocumentView, SalesLineInput, SalesSummary,
};
pub use setup::{
    BusinessProfile, OrganisationInput, RegistrationInput, SeriesInput, SeriesUpdateInput,
    SeriesView, SetupBusinessInput, UpdateRegistrationInput,
};

use av_domain::Money;
use av_persistence::records::OrganisationRecord;
use av_persistence::{
    BankingRepo, HsnDirectory, LedgerQueries, OrganisationRepo, RegistrationRepo, SqliteStore,
    Store,
};
use chrono::NaiveDate;
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthInfo {
    pub app_version: &'static str,
    pub schema_version: i64,
    pub database: String,
    pub organisation_configured: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GstContext {
    pub gstin: String,
    pub filing_frequency: String,
}

/// Figures for the Overview cards (spec §6.2). All amounts are estimates
/// derived from the books, never filed liabilities.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSummary {
    pub sales_this_month: Money,
    pub purchases_this_month: Money,
    pub cash_and_bank: Money,
    pub customer_dues: Money,
    pub estimated_gst_payable: Money,
    pub registrations: Vec<GstContext>,
    pub organisation_configured: bool,
}

type Today = Arc<dyn Fn() -> NaiveDate + Send + Sync>;

/// The application core. Generic over the storage adapter; personal mode uses
/// the default, [`SqliteStore`].
pub struct AppCore<S: Store = SqliteStore> {
    store: S,
    today: Today,
    /// The local HSN/SAC directory (Phase 2.5.1), when the host opened one.
    hsn_directory: Option<HsnDirectory>,
}

fn system_today() -> Today {
    Arc::new(|| chrono::Local::now().date_naive())
}

impl AppCore<SqliteStore> {
    /// Personal mode: open the local SQLite database.
    pub async fn open(db_path: &Path) -> AppResult<Self> {
        Ok(Self::with_store(SqliteStore::open(db_path).await?))
    }

    /// File-backed core with a fixed "today", for tests that need a real file
    /// (backups snapshot the database file).
    #[doc(hidden)]
    pub async fn open_on(db_path: &Path, today: NaiveDate) -> AppResult<Self> {
        Ok(AppCore {
            store: SqliteStore::open(db_path).await?,
            today: Arc::new(move || today),
            hsn_directory: None,
        })
    }

    /// Close the database; the core can't be used afterwards. Called before a
    /// restart so a pending restore can replace the file.
    pub async fn close(&self) {
        self.store.close().await;
    }

    pub async fn in_memory() -> AppResult<Self> {
        Ok(Self::with_store(SqliteStore::open_in_memory().await?))
    }

    /// In-memory core with a fixed "today", for deterministic tests.
    pub async fn in_memory_on(today: NaiveDate) -> AppResult<Self> {
        Ok(AppCore {
            store: SqliteStore::open_in_memory().await?,
            today: Arc::new(move || today),
            hsn_directory: None,
        })
    }
}

impl<S: Store> AppCore<S> {
    pub fn with_store(store: S) -> Self {
        AppCore {
            store,
            today: system_today(),
            hsn_directory: None,
        }
    }

    /// Attach the local HSN/SAC directory (a separate reference database).
    pub fn with_hsn_directory(mut self, directory: HsnDirectory) -> Self {
        self.hsn_directory = Some(directory);
        self
    }

    pub(crate) fn today(&self) -> NaiveDate {
        (self.today)()
    }

    pub(crate) fn now(&self) -> String {
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    /// The organisation the context acts on, if it exists.
    pub(crate) async fn find_org(
        &self,
        ctx: &RequestContext,
        uow: &mut S::Uow,
    ) -> AppResult<Option<OrganisationRecord>> {
        Ok(match &ctx.scope {
            OrgScope::Install => uow.install_organisation().await?,
            OrgScope::Organisation(id) => uow.get_organisation(id).await?,
        })
    }

    pub(crate) async fn require_org(
        &self,
        ctx: &RequestContext,
        uow: &mut S::Uow,
    ) -> AppResult<OrganisationRecord> {
        self.find_org(ctx, uow).await?.ok_or(AppError::NotSetUp)
    }

    /// Install-level diagnostics; needs no organisation.
    pub async fn health(&self) -> AppResult<HealthInfo> {
        // Read the schema version before opening the unit of work: holding a
        // transaction while asking the pool for another connection can deadlock.
        let schema_version = self.store.schema_version().await?;
        let mut uow = self.store.begin().await?;
        Ok(HealthInfo {
            app_version: env!("CARGO_PKG_VERSION"),
            schema_version,
            database: self.store.location().to_owned(),
            organisation_configured: uow.organisation_count().await? > 0,
        })
    }

    /// `month_start`/`month_end` are inclusive ISO dates supplied by the caller
    /// (the UI knows the user's local calendar).
    pub async fn dashboard_summary(
        &self,
        ctx: &RequestContext,
        month_start: &str,
        month_end: &str,
    ) -> AppResult<DashboardSummary> {
        ctx.require(Permission::ViewBusiness)?;
        validate_iso_date(month_start)?;
        validate_iso_date(month_end)?;
        let mut uow = self.store.begin().await?;
        let Some(org) = self.find_org(ctx, &mut uow).await? else {
            return Ok(DashboardSummary {
                sales_this_month: Money::ZERO,
                purchases_this_month: Money::ZERO,
                cash_and_bank: Money::ZERO,
                customer_dues: Money::ZERO,
                estimated_gst_payable: Money::ZERO,
                registrations: Vec::new(),
                organisation_configured: false,
            });
        };
        let o = org.id.as_str();
        let m = (Some(month_start), Some(month_end));
        // Income/liability roles carry credit balances, so negate them.
        let sales = -uow.role_balance_paise(o, "sales", m.0, m.1).await?;
        let purchases = uow.role_balance_paise(o, "purchases", m.0, m.1).await?;
        let mut cash = 0;
        for m in uow.list_money_accounts(o).await? {
            cash += uow
                .account_balance_paise(o, &m.account_id, None, None)
                .await?;
        }
        let dues = uow.role_balance_paise(o, "receivables", None, None).await?;
        let mut output_tax = 0;
        for role in [
            "output_cgst",
            "output_sgst",
            "output_utgst",
            "output_igst",
            "output_cess",
        ] {
            output_tax -= uow.role_balance_paise(o, role, None, None).await?;
        }
        let mut input_tax = 0;
        for role in [
            "input_cgst",
            "input_sgst",
            "input_utgst",
            "input_igst",
            "input_cess",
        ] {
            input_tax += uow.role_balance_paise(o, role, None, None).await?;
        }
        let registrations = uow
            .list_registrations(o)
            .await?
            .into_iter()
            .filter(|r| r.status == "active")
            .map(|r| GstContext {
                gstin: r.gstin,
                filing_frequency: r.filing_frequency,
            })
            .collect();

        Ok(DashboardSummary {
            sales_this_month: Money::from_paise(sales),
            purchases_this_month: Money::from_paise(purchases),
            cash_and_bank: Money::from_paise(cash),
            customer_dues: Money::from_paise(dues),
            estimated_gst_payable: Money::from_paise((output_tax - input_tax).max(0)),
            registrations,
            organisation_configured: true,
        })
    }
}

fn validate_iso_date(s: &str) -> AppResult<()> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| AppError::Invalid(format!("expected YYYY-MM-DD, got {s:?}")))
}

#[cfg(test)]
mod tests;
