//! Backup support for the SQLite adapter: consistent snapshots, checking a
//! database file before it is restored, and swapping a restored file in at
//! start-up (never while the pool is open).

use std::path::{Path, PathBuf};

use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Connection, SqliteConnection, SqlitePool};

use super::SqliteUow;
use crate::DbError;
use crate::records::{BackupLogRecord, DatabaseCheck};
use crate::repo::{BackupRepo, DbResult};

/// `VACUUM INTO` writes a compact, transactionally consistent copy.
pub(super) async fn vacuum_into(pool: &SqlitePool, dest: &Path) -> DbResult<()> {
    if dest.exists() {
        return Err(DbError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "snapshot target already exists",
        )));
    }
    sqlx::query("VACUUM INTO ?1")
        .bind(dest.to_string_lossy().to_string())
        .execute(pool)
        .await?;
    // An in-memory database vacuums into memory, not into a file.
    if !dest.exists() {
        return Err(DbError::Io(std::io::Error::other(
            "this database can't be copied to a file (in-memory databases have no snapshot)",
        )));
    }
    Ok(())
}

async fn scalar(c: &mut SqliteConnection, sql: &str) -> DbResult<i64> {
    Ok(sqlx::query_scalar::<_, i64>(sql).fetch_one(&mut *c).await?)
}

async fn table_exists(c: &mut SqliteConnection, name: &str) -> DbResult<bool> {
    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1")
            .bind(name)
            .fetch_one(&mut *c)
            .await?;
    Ok(n > 0)
}

async fn count(c: &mut SqliteConnection, table: &str) -> DbResult<i64> {
    if !table_exists(c, table).await? {
        return Ok(0);
    }
    scalar(c, &format!("SELECT COUNT(*) FROM {table}")).await
}

/// Opens `path` read-only and reports what it holds and whether it is sound.
/// Fails if the file isn't an AccountsVU database at all.
pub async fn inspect_database(path: &Path) -> DbResult<DatabaseCheck> {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false);
    let mut c = SqliteConnection::connect_with(&opts).await?;
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut c)
        .await?;
    let foreign_key_problems = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&mut c)
        .await?
        .len();
    if !table_exists(&mut c, "_sqlx_migrations").await?
        || !table_exists(&mut c, "journal_line").await?
    {
        return Err(DbError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not an AccountsVU database",
        )));
    }
    let schema_version = scalar(
        &mut c,
        "SELECT IFNULL(MAX(version), 0) FROM _sqlx_migrations WHERE success = 1",
    )
    .await?;
    let business_name: Option<String> = sqlx::query_scalar(
        "SELECT COALESCE(trade_name, legal_name) FROM organisation ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&mut c)
    .await?;
    let unbalanced_entries = scalar(
        &mut c,
        "SELECT COUNT(*) FROM (SELECT journal_entry_id FROM journal_line GROUP BY journal_entry_id \
         HAVING SUM(debit_paise) <> SUM(credit_paise))",
    )
    .await?;
    let last_entry_date: Option<String> =
        sqlx::query_scalar("SELECT MAX(entry_date) FROM journal_entry")
            .fetch_one(&mut c)
            .await?;
    let check = DatabaseCheck {
        integrity,
        foreign_key_problems: i64::try_from(foreign_key_problems).unwrap_or(i64::MAX),
        schema_version,
        business_name,
        parties: count(&mut c, "party").await?,
        items: count(&mut c, "item").await?,
        sales_documents: count(&mut c, "sales_document").await?,
        purchase_documents: count(&mut c, "purchase_document").await?,
        vouchers: count(&mut c, "money_voucher").await?,
        journal_entries: count(&mut c, "journal_entry").await?,
        unbalanced_entries,
        attachments: count(&mut c, "attachment").await?,
        last_entry_date,
    };
    c.close().await?;
    Ok(check)
}

/// Where a verified restore waits until the next start: `<db>.restore`.
pub fn pending_restore_path(db_path: &Path) -> PathBuf {
    let mut p = db_path.as_os_str().to_owned();
    p.push(".restore");
    PathBuf::from(p)
}

/// Rename, retrying briefly: on Windows a file that was just written or
/// closed can stay locked for a moment (antivirus scans, handle teardown).
fn rename_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last = None;
    for _ in 0..40 {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("rename failed")))
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(suffix);
    PathBuf::from(p)
}

/// Call before opening the database. If a restore is waiting, the current
/// database (with its WAL files) is moved aside as
/// `<stem>.before-restore-<stamp>.<ext>` and the restored file takes its
/// place. Returns the path the old data was kept at.
pub fn apply_pending_restore(db_path: &Path, stamp: &str) -> std::io::Result<Option<PathBuf>> {
    let pending = pending_restore_path(db_path);
    if !pending.exists() {
        return Ok(None);
    }
    let stem = db_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "accountsvu".into());
    let ext = db_path
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "sqlite3".into());
    let kept = db_path.with_file_name(format!("{stem}.before-restore-{stamp}.{ext}"));
    if db_path.exists() {
        rename_retry(db_path, &kept)?;
        for s in ["-wal", "-shm"] {
            let side = with_suffix(db_path, s);
            if side.exists() {
                rename_retry(&side, &with_suffix(&kept, s))?;
            }
        }
    }
    rename_retry(&pending, db_path)?;
    Ok(kept.exists().then_some(kept))
}

const LOG_COLS: &str =
    "id, organisation_id, kind, happened_at, file_name, size_bytes, sha256, actor";

impl BackupRepo for SqliteUow {
    async fn insert_backup_log(&mut self, r: &BackupLogRecord) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO backup_log (id, organisation_id, kind, happened_at, file_name, size_bytes, sha256, actor) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(&r.id)
        .bind(&r.organisation_id)
        .bind(&r.kind)
        .bind(&r.happened_at)
        .bind(&r.file_name)
        .bind(r.size_bytes)
        .bind(&r.sha256)
        .bind(&r.actor)
        .execute(self.c())
        .await?;
        Ok(())
    }

    async fn last_backup_log(
        &mut self,
        org_id: &str,
        kind: &str,
    ) -> DbResult<Option<BackupLogRecord>> {
        let sql = format!(
            "SELECT {LOG_COLS} FROM backup_log WHERE organisation_id = ?1 AND kind = ?2 \
             ORDER BY happened_at DESC LIMIT 1"
        );
        Ok(sqlx::query_as(&sql)
            .bind(org_id)
            .bind(kind)
            .fetch_optional(self.c())
            .await?)
    }
}
