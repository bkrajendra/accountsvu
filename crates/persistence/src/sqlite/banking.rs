//! SQLite implementation of the banking repository (Phase 1.5).

use super::SqliteUow;
use crate::records::{
    AccountOpeningRecord, MoneyAccountRecord, MoneyVoucherRecord, OpenDocumentRecord,
    StatementLineRecord, VoucherAllocationRecord,
};
use crate::repo::{BankingRepo, DbResult};

const MONEY_COLS: &str = "id, organisation_id, account_id, kind, name, bank_name, account_number, ifsc, \
    statement_mapping, is_active, row_version, created_at, updated_at";

const VOUCHER_COLS: &str = "id, organisation_id, voucher_type, status, number, financial_year, seq, voucher_date, \
    money_account_id, party_id, counter_account_id, amount_paise, mode, reference, narration, journal_entry_id, \
    cancel_entry_id, cancel_reason, row_version, created_at, updated_at, cancelled_at";

const LINE_COLS: &str = "id, organisation_id, money_account_id, line_date, description, reference, withdrawal_paise, \
    deposit_paise, balance_paise, fingerprint, import_batch, source_file, status, journal_entry_id, matched_at, \
    matched_by, created_at";

impl BankingRepo for SqliteUow {
    async fn list_money_accounts(&mut self, org_id: &str) -> DbResult<Vec<MoneyAccountRecord>> {
        let sql = format!(
            "SELECT {MONEY_COLS} FROM money_account WHERE organisation_id = ?1
              ORDER BY is_active DESC, CASE kind WHEN 'cash' THEN 0 ELSE 1 END, created_at"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn get_money_account(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> DbResult<Option<MoneyAccountRecord>> {
        let sql = format!(
            "SELECT {MONEY_COLS} FROM money_account WHERE organisation_id = ?1 AND id = ?2"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(id)
            .fetch_optional(self.c())
            .await?)
    }

    async fn money_account_name_taken(
        &mut self,
        org_id: &str,
        name: &str,
        except_id: Option<&str>,
    ) -> DbResult<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM money_account
              WHERE organisation_id = ?1 AND UPPER(name) = UPPER(?2) AND (?3 IS NULL OR id <> ?3)",
        )
        .bind(org_id)
        .bind(name)
        .bind(except_id)
        .fetch_one(self.c())
        .await?;
        Ok(n > 0)
    }

    async fn insert_money_account(&mut self, m: &MoneyAccountRecord) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO money_account (id, organisation_id, account_id, kind, name, bank_name, account_number,
                 ifsc, statement_mapping, is_active, row_version, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11, ?12)",
        )
        .bind(&m.id)
        .bind(&m.organisation_id)
        .bind(&m.account_id)
        .bind(&m.kind)
        .bind(&m.name)
        .bind(&m.bank_name)
        .bind(&m.account_number)
        .bind(&m.ifsc)
        .bind(&m.statement_mapping)
        .bind(m.is_active)
        .bind(&m.created_at)
        .bind(&m.updated_at)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn update_money_account(
        &mut self,
        m: &MoneyAccountRecord,
        expected_version: i64,
    ) -> DbResult<bool> {
        let r = sqlx::query(
            "UPDATE money_account SET name = ?3, bank_name = ?4, account_number = ?5, ifsc = ?6,
                 statement_mapping = ?7, is_active = ?8, updated_at = ?9, row_version = row_version + 1
              WHERE organisation_id = ?1 AND id = ?2 AND row_version = ?10",
        )
        .bind(&m.organisation_id)
        .bind(&m.id)
        .bind(&m.name)
        .bind(&m.bank_name)
        .bind(&m.account_number)
        .bind(&m.ifsc)
        .bind(&m.statement_mapping)
        .bind(m.is_active)
        .bind(&m.updated_at)
        .bind(expected_version)
        .execute(self.c())
        .await?;
        Ok(r.rows_affected() == 1)
    }

    async fn rename_account(&mut self, org_id: &str, account_id: &str, name: &str) -> DbResult<()> {
        sqlx::query("UPDATE account SET name = ?3 WHERE organisation_id = ?1 AND id = ?2")
            .bind(org_id)
            .bind(account_id)
            .bind(name)
            .execute(self.c())
            .await?;
        Ok(())
    }

    async fn list_openings(&mut self, org_id: &str) -> DbResult<Vec<AccountOpeningRecord>> {
        Ok(sqlx::query_as(
            "SELECT account_id, organisation_id, amount_paise, entry_id, updated_at
               FROM account_opening WHERE organisation_id = ?1",
        )
        .bind(org_id)
        .fetch_all(self.c())
        .await?)
    }

    async fn get_opening(
        &mut self,
        org_id: &str,
        account_id: &str,
    ) -> DbResult<Option<AccountOpeningRecord>> {
        Ok(sqlx::query_as(
            "SELECT account_id, organisation_id, amount_paise, entry_id, updated_at
               FROM account_opening WHERE organisation_id = ?1 AND account_id = ?2",
        )
        .bind(org_id)
        .bind(account_id)
        .fetch_optional(self.c())
        .await?)
    }

    async fn upsert_opening(&mut self, o: &AccountOpeningRecord) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO account_opening (account_id, organisation_id, amount_paise, entry_id, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (account_id) DO UPDATE SET amount_paise = excluded.amount_paise,
                 entry_id = excluded.entry_id, updated_at = excluded.updated_at
             WHERE account_opening.organisation_id = excluded.organisation_id",
        )
        .bind(&o.account_id)
        .bind(&o.organisation_id)
        .bind(o.amount_paise)
        .bind(&o.entry_id)
        .bind(&o.updated_at)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn next_voucher_seq(
        &mut self,
        org_id: &str,
        voucher_type: &str,
        financial_year: &str,
    ) -> DbResult<i64> {
        let max: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(seq) FROM money_voucher
              WHERE organisation_id = ?1 AND voucher_type = ?2 AND financial_year = ?3",
        )
        .bind(org_id)
        .bind(voucher_type)
        .bind(financial_year)
        .fetch_one(self.c())
        .await?;
        Ok(max.unwrap_or(0) + 1)
    }

    async fn insert_voucher(&mut self, v: &MoneyVoucherRecord) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO money_voucher (id, organisation_id, voucher_type, status, number, financial_year, seq,
                 voucher_date, money_account_id, party_id, counter_account_id, amount_paise, mode, reference,
                 narration, journal_entry_id, cancel_entry_id, cancel_reason, row_version, created_at, updated_at,
                 cancelled_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, 1, ?19, ?20, ?21)",
        )
        .bind(&v.id)
        .bind(&v.organisation_id)
        .bind(&v.voucher_type)
        .bind(&v.status)
        .bind(&v.number)
        .bind(&v.financial_year)
        .bind(v.seq)
        .bind(&v.voucher_date)
        .bind(&v.money_account_id)
        .bind(&v.party_id)
        .bind(&v.counter_account_id)
        .bind(v.amount_paise)
        .bind(&v.mode)
        .bind(&v.reference)
        .bind(&v.narration)
        .bind(&v.journal_entry_id)
        .bind(&v.cancel_entry_id)
        .bind(&v.cancel_reason)
        .bind(&v.created_at)
        .bind(&v.updated_at)
        .bind(&v.cancelled_at)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn update_voucher(
        &mut self,
        v: &MoneyVoucherRecord,
        expected_version: i64,
    ) -> DbResult<bool> {
        let r = sqlx::query(
            "UPDATE money_voucher SET status = ?3, cancel_entry_id = ?4, cancel_reason = ?5, cancelled_at = ?6,
                 updated_at = ?7, row_version = row_version + 1
              WHERE organisation_id = ?1 AND id = ?2 AND row_version = ?8",
        )
        .bind(&v.organisation_id)
        .bind(&v.id)
        .bind(&v.status)
        .bind(&v.cancel_entry_id)
        .bind(&v.cancel_reason)
        .bind(&v.cancelled_at)
        .bind(&v.updated_at)
        .bind(expected_version)
        .execute(self.c())
        .await?;
        Ok(r.rows_affected() == 1)
    }

    async fn get_voucher(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> DbResult<Option<MoneyVoucherRecord>> {
        let sql = format!(
            "SELECT {VOUCHER_COLS} FROM money_voucher WHERE organisation_id = ?1 AND id = ?2"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(id)
            .fetch_optional(self.c())
            .await?)
    }

    async fn list_vouchers(&mut self, org_id: &str) -> DbResult<Vec<MoneyVoucherRecord>> {
        let sql = format!(
            "SELECT {VOUCHER_COLS} FROM money_voucher WHERE organisation_id = ?1
              ORDER BY voucher_date DESC, created_at DESC"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn insert_allocation(&mut self, a: &VoucherAllocationRecord) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO voucher_allocation (id, voucher_id, document_kind, document_id, amount_paise)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(&a.id)
        .bind(&a.voucher_id)
        .bind(&a.document_kind)
        .bind(&a.document_id)
        .bind(a.amount_paise)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn voucher_allocations(
        &mut self,
        voucher_id: &str,
    ) -> DbResult<Vec<VoucherAllocationRecord>> {
        Ok(sqlx::query_as(
            "SELECT id, voucher_id, document_kind, document_id, amount_paise
               FROM voucher_allocation WHERE voucher_id = ?1 ORDER BY rowid",
        )
        .bind(voucher_id)
        .fetch_all(self.c())
        .await?)
    }

    async fn document_allocations(
        &mut self,
        org_id: &str,
        document_kind: &str,
        document_id: &str,
    ) -> DbResult<Vec<(MoneyVoucherRecord, i64)>> {
        let cols: String = VOUCHER_COLS
            .split(", ")
            .map(|c| format!("v.{}", c.trim()))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT {cols}, a.amount_paise AS allocated_paise
               FROM voucher_allocation a JOIN money_voucher v ON v.id = a.voucher_id
              WHERE v.organisation_id = ?1 AND a.document_kind = ?2 AND a.document_id = ?3
                AND v.status = 'posted'
              ORDER BY v.voucher_date, v.created_at"
        );
        let rows = sqlx::query(&sql)
            .bind(org_id)
            .bind(document_kind)
            .bind(document_id)
            .fetch_all(self.c())
            .await?;
        use sqlx::{FromRow, Row};
        rows.iter()
            .map(|r| {
                Ok((
                    MoneyVoucherRecord::from_row(r)?,
                    r.try_get("allocated_paise")?,
                ))
            })
            .collect()
    }

    async fn adjust_paid(
        &mut self,
        org_id: &str,
        document_kind: &str,
        document_id: &str,
        delta_paise: i64,
    ) -> DbResult<bool> {
        let table = match document_kind {
            "sales" => "sales_document",
            "purchase" => "purchase_document",
            _ => return Ok(false),
        };
        let sql = format!(
            "UPDATE {table} SET paid_paise = paid_paise + ?3, row_version = row_version + 1
              WHERE organisation_id = ?1 AND id = ?2 AND paid_paise + ?3 >= 0"
        );
        let r = sqlx::query(&sql)
            .bind(org_id)
            .bind(document_id)
            .bind(delta_paise)
            .execute(self.c())
            .await?;
        Ok(r.rows_affected() == 1)
    }

    async fn open_documents(
        &mut self,
        org_id: &str,
        party_id: &str,
        document_kind: &str,
    ) -> DbResult<Vec<OpenDocumentRecord>> {
        let sql = match document_kind {
            "sales" => {
                "SELECT * FROM (
                   SELECT 'sales' AS document_kind, d.id, d.doc_type, d.number, d.doc_date, d.due_date,
                          d.total_paise, d.paid_paise,
                          IFNULL((SELECT SUM(c.total_paise) FROM sales_document c
                                   WHERE c.organisation_id = d.organisation_id
                                     AND c.original_document_id = d.id AND c.status = 'issued'), 0) AS adjusted_paise,
                          d.created_at
                     FROM sales_document d
                    WHERE d.organisation_id = ?1 AND d.party_id = ?2 AND d.status = 'issued'
                      AND d.doc_type <> 'credit_note')
                  WHERE total_paise - paid_paise - adjusted_paise > 0
                  ORDER BY doc_date, created_at"
            }
            "purchase" => {
                "SELECT * FROM (
                   SELECT 'purchase' AS document_kind, d.id, d.doc_type,
                          COALESCE(d.supplier_invoice_number, d.number) AS number,
                          COALESCE(d.supplier_invoice_date, d.doc_date) AS doc_date, d.due_date,
                          d.total_paise, d.paid_paise,
                          IFNULL((SELECT SUM(n.total_paise) FROM purchase_document n
                                   WHERE n.organisation_id = d.organisation_id
                                     AND n.original_document_id = d.id AND n.status = 'recorded'), 0) AS adjusted_paise,
                          d.created_at
                     FROM purchase_document d
                    WHERE d.organisation_id = ?1 AND d.party_id = ?2 AND d.status = 'recorded'
                      AND d.doc_type <> 'debit_note' AND d.payment_mode = 'credit')
                  WHERE total_paise - paid_paise - adjusted_paise > 0
                  ORDER BY doc_date, created_at"
            }
            _ => return Ok(Vec::new()),
        };
        Ok(sqlx::query_as(sql)
            .bind(org_id)
            .bind(party_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn insert_statement_line(&mut self, l: &StatementLineRecord) -> DbResult<bool> {
        let r = sqlx::query(
            "INSERT INTO bank_statement_line (id, organisation_id, money_account_id, line_date, description,
                 reference, withdrawal_paise, deposit_paise, balance_paise, fingerprint, import_batch, source_file,
                 status, journal_entry_id, matched_at, matched_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
             ON CONFLICT (money_account_id, fingerprint) DO NOTHING",
        )
        .bind(&l.id)
        .bind(&l.organisation_id)
        .bind(&l.money_account_id)
        .bind(&l.line_date)
        .bind(&l.description)
        .bind(&l.reference)
        .bind(l.withdrawal_paise)
        .bind(l.deposit_paise)
        .bind(l.balance_paise)
        .bind(&l.fingerprint)
        .bind(&l.import_batch)
        .bind(&l.source_file)
        .bind(&l.status)
        .bind(&l.journal_entry_id)
        .bind(&l.matched_at)
        .bind(&l.matched_by)
        .bind(&l.created_at)
        .execute(self.c())
        .await?;
        Ok(r.rows_affected() == 1)
    }

    async fn list_statement_lines(
        &mut self,
        org_id: &str,
        money_account_id: &str,
    ) -> DbResult<Vec<StatementLineRecord>> {
        let sql = format!(
            "SELECT {LINE_COLS} FROM bank_statement_line
              WHERE organisation_id = ?1 AND money_account_id = ?2
              ORDER BY line_date DESC, created_at DESC, rowid DESC"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(money_account_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn get_statement_line(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> DbResult<Option<StatementLineRecord>> {
        let sql = format!(
            "SELECT {LINE_COLS} FROM bank_statement_line WHERE organisation_id = ?1 AND id = ?2"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(id)
            .fetch_optional(self.c())
            .await?)
    }

    async fn update_statement_match(&mut self, l: &StatementLineRecord) -> DbResult<()> {
        sqlx::query(
            "UPDATE bank_statement_line SET status = ?3, journal_entry_id = ?4, matched_at = ?5, matched_by = ?6
              WHERE organisation_id = ?1 AND id = ?2",
        )
        .bind(&l.organisation_id)
        .bind(&l.id)
        .bind(&l.status)
        .bind(&l.journal_entry_id)
        .bind(&l.matched_at)
        .bind(&l.matched_by)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn statement_line_for_entry(
        &mut self,
        org_id: &str,
        entry_id: &str,
    ) -> DbResult<Option<StatementLineRecord>> {
        let sql = format!(
            "SELECT {LINE_COLS} FROM bank_statement_line WHERE organisation_id = ?1 AND journal_entry_id = ?2"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(entry_id)
            .fetch_optional(self.c())
            .await?)
    }

    async fn delete_statement_batch(
        &mut self,
        org_id: &str,
        money_account_id: &str,
        batch: &str,
    ) -> DbResult<u64> {
        let r = sqlx::query(
            "DELETE FROM bank_statement_line
              WHERE organisation_id = ?1 AND money_account_id = ?2 AND import_batch = ?3 AND status <> 'matched'",
        )
        .bind(org_id)
        .bind(money_account_id)
        .bind(batch)
        .execute(self.c())
        .await?;
        Ok(r.rows_affected())
    }
}
