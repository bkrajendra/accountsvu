//! SQLite implementation of the sales repository (Phase 1.3).

use super::SqliteUow;
use crate::records::{SalesDocumentRecord, SalesLineRecord};
use crate::repo::{DbResult, SalesRepo};

const DOC_COLS: &str = "id, organisation_id, doc_type, status, gst_registration_id, series_id, financial_year, number, \
    doc_date, due_date, party_id, party_name, party_gstin, party_gst_type, party_address, place_of_supply, \
    supplier_state, supply_kind, zero_rated, reverse_charge, prices_include_tax, original_document_id, reason, notes, \
    taxable_paise, cgst_paise, sgst_paise, utgst_paise, igst_paise, cess_paise, round_off_paise, total_paise, \
    paid_paise, journal_entry_id, cancel_entry_id, cancel_reason, row_version, created_at, updated_at, issued_at, \
    cancelled_at, rule_pack_version";

const LINE_COLS: &str = "id, document_id, line_no, item_id, description, hsn_sac, unit, quantity, unit_price_paise, \
    discount_paise, tax_category, gst_rate_bp, cess_rate_bp, amount_paise, taxable_paise, cgst_paise, sgst_paise, \
    utgst_paise, igst_paise, cess_paise, total_paise";

impl SalesRepo for SqliteUow {
    async fn list_sales(&mut self, org_id: &str) -> DbResult<Vec<SalesDocumentRecord>> {
        let sql = format!(
            "SELECT {DOC_COLS} FROM sales_document WHERE organisation_id = ?1
              ORDER BY doc_date DESC, IFNULL(number, '~') DESC, created_at DESC"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn get_sales(&mut self, org_id: &str, id: &str) -> DbResult<Option<SalesDocumentRecord>> {
        let sql =
            format!("SELECT {DOC_COLS} FROM sales_document WHERE organisation_id = ?1 AND id = ?2");
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(id)
            .fetch_optional(self.c())
            .await?)
    }

    async fn sales_lines(&mut self, document_id: &str) -> DbResult<Vec<SalesLineRecord>> {
        let sql =
            format!("SELECT {LINE_COLS} FROM sales_line WHERE document_id = ?1 ORDER BY line_no");
        Ok(sqlx::query_as(&sql)
            .bind(document_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn insert_sales(&mut self, d: &SalesDocumentRecord) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO sales_document (id, organisation_id, doc_type, status, gst_registration_id, series_id,
                financial_year, number, doc_date, due_date, party_id, party_name, party_gstin, party_gst_type,
                party_address, place_of_supply, supplier_state, supply_kind, zero_rated, reverse_charge,
                prices_include_tax, original_document_id, reason, notes, taxable_paise, cgst_paise, sgst_paise,
                utgst_paise, igst_paise, cess_paise, round_off_paise, total_paise, paid_paise, journal_entry_id,
                cancel_entry_id, cancel_reason, row_version, created_at, updated_at, issued_at, cancelled_at,
                rule_pack_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41,
                ?42)",
        )
        .bind(&d.id)
        .bind(&d.organisation_id)
        .bind(&d.doc_type)
        .bind(&d.status)
        .bind(&d.gst_registration_id)
        .bind(&d.series_id)
        .bind(&d.financial_year)
        .bind(&d.number)
        .bind(&d.doc_date)
        .bind(&d.due_date)
        .bind(&d.party_id)
        .bind(&d.party_name)
        .bind(&d.party_gstin)
        .bind(&d.party_gst_type)
        .bind(&d.party_address)
        .bind(d.place_of_supply)
        .bind(d.supplier_state)
        .bind(&d.supply_kind)
        .bind(d.zero_rated)
        .bind(d.reverse_charge)
        .bind(d.prices_include_tax)
        .bind(&d.original_document_id)
        .bind(&d.reason)
        .bind(&d.notes)
        .bind(d.taxable_paise)
        .bind(d.cgst_paise)
        .bind(d.sgst_paise)
        .bind(d.utgst_paise)
        .bind(d.igst_paise)
        .bind(d.cess_paise)
        .bind(d.round_off_paise)
        .bind(d.total_paise)
        .bind(d.paid_paise)
        .bind(&d.journal_entry_id)
        .bind(&d.cancel_entry_id)
        .bind(&d.cancel_reason)
        .bind(d.row_version)
        .bind(&d.created_at)
        .bind(&d.updated_at)
        .bind(&d.issued_at)
        .bind(&d.cancelled_at)
        .bind(&d.rule_pack_version)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn update_sales(
        &mut self,
        d: &SalesDocumentRecord,
        expected_version: i64,
    ) -> DbResult<bool> {
        let r = sqlx::query(
            "UPDATE sales_document SET doc_type = ?3, status = ?4, gst_registration_id = ?5, series_id = ?6,
                financial_year = ?7, number = ?8, doc_date = ?9, due_date = ?10, party_id = ?11, party_name = ?12,
                party_gstin = ?13, party_gst_type = ?14, party_address = ?15, place_of_supply = ?16,
                supplier_state = ?17, supply_kind = ?18, zero_rated = ?19, reverse_charge = ?20,
                prices_include_tax = ?21, original_document_id = ?22, reason = ?23, notes = ?24, taxable_paise = ?25,
                cgst_paise = ?26, sgst_paise = ?27, utgst_paise = ?28, igst_paise = ?29, cess_paise = ?30,
                round_off_paise = ?31, total_paise = ?32, paid_paise = ?33, journal_entry_id = ?34,
                cancel_entry_id = ?35, cancel_reason = ?36, updated_at = ?37, issued_at = ?38, cancelled_at = ?39,
                rule_pack_version = ?40, row_version = row_version + 1
             WHERE id = ?1 AND organisation_id = ?2 AND row_version = ?41",
        )
        .bind(&d.id)
        .bind(&d.organisation_id)
        .bind(&d.doc_type)
        .bind(&d.status)
        .bind(&d.gst_registration_id)
        .bind(&d.series_id)
        .bind(&d.financial_year)
        .bind(&d.number)
        .bind(&d.doc_date)
        .bind(&d.due_date)
        .bind(&d.party_id)
        .bind(&d.party_name)
        .bind(&d.party_gstin)
        .bind(&d.party_gst_type)
        .bind(&d.party_address)
        .bind(d.place_of_supply)
        .bind(d.supplier_state)
        .bind(&d.supply_kind)
        .bind(d.zero_rated)
        .bind(d.reverse_charge)
        .bind(d.prices_include_tax)
        .bind(&d.original_document_id)
        .bind(&d.reason)
        .bind(&d.notes)
        .bind(d.taxable_paise)
        .bind(d.cgst_paise)
        .bind(d.sgst_paise)
        .bind(d.utgst_paise)
        .bind(d.igst_paise)
        .bind(d.cess_paise)
        .bind(d.round_off_paise)
        .bind(d.total_paise)
        .bind(d.paid_paise)
        .bind(&d.journal_entry_id)
        .bind(&d.cancel_entry_id)
        .bind(&d.cancel_reason)
        .bind(&d.updated_at)
        .bind(&d.issued_at)
        .bind(&d.cancelled_at)
        .bind(&d.rule_pack_version)
        .bind(expected_version)
        .execute(self.c())
        .await?;
        Ok(r.rows_affected() == 1)
    }

    async fn replace_sales_lines(
        &mut self,
        document_id: &str,
        lines: &[SalesLineRecord],
    ) -> DbResult<()> {
        sqlx::query("DELETE FROM sales_line WHERE document_id = ?1")
            .bind(document_id)
            .execute(self.c())
            .await?;
        for l in lines {
            sqlx::query(
                "INSERT INTO sales_line (id, document_id, line_no, item_id, description, hsn_sac, unit, quantity,
                    unit_price_paise, discount_paise, tax_category, gst_rate_bp, cess_rate_bp, amount_paise,
                    taxable_paise, cgst_paise, sgst_paise, utgst_paise, igst_paise, cess_paise, total_paise)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
            )
            .bind(&l.id)
            .bind(document_id)
            .bind(l.line_no)
            .bind(&l.item_id)
            .bind(&l.description)
            .bind(&l.hsn_sac)
            .bind(&l.unit)
            .bind(&l.quantity)
            .bind(l.unit_price_paise)
            .bind(l.discount_paise)
            .bind(&l.tax_category)
            .bind(l.gst_rate_bp)
            .bind(l.cess_rate_bp)
            .bind(l.amount_paise)
            .bind(l.taxable_paise)
            .bind(l.cgst_paise)
            .bind(l.sgst_paise)
            .bind(l.utgst_paise)
            .bind(l.igst_paise)
            .bind(l.cess_paise)
            .bind(l.total_paise)
            .execute(self.c())
            .await?;
        }
        Ok(())
    }

    async fn delete_sales_draft(&mut self, org_id: &str, id: &str) -> DbResult<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sales_document WHERE organisation_id = ?1 AND id = ?2 AND status = 'draft'",
        )
        .bind(org_id)
        .bind(id)
        .fetch_one(self.c())
        .await?;
        if n == 0 {
            return Ok(false);
        }
        sqlx::query("DELETE FROM sales_line WHERE document_id = ?1")
            .bind(id)
            .execute(self.c())
            .await?;
        sqlx::query("DELETE FROM sales_document WHERE id = ?1")
            .bind(id)
            .execute(self.c())
            .await?;
        Ok(true)
    }

    async fn latest_issued_date(
        &mut self,
        org_id: &str,
        series_id: &str,
    ) -> DbResult<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT MAX(doc_date) FROM sales_document
              WHERE organisation_id = ?1 AND series_id = ?2 AND status <> 'draft'",
        )
        .bind(org_id)
        .bind(series_id)
        .fetch_one(self.c())
        .await?)
    }

    async fn credit_notes_for(
        &mut self,
        org_id: &str,
        original_id: &str,
    ) -> DbResult<Vec<SalesDocumentRecord>> {
        let sql = format!(
            "SELECT {DOC_COLS} FROM sales_document
              WHERE organisation_id = ?1 AND original_document_id = ?2 ORDER BY doc_date, created_at"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(original_id)
            .fetch_all(self.c())
            .await?)
    }
}
