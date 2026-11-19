//! Encrypted backup and verified restore (spec D17, §9.2 item 9).
//!
//! File layout (`.avbackup`):
//!
//! ```text
//! "AVUBACKUP\0"            10 bytes, magic
//! header length            u32 little-endian
//! header                   UTF-8 JSON (BackupHeader), readable without the passphrase
//! ciphertext               AES-256-GCM(SQLite snapshot), header bound as associated data
//! ```
//!
//! A restore never touches the live database directly: the decrypted file is
//! written next to it, checked (integrity, foreign keys, balanced journals,
//! schema version), and only swapped in by the host at the next start.

use std::path::Path;

use av_domain::new_id;
use av_persistence::records::{BackupLogRecord, DatabaseCheck};
use av_persistence::sqlite::inspect_database;
use av_persistence::{BackupRepo, Store, UnitOfWork};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::AppCore;
use crate::context::{Permission, RequestContext};
use crate::crypto::{self, CryptoError, KdfParams};
use crate::error::{AppError, AppResult, FieldError};
use crate::purchases::sha256_hex;
use crate::setup::{Change, audit};

const MAGIC: &[u8; 10] = b"AVUBACKUP\0";
const FORMAT: u32 = 1;
/// The longest header we accept (it is a few hundred bytes in practice).
const MAX_HEADER: usize = 16 * 1024;
pub const MIN_PASSPHRASE: usize = 8;
/// Days after which the Overview nudges for a new backup.
pub const BACKUP_DUE_DAYS: i64 = 7;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupHeader {
    pub format: u32,
    pub created_at: String,
    pub app_version: String,
    pub schema_version: i64,
    pub business_name: String,
    pub kdf: KdfParams,
    pub salt: String,
    pub nonce: String,
    /// Size of the database inside, bytes.
    pub database_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFile {
    pub file_name: String,
    #[serde(skip)]
    pub bytes: Vec<u8>,
    pub header: BackupHeader,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatus {
    pub last_backup: Option<BackupLogRecord>,
    pub last_restore: Option<BackupLogRecord>,
    /// Whole days since the last backup; `None` if there never was one.
    pub days_since_backup: Option<i64>,
    pub due: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreCheck {
    pub header: BackupHeader,
    pub check: DatabaseCheck,
}

fn passphrase_error(msg: &str) -> AppError {
    AppError::Validation(vec![FieldError {
        field: "passphrase".into(),
        message: msg.into(),
    }])
}

/// Reads the plaintext header. Fails if the file isn't an AccountsVU backup.
pub fn read_backup_header(bytes: &[u8]) -> AppResult<(BackupHeader, usize)> {
    let not_backup = || AppError::Invalid("this file isn't an AccountsVU backup".into());
    if bytes.len() < MAGIC.len() + 4 || &bytes[..MAGIC.len()] != MAGIC {
        return Err(not_backup());
    }
    let len_bytes: [u8; 4] = bytes[MAGIC.len()..MAGIC.len() + 4]
        .try_into()
        .map_err(|_| not_backup())?;
    let len = usize::try_from(u32::from_le_bytes(len_bytes)).map_err(|_| not_backup())?;
    let start = MAGIC.len() + 4;
    if len > MAX_HEADER || bytes.len() < start + len {
        return Err(not_backup());
    }
    let header: BackupHeader =
        serde_json::from_slice(&bytes[start..start + len]).map_err(|_| not_backup())?;
    if header.format != FORMAT {
        return Err(AppError::Invalid(
            "this backup was made by a newer version of AccountsVU; update the app first".into(),
        ));
    }
    Ok((header, start + len))
}

/// Encrypts `database` into a complete backup file.
pub(crate) fn seal_backup(
    database: &[u8],
    passphrase: &str,
    mut header: BackupHeader,
    kdf: KdfParams,
) -> Result<Vec<u8>, CryptoError> {
    let salt = crypto::random_bytes::<16>()?;
    let nonce = crypto::random_bytes::<12>()?;
    header.kdf = kdf;
    header.salt = crypto::to_hex(&salt);
    header.nonce = crypto::to_hex(&nonce);
    header.database_bytes = database.len() as u64;
    let header_json = serde_json::to_vec(&header).expect("header serialises");
    let mut prefix = Vec::with_capacity(MAGIC.len() + 4 + header_json.len());
    prefix.extend_from_slice(MAGIC);
    prefix.extend_from_slice(
        &u32::try_from(header_json.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    prefix.extend_from_slice(&header_json);
    let key = crypto::derive_key(passphrase.as_bytes(), &salt, kdf)?;
    let ct = crypto::seal(&key, &nonce, &prefix, database);
    prefix.extend_from_slice(&ct);
    Ok(prefix)
}

/// Decrypts a backup file; the header must be untouched.
pub(crate) fn open_backup(
    bytes: &[u8],
    passphrase: &str,
) -> AppResult<(BackupHeader, Zeroizing<Vec<u8>>)> {
    let (header, body_at) = read_backup_header(bytes)?;
    let (Some(salt), Some(nonce)) = (
        crypto::from_hex(&header.salt),
        crypto::from_hex(&header.nonce),
    ) else {
        return Err(AppError::Invalid("the backup header is damaged".into()));
    };
    let key =
        crypto::derive_key(passphrase.as_bytes(), &salt, header.kdf).map_err(|e| match e {
            CryptoError::Unsupported => {
                AppError::Invalid("this backup uses settings this version can't read".into())
            }
            other => AppError::Invalid(other.to_string()),
        })?;
    let db = crypto::open(&key, &nonce, &bytes[..body_at], &bytes[body_at..]).map_err(|_| {
        passphrase_error("That passphrase doesn't open this backup, or the file has been changed.")
    })?;
    Ok((header, db))
}

/// "Khope Traders" → "Khope-Traders".
fn name_stem(name: &str) -> String {
    let mut s = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c);
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    let t = s.trim_end_matches('-');
    if t.is_empty() {
        "AccountsVU".into()
    } else {
        t.chars().take(40).collect()
    }
}

impl<S: Store> AppCore<S> {
    /// A consistent copy of the database as bytes (via a temporary file).
    async fn snapshot_bytes(&self) -> AppResult<Zeroizing<Vec<u8>>> {
        let tmp = std::env::temp_dir().join(format!("accountsvu-snapshot-{}.sqlite3", new_id()));
        let result = async {
            self.store.snapshot(&tmp).await?;
            let bytes = std::fs::read(&tmp).map_err(av_persistence::DbError::from)?;
            Ok::<_, AppError>(Zeroizing::new(bytes))
        }
        .await;
        let _ = std::fs::remove_file(&tmp);
        result
    }

    /// Owner only. Encrypts a consistent snapshot of the whole database. The
    /// caller saves the bytes somewhere, then calls [`Self::record_backup`].
    pub async fn create_backup(
        &self,
        ctx: &RequestContext,
        passphrase: &str,
    ) -> AppResult<BackupFile> {
        self.create_backup_with(ctx, passphrase, KdfParams::STANDARD)
            .await
    }

    pub(crate) async fn create_backup_with(
        &self,
        ctx: &RequestContext,
        passphrase: &str,
        kdf: KdfParams,
    ) -> AppResult<BackupFile> {
        ctx.require(Permission::ManageBusiness)?;
        if passphrase.chars().count() < MIN_PASSPHRASE {
            return Err(passphrase_error("Use at least 8 characters."));
        }
        let org = {
            let mut uow = self.store.begin().await?;
            self.require_org(ctx, &mut uow).await?
        };
        let schema_version = self.store.schema_version().await?;
        let database = self.snapshot_bytes().await?;
        let now: DateTime<Utc> = Utc::now();
        let business_name = org
            .trade_name
            .clone()
            .unwrap_or_else(|| org.legal_name.clone());
        let file_name = format!(
            "{}-{}.avbackup",
            name_stem(&business_name),
            chrono::Local::now().format("%Y-%m-%d-%H%M")
        );
        let header = BackupHeader {
            format: FORMAT,
            created_at: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            app_version: env!("CARGO_PKG_VERSION").into(),
            schema_version,
            business_name,
            kdf,
            salt: String::new(),
            nonce: String::new(),
            database_bytes: 0,
        };
        let bytes = seal_backup(&database, passphrase, header, kdf)
            .map_err(|e| AppError::Invalid(e.to_string()))?;
        let (header, _) = read_backup_header(&bytes)?;
        Ok(BackupFile {
            file_name,
            bytes,
            header,
        })
    }

    /// Notes a backup the host has written to disk (last-backup indicator, audit).
    pub async fn record_backup(
        &self,
        ctx: &RequestContext,
        file_name: &str,
        bytes: &[u8],
    ) -> AppResult<BackupStatus> {
        ctx.require(Permission::ManageBusiness)?;
        let now = self.now();
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let rec = BackupLogRecord {
            id: new_id().to_string(),
            organisation_id: org.id.clone(),
            kind: "backup".into(),
            happened_at: now.clone(),
            file_name: file_name.into(),
            size_bytes: i64::try_from(bytes.len()).unwrap_or(i64::MAX),
            sha256: Some(sha256_hex(bytes)),
            actor: Some(ctx.actor.user_id.clone()),
        };
        uow.insert_backup_log(&rec).await?;
        let c = Change {
            action: "backup.create",
            entity_id: Some(&rec.id),
            before: None,
            after: crate::setup::json(&rec),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        uow.commit().await?;
        self.backup_status(ctx).await
    }

    /// Last backup and restore, and whether a backup is due.
    pub async fn backup_status(&self, ctx: &RequestContext) -> AppResult<BackupStatus> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let Some(org) = self.find_org(ctx, &mut uow).await? else {
            return Ok(BackupStatus {
                last_backup: None,
                last_restore: None,
                days_since_backup: None,
                due: false,
            });
        };
        let last_backup = uow.last_backup_log(&org.id, "backup").await?;
        let last_restore = uow.last_backup_log(&org.id, "restore").await?;
        let days_since_backup = last_backup.as_ref().and_then(|b| {
            DateTime::parse_from_rfc3339(&b.happened_at)
                .ok()
                .map(|t| (Utc::now() - t.with_timezone(&Utc)).num_days())
        });
        Ok(BackupStatus {
            due: days_since_backup.is_none_or(|d| d >= BACKUP_DUE_DAYS),
            last_backup,
            last_restore,
            days_since_backup,
        })
    }

    /// Restoring replaces everything: on a set-up install only the Owner may;
    /// on a fresh install (nothing to lose) anyone may.
    pub async fn require_restore_permission(&self, ctx: &RequestContext) -> AppResult<()> {
        let mut uow = self.store.begin().await?;
        if self.find_org(ctx, &mut uow).await?.is_some() {
            ctx.require(Permission::ManageBusiness)?;
        }
        Ok(())
    }

    /// Decrypts `bytes` into `staging` and checks the result. On any problem
    /// the staging file is removed and nothing else changes.
    pub async fn prepare_restore(
        &self,
        ctx: &RequestContext,
        bytes: &[u8],
        passphrase: &str,
        staging: &Path,
    ) -> AppResult<RestoreCheck> {
        self.require_restore_permission(ctx).await?;
        let (header, db) = open_backup(bytes, passphrase)?;
        let ours = self.store.schema_version().await?;
        let newer = || {
            AppError::Invalid(
                "this backup was made by a newer version of AccountsVU; update the app first"
                    .into(),
            )
        };
        if header.schema_version > ours {
            return Err(newer());
        }
        let _ = std::fs::remove_file(staging);
        let result = async {
            std::fs::write(staging, db.as_slice()).map_err(av_persistence::DbError::from)?;
            let check = inspect_database(staging).await.map_err(|_| {
                AppError::Invalid("the backup doesn't contain an AccountsVU database".into())
            })?;
            if check.schema_version > ours {
                return Err(newer());
            }
            if !check.is_sound() {
                return Err(AppError::Conflict(format!(
                    "The backup opened but failed its checks (integrity: {}, foreign-key problems: {}, unbalanced entries: {}). Nothing was restored.",
                    check.integrity, check.foreign_key_problems, check.unbalanced_entries
                )));
            }
            Ok(check)
        }
        .await;
        match result {
            Ok(check) => Ok(RestoreCheck { header, check }),
            Err(e) => {
                let _ = std::fs::remove_file(staging);
                Err(e)
            }
        }
    }

    /// Called by the host after a restored database was swapped in.
    pub async fn record_restore(
        &self,
        ctx: &RequestContext,
        kept_copy: Option<&str>,
    ) -> AppResult<()> {
        let now = self.now();
        let mut uow = self.store.begin().await?;
        let Some(org) = self.find_org(ctx, &mut uow).await? else {
            return Ok(());
        };
        let rec = BackupLogRecord {
            id: new_id().to_string(),
            organisation_id: org.id.clone(),
            kind: "restore".into(),
            happened_at: now.clone(),
            file_name: kept_copy.unwrap_or("").into(),
            size_bytes: 0,
            sha256: None,
            actor: Some(ctx.actor.user_id.clone()),
        };
        uow.insert_backup_log(&rec).await?;
        let c = Change {
            action: "backup.restore",
            entity_id: Some(&rec.id),
            before: None,
            after: crate::setup::json(&rec),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        uow.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::TEST_KDF;

    fn header() -> BackupHeader {
        BackupHeader {
            format: FORMAT,
            created_at: "2026-11-19T10:00:00Z".into(),
            app_version: "0.1.0".into(),
            schema_version: 8,
            business_name: "Khope Traders".into(),
            kdf: TEST_KDF,
            salt: String::new(),
            nonce: String::new(),
            database_bytes: 0,
        }
    }

    #[test]
    fn backup_files_round_trip_and_refuse_tampering() {
        let file =
            seal_backup(b"SQLite format 3\0...", "correct horse", header(), TEST_KDF).unwrap();
        let (h, _) = read_backup_header(&file).unwrap();
        assert_eq!(h.business_name, "Khope Traders");
        assert_eq!(h.database_bytes, 19);
        let (_, db) = open_backup(&file, "correct horse").unwrap();
        assert_eq!(db.as_slice(), b"SQLite format 3\0...");

        let err = open_backup(&file, "wrong horse!").unwrap_err();
        assert_eq!(err.field_errors()[0].field, "passphrase");

        // Changing the readable header (e.g. the business name) breaks the seal.
        let mut edited = file.clone();
        let pos = edited.windows(5).position(|w| w == b"Khope").unwrap();
        edited[pos] = b'X';
        assert!(open_backup(&edited, "correct horse").is_err());

        assert!(read_backup_header(b"PK\x03\x04 not a backup").is_err());
        assert!(read_backup_header(&file[..12]).is_err());
    }

    #[test]
    fn backup_names() {
        assert_eq!(
            name_stem("Khope Traders Pvt. Ltd."),
            "Khope-Traders-Pvt-Ltd"
        );
        assert_eq!(name_stem("..."), "AccountsVU");
    }
}
