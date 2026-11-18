//! SQLite implementation of the masters and journal repositories (Phase 1.2).

use super::SqliteUow;
use crate::records::{ItemRecord, JournalEntryRecord, JournalLineRecord, PartyRecord};
use crate::repo::{DbResult, ItemRepo, JournalRepo, PartyRepo};

// ---------------------------------------------------------------------------
// Parties
// ---------------------------------------------------------------------------

const PARTY_COLS: &str = "id, organisation_id, kind, name, legal_name, gst_type, gstin, pan, state_code, \
    address_line1, address_line2, city, pincode, shipping_same, shipping_line1, shipping_line2, shipping_city, \
    shipping_pincode, shipping_state_code, contact_person, email, phone, payment_terms_days, credit_limit_paise, \
    opening_balance_paise, opening_entry_id, notes, is_active, row_version, created_at, updated_at";

impl PartyRepo for SqliteUow {
    async fn list_parties(&mut self, org_id: &str) -> DbResult<Vec<PartyRecord>> {
        let sql = format!(
            "SELECT {PARTY_COLS} FROM party WHERE organisation_id = ?1 ORDER BY is_active DESC, name COLLATE NOCASE"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn get_party(&mut self, org_id: &str, id: &str) -> DbResult<Option<PartyRecord>> {
        let sql = format!("SELECT {PARTY_COLS} FROM party WHERE organisation_id = ?1 AND id = ?2");
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(id)
            .fetch_optional(self.c())
            .await?)
    }

    async fn party_name_taken(
        &mut self,
        org_id: &str,
        name: &str,
        except_id: Option<&str>,
    ) -> DbResult<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM party WHERE organisation_id = ?1 AND name = ?2 COLLATE NOCASE AND id <> IFNULL(?3, '')",
        )
        .bind(org_id)
        .bind(name)
        .bind(except_id)
        .fetch_one(self.c())
        .await?;
        Ok(n > 0)
    }

    async fn party_gstin_taken(
        &mut self,
        org_id: &str,
        gstin: &str,
        except_id: Option<&str>,
    ) -> DbResult<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM party WHERE organisation_id = ?1 AND gstin = ?2 AND id <> IFNULL(?3, '')",
        )
        .bind(org_id)
        .bind(gstin)
        .bind(except_id)
        .fetch_one(self.c())
        .await?;
        Ok(n > 0)
    }

    async fn insert_party(&mut self, p: &PartyRecord) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO party (id, organisation_id, kind, name, legal_name, gst_type, gstin, pan, state_code,
                address_line1, address_line2, city, pincode, shipping_same, shipping_line1, shipping_line2, shipping_city,
                shipping_pincode, shipping_state_code, contact_person, email, phone, payment_terms_days,
                credit_limit_paise, opening_balance_paise, opening_entry_id, notes, is_active, row_version,
                created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31)",
        )
        .bind(&p.id)
        .bind(&p.organisation_id)
        .bind(&p.kind)
        .bind(&p.name)
        .bind(&p.legal_name)
        .bind(&p.gst_type)
        .bind(&p.gstin)
        .bind(&p.pan)
        .bind(p.state_code)
        .bind(&p.address_line1)
        .bind(&p.address_line2)
        .bind(&p.city)
        .bind(&p.pincode)
        .bind(p.shipping_same)
        .bind(&p.shipping_line1)
        .bind(&p.shipping_line2)
        .bind(&p.shipping_city)
        .bind(&p.shipping_pincode)
        .bind(p.shipping_state_code)
        .bind(&p.contact_person)
        .bind(&p.email)
        .bind(&p.phone)
        .bind(p.payment_terms_days)
        .bind(p.credit_limit_paise)
        .bind(p.opening_balance_paise)
        .bind(&p.opening_entry_id)
        .bind(&p.notes)
        .bind(p.is_active)
        .bind(p.row_version)
        .bind(&p.created_at)
        .bind(&p.updated_at)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn update_party(&mut self, p: &PartyRecord, expected_version: i64) -> DbResult<bool> {
        let r = sqlx::query(
            "UPDATE party SET kind = ?3, name = ?4, legal_name = ?5, gst_type = ?6, gstin = ?7, pan = ?8,
                state_code = ?9, address_line1 = ?10, address_line2 = ?11, city = ?12, pincode = ?13,
                shipping_same = ?14, shipping_line1 = ?15, shipping_line2 = ?16, shipping_city = ?17,
                shipping_pincode = ?18, shipping_state_code = ?19, contact_person = ?20, email = ?21, phone = ?22,
                payment_terms_days = ?23, credit_limit_paise = ?24, opening_balance_paise = ?25,
                opening_entry_id = ?26, notes = ?27, is_active = ?28, updated_at = ?29,
                row_version = row_version + 1
             WHERE id = ?1 AND organisation_id = ?2 AND row_version = ?30",
        )
        .bind(&p.id)
        .bind(&p.organisation_id)
        .bind(&p.kind)
        .bind(&p.name)
        .bind(&p.legal_name)
        .bind(&p.gst_type)
        .bind(&p.gstin)
        .bind(&p.pan)
        .bind(p.state_code)
        .bind(&p.address_line1)
        .bind(&p.address_line2)
        .bind(&p.city)
        .bind(&p.pincode)
        .bind(p.shipping_same)
        .bind(&p.shipping_line1)
        .bind(&p.shipping_line2)
        .bind(&p.shipping_city)
        .bind(&p.shipping_pincode)
        .bind(p.shipping_state_code)
        .bind(&p.contact_person)
        .bind(&p.email)
        .bind(&p.phone)
        .bind(p.payment_terms_days)
        .bind(p.credit_limit_paise)
        .bind(p.opening_balance_paise)
        .bind(&p.opening_entry_id)
        .bind(&p.notes)
        .bind(p.is_active)
        .bind(&p.updated_at)
        .bind(expected_version)
        .execute(self.c())
        .await?;
        Ok(r.rows_affected() == 1)
    }
}

// ---------------------------------------------------------------------------
// Items
// ---------------------------------------------------------------------------

const ITEM_COLS: &str = "id, organisation_id, kind, name, code, description, hsn_sac, unit, tax_category, gst_rate_bp, \
    cess_rate_bp, sales_price_paise, purchase_price_paise, price_includes_tax, track_stock, is_active, row_version, \
    created_at, updated_at";

impl ItemRepo for SqliteUow {
    async fn list_items(&mut self, org_id: &str) -> DbResult<Vec<ItemRecord>> {
        let sql = format!(
            "SELECT {ITEM_COLS} FROM item WHERE organisation_id = ?1 ORDER BY is_active DESC, name COLLATE NOCASE"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn get_item(&mut self, org_id: &str, id: &str) -> DbResult<Option<ItemRecord>> {
        let sql = format!("SELECT {ITEM_COLS} FROM item WHERE organisation_id = ?1 AND id = ?2");
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(id)
            .fetch_optional(self.c())
            .await?)
    }

    async fn item_name_taken(
        &mut self,
        org_id: &str,
        name: &str,
        except_id: Option<&str>,
    ) -> DbResult<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM item WHERE organisation_id = ?1 AND name = ?2 COLLATE NOCASE AND id <> IFNULL(?3, '')",
        )
        .bind(org_id)
        .bind(name)
        .bind(except_id)
        .fetch_one(self.c())
        .await?;
        Ok(n > 0)
    }

    async fn item_code_taken(
        &mut self,
        org_id: &str,
        code: &str,
        except_id: Option<&str>,
    ) -> DbResult<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM item WHERE organisation_id = ?1 AND code = ?2 COLLATE NOCASE AND id <> IFNULL(?3, '')",
        )
        .bind(org_id)
        .bind(code)
        .bind(except_id)
        .fetch_one(self.c())
        .await?;
        Ok(n > 0)
    }

    async fn insert_item(&mut self, i: &ItemRecord) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO item (id, organisation_id, kind, name, code, description, hsn_sac, unit, tax_category,
                gst_rate_bp, cess_rate_bp, sales_price_paise, purchase_price_paise, price_includes_tax, track_stock,
                is_active, row_version, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
        )
        .bind(&i.id)
        .bind(&i.organisation_id)
        .bind(&i.kind)
        .bind(&i.name)
        .bind(&i.code)
        .bind(&i.description)
        .bind(&i.hsn_sac)
        .bind(&i.unit)
        .bind(&i.tax_category)
        .bind(i.gst_rate_bp)
        .bind(i.cess_rate_bp)
        .bind(i.sales_price_paise)
        .bind(i.purchase_price_paise)
        .bind(i.price_includes_tax)
        .bind(i.track_stock)
        .bind(i.is_active)
        .bind(i.row_version)
        .bind(&i.created_at)
        .bind(&i.updated_at)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn update_item(&mut self, i: &ItemRecord, expected_version: i64) -> DbResult<bool> {
        let r = sqlx::query(
            "UPDATE item SET kind = ?3, name = ?4, code = ?5, description = ?6, hsn_sac = ?7, unit = ?8,
                tax_category = ?9, gst_rate_bp = ?10, cess_rate_bp = ?11, sales_price_paise = ?12,
                purchase_price_paise = ?13, price_includes_tax = ?14, track_stock = ?15, is_active = ?16,
                updated_at = ?17, row_version = row_version + 1
             WHERE id = ?1 AND organisation_id = ?2 AND row_version = ?18",
        )
        .bind(&i.id)
        .bind(&i.organisation_id)
        .bind(&i.kind)
        .bind(&i.name)
        .bind(&i.code)
        .bind(&i.description)
        .bind(&i.hsn_sac)
        .bind(&i.unit)
        .bind(&i.tax_category)
        .bind(i.gst_rate_bp)
        .bind(i.cess_rate_bp)
        .bind(i.sales_price_paise)
        .bind(i.purchase_price_paise)
        .bind(i.price_includes_tax)
        .bind(i.track_stock)
        .bind(i.is_active)
        .bind(&i.updated_at)
        .bind(expected_version)
        .execute(self.c())
        .await?;
        Ok(r.rows_affected() == 1)
    }
}

// ---------------------------------------------------------------------------
// Journals
// ---------------------------------------------------------------------------

impl JournalRepo for SqliteUow {
    async fn account_id_by_role(&mut self, org_id: &str, role: &str) -> DbResult<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT id FROM account WHERE organisation_id = ?1 AND system_role = ?2",
        )
        .bind(org_id)
        .bind(role)
        .fetch_optional(self.c())
        .await?)
    }

    async fn insert_journal(
        &mut self,
        e: &JournalEntryRecord,
        lines: &[JournalLineRecord],
    ) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO journal_entry (id, organisation_id, entry_date, source_type, source_id, narration,
                reverses_entry_id, idempotency_key, posted_at, posted_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(&e.id)
        .bind(&e.organisation_id)
        .bind(&e.entry_date)
        .bind(&e.source_type)
        .bind(&e.source_id)
        .bind(&e.narration)
        .bind(&e.reverses_entry_id)
        .bind(&e.idempotency_key)
        .bind(&e.posted_at)
        .bind(&e.posted_by)
        .execute(self.c())
        .await?;
        for l in lines {
            sqlx::query(
                "INSERT INTO journal_line (id, journal_entry_id, line_no, account_id, debit_paise, credit_paise, memo,
                    party_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(&l.id)
            .bind(&l.journal_entry_id)
            .bind(l.line_no)
            .bind(&l.account_id)
            .bind(l.debit_paise)
            .bind(l.credit_paise)
            .bind(&l.memo)
            .bind(&l.party_id)
            .execute(self.c())
            .await?;
        }
        Ok(())
    }

    async fn party_balances(&mut self, org_id: &str) -> DbResult<Vec<(String, i64)>> {
        Ok(sqlx::query_as(
            "SELECT l.party_id, SUM(l.debit_paise - l.credit_paise)
               FROM journal_line l
               JOIN journal_entry e ON e.id = l.journal_entry_id
              WHERE e.organisation_id = ?1 AND l.party_id IS NOT NULL
              GROUP BY l.party_id",
        )
        .bind(org_id)
        .fetch_all(self.c())
        .await?)
    }
}
