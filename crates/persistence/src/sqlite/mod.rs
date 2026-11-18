//! SQLite adapter (personal mode, spec D9/D20).
//!
//! Queries are runtime-checked (`sqlx::query`), so building the workspace does
//! not need a `DATABASE_URL`. Migrations are embedded in the binary.
//! SQLite-specific SQL (`?n` placeholders, `IFNULL`, triggers) stays here.

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::DbError;
use crate::records::{
    AccountRecord, AuditRecord, BookLineRecord, OrganisationRecord, RegistrationRecord,
    SeriesRecord,
};
use crate::repo::{
    AccountRepo, AuditRepo, DbResult, LedgerQueries, OrganisationRepo, RegistrationRepo,
    SeriesRepo, Store, UnitOfWork,
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations/sqlite");

/// Default database file name inside the app data directory.
pub fn default_db_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("accountsvu.sqlite3")
}

#[derive(Clone, Debug)]
pub struct SqliteStore {
    pool: SqlitePool,
    location: String,
}

impl SqliteStore {
    /// Open (creating if needed) the database file and apply migrations.
    pub async fn open(path: &Path) -> Result<Self, DbError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full) // durability over speed for ledgers
            .foreign_keys(true);
        Self::connect(opts, path.display().to_string(), 4).await
    }

    /// In-memory database for tests.
    pub async fn open_in_memory() -> Result<Self, DbError> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        // A single connection: each in-memory connection is its own database.
        Self::connect(opts, ":memory:".into(), 1).await
    }

    async fn connect(
        opts: SqliteConnectOptions,
        location: String,
        max: u32,
    ) -> Result<Self, DbError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(max)
            .connect_with(opts)
            .await?;
        MIGRATOR.run(&pool).await?;
        tracing::info!(%location, "database ready");
        Ok(SqliteStore { pool, location })
    }

    /// Raw pool, for adapter tests only.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

impl Store for SqliteStore {
    type Uow = SqliteUow;

    async fn begin(&self) -> DbResult<SqliteUow> {
        Ok(SqliteUow(self.pool.begin().await?))
    }

    fn location(&self) -> &str {
        &self.location
    }

    async fn schema_version(&self) -> DbResult<i64> {
        let v: Option<i64> =
            sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1")
                .fetch_one(&self.pool)
                .await?;
        Ok(v.unwrap_or(0))
    }
}

/// A SQLite transaction. Rolls back when dropped without `commit`.
pub struct SqliteUow(Transaction<'static, Sqlite>);

impl SqliteUow {
    fn c(&mut self) -> &mut SqliteConnection {
        &mut self.0
    }
}

impl UnitOfWork for SqliteUow {
    async fn commit(self) -> DbResult<()> {
        Ok(self.0.commit().await?)
    }
}

// ---------------------------------------------------------------------------
// Organisation
// ---------------------------------------------------------------------------

const ORG_COLS: &str = "id, legal_name, trade_name, pan, constitution, address_line1, address_line2, city, pincode, \
    state_code, email, phone, fy_start_month, books_start_date, round_to_rupee, row_version, created_at, updated_at";

impl OrganisationRepo for SqliteUow {
    async fn get_organisation(&mut self, org_id: &str) -> DbResult<Option<OrganisationRecord>> {
        let sql = format!("SELECT {ORG_COLS} FROM organisation WHERE id = ?1");
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .fetch_optional(self.c())
            .await?)
    }

    async fn install_organisation(&mut self) -> DbResult<Option<OrganisationRecord>> {
        let sql = format!("SELECT {ORG_COLS} FROM organisation ORDER BY created_at LIMIT 1");
        Ok(sqlx::query_as(&sql).fetch_optional(self.c()).await?)
    }

    async fn organisation_count(&mut self) -> DbResult<i64> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM organisation")
            .fetch_one(self.c())
            .await?)
    }

    async fn insert_organisation(&mut self, o: &OrganisationRecord) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO organisation (id, legal_name, trade_name, pan, constitution, address_line1, address_line2, city, \
             pincode, state_code, email, phone, fy_start_month, books_start_date, round_to_rupee, row_version, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        )
        .bind(&o.id)
        .bind(&o.legal_name)
        .bind(&o.trade_name)
        .bind(&o.pan)
        .bind(&o.constitution)
        .bind(&o.address_line1)
        .bind(&o.address_line2)
        .bind(&o.city)
        .bind(&o.pincode)
        .bind(o.state_code)
        .bind(&o.email)
        .bind(&o.phone)
        .bind(o.fy_start_month)
        .bind(&o.books_start_date)
        .bind(o.round_to_rupee)
        .bind(o.row_version)
        .bind(&o.created_at)
        .bind(&o.updated_at)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn update_organisation(
        &mut self,
        o: &OrganisationRecord,
        expected_version: i64,
    ) -> DbResult<bool> {
        let r = sqlx::query(
            "UPDATE organisation SET legal_name = ?2, trade_name = ?3, pan = ?4, constitution = ?5, address_line1 = ?6,
                address_line2 = ?7, city = ?8, pincode = ?9, state_code = ?10, email = ?11, phone = ?12,
                fy_start_month = ?13, books_start_date = ?14, round_to_rupee = ?15, updated_at = ?16,
                row_version = row_version + 1
             WHERE id = ?1 AND row_version = ?17",
        )
        .bind(&o.id)
        .bind(&o.legal_name)
        .bind(&o.trade_name)
        .bind(&o.pan)
        .bind(&o.constitution)
        .bind(&o.address_line1)
        .bind(&o.address_line2)
        .bind(&o.city)
        .bind(&o.pincode)
        .bind(o.state_code)
        .bind(&o.email)
        .bind(&o.phone)
        .bind(o.fy_start_month)
        .bind(&o.books_start_date)
        .bind(o.round_to_rupee)
        .bind(&o.updated_at)
        .bind(expected_version)
        .execute(self.c())
        .await?;
        Ok(r.rows_affected() == 1)
    }
}

// ---------------------------------------------------------------------------
// GST registrations
// ---------------------------------------------------------------------------

const REG_COLS: &str = "id, organisation_id, gstin, state_code, registration_type, filing_frequency, signatory_name, \
    effective_from, effective_to, status, row_version, created_at, updated_at";

impl RegistrationRepo for SqliteUow {
    async fn list_registrations(&mut self, org_id: &str) -> DbResult<Vec<RegistrationRecord>> {
        let sql = format!(
            "SELECT {REG_COLS} FROM gst_registration WHERE organisation_id = ?1 ORDER BY status = 'active' DESC, created_at"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn get_registration(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> DbResult<Option<RegistrationRecord>> {
        let sql = format!(
            "SELECT {REG_COLS} FROM gst_registration WHERE organisation_id = ?1 AND id = ?2"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(id)
            .fetch_optional(self.c())
            .await?)
    }

    async fn gstin_exists(&mut self, org_id: &str, gstin: &str) -> DbResult<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM gst_registration WHERE organisation_id = ?1 AND gstin = ?2",
        )
        .bind(org_id)
        .bind(gstin)
        .fetch_one(self.c())
        .await?;
        Ok(n > 0)
    }

    async fn insert_registration(&mut self, r: &RegistrationRecord) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO gst_registration (id, organisation_id, gstin, state_code, registration_type, filing_frequency,
                signatory_name, effective_from, effective_to, status, row_version, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )
        .bind(&r.id)
        .bind(&r.organisation_id)
        .bind(&r.gstin)
        .bind(r.state_code)
        .bind(&r.registration_type)
        .bind(&r.filing_frequency)
        .bind(&r.signatory_name)
        .bind(&r.effective_from)
        .bind(&r.effective_to)
        .bind(&r.status)
        .bind(r.row_version)
        .bind(&r.created_at)
        .bind(&r.updated_at)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn update_registration(
        &mut self,
        r: &RegistrationRecord,
        expected_version: i64,
    ) -> DbResult<bool> {
        let res = sqlx::query(
            "UPDATE gst_registration SET registration_type = ?3, filing_frequency = ?4, signatory_name = ?5,
                effective_from = ?6, effective_to = ?7, status = ?8, updated_at = ?9, row_version = row_version + 1
             WHERE id = ?1 AND organisation_id = ?2 AND row_version = ?10",
        )
        .bind(&r.id)
        .bind(&r.organisation_id)
        .bind(&r.registration_type)
        .bind(&r.filing_frequency)
        .bind(&r.signatory_name)
        .bind(&r.effective_from)
        .bind(&r.effective_to)
        .bind(&r.status)
        .bind(&r.updated_at)
        .bind(expected_version)
        .execute(self.c())
        .await?;
        Ok(res.rows_affected() == 1)
    }
}

// ---------------------------------------------------------------------------
// Chart of accounts
// ---------------------------------------------------------------------------

impl AccountRepo for SqliteUow {
    async fn insert_account(&mut self, a: &AccountRecord) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO account (id, organisation_id, code, name, kind, system_role, is_active, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(&a.id)
        .bind(&a.organisation_id)
        .bind(&a.code)
        .bind(&a.name)
        .bind(&a.kind)
        .bind(&a.system_role)
        .bind(a.is_active)
        .bind(&a.created_at)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn list_accounts(&mut self, org_id: &str) -> DbResult<Vec<AccountRecord>> {
        Ok(sqlx::query_as(
            "SELECT id, organisation_id, code, name, kind, system_role, is_active, created_at
               FROM account WHERE organisation_id = ?1 ORDER BY code",
        )
        .bind(org_id)
        .fetch_all(self.c())
        .await?)
    }

    async fn get_account(&mut self, org_id: &str, id: &str) -> DbResult<Option<AccountRecord>> {
        Ok(sqlx::query_as(
            "SELECT id, organisation_id, code, name, kind, system_role, is_active, created_at
               FROM account WHERE organisation_id = ?1 AND id = ?2",
        )
        .bind(org_id)
        .bind(id)
        .fetch_optional(self.c())
        .await?)
    }
}

// ---------------------------------------------------------------------------
// Document number series
// ---------------------------------------------------------------------------

const SERIES_COLS: &str = "id, organisation_id, gst_registration_id, document_type, financial_year, prefix, suffix, \
    padding, next_number, is_default, row_version, created_at, updated_at";

impl SeriesRepo for SqliteUow {
    async fn list_series(&mut self, org_id: &str) -> DbResult<Vec<SeriesRecord>> {
        let sql = format!(
            "SELECT {SERIES_COLS} FROM invoice_series WHERE organisation_id = ?1
              ORDER BY financial_year DESC, gst_registration_id, document_type, is_default DESC, prefix"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn get_series(&mut self, org_id: &str, id: &str) -> DbResult<Option<SeriesRecord>> {
        let sql = format!(
            "SELECT {SERIES_COLS} FROM invoice_series WHERE organisation_id = ?1 AND id = ?2"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(id)
            .fetch_optional(self.c())
            .await?)
    }

    async fn series_scope_taken(
        &mut self,
        org_id: &str,
        gst_registration_id: Option<&str>,
        financial_year: &str,
        prefix: &str,
        suffix: &str,
        except_id: Option<&str>,
    ) -> DbResult<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoice_series
              WHERE organisation_id = ?1 AND IFNULL(gst_registration_id, '') = IFNULL(?2, '') AND financial_year = ?3
                AND prefix = ?4 AND suffix = ?5 AND id <> IFNULL(?6, '')",
        )
        .bind(org_id)
        .bind(gst_registration_id)
        .bind(financial_year)
        .bind(prefix)
        .bind(suffix)
        .bind(except_id)
        .fetch_one(self.c())
        .await?;
        Ok(n > 0)
    }

    async fn default_series(
        &mut self,
        org_id: &str,
        gst_registration_id: Option<&str>,
        document_type: &str,
        financial_year: &str,
    ) -> DbResult<Option<SeriesRecord>> {
        let sql = format!(
            "SELECT {SERIES_COLS} FROM invoice_series
              WHERE organisation_id = ?1 AND IFNULL(gst_registration_id, '') = IFNULL(?2, '')
                AND document_type = ?3 AND financial_year = ?4 AND is_default = 1"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(gst_registration_id)
            .bind(document_type)
            .bind(financial_year)
            .fetch_optional(self.c())
            .await?)
    }

    async fn insert_series(&mut self, s: &SeriesRecord) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO invoice_series (id, organisation_id, gst_registration_id, document_type, financial_year, prefix,
                suffix, padding, next_number, is_default, row_version, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )
        .bind(&s.id)
        .bind(&s.organisation_id)
        .bind(&s.gst_registration_id)
        .bind(&s.document_type)
        .bind(&s.financial_year)
        .bind(&s.prefix)
        .bind(&s.suffix)
        .bind(s.padding)
        .bind(s.next_number)
        .bind(s.is_default)
        .bind(s.row_version)
        .bind(&s.created_at)
        .bind(&s.updated_at)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn update_series(&mut self, s: &SeriesRecord, expected_version: i64) -> DbResult<bool> {
        let r = sqlx::query(
            "UPDATE invoice_series SET prefix = ?3, suffix = ?4, padding = ?5, next_number = ?6, is_default = ?7,
                updated_at = ?8, row_version = row_version + 1
             WHERE id = ?1 AND organisation_id = ?2 AND row_version = ?9",
        )
        .bind(&s.id)
        .bind(&s.organisation_id)
        .bind(&s.prefix)
        .bind(&s.suffix)
        .bind(s.padding)
        .bind(s.next_number)
        .bind(s.is_default)
        .bind(&s.updated_at)
        .bind(expected_version)
        .execute(self.c())
        .await?;
        Ok(r.rows_affected() == 1)
    }

    async fn clear_default_series(&mut self, s: &SeriesRecord, now: &str) -> DbResult<()> {
        sqlx::query(
            "UPDATE invoice_series SET is_default = 0, updated_at = ?6, row_version = row_version + 1
              WHERE organisation_id = ?1 AND IFNULL(gst_registration_id, '') = IFNULL(?2, '') AND document_type = ?3
                AND financial_year = ?4 AND id <> ?5 AND is_default = 1",
        )
        .bind(&s.organisation_id)
        .bind(&s.gst_registration_id)
        .bind(&s.document_type)
        .bind(&s.financial_year)
        .bind(&s.id)
        .bind(now)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn reserve_next_number(
        &mut self,
        org_id: &str,
        series_id: &str,
        now: &str,
    ) -> DbResult<i64> {
        let n: i64 = sqlx::query_scalar(
            "UPDATE invoice_series SET next_number = next_number + 1, updated_at = ?3, row_version = row_version + 1
              WHERE organisation_id = ?1 AND id = ?2 RETURNING next_number - 1",
        )
        .bind(org_id)
        .bind(series_id)
        .bind(now)
        .fetch_one(self.c())
        .await?;
        Ok(n)
    }
}

// ---------------------------------------------------------------------------
// Audit log (spec §7.6)
// ---------------------------------------------------------------------------

impl AuditRepo for SqliteUow {
    async fn insert_audit(&mut self, a: &AuditRecord) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO audit_event (id, organisation_id, occurred_at, actor, action, entity_type, entity_id, reason,
                before_json, after_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(&a.id)
        .bind(&a.organisation_id)
        .bind(&a.occurred_at)
        .bind(&a.actor)
        .bind(&a.action)
        .bind(&a.entity_type)
        .bind(&a.entity_id)
        .bind(&a.reason)
        .bind(&a.before_json)
        .bind(&a.after_json)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn list_audit(&mut self, org_id: &str, limit: i64) -> DbResult<Vec<AuditRecord>> {
        Ok(sqlx::query_as(
            "SELECT id, organisation_id, occurred_at, actor, action, entity_type, entity_id, reason, before_json, after_json
               FROM audit_event WHERE organisation_id = ?1 ORDER BY occurred_at DESC, id DESC LIMIT ?2",
        )
        .bind(org_id)
        .bind(limit)
        .fetch_all(self.c())
        .await?)
    }
}

// ---------------------------------------------------------------------------
// Ledger queries
// ---------------------------------------------------------------------------

impl LedgerQueries for SqliteUow {
    async fn journal_entry_count(&mut self, org_id: &str) -> DbResult<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM journal_entry WHERE organisation_id = ?1")
                .bind(org_id)
                .fetch_one(self.c())
                .await?,
        )
    }

    async fn role_balance_paise(
        &mut self,
        org_id: &str,
        role: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> DbResult<i64> {
        let v: Option<i64> = sqlx::query_scalar(
            "SELECT SUM(l.debit_paise - l.credit_paise)
               FROM journal_line l
               JOIN journal_entry e ON e.id = l.journal_entry_id
               JOIN account a ON a.id = l.account_id
              WHERE e.organisation_id = ?1 AND a.organisation_id = ?1
                AND a.system_role = ?2
                AND (?3 IS NULL OR e.entry_date >= ?3)
                AND (?4 IS NULL OR e.entry_date <= ?4)",
        )
        .bind(org_id)
        .bind(role)
        .bind(from)
        .bind(to)
        .fetch_one(self.c())
        .await?;
        Ok(v.unwrap_or(0))
    }

    async fn account_balance_paise(
        &mut self,
        org_id: &str,
        account_id: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> DbResult<i64> {
        let v: Option<i64> = sqlx::query_scalar(
            "SELECT SUM(l.debit_paise - l.credit_paise)
               FROM journal_line l
               JOIN journal_entry e ON e.id = l.journal_entry_id
              WHERE e.organisation_id = ?1 AND l.account_id = ?2
                AND (?3 IS NULL OR e.entry_date >= ?3)
                AND (?4 IS NULL OR e.entry_date <= ?4)",
        )
        .bind(org_id)
        .bind(account_id)
        .bind(from)
        .bind(to)
        .fetch_one(self.c())
        .await?;
        Ok(v.unwrap_or(0))
    }

    async fn account_book(
        &mut self,
        org_id: &str,
        account_id: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> DbResult<Vec<BookLineRecord>> {
        Ok(sqlx::query_as(
            "SELECT e.id AS entry_id, e.entry_date, e.source_type, e.source_id, e.narration,
                    SUM(l.debit_paise) AS debit_paise, SUM(l.credit_paise) AS credit_paise,
                    (SELECT p.name FROM journal_line pl JOIN party p ON p.id = pl.party_id
                      WHERE pl.journal_entry_id = e.id ORDER BY pl.line_no LIMIT 1) AS party_name,
                    e.posted_at,
                    (SELECT s.id FROM bank_statement_line s
                      WHERE s.organisation_id = e.organisation_id AND s.journal_entry_id = e.id LIMIT 1)
                      AS statement_line_id
               FROM journal_line l
               JOIN journal_entry e ON e.id = l.journal_entry_id
              WHERE e.organisation_id = ?1 AND l.account_id = ?2
                AND (?3 IS NULL OR e.entry_date >= ?3)
                AND (?4 IS NULL OR e.entry_date <= ?4)
              GROUP BY e.id
              ORDER BY e.entry_date, e.posted_at, e.id",
        )
        .bind(org_id)
        .bind(account_id)
        .bind(from)
        .bind(to)
        .fetch_all(self.c())
        .await?)
    }
}

mod banking;
mod masters;
mod purchases;
mod sales;

#[cfg(test)]
mod tests;
