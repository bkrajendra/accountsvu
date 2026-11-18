//! SQLite implementation of the purchase and attachment repositories (Phase 1.4).

use super::SqliteUow;
use crate::records::{AttachmentRecord, PurchaseDocumentRecord, PurchaseLineRecord};
use crate::repo::{AttachmentRepo, DbResult, PurchaseRepo};

const DOC_COLS: &str = "id, organisation_id, doc_type, status, gst_registration_id, series_id, number, financial_year, \
    supplier_invoice_number, supplier_invoice_date, doc_date, due_date, party_id, party_name, party_gstin, \
    party_gst_type, place_of_supply, supplier_state, supply_kind, reverse_charge, prices_include_tax, payment_mode, \
    original_document_id, reason, notes, taxable_paise, cgst_paise, sgst_paise, utgst_paise, igst_paise, cess_paise, \
    round_off_paise, total_paise, paid_paise, journal_entry_id, cancel_entry_id, cancel_reason, row_version, \
    created_at, updated_at, recorded_at, cancelled_at";

const LINE_COLS: &str = "id, document_id, line_no, item_id, account_id, description, hsn_sac, unit, quantity, \
    unit_price_paise, discount_paise, tax_category, gst_rate_bp, cess_rate_bp, amount_paise, taxable_paise, \
    cgst_paise, sgst_paise, utgst_paise, igst_paise, cess_paise, total_paise, itc_status, itc_reason, \
    itc_eligible_paise, itc_reviewed_at, itc_reviewed_by";

const ATT_COLS: &str = "id, organisation_id, entity_type, entity_id, file_name, content_type, size_bytes, sha256, uploaded_at, uploaded_by";

macro_rules! bind_doc {
    ($q:expr, $d:expr) => {
        $q.bind(&$d.doc_type)
            .bind(&$d.status)
            .bind(&$d.gst_registration_id)
            .bind(&$d.series_id)
            .bind(&$d.number)
            .bind(&$d.financial_year)
            .bind(&$d.supplier_invoice_number)
            .bind(&$d.supplier_invoice_date)
            .bind(&$d.doc_date)
            .bind(&$d.due_date)
            .bind(&$d.party_id)
            .bind(&$d.party_name)
            .bind(&$d.party_gstin)
            .bind(&$d.party_gst_type)
            .bind($d.place_of_supply)
            .bind($d.supplier_state)
            .bind(&$d.supply_kind)
            .bind($d.reverse_charge)
            .bind($d.prices_include_tax)
            .bind(&$d.payment_mode)
            .bind(&$d.original_document_id)
            .bind(&$d.reason)
            .bind(&$d.notes)
            .bind($d.taxable_paise)
            .bind($d.cgst_paise)
            .bind($d.sgst_paise)
            .bind($d.utgst_paise)
            .bind($d.igst_paise)
            .bind($d.cess_paise)
            .bind($d.round_off_paise)
            .bind($d.total_paise)
            .bind($d.paid_paise)
            .bind(&$d.journal_entry_id)
            .bind(&$d.cancel_entry_id)
            .bind(&$d.cancel_reason)
            .bind(&$d.updated_at)
            .bind(&$d.recorded_at)
            .bind(&$d.cancelled_at)
    };
}

impl PurchaseRepo for SqliteUow {
    async fn list_purchases(&mut self, org_id: &str) -> DbResult<Vec<PurchaseDocumentRecord>> {
        let sql = format!(
            "SELECT {DOC_COLS} FROM purchase_document WHERE organisation_id = ?1 ORDER BY doc_date DESC, created_at DESC"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn get_purchase(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> DbResult<Option<PurchaseDocumentRecord>> {
        let sql = format!(
            "SELECT {DOC_COLS} FROM purchase_document WHERE organisation_id = ?1 AND id = ?2"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(id)
            .fetch_optional(self.c())
            .await?)
    }

    async fn purchase_lines(&mut self, document_id: &str) -> DbResult<Vec<PurchaseLineRecord>> {
        let sql = format!(
            "SELECT {LINE_COLS} FROM purchase_line WHERE document_id = ?1 ORDER BY line_no"
        );
        Ok(sqlx::query_as(&sql)
            .bind(document_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn insert_purchase(&mut self, d: &PurchaseDocumentRecord) -> DbResult<()> {
        let q = sqlx::query(
            "INSERT INTO purchase_document (id, organisation_id, row_version, created_at, doc_type, status,
                gst_registration_id, series_id, number, financial_year, supplier_invoice_number, supplier_invoice_date,
                doc_date, due_date, party_id, party_name, party_gstin, party_gst_type, place_of_supply, supplier_state,
                supply_kind, reverse_charge, prices_include_tax, payment_mode, original_document_id, reason, notes,
                taxable_paise, cgst_paise, sgst_paise, utgst_paise, igst_paise, cess_paise, round_off_paise, total_paise,
                paid_paise, journal_entry_id, cancel_entry_id, cancel_reason, updated_at, recorded_at, cancelled_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21,
                ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42)",
        )
        .bind(&d.id)
        .bind(&d.organisation_id)
        .bind(d.row_version)
        .bind(&d.created_at);
        bind_doc!(q, d).execute(self.c()).await?;
        Ok(())
    }

    async fn update_purchase(
        &mut self,
        d: &PurchaseDocumentRecord,
        expected_version: i64,
    ) -> DbResult<bool> {
        let q = sqlx::query(
            "UPDATE purchase_document SET doc_type = ?4, status = ?5, gst_registration_id = ?6, series_id = ?7,
                number = ?8, financial_year = ?9, supplier_invoice_number = ?10, supplier_invoice_date = ?11,
                doc_date = ?12, due_date = ?13, party_id = ?14, party_name = ?15, party_gstin = ?16,
                party_gst_type = ?17, place_of_supply = ?18, supplier_state = ?19, supply_kind = ?20,
                reverse_charge = ?21, prices_include_tax = ?22, payment_mode = ?23, original_document_id = ?24,
                reason = ?25, notes = ?26, taxable_paise = ?27, cgst_paise = ?28, sgst_paise = ?29, utgst_paise = ?30,
                igst_paise = ?31, cess_paise = ?32, round_off_paise = ?33, total_paise = ?34, paid_paise = ?35,
                journal_entry_id = ?36, cancel_entry_id = ?37, cancel_reason = ?38, updated_at = ?39,
                recorded_at = ?40, cancelled_at = ?41, row_version = row_version + 1
             WHERE id = ?1 AND organisation_id = ?2 AND row_version = ?3",
        )
        .bind(&d.id)
        .bind(&d.organisation_id)
        .bind(expected_version);
        let r = bind_doc!(q, d).execute(self.c()).await?;
        Ok(r.rows_affected() == 1)
    }

    async fn replace_purchase_lines(
        &mut self,
        document_id: &str,
        lines: &[PurchaseLineRecord],
    ) -> DbResult<()> {
        sqlx::query("DELETE FROM purchase_line WHERE document_id = ?1")
            .bind(document_id)
            .execute(self.c())
            .await?;
        for l in lines {
            sqlx::query(
                "INSERT INTO purchase_line (id, document_id, line_no, item_id, account_id, description, hsn_sac, unit,
                    quantity, unit_price_paise, discount_paise, tax_category, gst_rate_bp, cess_rate_bp, amount_paise,
                    taxable_paise, cgst_paise, sgst_paise, utgst_paise, igst_paise, cess_paise, total_paise, itc_status,
                    itc_reason, itc_eligible_paise, itc_reviewed_at, itc_reviewed_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                    ?21, ?22, ?23, ?24, ?25, ?26, ?27)",
            )
            .bind(&l.id)
            .bind(document_id)
            .bind(l.line_no)
            .bind(&l.item_id)
            .bind(&l.account_id)
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
            .bind(&l.itc_status)
            .bind(&l.itc_reason)
            .bind(l.itc_eligible_paise)
            .bind(&l.itc_reviewed_at)
            .bind(&l.itc_reviewed_by)
            .execute(self.c())
            .await?;
        }
        Ok(())
    }

    async fn update_line_itc(&mut self, l: &PurchaseLineRecord) -> DbResult<()> {
        sqlx::query(
            "UPDATE purchase_line SET itc_status = ?2, itc_reason = ?3, itc_eligible_paise = ?4, itc_reviewed_at = ?5,
                itc_reviewed_by = ?6
             WHERE id = ?1",
        )
        .bind(&l.id)
        .bind(&l.itc_status)
        .bind(&l.itc_reason)
        .bind(l.itc_eligible_paise)
        .bind(&l.itc_reviewed_at)
        .bind(&l.itc_reviewed_by)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn delete_purchase_draft(&mut self, org_id: &str, id: &str) -> DbResult<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM purchase_document WHERE organisation_id = ?1 AND id = ?2 AND status = 'draft'",
        )
        .bind(org_id)
        .bind(id)
        .fetch_one(self.c())
        .await?;
        if n == 0 {
            return Ok(false);
        }
        sqlx::query("DELETE FROM purchase_line WHERE document_id = ?1")
            .bind(id)
            .execute(self.c())
            .await?;
        sqlx::query("DELETE FROM attachment WHERE organisation_id = ?1 AND entity_type = 'purchase_document' AND entity_id = ?2")
            .bind(org_id)
            .bind(id)
            .execute(self.c())
            .await?;
        sqlx::query("DELETE FROM purchase_document WHERE id = ?1")
            .bind(id)
            .execute(self.c())
            .await?;
        Ok(true)
    }

    async fn find_supplier_invoice(
        &mut self,
        org_id: &str,
        party_id: &str,
        supplier_invoice_number: &str,
        financial_year: &str,
        except_id: Option<&str>,
    ) -> DbResult<Option<PurchaseDocumentRecord>> {
        let sql = format!(
            "SELECT {DOC_COLS} FROM purchase_document
              WHERE organisation_id = ?1 AND party_id = ?2 AND UPPER(supplier_invoice_number) = UPPER(?3)
                AND financial_year = ?4 AND status = 'recorded' AND doc_type <> 'debit_note' AND id <> IFNULL(?5, '')
              LIMIT 1"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(party_id)
            .bind(supplier_invoice_number)
            .bind(financial_year)
            .bind(except_id)
            .fetch_optional(self.c())
            .await?)
    }

    async fn debit_notes_for(
        &mut self,
        org_id: &str,
        original_id: &str,
    ) -> DbResult<Vec<PurchaseDocumentRecord>> {
        let sql = format!(
            "SELECT {DOC_COLS} FROM purchase_document
              WHERE organisation_id = ?1 AND original_document_id = ?2 ORDER BY doc_date, created_at"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(original_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn latest_purchase_number_date(
        &mut self,
        org_id: &str,
        series_id: &str,
    ) -> DbResult<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT MAX(doc_date) FROM purchase_document WHERE organisation_id = ?1 AND series_id = ?2 AND status <> 'draft'",
        )
        .bind(org_id)
        .bind(series_id)
        .fetch_one(self.c())
        .await?)
    }
}

impl AttachmentRepo for SqliteUow {
    async fn put_blob(&mut self, sha256: &str, content: &[u8]) -> DbResult<()> {
        sqlx::query("INSERT INTO attachment_blob (sha256, content, size_bytes) VALUES (?1, ?2, ?3) ON CONFLICT(sha256) DO NOTHING")
            .bind(sha256)
            .bind(content)
            .bind(i64::try_from(content.len()).unwrap_or(i64::MAX))
            .execute(self.c())
            .await?;
        Ok(())
    }

    async fn insert_attachment(&mut self, a: &AttachmentRecord) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO attachment (id, organisation_id, entity_type, entity_id, file_name, content_type, size_bytes,
                sha256, uploaded_at, uploaded_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(&a.id)
        .bind(&a.organisation_id)
        .bind(&a.entity_type)
        .bind(&a.entity_id)
        .bind(&a.file_name)
        .bind(&a.content_type)
        .bind(a.size_bytes)
        .bind(&a.sha256)
        .bind(&a.uploaded_at)
        .bind(&a.uploaded_by)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn list_attachments(
        &mut self,
        org_id: &str,
        entity_type: &str,
        entity_id: &str,
    ) -> DbResult<Vec<AttachmentRecord>> {
        let sql = format!(
            "SELECT {ATT_COLS} FROM attachment WHERE organisation_id = ?1 AND entity_type = ?2 AND entity_id = ?3
              ORDER BY uploaded_at"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(entity_type)
            .bind(entity_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn get_attachment(
        &mut self,
        org_id: &str,
        id: &str,
    ) -> DbResult<Option<AttachmentRecord>> {
        let sql =
            format!("SELECT {ATT_COLS} FROM attachment WHERE organisation_id = ?1 AND id = ?2");
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(id)
            .fetch_optional(self.c())
            .await?)
    }

    async fn blob(&mut self, sha256: &str) -> DbResult<Option<Vec<u8>>> {
        Ok(
            sqlx::query_scalar("SELECT content FROM attachment_blob WHERE sha256 = ?1")
                .bind(sha256)
                .fetch_optional(self.c())
                .await?,
        )
    }

    async fn delete_attachment(&mut self, org_id: &str, id: &str) -> DbResult<()> {
        sqlx::query("DELETE FROM attachment WHERE organisation_id = ?1 AND id = ?2")
            .bind(org_id)
            .bind(id)
            .execute(self.c())
            .await?;
        Ok(())
    }
}
