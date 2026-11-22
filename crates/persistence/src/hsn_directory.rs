//! Local HSN/SAC directory (Phase 2.5.1).
//!
//! A lookup aid built from the GSTN master (`HSN_SAC.xlsx`). It's reference
//! data, not books: it lives in its own SQLite file next to the database
//! (`hsn-directory.sqlite3`), is shared by every organisation on the install,
//! isn't part of backups and can be rebuilt by syncing again. Search uses an
//! FTS5 index over each code's description and the descriptions of the
//! heading and sub-heading it falls under, so the many "Other" rows are still
//! found by what they are.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;

use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use crate::DbError;

/// Default file name inside the app data directory.
pub const FILE_NAME: &str = "hsn-directory.sqlite3";

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS hsn_code (
    code        TEXT PRIMARY KEY,
    kind        TEXT NOT NULL CHECK (kind IN ('hsn', 'sac')),
    description TEXT NOT NULL,
    context     TEXT NOT NULL DEFAULT '',
    added_at    TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    delisted_at TEXT
);
CREATE VIRTUAL TABLE IF NOT EXISTS hsn_fts USING fts5(
    description, context,
    content = 'hsn_code', content_rowid = 'rowid',
    tokenize = 'unicode61 remove_diacritics 2',
    prefix = '2 3'
);
CREATE TABLE IF NOT EXISTS directory_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

/// One code read from the master file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryEntry {
    pub code: String,
    /// `hsn` or `sac`.
    pub kind: &'static str,
    pub description: String,
}

/// Where a sync came from, recorded with it.
#[derive(Clone, Debug)]
pub struct SyncSource {
    /// `gstn` (downloaded) or `file` (imported by hand).
    pub channel: String,
    /// URL or file name.
    pub origin: String,
    pub sha256: String,
    pub bytes: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncDiff {
    pub added: usize,
    pub changed: usize,
    /// No longer in the master; kept and flagged.
    pub delisted: usize,
    /// Back in the master after being delisted.
    pub restored: usize,
    pub unchanged: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryStatus {
    pub hsn_codes: i64,
    pub sac_codes: i64,
    pub delisted: i64,
    pub synced_at: Option<String>,
    pub channel: Option<String>,
    pub origin: Option<String>,
    pub sha256: Option<String>,
    pub last_diff: Option<SyncDiff>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeRef {
    pub code: String,
    pub description: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryRow {
    pub code: String,
    pub kind: String,
    pub description: String,
    pub delisted: bool,
    /// The 2-digit chapter, 4-digit heading and 6-digit sub-heading the code
    /// falls under (each only when it's in the directory and isn't the code).
    pub chapter: Option<CodeRef>,
    pub heading: Option<CodeRef>,
    pub subheading: Option<CodeRef>,
}

#[derive(Clone, Debug)]
pub struct DirectoryQuery {
    pub text: String,
    /// `hsn`, `sac` or `None` for both.
    pub kind: Option<String>,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct HsnDirectory {
    pool: SqlitePool,
}

impl HsnDirectory {
    pub async fn open(path: &Path) -> Result<Self, DbError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);
        Self::connect(opts, 2).await
    }

    pub async fn open_in_memory() -> Result<Self, DbError> {
        Self::connect(SqliteConnectOptions::from_str("sqlite::memory:")?, 1).await
    }

    async fn connect(opts: SqliteConnectOptions, max: u32) -> Result<Self, DbError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(max)
            .connect_with(opts)
            .await?;
        sqlx::raw_sql(SCHEMA).execute(&pool).await?;
        Ok(HsnDirectory { pool })
    }

    async fn meta(&self) -> Result<HashMap<String, String>, DbError> {
        let rows = sqlx::query("SELECT key, value FROM directory_meta")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get::<String, _>(0), r.get::<String, _>(1)))
            .collect())
    }

    /// SHA-256 of the file the directory was last built from.
    pub async fn last_sha256(&self) -> Result<Option<String>, DbError> {
        Ok(self.meta().await?.remove("sha256"))
    }

    pub async fn status(&self) -> Result<DirectoryStatus, DbError> {
        let row = sqlx::query(
            "SELECT \
               IFNULL(SUM(kind = 'hsn' AND delisted_at IS NULL), 0), \
               IFNULL(SUM(kind = 'sac' AND delisted_at IS NULL), 0), \
               IFNULL(SUM(delisted_at IS NOT NULL), 0) \
             FROM hsn_code",
        )
        .fetch_one(&self.pool)
        .await?;
        let mut meta = self.meta().await?;
        Ok(DirectoryStatus {
            hsn_codes: row.get(0),
            sac_codes: row.get(1),
            delisted: row.get(2),
            synced_at: meta.remove("synced_at"),
            channel: meta.remove("channel"),
            origin: meta.remove("origin"),
            sha256: meta.remove("sha256"),
            last_diff: meta
                .remove("last_diff")
                .and_then(|j| serde_json::from_str::<DiffJson>(&j).ok())
                .map(Into::into),
        })
    }

    /// Replace the directory with `entries` in one transaction. Codes no
    /// longer listed are kept and marked delisted, so an item that uses one
    /// still finds it.
    pub async fn replace(
        &self,
        entries: &[DirectoryEntry],
        source: &SyncSource,
        now: &str,
    ) -> Result<SyncDiff, DbError> {
        let mut tx = self.pool.begin().await?;
        let existing: HashMap<String, (String, String, bool)> =
            sqlx::query("SELECT code, kind, description, delisted_at IS NOT NULL FROM hsn_code")
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .map(|r| {
                    (
                        r.get::<String, _>(0),
                        (
                            r.get::<String, _>(1),
                            r.get::<String, _>(2),
                            r.get::<bool, _>(3),
                        ),
                    )
                })
                .collect();

        let by_code: HashMap<&str, &str> = entries
            .iter()
            .map(|e| (e.code.as_str(), e.description.as_str()))
            .collect();
        let context_of = |code: &str| -> String {
            let mut parts = Vec::new();
            for len in [2usize, 4, 6] {
                if code.len() > len
                    && let Some(d) = by_code.get(&code[..len])
                {
                    parts.push(*d);
                }
            }
            parts.join(" · ")
        };

        let mut diff = SyncDiff::default();
        let mut seen = HashSet::new();
        for e in entries {
            if !seen.insert(e.code.as_str()) {
                continue;
            }
            let context = context_of(&e.code);
            match existing.get(&e.code) {
                None => {
                    diff.added += 1;
                    sqlx::query(
                        "INSERT INTO hsn_code (code, kind, description, context, added_at, updated_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                    )
                    .bind(&e.code)
                    .bind(e.kind)
                    .bind(&e.description)
                    .bind(&context)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
                }
                Some((kind, desc, delisted)) => {
                    let changed = kind != e.kind || desc != &e.description;
                    if *delisted {
                        diff.restored += 1;
                    } else if changed {
                        diff.changed += 1;
                    } else {
                        diff.unchanged += 1;
                    }
                    sqlx::query(
                        "UPDATE hsn_code SET kind = ?2, description = ?3, context = ?4, delisted_at = NULL, \
                           updated_at = CASE WHEN ?5 THEN ?6 ELSE updated_at END \
                         WHERE code = ?1",
                    )
                    .bind(&e.code)
                    .bind(e.kind)
                    .bind(&e.description)
                    .bind(&context)
                    .bind(changed || *delisted)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }
        for (code, (_, _, delisted)) in &existing {
            if !*delisted && !seen.contains(code.as_str()) {
                diff.delisted += 1;
                sqlx::query("UPDATE hsn_code SET delisted_at = ?2 WHERE code = ?1")
                    .bind(code)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        sqlx::query("INSERT INTO hsn_fts (hsn_fts) VALUES ('rebuild')")
            .execute(&mut *tx)
            .await?;

        let diff_json = serde_json::to_string(&DiffJson::from(&diff)).unwrap_or_default();
        let bytes = source.bytes.to_string();
        for (k, v) in [
            ("synced_at", now),
            ("channel", source.channel.as_str()),
            ("origin", source.origin.as_str()),
            ("sha256", source.sha256.as_str()),
            ("bytes", bytes.as_str()),
            ("last_diff", diff_json.as_str()),
        ] {
            sqlx::query(
                "INSERT INTO directory_meta (key, value) VALUES (?1, ?2) \
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            )
            .bind(k)
            .bind(v)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(diff)
    }

    /// Record that a sync found the same file again.
    pub async fn touch(&self, now: &str) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO directory_meta (key, value) VALUES ('synced_at', ?1) \
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        )
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Codes starting with the digits typed, in code order (a heading, then
    /// what falls under it); or codes whose description, heading or
    /// sub-heading contains every word typed (prefixes match), best first.
    /// Delisted codes come last. Returns at most `limit + 1` rows so the
    /// caller can tell there are more.
    pub async fn search(&self, q: &DirectoryQuery) -> Result<Vec<DirectoryRow>, DbError> {
        let text = q.text.trim();
        let limit = i64::try_from(q.limit.clamp(1, 200) + 1).unwrap_or(51);
        let kind = q.kind.as_deref().filter(|k| matches!(*k, "hsn" | "sac"));
        let digits: String = text
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '.')
            .collect();
        let rows = if text.is_empty() {
            Vec::new()
        } else if digits.chars().all(|c| c.is_ascii_digit()) {
            sqlx::query(
                "SELECT code, kind, description, delisted_at IS NOT NULL FROM hsn_code \
                 WHERE code LIKE ?1 || '%' AND (?2 IS NULL OR kind = ?2) \
                 ORDER BY delisted_at IS NOT NULL, code LIMIT ?3",
            )
            .bind(&digits)
            .bind(kind)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            let Some(fts) = fts_query(text) else {
                return Ok(Vec::new());
            };
            sqlx::query(
                "SELECT c.code, c.kind, c.description, c.delisted_at IS NOT NULL \
                 FROM hsn_fts JOIN hsn_code c ON c.rowid = hsn_fts.rowid \
                 WHERE hsn_fts MATCH ?1 AND (?2 IS NULL OR c.kind = ?2) \
                 ORDER BY c.delisted_at IS NOT NULL, bm25(hsn_fts, 10.0, 2.0), length(c.code) DESC, c.code \
                 LIMIT ?3",
            )
            .bind(&fts)
            .bind(kind)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };
        let found: Vec<(String, String, String, bool)> = rows
            .into_iter()
            .map(|r| (r.get(0), r.get(1), r.get(2), r.get(3)))
            .collect();
        self.with_parents(found).await
    }

    /// One code with its chapter, heading and sub-heading.
    pub async fn get(&self, code: &str) -> Result<Option<DirectoryRow>, DbError> {
        let row = sqlx::query(
            "SELECT code, kind, description, delisted_at IS NOT NULL FROM hsn_code WHERE code = ?1",
        )
        .bind(code.trim())
        .fetch_optional(&self.pool)
        .await?;
        let Some(r) = row else { return Ok(None) };
        Ok(self
            .with_parents(vec![(r.get(0), r.get(1), r.get(2), r.get(3))])
            .await?
            .pop())
    }

    async fn with_parents(
        &self,
        found: Vec<(String, String, String, bool)>,
    ) -> Result<Vec<DirectoryRow>, DbError> {
        let mut wanted: Vec<String> = Vec::new();
        for (code, ..) in &found {
            for len in [2usize, 4, 6] {
                if code.len() > len {
                    wanted.push(code[..len].to_owned());
                }
            }
        }
        wanted.sort();
        wanted.dedup();
        let mut parents: HashMap<String, String> = HashMap::new();
        for chunk in wanted.chunks(500) {
            let marks = vec!["?"; chunk.len()].join(",");
            let sql = format!("SELECT code, description FROM hsn_code WHERE code IN ({marks})");
            let mut query = sqlx::query(&sql);
            for c in chunk {
                query = query.bind(c);
            }
            for r in query.fetch_all(&self.pool).await? {
                parents.insert(r.get(0), r.get(1));
            }
        }
        let parent = |code: &str, len: usize| -> Option<CodeRef> {
            if code.len() <= len {
                return None;
            }
            let p = &code[..len];
            parents.get(p).map(|d| CodeRef {
                code: p.to_owned(),
                description: d.clone(),
            })
        };
        Ok(found
            .into_iter()
            .map(|(code, kind, description, delisted)| DirectoryRow {
                chapter: parent(&code, 2),
                heading: parent(&code, 4),
                subheading: parent(&code, 6),
                code,
                kind,
                description,
                delisted,
            })
            .collect())
    }
}

/// Every word as a quoted prefix term, all required: `"pork"* "fresh"*`.
fn fts_query(text: &str) -> Option<String> {
    let terms: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(8)
        .map(|w| format!("\"{}\"*", w.to_lowercase()))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

#[derive(serde::Deserialize, Serialize)]
struct DiffJson {
    added: usize,
    changed: usize,
    delisted: usize,
    restored: usize,
    unchanged: usize,
}

impl From<&SyncDiff> for DiffJson {
    fn from(d: &SyncDiff) -> Self {
        DiffJson {
            added: d.added,
            changed: d.changed,
            delisted: d.delisted,
            restored: d.restored,
            unchanged: d.unchanged,
        }
    }
}

impl From<DiffJson> for SyncDiff {
    fn from(d: DiffJson) -> Self {
        SyncDiff {
            added: d.added,
            changed: d.changed,
            delisted: d.delisted,
            restored: d.restored,
            unchanged: d.unchanged,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(code: &str, desc: &str) -> DirectoryEntry {
        DirectoryEntry {
            code: code.into(),
            kind: if code.starts_with("99") { "sac" } else { "hsn" },
            description: desc.into(),
        }
    }

    fn src(sha: &str) -> SyncSource {
        SyncSource {
            channel: "file".into(),
            origin: "HSN_SAC.xlsx".into(),
            sha256: sha.into(),
            bytes: 10,
        }
    }

    #[tokio::test]
    async fn sync_search_and_resync() {
        let dir = HsnDirectory::open_in_memory().await.unwrap();
        let first = vec![
            e("02", "Meat and edible meat offal"),
            e("0202", "Meat of bovine animals, frozen"),
            e("020210", "Carcases and half-carcases"),
            e("02021000", "Carcases and half-carcases"),
            e("02029000", "Other"),
            e("99", "All Services"),
            e("9954", "Construction services"),
            e("995411", "Construction services of residential buildings"),
        ];
        let d = dir
            .replace(&first, &src("a"), "2026-11-22T00:00:00Z")
            .await
            .unwrap();
        assert_eq!(d.added, 8);

        let s = dir.status().await.unwrap();
        assert_eq!((s.hsn_codes, s.sac_codes, s.delisted), (5, 3, 0));
        assert_eq!(dir.last_sha256().await.unwrap().as_deref(), Some("a"));

        // By code: the heading, then what falls under it.
        let q = |t: &str, k: Option<&str>| DirectoryQuery {
            text: t.into(),
            kind: k.map(Into::into),
            limit: 20,
        };
        let rows = dir.search(&q("0202", None)).await.unwrap();
        let codes: Vec<_> = rows.iter().map(|r| r.code.as_str()).collect();
        assert_eq!(codes, ["0202", "020210", "02021000", "02029000"]);
        let r = &rows[2];
        assert_eq!(
            r.chapter.as_ref().unwrap().description,
            "Meat and edible meat offal"
        );
        assert_eq!(r.heading.as_ref().unwrap().code, "0202");
        assert_eq!(r.subheading.as_ref().unwrap().code, "020210");

        // By words, including the heading's words for an "Other" row.
        let rows = dir.search(&q("bovine oth", None)).await.unwrap();
        assert_eq!(rows.first().map(|r| r.code.as_str()), Some("02029000"));
        let rows = dir.search(&q("construct", Some("sac"))).await.unwrap();
        assert!(rows.iter().all(|r| r.kind == "sac") && !rows.is_empty());
        assert!(
            dir.search(&q("construct", Some("hsn")))
                .await
                .unwrap()
                .is_empty()
        );
        assert!(dir.search(&q("\"*", None)).await.unwrap().is_empty());

        // Resync: one changed, one dropped, one new.
        let mut second = first.clone();
        second.retain(|x| x.code != "02029000");
        second[5].description = "Construction services, all kinds".into();
        second.push(e("02023000", "Boneless"));
        let d = dir
            .replace(&second, &src("b"), "2026-11-23T00:00:00Z")
            .await
            .unwrap();
        assert_eq!(
            (d.added, d.changed, d.delisted, d.restored, d.unchanged),
            (1, 1, 1, 0, 6)
        );
        let gone = dir.get("02029000").await.unwrap().unwrap();
        assert!(gone.delisted);
        let rows = dir.search(&q("0202", None)).await.unwrap();
        assert_eq!(rows.last().map(|r| r.code.as_str()), Some("02029000"));
        let s = dir.status().await.unwrap();
        assert_eq!(s.delisted, 1);
        assert_eq!(s.last_diff.unwrap().added, 1);

        // Listed again: restored.
        let d = dir
            .replace(&first, &src("c"), "2026-11-24T00:00:00Z")
            .await
            .unwrap();
        assert_eq!(d.restored, 1);
        assert_eq!(d.delisted, 1); // 02023000 dropped this time
    }
}
