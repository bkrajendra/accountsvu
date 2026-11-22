//! GSTR-2B statements and reconciliation decisions (Phase 2.4).

use super::SqliteUow;
use crate::records::{Gstr2bDecisionRecord, Gstr2bLineRecord, Gstr2bStatementRecord};
use crate::repo::{DbResult, Gstr2bRepo};

const STATEMENT_COLS: &str = "id, organisation_id, gst_registration_id, period, gstin, generated_on, \
    format_version, adapter_version, file_name, sha256, skipped_summary, imported_at, imported_by";

const LINE_COLS: &str = "id, statement_id, line_no, section, supplier_gstin, supplier_name, doc_kind, \
    doc_number, doc_date, value_paise, taxable_paise, igst_paise, cgst_paise, sgst_paise, cess_paise, \
    place_of_supply, reverse_charge, itc_available, itc_reason, supplier_filed_on, supplier_period, \
    original_number";

const DECISION_COLS: &str = "id, organisation_id, gst_registration_id, period, row_key, purchase_id, \
    decision, note, decided_at, decided_by";

impl Gstr2bRepo for SqliteUow {
    async fn list_gstr2b_statements(
        &mut self,
        org_id: &str,
        registration_id: &str,
    ) -> DbResult<Vec<Gstr2bStatementRecord>> {
        let sql = format!(
            "SELECT {STATEMENT_COLS} FROM gstr2b_statement \
             WHERE organisation_id = ?1 AND gst_registration_id = ?2 ORDER BY period"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(registration_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn get_gstr2b_statement(
        &mut self,
        org_id: &str,
        registration_id: &str,
        period: &str,
    ) -> DbResult<Option<Gstr2bStatementRecord>> {
        let sql = format!(
            "SELECT {STATEMENT_COLS} FROM gstr2b_statement \
             WHERE organisation_id = ?1 AND gst_registration_id = ?2 AND period = ?3"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(registration_id)
            .bind(period)
            .fetch_optional(self.c())
            .await?)
    }

    async fn replace_gstr2b_statement(
        &mut self,
        s: &Gstr2bStatementRecord,
        lines: &[Gstr2bLineRecord],
    ) -> DbResult<()> {
        sqlx::query(
            "DELETE FROM gstr2b_line WHERE statement_id IN (SELECT id FROM gstr2b_statement \
             WHERE organisation_id = ?1 AND gst_registration_id = ?2 AND period = ?3)",
        )
        .bind(&s.organisation_id)
        .bind(&s.gst_registration_id)
        .bind(&s.period)
        .execute(self.c())
        .await?;
        sqlx::query(
            "DELETE FROM gstr2b_statement \
             WHERE organisation_id = ?1 AND gst_registration_id = ?2 AND period = ?3",
        )
        .bind(&s.organisation_id)
        .bind(&s.gst_registration_id)
        .bind(&s.period)
        .execute(self.c())
        .await?;
        sqlx::query(&format!(
            "INSERT INTO gstr2b_statement ({STATEMENT_COLS}) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"
        ))
        .bind(&s.id)
        .bind(&s.organisation_id)
        .bind(&s.gst_registration_id)
        .bind(&s.period)
        .bind(&s.gstin)
        .bind(&s.generated_on)
        .bind(&s.format_version)
        .bind(&s.adapter_version)
        .bind(&s.file_name)
        .bind(&s.sha256)
        .bind(&s.skipped_summary)
        .bind(&s.imported_at)
        .bind(&s.imported_by)
        .execute(self.c())
        .await?;
        let sql = format!(
            "INSERT INTO gstr2b_line ({LINE_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, \
             ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)"
        );
        for l in lines {
            sqlx::query(&sql)
                .bind(&l.id)
                .bind(&l.statement_id)
                .bind(l.line_no)
                .bind(&l.section)
                .bind(&l.supplier_gstin)
                .bind(&l.supplier_name)
                .bind(&l.doc_kind)
                .bind(&l.doc_number)
                .bind(&l.doc_date)
                .bind(l.value_paise)
                .bind(l.taxable_paise)
                .bind(l.igst_paise)
                .bind(l.cgst_paise)
                .bind(l.sgst_paise)
                .bind(l.cess_paise)
                .bind(&l.place_of_supply)
                .bind(l.reverse_charge)
                .bind(l.itc_available)
                .bind(&l.itc_reason)
                .bind(&l.supplier_filed_on)
                .bind(&l.supplier_period)
                .bind(&l.original_number)
                .execute(self.c())
                .await?;
        }
        Ok(())
    }

    async fn gstr2b_lines(&mut self, statement_id: &str) -> DbResult<Vec<Gstr2bLineRecord>> {
        let sql =
            format!("SELECT {LINE_COLS} FROM gstr2b_line WHERE statement_id = ?1 ORDER BY line_no");
        Ok(sqlx::query_as(&sql)
            .bind(statement_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn list_gstr2b_decisions(
        &mut self,
        org_id: &str,
        registration_id: &str,
    ) -> DbResult<Vec<Gstr2bDecisionRecord>> {
        let sql = format!(
            "SELECT {DECISION_COLS} FROM gstr2b_decision \
             WHERE organisation_id = ?1 AND gst_registration_id = ?2 ORDER BY period, decided_at"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(registration_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn upsert_gstr2b_decision(&mut self, d: &Gstr2bDecisionRecord) -> DbResult<()> {
        sqlx::query(&format!(
            "INSERT INTO gstr2b_decision ({DECISION_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
             ON CONFLICT (organisation_id, gst_registration_id, period, row_key) DO UPDATE SET \
             purchase_id = excluded.purchase_id, decision = excluded.decision, note = excluded.note, \
             decided_at = excluded.decided_at, decided_by = excluded.decided_by"
        ))
        .bind(&d.id)
        .bind(&d.organisation_id)
        .bind(&d.gst_registration_id)
        .bind(&d.period)
        .bind(&d.row_key)
        .bind(&d.purchase_id)
        .bind(&d.decision)
        .bind(&d.note)
        .bind(&d.decided_at)
        .bind(&d.decided_by)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn delete_gstr2b_decision(
        &mut self,
        org_id: &str,
        registration_id: &str,
        period: &str,
        row_key: &str,
    ) -> DbResult<bool> {
        let r = sqlx::query(
            "DELETE FROM gstr2b_decision WHERE organisation_id = ?1 AND gst_registration_id = ?2 \
             AND period = ?3 AND row_key = ?4",
        )
        .bind(org_id)
        .bind(registration_id)
        .bind(period)
        .bind(row_key)
        .execute(self.c())
        .await?;
        Ok(r.rows_affected() > 0)
    }
}
