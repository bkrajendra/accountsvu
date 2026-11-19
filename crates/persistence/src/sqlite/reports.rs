//! SQLite implementation of the report and ledger-control repository (Phase 1.6).

use super::SqliteUow;
use crate::records::{
    AccountTotalsRecord, AgeingDocRecord, BookLineRecord, BooksLockRecord, FyCloseRecord,
    JournalRowRecord, LogoRecord,
};
use crate::repo::{DbResult, ReportRepo};

const FY_COLS: &str = "id, organisation_id, financial_year, start_date, end_date, status, entry_id, profit_paise, \
    closed_at, closed_by, reopen_entry_id, reopen_reason, reopened_at, reopened_by";

impl ReportRepo for SqliteUow {
    async fn account_totals(
        &mut self,
        org_id: &str,
        from: Option<&str>,
        to: Option<&str>,
        exclude_source: Option<&str>,
    ) -> DbResult<Vec<AccountTotalsRecord>> {
        Ok(sqlx::query_as(
            "SELECT a.id AS account_id, a.code, a.name, a.kind, a.system_role,
                    IFNULL(SUM(CASE WHEN e.id IS NOT NULL AND ?2 IS NOT NULL AND e.entry_date < ?2
                                    THEN l.debit_paise - l.credit_paise END), 0) AS opening_paise,
                    IFNULL(SUM(CASE WHEN e.id IS NOT NULL AND (?2 IS NULL OR e.entry_date >= ?2)
                                     AND (?3 IS NULL OR e.entry_date <= ?3)
                                    THEN l.debit_paise END), 0) AS debit_paise,
                    IFNULL(SUM(CASE WHEN e.id IS NOT NULL AND (?2 IS NULL OR e.entry_date >= ?2)
                                     AND (?3 IS NULL OR e.entry_date <= ?3)
                                    THEN l.credit_paise END), 0) AS credit_paise
               FROM account a
               LEFT JOIN journal_line l ON l.account_id = a.id
               LEFT JOIN journal_entry e ON e.id = l.journal_entry_id
                     AND e.organisation_id = a.organisation_id
                     AND (?4 IS NULL OR e.source_type <> ?4)
              WHERE a.organisation_id = ?1
              GROUP BY a.id
              ORDER BY a.code",
        )
        .bind(org_id)
        .bind(from)
        .bind(to)
        .bind(exclude_source)
        .fetch_all(self.c())
        .await?)
    }

    async fn journal_rows(
        &mut self,
        org_id: &str,
        from: Option<&str>,
        to: Option<&str>,
        limit: i64,
    ) -> DbResult<Vec<JournalRowRecord>> {
        // Limit whole entries, never cut one in half.
        Ok(sqlx::query_as(
            "SELECT e.id AS entry_id, e.entry_date, e.source_type, e.source_id, e.narration,
                    e.reverses_entry_id, e.posted_at, e.posted_by, l.line_no,
                    a.id AS account_id, a.code AS account_code, a.name AS account_name,
                    l.debit_paise, l.credit_paise, p.name AS party_name
               FROM journal_entry e
               JOIN journal_line l ON l.journal_entry_id = e.id
               JOIN account a ON a.id = l.account_id
               LEFT JOIN party p ON p.id = l.party_id
              WHERE e.id IN (SELECT id FROM journal_entry
                              WHERE organisation_id = ?1
                                AND (?2 IS NULL OR entry_date >= ?2)
                                AND (?3 IS NULL OR entry_date <= ?3)
                              ORDER BY entry_date, posted_at, id
                              LIMIT ?4)
              ORDER BY e.entry_date, e.posted_at, e.id, l.line_no",
        )
        .bind(org_id)
        .bind(from)
        .bind(to)
        .bind(limit)
        .fetch_all(self.c())
        .await?)
    }

    async fn party_book(
        &mut self,
        org_id: &str,
        party_id: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> DbResult<Vec<BookLineRecord>> {
        Ok(sqlx::query_as(
            "SELECT e.id AS entry_id, e.entry_date, e.source_type, e.source_id, e.narration,
                    SUM(l.debit_paise) AS debit_paise, SUM(l.credit_paise) AS credit_paise,
                    (SELECT name FROM party WHERE id = ?2) AS party_name,
                    e.posted_at,
                    CAST(NULL AS TEXT) AS statement_line_id
               FROM journal_line l
               JOIN journal_entry e ON e.id = l.journal_entry_id
              WHERE e.organisation_id = ?1 AND l.party_id = ?2
                AND (?3 IS NULL OR e.entry_date >= ?3)
                AND (?4 IS NULL OR e.entry_date <= ?4)
              GROUP BY e.id
              ORDER BY e.entry_date, e.posted_at, e.id",
        )
        .bind(org_id)
        .bind(party_id)
        .bind(from)
        .bind(to)
        .fetch_all(self.c())
        .await?)
    }

    async fn party_balance_until(
        &mut self,
        org_id: &str,
        party_id: &str,
        to: Option<&str>,
    ) -> DbResult<i64> {
        let v: Option<i64> = sqlx::query_scalar(
            "SELECT SUM(l.debit_paise - l.credit_paise)
               FROM journal_line l
               JOIN journal_entry e ON e.id = l.journal_entry_id
              WHERE e.organisation_id = ?1 AND l.party_id = ?2
                AND (?3 IS NULL OR e.entry_date <= ?3)",
        )
        .bind(org_id)
        .bind(party_id)
        .bind(to)
        .fetch_one(self.c())
        .await?;
        Ok(v.unwrap_or(0))
    }

    async fn outstanding_documents(
        &mut self,
        org_id: &str,
        document_kind: &str,
        as_of: &str,
    ) -> DbResult<Vec<AgeingDocRecord>> {
        let sql = match document_kind {
            "sales" => {
                "SELECT document_kind, id, party_id, party_name, number, doc_date, due_date, total_paise, outstanding_paise
                   FROM (
                   SELECT 'sales' AS document_kind, d.id, d.party_id, IFNULL(p.name, d.party_name) AS party_name,
                          d.number, d.doc_date, d.due_date, d.total_paise,
                          d.total_paise - d.paid_paise
                            - IFNULL((SELECT SUM(c.total_paise) FROM sales_document c
                                       WHERE c.organisation_id = d.organisation_id
                                         AND c.original_document_id = d.id AND c.status = 'issued'), 0)
                            AS outstanding_paise
                     FROM sales_document d
                     LEFT JOIN party p ON p.id = d.party_id
                    WHERE d.organisation_id = ?1 AND d.status = 'issued' AND d.doc_type <> 'credit_note'
                      AND d.party_id IS NOT NULL AND d.doc_date <= ?2)
                  WHERE outstanding_paise > 0
                  ORDER BY party_name, doc_date"
            }
            "purchase" => {
                "SELECT document_kind, id, party_id, party_name, number, doc_date, due_date, total_paise, outstanding_paise
                   FROM (
                   SELECT 'purchase' AS document_kind, d.id, d.party_id, IFNULL(p.name, d.party_name) AS party_name,
                          COALESCE(d.supplier_invoice_number, d.number) AS number,
                          COALESCE(d.supplier_invoice_date, d.doc_date) AS doc_date, d.due_date, d.total_paise,
                          d.total_paise - d.paid_paise
                            - IFNULL((SELECT SUM(n.total_paise) FROM purchase_document n
                                       WHERE n.organisation_id = d.organisation_id
                                         AND n.original_document_id = d.id AND n.status = 'recorded'), 0)
                            AS outstanding_paise
                     FROM purchase_document d
                     LEFT JOIN party p ON p.id = d.party_id
                    WHERE d.organisation_id = ?1 AND d.status = 'recorded' AND d.doc_type <> 'debit_note'
                      AND d.payment_mode = 'credit' AND d.party_id IS NOT NULL AND d.doc_date <= ?2)
                  WHERE outstanding_paise > 0
                  ORDER BY party_name, doc_date"
            }
            _ => return Ok(Vec::new()),
        };
        Ok(sqlx::query_as(sql)
            .bind(org_id)
            .bind(as_of)
            .fetch_all(self.c())
            .await?)
    }

    async fn get_books_lock(&mut self, org_id: &str) -> DbResult<Option<BooksLockRecord>> {
        Ok(sqlx::query_as(
            "SELECT organisation_id, locked_until, reason, updated_at, updated_by
               FROM books_lock WHERE organisation_id = ?1",
        )
        .bind(org_id)
        .fetch_optional(self.c())
        .await?)
    }

    async fn set_books_lock(&mut self, l: &BooksLockRecord) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO books_lock (organisation_id, locked_until, reason, updated_at, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (organisation_id) DO UPDATE SET locked_until = excluded.locked_until,
                 reason = excluded.reason, updated_at = excluded.updated_at, updated_by = excluded.updated_by",
        )
        .bind(&l.organisation_id)
        .bind(&l.locked_until)
        .bind(&l.reason)
        .bind(&l.updated_at)
        .bind(&l.updated_by)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn clear_books_lock(&mut self, org_id: &str) -> DbResult<()> {
        sqlx::query("DELETE FROM books_lock WHERE organisation_id = ?1")
            .bind(org_id)
            .execute(self.c())
            .await?;
        Ok(())
    }

    async fn list_fy_closes(&mut self, org_id: &str) -> DbResult<Vec<FyCloseRecord>> {
        let sql = format!(
            "SELECT {FY_COLS} FROM fy_close WHERE organisation_id = ?1 ORDER BY start_date DESC, closed_at DESC"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn insert_fy_close(&mut self, c: &FyCloseRecord) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO fy_close (id, organisation_id, financial_year, start_date, end_date, status, entry_id,
                 profit_paise, closed_at, closed_by, reopen_entry_id, reopen_reason, reopened_at, reopened_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        )
        .bind(&c.id)
        .bind(&c.organisation_id)
        .bind(&c.financial_year)
        .bind(&c.start_date)
        .bind(&c.end_date)
        .bind(&c.status)
        .bind(&c.entry_id)
        .bind(c.profit_paise)
        .bind(&c.closed_at)
        .bind(&c.closed_by)
        .bind(&c.reopen_entry_id)
        .bind(&c.reopen_reason)
        .bind(&c.reopened_at)
        .bind(&c.reopened_by)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn update_fy_close(&mut self, c: &FyCloseRecord) -> DbResult<()> {
        sqlx::query(
            "UPDATE fy_close SET status = ?3, reopen_entry_id = ?4, reopen_reason = ?5, reopened_at = ?6,
                 reopened_by = ?7
              WHERE organisation_id = ?1 AND id = ?2",
        )
        .bind(&c.organisation_id)
        .bind(&c.id)
        .bind(&c.status)
        .bind(&c.reopen_entry_id)
        .bind(&c.reopen_reason)
        .bind(&c.reopened_at)
        .bind(&c.reopened_by)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn get_logo(&mut self, org_id: &str) -> DbResult<Option<LogoRecord>> {
        Ok(sqlx::query_as(
            "SELECT organisation_id, sha256, file_name, content_type, size_bytes, updated_at, updated_by
               FROM organisation_logo WHERE organisation_id = ?1",
        )
        .bind(org_id)
        .fetch_optional(self.c())
        .await?)
    }

    async fn set_logo(&mut self, l: &LogoRecord) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO organisation_logo (organisation_id, sha256, file_name, content_type, size_bytes,
                 updated_at, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (organisation_id) DO UPDATE SET sha256 = excluded.sha256, file_name = excluded.file_name,
                 content_type = excluded.content_type, size_bytes = excluded.size_bytes,
                 updated_at = excluded.updated_at, updated_by = excluded.updated_by",
        )
        .bind(&l.organisation_id)
        .bind(&l.sha256)
        .bind(&l.file_name)
        .bind(&l.content_type)
        .bind(l.size_bytes)
        .bind(&l.updated_at)
        .bind(&l.updated_by)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn delete_logo(&mut self, org_id: &str) -> DbResult<()> {
        sqlx::query("DELETE FROM organisation_logo WHERE organisation_id = ?1")
            .bind(org_id)
            .execute(self.c())
            .await?;
        Ok(())
    }
}
