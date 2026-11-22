//! Offline filing records (Phase 2.5).

use super::SqliteUow;
use crate::records::{GstFilingFileRecord, GstFilingRecord};
use crate::repo::{DbResult, GstFilingRepo};

const FILING_COLS: &str = "id, organisation_id, gst_registration_id, return_type, period, status, channel, \
    arn, filed_on, note, return_pack_version, rule_pack_version, summary_json, workpaper_sha256, \
    warning_count, recorded_at, recorded_by, removed_reason, removed_at, removed_by";

const FILE_COLS: &str = "id, filing_id, kind, file_name, content_type, size_bytes, sha256";

impl GstFilingRepo for SqliteUow {
    async fn list_gst_filings(
        &mut self,
        org_id: &str,
        registration_id: &str,
    ) -> DbResult<Vec<GstFilingRecord>> {
        let sql = format!(
            "SELECT {FILING_COLS} FROM gst_filing \
             WHERE organisation_id = ?1 AND gst_registration_id = ?2 ORDER BY period, recorded_at"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(registration_id)
            .fetch_all(self.c())
            .await?)
    }

    async fn insert_gst_filing(&mut self, f: &GstFilingRecord) -> DbResult<()> {
        sqlx::query(&format!(
            "INSERT INTO gst_filing ({FILING_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, \
             ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)"
        ))
        .bind(&f.id)
        .bind(&f.organisation_id)
        .bind(&f.gst_registration_id)
        .bind(&f.return_type)
        .bind(&f.period)
        .bind(&f.status)
        .bind(&f.channel)
        .bind(&f.arn)
        .bind(&f.filed_on)
        .bind(&f.note)
        .bind(&f.return_pack_version)
        .bind(&f.rule_pack_version)
        .bind(&f.summary_json)
        .bind(&f.workpaper_sha256)
        .bind(f.warning_count)
        .bind(&f.recorded_at)
        .bind(&f.recorded_by)
        .bind(&f.removed_reason)
        .bind(&f.removed_at)
        .bind(&f.removed_by)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn remove_gst_filing(
        &mut self,
        org_id: &str,
        id: &str,
        reason: &str,
        at: &str,
        by: &str,
    ) -> DbResult<bool> {
        let r = sqlx::query(
            "UPDATE gst_filing SET status = 'removed', removed_reason = ?3, removed_at = ?4, removed_by = ?5 \
             WHERE organisation_id = ?1 AND id = ?2 AND status = 'filed'",
        )
        .bind(org_id)
        .bind(id)
        .bind(reason)
        .bind(at)
        .bind(by)
        .execute(self.c())
        .await?;
        Ok(r.rows_affected() == 1)
    }

    async fn insert_gst_filing_file(&mut self, f: &GstFilingFileRecord) -> DbResult<()> {
        sqlx::query(&format!(
            "INSERT INTO gst_filing_file ({FILE_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
        ))
        .bind(&f.id)
        .bind(&f.filing_id)
        .bind(&f.kind)
        .bind(&f.file_name)
        .bind(&f.content_type)
        .bind(f.size_bytes)
        .bind(&f.sha256)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn gst_filing_files(&mut self, filing_id: &str) -> DbResult<Vec<GstFilingFileRecord>> {
        let sql =
            format!("SELECT {FILE_COLS} FROM gst_filing_file WHERE filing_id = ?1 ORDER BY kind");
        Ok(sqlx::query_as(&sql)
            .bind(filing_id)
            .fetch_all(self.c())
            .await?)
    }
}
