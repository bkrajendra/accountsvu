//! HSN/SAC directory (Phase 2.5.1): a local, searchable copy of the GSTN
//! HSN/SAC master, used as a lookup aid when entering items.
//!
//! The host downloads the master (or the owner picks a copy) and hands the
//! bytes here; this module checks and reads the workbook and rebuilds the
//! directory (`av_persistence::hsn_directory`). Rates aren't in the master,
//! so each hit carries the rule pack's rate for the code when the pack has
//! one. Without a directory, search falls back to the rule pack's headings.

use std::io::Cursor;

use av_domain::{ItemKind, TaxCategory};
use av_gst_engine::RulePack;
use av_persistence::hsn_directory::{
    CodeRef, DirectoryEntry, DirectoryQuery, DirectoryRow, DirectoryStatus, SyncDiff, SyncSource,
};
use av_persistence::{HsnDirectory, Store};
use calamine::{Data, Reader, open_workbook_auto_from_rs};
use serde::{Deserialize, Serialize};

use crate::AppCore;
use crate::context::{Permission, RequestContext};
use crate::error::{AppError, AppResult};
use crate::import::cell_text;
use crate::purchases::sha256_hex;
use crate::setup::{Change, audit};

/// The GSTN master the directory syncs from. The host fetches only this URL.
pub const GSTN_MASTER_URL: &str = "https://tutorial.gst.gov.in/downloads/HSN_SAC.xlsx";

/// Largest master accepted (the 2026 file is about 0.7 MB).
pub const MAX_MASTER_BYTES: usize = 20 * 1024 * 1024;

/// Fewer codes than this means the file isn't the full master.
const MIN_CODES: usize = 1000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryStatusView {
    /// The directory holds codes.
    pub ready: bool,
    #[serde(flatten)]
    pub status: DirectoryStatus,
    pub source_url: &'static str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectorySearchInput {
    pub query: String,
    /// `hsn`, `sac` or absent for both.
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    /// Date for the rule-pack rate, `YYYY-MM-DD`; today when absent.
    #[serde(default)]
    pub date: Option<String>,
}

/// The rule pack's rate for a code (the most specific heading it falls under).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackRate {
    pub rule_code: String,
    pub tax_category: TaxCategory,
    pub rate_bp: u32,
    pub conditional: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryHit {
    pub code: String,
    pub kind: ItemKind,
    pub description: String,
    pub delisted: bool,
    pub chapter: Option<CodeRef>,
    pub heading: Option<CodeRef>,
    pub subheading: Option<CodeRef>,
    pub rate: Option<PackRate>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectorySearchResult {
    pub hits: Vec<DirectoryHit>,
    /// More matched than were returned.
    pub more: bool,
    /// `directory`, or `rule_pack` when the directory hasn't been synced.
    pub source: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectorySyncResult {
    /// The file was the same as the last one; nothing changed.
    pub unchanged: bool,
    pub diff: SyncDiff,
    pub skipped_rows: usize,
    pub status: DirectoryStatusView,
}

/// The master as read from the workbook.
#[derive(Debug)]
pub struct ParsedMaster {
    pub entries: Vec<DirectoryEntry>,
    /// Rows without a valid code (blank, not 2–8 digits, duplicates).
    pub skipped: usize,
}

/// Read the GSTN HSN/SAC master. Each sheet whose first column header (in the
/// first few rows) is `HSN_CD` or `SAC_CD` is read; the next column is the
/// description. Codes are kept as text: numeric HSN cells that lost a leading
/// zero get it back.
pub fn parse_master(bytes: &[u8]) -> AppResult<ParsedMaster> {
    let not_master = || {
        AppError::Invalid(
            "this file isn't the GSTN HSN/SAC master (HSN_SAC.xlsx with HSN_CD and SAC_CD sheets)"
                .into(),
        )
    };
    if !bytes.starts_with(b"PK\x03\x04") {
        return Err(not_master());
    }
    let mut wb = open_workbook_auto_from_rs(Cursor::new(bytes.to_vec()))
        .map_err(|e| AppError::Invalid(format!("the workbook could not be read: {e}")))?;
    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut skipped = 0usize;
    let mut found_hsn = false;
    for name in wb.sheet_names() {
        let Ok(range) = wb.worksheet_range(&name) else {
            continue;
        };
        let mut rows = range.rows();
        let mut kind = None;
        for row in rows.by_ref().take(5) {
            let head = row.first().map(cell_text).unwrap_or_default();
            match head.trim().to_ascii_uppercase().as_str() {
                "HSN_CD" | "HSN CODE" | "HSN" => kind = Some("hsn"),
                "SAC_CD" | "SAC CODE" | "SAC" => kind = Some("sac"),
                _ => continue,
            }
            break;
        }
        let Some(kind) = kind else { continue };
        found_hsn |= kind == "hsn";
        for row in rows {
            let code = row.first().map(code_text).unwrap_or_default();
            let description = row
                .get(1)
                .map(cell_text)
                .unwrap_or_default()
                .replace("_x000D_", " ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if code.is_empty() && description.is_empty() {
                continue;
            }
            let code = if kind == "hsn" && code.len() % 2 == 1 && code.len() < 8 {
                format!("0{code}")
            } else {
                code
            };
            let valid = (2..=8).contains(&code.len())
                && code.bytes().all(|b| b.is_ascii_digit())
                && !description.is_empty();
            if !valid || !seen.insert(code.clone()) {
                skipped += 1;
                continue;
            }
            entries.push(DirectoryEntry {
                code,
                kind,
                description,
            });
        }
    }
    if !found_hsn || entries.len() < MIN_CODES {
        return Err(not_master());
    }
    Ok(ParsedMaster { entries, skipped })
}

fn code_text(c: &Data) -> String {
    cell_text(c)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn item_kind(kind: &str) -> ItemKind {
    if kind == "sac" {
        ItemKind::Service
    } else {
        ItemKind::Goods
    }
}

impl<S: Store> AppCore<S> {
    fn directory(&self) -> AppResult<&HsnDirectory> {
        self.hsn_directory
            .as_ref()
            .ok_or_else(|| AppError::Invalid("the HSN/SAC directory isn't available".into()))
    }

    pub async fn hsn_directory_status(
        &self,
        ctx: &RequestContext,
    ) -> AppResult<DirectoryStatusView> {
        ctx.require(Permission::ViewBusiness)?;
        let status = match &self.hsn_directory {
            Some(d) => d.status().await?,
            None => DirectoryStatus::default(),
        };
        Ok(DirectoryStatusView {
            ready: status.hsn_codes + status.sac_codes > 0,
            status,
            source_url: GSTN_MASTER_URL,
        })
    }

    /// Search the directory by code prefix or words; each hit carries the
    /// rule pack's rate for it. Before the first sync, searches the rule
    /// pack's headings instead.
    pub async fn search_hsn_directory(
        &self,
        ctx: &RequestContext,
        input: &DirectorySearchInput,
    ) -> AppResult<DirectorySearchResult> {
        ctx.require(Permission::ViewBusiness)?;
        let limit = input.limit.unwrap_or(50).clamp(1, 200);
        let date = input
            .date
            .as_deref()
            .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .unwrap_or_else(|| self.today());
        let kind = input
            .kind
            .clone()
            .filter(|k| matches!(k.as_str(), "hsn" | "sac"));
        let pack = RulePack::bundled();
        let rate_for = |code: &str| {
            pack.hsn_rate(code, date).map(|(rule, r)| PackRate {
                rule_code: rule.code.clone(),
                tax_category: r.category,
                rate_bp: r.bp,
                conditional: rule.conditional,
            })
        };

        let ready = match &self.hsn_directory {
            Some(d) => {
                let s = d.status().await?;
                s.hsn_codes + s.sac_codes > 0
            }
            None => false,
        };
        if !ready {
            let want = kind.as_deref().map(item_kind);
            let mut hits: Vec<DirectoryHit> = pack
                .search_hsn(&input.query, limit + 1)
                .into_iter()
                .filter(|h| want.is_none_or(|k| h.kind == k))
                .map(|h| DirectoryHit {
                    code: h.code.clone(),
                    kind: h.kind,
                    description: h.description.clone(),
                    delisted: false,
                    chapter: None,
                    heading: None,
                    subheading: None,
                    rate: rate_for(&h.code),
                })
                .collect();
            let more = hits.len() > limit;
            hits.truncate(limit);
            return Ok(DirectorySearchResult {
                hits,
                more,
                source: "rule_pack",
            });
        }

        let rows = self
            .directory()?
            .search(&DirectoryQuery {
                text: input.query.clone(),
                kind,
                limit,
            })
            .await?;
        let more = rows.len() > limit;
        let hits = rows
            .into_iter()
            .take(limit)
            .map(|r: DirectoryRow| DirectoryHit {
                rate: rate_for(&r.code),
                kind: item_kind(&r.kind),
                code: r.code,
                description: r.description,
                delisted: r.delisted,
                chapter: r.chapter,
                heading: r.heading,
                subheading: r.subheading,
            })
            .collect();
        Ok(DirectorySearchResult {
            hits,
            more,
            source: "directory",
        })
    }

    /// Rebuild the directory from a copy of the master. `channel` is `gstn`
    /// (downloaded by the host) or `file` (picked by the owner); `origin` is
    /// the URL or file name. A file identical to the last one changes nothing.
    pub async fn import_hsn_master(
        &self,
        ctx: &RequestContext,
        bytes: &[u8],
        channel: &str,
        origin: &str,
    ) -> AppResult<DirectorySyncResult> {
        ctx.require(Permission::ManageBusiness)?;
        let dir = self.directory()?;
        if bytes.len() > MAX_MASTER_BYTES {
            return Err(AppError::Invalid(
                "the file is larger than 20 MB; it isn't the HSN/SAC master".into(),
            ));
        }
        let sha256 = sha256_hex(bytes);
        let now = self.now();
        if dir.last_sha256().await?.as_deref() == Some(sha256.as_str()) {
            dir.touch(&now).await?;
            return Ok(DirectorySyncResult {
                unchanged: true,
                diff: SyncDiff::default(),
                skipped_rows: 0,
                status: self.hsn_directory_status(ctx).await?,
            });
        }
        let parsed = parse_master(bytes)?;
        let diff = dir
            .replace(
                &parsed.entries,
                &SyncSource {
                    channel: channel.to_owned(),
                    origin: origin.to_owned(),
                    sha256: sha256.clone(),
                    bytes: bytes.len(),
                },
                &now,
            )
            .await?;

        // Audit in the organisation's log when there is one.
        let mut uow = self.store.begin().await?;
        if let Some(org) = self.find_org(ctx, &mut uow).await? {
            audit(
                &mut uow,
                ctx,
                &org.id,
                Change {
                    action: "hsn_directory.synced",
                    entity_id: None,
                    before: None,
                    after: Some(
                        serde_json::json!({
                            "channel": channel,
                            "origin": origin,
                            "sha256": sha256,
                            "codes": parsed.entries.len(),
                            "diff": diff,
                        })
                        .to_string(),
                    ),
                },
                &now,
            )
            .await?;
            av_persistence::UnitOfWork::commit(uow).await?;
        }
        Ok(DirectorySyncResult {
            unchanged: false,
            diff,
            skipped_rows: parsed.skipped,
            status: self.hsn_directory_status(ctx).await?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_xlsxwriter::Workbook;

    fn master(extra: usize) -> Vec<u8> {
        let mut wb = Workbook::new();
        let ws = wb.add_worksheet();
        ws.set_name("HSN_MSTR").unwrap();
        ws.write_string(0, 0, "HSN_CD").unwrap();
        ws.write_string(0, 1, "HSN_Description").unwrap();
        let mut rows: Vec<(String, String)> = vec![
            ("02".into(), "Meat and edible meat offal".into()),
            ("0202".into(), "Meat of bovine animals, frozen".into()),
            ("020210".into(), "Carcases and half-carcases".into()),
            ("02021000".into(), "CARCASES  AND HALF-CARCASES".into()),
            ("02029000".into(), "Other".into()),
            ("84".into(), "Machinery".into()),
            ("8471".into(), "Automatic data processing machines".into()),
        ];
        for i in 0..1000 + extra {
            rows.push((format!("8471{i:04}"), format!("Computer part {i}")));
        }
        rows.push(("".into(), "".into()));
        rows.push(("0202".into(), "duplicate".into()));
        for (r, (c, d)) in rows.iter().enumerate() {
            let r = u32::try_from(r + 1).unwrap();
            ws.write_string(r, 0, c).unwrap();
            ws.write_string(r, 1, d).unwrap();
        }
        // A numeric HSN that lost its leading zero.
        let last = u32::try_from(rows.len() + 1).unwrap();
        ws.write_number(last, 0, 1_011_010).unwrap();
        ws.write_string(last, 1, "Pure-bred horses").unwrap();

        let sac = wb.add_worksheet();
        sac.set_name("SAC_MSTR").unwrap();
        sac.write_string(0, 0, "SAC_CD").unwrap();
        sac.write_string(0, 1, "SAC_Description").unwrap();
        for (r, (c, d)) in [
            (99, "All Services"),
            (9954, "Construction services"),
            (995_411, "Construction of residential buildings"),
        ]
        .iter()
        .enumerate()
        {
            let r = u32::try_from(r + 1).unwrap();
            sac.write_number(r, 0, *c).unwrap();
            sac.write_string(r, 1, *d).unwrap();
        }
        wb.save_to_buffer().unwrap()
    }

    fn q(text: &str, kind: Option<&str>) -> DirectorySearchInput {
        DirectorySearchInput {
            query: text.into(),
            kind: kind.map(Into::into),
            limit: Some(20),
            date: None,
        }
    }

    #[test]
    fn reads_the_gstn_master() {
        let p = parse_master(&master(0)).unwrap();
        assert_eq!(p.skipped, 1); // the duplicate; blank rows are passed over
        let get = |c: &str| p.entries.iter().find(|e| e.code == c);
        assert_eq!(
            get("02021000").unwrap().description,
            "CARCASES AND HALF-CARCASES"
        );
        assert_eq!(get("01011010").unwrap().kind, "hsn");
        assert_eq!(get("995411").unwrap().kind, "sac");
        assert!(matches!(
            parse_master(b"not a workbook"),
            Err(AppError::Invalid(_))
        ));
    }

    /// Reads a real copy of the master when `HSN_SAC_XLSX` points at one.
    #[test]
    fn reads_a_real_master_when_given() {
        let Ok(path) = std::env::var("HSN_SAC_XLSX") else {
            return;
        };
        let p = parse_master(&std::fs::read(path).unwrap()).unwrap();
        let hsn = p.entries.iter().filter(|e| e.kind == "hsn").count();
        let sac = p.entries.len() - hsn;
        println!("hsn {hsn} sac {sac} skipped {}", p.skipped);
        assert!(hsn > 10_000 && sac > 500);
    }

    #[tokio::test]
    async fn sync_then_search_with_pack_rates() {
        let day = chrono::NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
        let core = AppCore::in_memory_on(day)
            .await
            .unwrap()
            .with_hsn_directory(HsnDirectory::open_in_memory().await.unwrap());
        let ctx = RequestContext::local_owner();

        assert!(!core.hsn_directory_status(&ctx).await.unwrap().ready);
        let before = core
            .search_hsn_directory(&ctx, &q("8471", None))
            .await
            .unwrap();
        assert_eq!(before.source, "rule_pack");

        let bytes = master(0);
        let r = core
            .import_hsn_master(&ctx, &bytes, "file", "HSN_SAC.xlsx")
            .await
            .unwrap();
        assert!(!r.unchanged);
        assert_eq!(r.diff.added, r.status.status.hsn_codes as usize + 3);
        assert!(r.status.ready);
        let again = core
            .import_hsn_master(&ctx, &bytes, "file", "HSN_SAC.xlsx")
            .await
            .unwrap();
        assert!(again.unchanged);

        let found = core
            .search_hsn_directory(&ctx, &q("carcases", None))
            .await
            .unwrap();
        assert_eq!(found.source, "directory");
        let hit = found.hits.iter().find(|h| h.code == "02021000").unwrap();
        assert_eq!(
            hit.chapter.as_ref().unwrap().description,
            "Meat and edible meat offal"
        );
        assert_eq!(hit.heading.as_ref().unwrap().code, "0202");

        let parts = core
            .search_hsn_directory(&ctx, &q("8471", Some("hsn")))
            .await
            .unwrap();
        assert!(parts.more);
        assert_eq!(parts.hits.len(), 20);
        let services = core
            .search_hsn_directory(&ctx, &q("construction", Some("sac")))
            .await
            .unwrap();
        assert!(services.hits.iter().all(|h| h.kind == ItemKind::Service));

        // A bigger file: the new codes are added, nothing else changes.
        let r = core
            .import_hsn_master(&ctx, &master(5), "gstn", GSTN_MASTER_URL)
            .await
            .unwrap();
        assert_eq!((r.diff.added, r.diff.changed, r.diff.delisted), (5, 0, 0));
        assert!(matches!(
            core.import_hsn_master(&ctx, b"PK\x03\x04junk", "file", "x.xlsx")
                .await,
            Err(AppError::Invalid(_))
        ));
    }
}
