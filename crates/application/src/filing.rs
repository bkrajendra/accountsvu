//! Phase 2.5: validation, offline filing and the GST period lock.
//!
//! A return is validated against the bundled return pack (errors block,
//! warnings don't), exported as a filing pack for the offline tool or the
//! portal, and recorded as filed only with its ARN. Recording freezes the
//! portal file and workpaper (stored by SHA-256 with the acknowledgement),
//! and locks the month: documents of the GSTIN dated in it can't be issued,
//! recorded or cancelled (the core refuses, and so does the database). ITC
//! reviewed afterwards on a line still pending moves to the first month whose
//! GSTR-3B isn't filed. The compliance pack zips what was frozen, read-only.

use std::io::Write;

use av_domain::new_id;
use av_gst_engine::RulePack;
use av_gst_engine::returns::{ReturnPack, Severity};
use av_gst_formats::gstr1 as fmt;
use av_persistence::records::{
    GstFilingFileRecord, GstFilingRecord, OrganisationRecord, RegistrationRecord,
};
use av_persistence::{AttachmentRepo, AuditRepo, GstFilingRepo, Store, UnitOfWork};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::AppCore;
use crate::context::{Permission, RequestContext};
use crate::error::{AppError, AppResult, Errors};
use crate::gstr3b::{WorkpaperFile, build_3b_series};
use crate::purchases::sha256_hex;
use crate::returns::{Period, ReturnException, build, iso};
use crate::sales::ymd;
use crate::setup::{Change, audit};

/// Acknowledgements are capped like attachments.
const MAX_ACK_BYTES: usize = 5 * 1024 * 1024;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReturnValidation {
    pub pack_version: String,
    /// Block the filing pack and recording the return as filed.
    pub errors: Vec<ReturnException>,
    pub warnings: Vec<ReturnException>,
    pub can_file: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilingFileView {
    pub id: String,
    pub kind: String,
    pub file_name: String,
    pub size_bytes: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilingRecordView {
    pub id: String,
    pub channel: String,
    pub arn: String,
    pub filed_on: String,
    pub note: Option<String>,
    pub return_pack_version: String,
    pub rule_pack_version: String,
    /// Headline figures as filed.
    pub summary: serde_json::Value,
    pub warning_count: i64,
    pub recorded_at: String,
    pub recorded_by: String,
    pub files: Vec<FilingFileView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedFilingView {
    pub arn: String,
    pub filed_on: String,
    pub removed_at: Option<String>,
    pub removed_by: Option<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilingView {
    pub registration_id: String,
    pub gstin: String,
    /// `gstr1` or `gstr3b`.
    pub return_type: String,
    pub label: String,
    pub period: String,
    pub period_label: String,
    pub due_date: String,
    pub filing: Option<FilingRecordView>,
    pub removed: Vec<RemovedFilingView>,
    /// Checks on the books as they are now.
    pub validation: ReturnValidation,
    /// Headline figures now.
    pub summary: serde_json::Value,
    /// Filed, and the books' headline figures no longer match what was filed.
    pub changed_since_filing: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcknowledgementInput {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilingInput {
    pub arn: String,
    /// `YYYY-MM-DD`.
    pub filed_on: String,
    pub note: Option<String>,
    pub acknowledgement: Option<AcknowledgementInput>,
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReturnKind {
    Gstr1,
    Gstr3b,
}

impl ReturnKind {
    pub(crate) fn parse(s: &str) -> AppResult<ReturnKind> {
        match s {
            "gstr1" => Ok(ReturnKind::Gstr1),
            "gstr3b" => Ok(ReturnKind::Gstr3b),
            _ => Err(AppError::Invalid("choose GSTR-1 or GSTR-3B".into())),
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            ReturnKind::Gstr1 => "gstr1",
            ReturnKind::Gstr3b => "gstr3b",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            ReturnKind::Gstr1 => "GSTR-1",
            ReturnKind::Gstr3b => "GSTR-3B",
        }
    }
}

/// ARN as the portal prints it: 15 letters and digits. Spaces are dropped and
/// letters upper-cased.
pub(crate) fn normalise_arn(s: &str) -> Option<String> {
    let a: String = s
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_uppercase();
    (a.len() == 15 && a.chars().all(|c| c.is_ascii_alphanumeric())).then_some(a)
}

fn kind_label(rt: &str) -> &'static str {
    if rt == "gstr1" { "GSTR-1" } else { "GSTR-3B" }
}

/// The filed return that locks `family` documents of a GSTIN dated `date`,
/// as "GSTR-1 for October 2026".
pub(crate) fn lock_label(filings: &[GstFilingRecord], date: &str, family: &str) -> Option<String> {
    let month = date.get(..7)?;
    let pack = ReturnPack::bundled();
    filings
        .iter()
        .filter(|f| f.status == "filed" && f.period == month && pack.locks(&f.return_type, family))
        .min_by_key(|f| f.return_type.clone())
        .map(|f| {
            format!(
                "{} for {}",
                kind_label(&f.return_type),
                Period::parse(&f.period).map_or(f.period.clone(), Period::label)
            )
        })
}

pub(crate) fn is_filed(filings: &[GstFilingRecord], rt: &str, period: &str) -> bool {
    filings
        .iter()
        .any(|f| f.status == "filed" && f.return_type == rt && f.period == period)
}

/// The first month from `from` on whose GSTR-3B isn't filed.
pub(crate) fn first_open_3b(filings: &[GstFilingRecord], from: Period) -> Period {
    let mut p = from;
    while is_filed(filings, "gstr3b", &p.code()) {
        p = p.next();
    }
    p
}

fn zip_files(files: &[(String, Vec<u8>)]) -> AppResult<Vec<u8>> {
    use zip::write::SimpleFileOptions;
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let fail =
        |e: &dyn std::fmt::Display| AppError::Invalid(format!("couldn't build the zip: {e}"));
    for (name, bytes) in files {
        w.start_file(name.as_str(), opts).map_err(|e| fail(&e))?;
        w.write_all(bytes).map_err(|e| fail(&e))?;
    }
    Ok(w.finish().map_err(|e| fail(&e))?.into_inner())
}

/// A return's figures, files and checks as the books are now.
struct Prepared {
    rule_pack_version: String,
    summary: serde_json::Value,
    /// (kind, file name, content type, bytes) frozen with a filing.
    files: Vec<(&'static str, String, &'static str, Vec<u8>)>,
    validation: ReturnValidation,
}

fn split_checks(kind: ReturnKind, exceptions: Vec<ReturnException>) -> ReturnValidation {
    let pack = ReturnPack::bundled();
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    for mut e in exceptions {
        let sev = pack.severity(kind.id(), e.code);
        e.severity = sev.as_str();
        if sev == Severity::Error {
            errors.push(e);
        } else {
            warnings.push(e);
        }
    }
    ReturnValidation {
        pack_version: pack.version.clone(),
        can_file: errors.is_empty(),
        errors,
        warnings,
    }
}

fn check(code: &'static str, message: String) -> ReturnException {
    ReturnException {
        severity: "warning",
        code,
        message,
        document_id: None,
        number: None,
    }
}

fn record_view(f: &GstFilingRecord, files: &[GstFilingFileRecord]) -> FilingRecordView {
    FilingRecordView {
        id: f.id.clone(),
        channel: f.channel.clone(),
        arn: f.arn.clone(),
        filed_on: f.filed_on.clone(),
        note: f.note.clone(),
        return_pack_version: f.return_pack_version.clone(),
        rule_pack_version: f.rule_pack_version.clone(),
        summary: serde_json::from_str(&f.summary_json).unwrap_or(serde_json::Value::Null),
        warning_count: f.warning_count,
        recorded_at: f.recorded_at.clone(),
        recorded_by: f.recorded_by.clone(),
        files: files
            .iter()
            .map(|x| FilingFileView {
                id: x.id.clone(),
                kind: x.kind.clone(),
                file_name: x.file_name.clone(),
                size_bytes: x.size_bytes,
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Use cases
// ---------------------------------------------------------------------------

impl<S: Store> AppCore<S> {
    /// Filed records of a GSTIN (status `filed` only).
    pub(crate) async fn filed_returns(
        &self,
        uow: &mut S::Uow,
        org_id: &str,
        registration_id: Option<&str>,
    ) -> AppResult<Vec<GstFilingRecord>> {
        let Some(reg) = registration_id else {
            return Ok(Vec::new());
        };
        Ok(uow
            .list_gst_filings(org_id, reg)
            .await?
            .into_iter()
            .filter(|f| f.status == "filed")
            .collect())
    }

    /// The filed return locking `family` documents of a GSTIN on `date`.
    pub(crate) async fn gst_lock(
        &self,
        uow: &mut S::Uow,
        org_id: &str,
        registration_id: Option<&str>,
        date: &str,
        family: &str,
    ) -> AppResult<Option<String>> {
        let filings = self.filed_returns(uow, org_id, registration_id).await?;
        Ok(lock_label(&filings, date, family))
    }

    async fn filing_context(
        &self,
        ctx: &RequestContext,
        uow: &mut S::Uow,
        registration_id: &str,
    ) -> AppResult<(OrganisationRecord, RegistrationRecord)> {
        let org = self.require_org(ctx, uow).await?;
        let (_, reg) = self
            .return_registration(uow, &org.id, Some(registration_id))
            .await?;
        let reg = reg.ok_or(AppError::NotFound("GST registration"))?;
        Ok((org, reg))
    }

    async fn prepare_return(
        &self,
        uow: &mut S::Uow,
        org: &OrganisationRecord,
        reg: &RegistrationRecord,
        period: Period,
        kind: ReturnKind,
    ) -> AppResult<Prepared> {
        let today = self.today();
        let filings = self.filed_returns(uow, &org.id, Some(&reg.id)).await?;
        let stamp = format!("{:02}{}", period.month, period.year);
        let mut exceptions;
        let rule_pack_version;
        let summary;
        let mut files = Vec::new();
        match kind {
            ReturnKind::Gstr1 => {
                let docs = self.load_return_docs(uow, &org.id).await?;
                let built = build(reg, &docs, period, today);
                exceptions = built.view.exceptions.clone();
                rule_pack_version = built.view.rule_pack_version.clone();
                summary = serde_json::json!({
                    "documents": built.view.documents.len(),
                    "totals": built.view.totals,
                });
                files.push((
                    "portal_json",
                    format!("GSTR1_{}_{stamp}.json", reg.gstin),
                    "application/json",
                    fmt::to_json(&built.portal).into_bytes(),
                ));
                files.push((
                    "workpaper_json",
                    format!("GSTR1_workpaper_{}_{stamp}.json", reg.gstin),
                    "application/json",
                    serde_json::to_vec_pretty(&built.view).unwrap_or_default(),
                ));
            }
            ReturnKind::Gstr3b => {
                let sales = self.load_return_docs(uow, &org.id).await?;
                let purchases = self.load_return_purchases(uow, &org.id).await?;
                let first = Self::chain_start(&org.books_start_date, period.first());
                let first = if first.first() > period.first() {
                    period
                } else {
                    first
                };
                let v = build_3b_series(reg, &sales, &purchases, first, period, today)
                    .into_iter()
                    .last()
                    .ok_or(AppError::NotFound("return period"))?;
                exceptions = v.exceptions.clone();
                rule_pack_version = RulePack::bundled().version.clone();
                summary = serde_json::json!({
                    "liability": v.liability,
                    "reverseCharge": v.reverse_charge,
                    "itcNet": v.itc_net,
                    "cashPayable": v.offset.cash_total,
                    "carryForward": v.offset.carry_forward,
                });
                let xlsx = crate::workpaper_xlsx::gstr3b_workbook(&v)
                    .map_err(|e| AppError::Invalid(format!("couldn't build the workbook: {e}")))?;
                files.push((
                    "workpaper_xlsx",
                    format!("GSTR3B_workpaper_{}_{stamp}.xlsx", reg.gstin),
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                    xlsx,
                ));
                files.push((
                    "workpaper_json",
                    format!("GSTR3B_workpaper_{}_{stamp}.json", reg.gstin),
                    "application/json",
                    serde_json::to_vec_pretty(&v).unwrap_or_default(),
                ));
                if !is_filed(&filings, "gstr1", &period.code()) {
                    exceptions.push(check(
                        "gstr1_unfiled",
                        format!(
                            "GSTR-1 for {} isn't recorded as filed. The portal expects it before GSTR-3B.",
                            period.label()
                        ),
                    ));
                }
                let recon = self.load_recon_data(uow, &org.id, reg).await?;
                if recon
                    .statements
                    .iter()
                    .any(|(s, _)| s.period == period.code())
                {
                    let r = crate::gstr2b::reconcile_period(&recon, period);
                    let n = r.rows.iter().filter(|r| r.needs_review).count();
                    if n > 0 {
                        exceptions.push(check(
                            "gstr2b_review",
                            format!(
                                "{n} GSTR-2B {} still to review.",
                                if n == 1 { "row is" } else { "rows are" }
                            ),
                        ));
                    }
                } else {
                    exceptions.push(check(
                        "gstr2b_missing",
                        format!(
                            "GSTR-2B for {} isn't imported, so ITC isn't checked against it.",
                            period.label()
                        ),
                    ));
                }
            }
        }
        if today <= period.last() {
            exceptions.push(check(
                "period_open",
                format!(
                    "{} isn't over yet; it can be filed from {}.",
                    period.label(),
                    period.next().first().format("%-d %b %Y")
                ),
            ));
        }
        let prev = period.previous();
        let books_start = Period::of(ymd(&org.books_start_date));
        if prev.first() >= books_start.first() && !is_filed(&filings, kind.id(), &prev.code()) {
            exceptions.push(check(
                "previous_unfiled",
                format!(
                    "{} for {} isn't recorded as filed. Returns are filed in order.",
                    kind.label(),
                    prev.label()
                ),
            ));
        }
        Ok(Prepared {
            rule_pack_version,
            summary,
            files,
            validation: split_checks(kind, exceptions),
        })
    }

    async fn filing_view(
        &self,
        uow: &mut S::Uow,
        org: &OrganisationRecord,
        reg: &RegistrationRecord,
        period: Period,
        kind: ReturnKind,
    ) -> AppResult<FilingView> {
        let prepared = self.prepare_return(uow, org, reg, period, kind).await?;
        let all = uow.list_gst_filings(&org.id, &reg.id).await?;
        let mine: Vec<&GstFilingRecord> = all
            .iter()
            .filter(|f| f.return_type == kind.id() && f.period == period.code())
            .collect();
        let filed = mine.iter().find(|f| f.status == "filed");
        let filing = match filed {
            Some(f) => {
                let files = uow.gst_filing_files(&f.id).await?;
                Some(record_view(f, &files))
            }
            None => None,
        };
        let changed = filed.is_some_and(|f| {
            serde_json::from_str::<serde_json::Value>(&f.summary_json)
                .ok()
                .as_ref()
                != Some(&prepared.summary)
        });
        Ok(FilingView {
            registration_id: reg.id.clone(),
            gstin: reg.gstin.clone(),
            return_type: kind.id().into(),
            label: kind.label().into(),
            period: period.code(),
            period_label: period.label(),
            due_date: iso(period.due(kind.id())),
            filing,
            removed: mine
                .iter()
                .filter(|f| f.status == "removed")
                .map(|f| RemovedFilingView {
                    arn: f.arn.clone(),
                    filed_on: f.filed_on.clone(),
                    removed_at: f.removed_at.clone(),
                    removed_by: f.removed_by.clone(),
                    reason: f.removed_reason.clone(),
                })
                .collect(),
            validation: prepared.validation,
            summary: prepared.summary,
            changed_since_filing: changed,
        })
    }

    /// Filing status and checks of a return (`gstr1` or `gstr3b`) for a month.
    pub async fn return_filing(
        &self,
        ctx: &RequestContext,
        registration_id: &str,
        period: &str,
        return_type: &str,
    ) -> AppResult<FilingView> {
        ctx.require(Permission::ViewBusiness)?;
        let period = Period::parse(period)?;
        let kind = ReturnKind::parse(return_type)?;
        let mut uow = self.store.begin().await?;
        let (org, reg) = self.filing_context(ctx, &mut uow, registration_id).await?;
        self.filing_view(&mut uow, &org, &reg, period, kind).await
    }

    /// The filing pack (zip) to file from: the portal file or workpaper, the
    /// checks and a manifest. Refused while errors remain. Audit-logged.
    pub async fn filing_pack(
        &self,
        ctx: &RequestContext,
        registration_id: &str,
        period: &str,
        return_type: &str,
    ) -> AppResult<WorkpaperFile> {
        ctx.require(Permission::ViewBusiness)?;
        let period = Period::parse(period)?;
        let kind = ReturnKind::parse(return_type)?;
        let mut uow = self.store.begin().await?;
        let (org, reg) = self.filing_context(ctx, &mut uow, registration_id).await?;
        let filings = self.filed_returns(&mut uow, &org.id, Some(&reg.id)).await?;
        if is_filed(&filings, kind.id(), &period.code()) {
            return Err(AppError::Conflict(format!(
                "{} for {} is recorded as filed. Save its compliance pack instead.",
                kind.label(),
                period.label()
            )));
        }
        let p = self
            .prepare_return(&mut uow, &org, &reg, period, kind)
            .await?;
        if !p.validation.can_file {
            return Err(AppError::Invalid(format!(
                "fix the {} {} before exporting the filing pack",
                p.validation.errors.len(),
                if p.validation.errors.len() == 1 {
                    "error"
                } else {
                    "errors"
                }
            )));
        }
        let mut files: Vec<(String, Vec<u8>)> = p
            .files
            .iter()
            .map(|(_, name, _, bytes)| (name.clone(), bytes.clone()))
            .collect();
        let checks: Vec<String> = p
            .validation
            .warnings
            .iter()
            .map(|w| match &w.number {
                Some(n) => format!("WARNING {} [{n}]: {}", w.code, w.message),
                None => format!("WARNING {}: {}", w.code, w.message),
            })
            .collect();
        files.push((
            "checks.txt".into(),
            if checks.is_empty() {
                "No warnings.\n".into()
            } else {
                format!("{}\n", checks.join("\n"))
            }
            .into_bytes(),
        ));
        let manifest = serde_json::json!({
            "app": "AccountsVU",
            "pack": "filing",
            "return": kind.label(),
            "gstin": reg.gstin,
            "period": period.code(),
            "preparedOn": iso(self.today()),
            "returnPack": p.validation.pack_version,
            "rulePack": p.rule_pack_version,
            "summary": p.summary,
            "files": files.iter().map(|(n, b)| serde_json::json!({"name": n, "sha256": sha256_hex(b), "bytes": b.len()})).collect::<Vec<_>>(),
        });
        files.push((
            "manifest.json".into(),
            serde_json::to_vec_pretty(&manifest).unwrap_or_default(),
        ));
        let bytes = zip_files(&files)?;
        let now = self.now();
        audit(
            &mut uow,
            ctx,
            &org.id,
            Change {
                action: "gst_filing.pack_exported",
                entity_id: Some(&reg.id),
                before: None,
                after: Some(
                    serde_json::json!({
                        "return": kind.id(),
                        "period": period.code(),
                        "sha256": sha256_hex(&bytes),
                        "warnings": p.validation.warnings.len(),
                    })
                    .to_string(),
                ),
            },
            &now,
        )
        .await?;
        uow.commit().await?;
        Ok(WorkpaperFile {
            file_name: format!(
                "{}_filing_pack_{}_{:02}{}.zip",
                kind.label().replace('-', ""),
                reg.gstin,
                period.month,
                period.year
            ),
            bytes,
        })
    }

    /// Records a return as filed with its ARN, freezing the portal file and
    /// workpaper and locking the month.
    pub async fn record_filing(
        &self,
        ctx: &RequestContext,
        registration_id: &str,
        period: &str,
        return_type: &str,
        input: FilingInput,
    ) -> AppResult<FilingView> {
        ctx.require(Permission::PostDocuments)?;
        let period = Period::parse(period)?;
        let kind = ReturnKind::parse(return_type)?;
        let mut uow = self.store.begin().await?;
        let (org, reg) = self.filing_context(ctx, &mut uow, registration_id).await?;
        let today = self.today();

        let mut e = Errors::new("");
        let arn = normalise_arn(&input.arn);
        if arn.is_none() {
            e.add(
                "arn",
                "Enter the 15-character ARN from the acknowledgement.",
            );
        }
        let filed_on = NaiveDate::parse_from_str(input.filed_on.trim(), "%Y-%m-%d").ok();
        match filed_on {
            None => e.add("filedOn", "Enter the filing date."),
            Some(d) if d > today => e.add("filedOn", "The filing date can't be in the future."),
            Some(d) if d <= period.last() => e.add(
                "filedOn",
                format!(
                    "A return for {} can only be filed after the month ends.",
                    period.label()
                ),
            ),
            Some(_) => {}
        }
        let note = input
            .note
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        if note.as_ref().is_some_and(|n| n.chars().count() > 500) {
            e.add("note", "Keep the note under 500 characters.");
        }
        let ack = match &input.acknowledgement {
            None => None,
            Some(a) => {
                let ct = crate::purchases::content_type_for(&a.file_name)
                    .filter(|ct| matches!(*ct, "application/pdf" | "image/png" | "image/jpeg"));
                if ct.is_none() {
                    e.add(
                        "acknowledgement",
                        "Attach the acknowledgement as a PDF, PNG or JPG.",
                    );
                }
                if a.bytes.len() > MAX_ACK_BYTES {
                    e.add(
                        "acknowledgement",
                        "The acknowledgement must be 5 MB or smaller.",
                    );
                }
                if a.bytes.is_empty() {
                    e.add("acknowledgement", "That file is empty.");
                }
                ct.map(|ct| (a.file_name.trim().to_owned(), ct, a.bytes.clone()))
            }
        };
        e.finish()?;
        let (arn, filed_on) = (arn.expect("validated"), filed_on.expect("validated"));

        let all = uow.list_gst_filings(&org.id, &reg.id).await?;
        if is_filed(&all, kind.id(), &period.code()) {
            return Err(AppError::Conflict(format!(
                "{} for {} is already recorded as filed.",
                kind.label(),
                period.label()
            )));
        }
        if all.iter().any(|f| f.status == "filed" && f.arn == arn) {
            return Err(AppError::Conflict(format!(
                "ARN {arn} is already recorded on another return."
            )));
        }
        let p = self
            .prepare_return(&mut uow, &org, &reg, period, kind)
            .await?;
        if !p.validation.can_file {
            return Err(AppError::Invalid(format!(
                "fix the {} {} first, so the books match what was filed",
                p.validation.errors.len(),
                if p.validation.errors.len() == 1 {
                    "error"
                } else {
                    "errors"
                }
            )));
        }

        let now = self.now();
        let id = new_id().to_string();
        let workpaper_sha = p
            .files
            .iter()
            .find(|f| f.0 == "workpaper_json")
            .map(|f| sha256_hex(&f.3))
            .unwrap_or_default();
        let record = GstFilingRecord {
            id: id.clone(),
            organisation_id: org.id.clone(),
            gst_registration_id: reg.id.clone(),
            return_type: kind.id().into(),
            period: period.code(),
            status: "filed".into(),
            channel: "offline".into(),
            arn: arn.clone(),
            filed_on: iso(filed_on),
            note,
            return_pack_version: p.validation.pack_version.clone(),
            rule_pack_version: p.rule_pack_version.clone(),
            summary_json: p.summary.to_string(),
            workpaper_sha256: workpaper_sha,
            warning_count: i64::try_from(p.validation.warnings.len()).unwrap_or(i64::MAX),
            recorded_at: now.clone(),
            recorded_by: ctx.actor.user_id.clone(),
            removed_reason: None,
            removed_at: None,
            removed_by: None,
        };
        uow.insert_gst_filing(&record).await?;
        let mut frozen: Vec<(&str, String, &str, Vec<u8>)> = p.files;
        if let Some((name, ct, bytes)) = ack {
            frozen.push(("acknowledgement", name, ct, bytes));
        }
        for (kind_, name, ct, bytes) in &frozen {
            let sha = sha256_hex(bytes);
            uow.put_blob(&sha, bytes).await?;
            uow.insert_gst_filing_file(&GstFilingFileRecord {
                id: new_id().to_string(),
                filing_id: id.clone(),
                kind: (*kind_).into(),
                file_name: name.clone(),
                content_type: (*ct).into(),
                size_bytes: i64::try_from(bytes.len()).unwrap_or(i64::MAX),
                sha256: sha,
            })
            .await?;
        }
        audit(
            &mut uow,
            ctx,
            &org.id,
            Change {
                action: "gst_filing.recorded",
                entity_id: Some(&id),
                before: None,
                after: Some(
                    serde_json::json!({
                        "return": kind.id(),
                        "gstin": reg.gstin,
                        "period": period.code(),
                        "arn": arn,
                        "filedOn": iso(filed_on),
                        "returnPack": record.return_pack_version,
                        "warnings": record.warning_count,
                    })
                    .to_string(),
                ),
            },
            &now,
        )
        .await?;
        let view = self.filing_view(&mut uow, &org, &reg, period, kind).await?;
        uow.commit().await?;
        Ok(view)
    }

    /// Removes a filing record (owner, with a reason), unlocking the month.
    /// The record stays, marked removed, for the audit trail.
    pub async fn remove_filing(
        &self,
        ctx: &RequestContext,
        registration_id: &str,
        period: &str,
        return_type: &str,
        reason: &str,
    ) -> AppResult<FilingView> {
        ctx.require(Permission::ManageBusiness)?;
        let period = Period::parse(period)?;
        let kind = ReturnKind::parse(return_type)?;
        let reason = reason.trim();
        let mut e = Errors::new("");
        if reason.is_empty() {
            e.add("reason", "Say why the filing record is being removed.");
        } else if reason.chars().count() > 300 {
            e.add("reason", "Keep the reason under 300 characters.");
        }
        e.finish()?;
        let mut uow = self.store.begin().await?;
        let (org, reg) = self.filing_context(ctx, &mut uow, registration_id).await?;
        let filings = self.filed_returns(&mut uow, &org.id, Some(&reg.id)).await?;
        let f = filings
            .iter()
            .find(|f| f.return_type == kind.id() && f.period == period.code())
            .ok_or(AppError::NotFound("filing record"))?;
        let now = self.now();
        if !uow
            .remove_gst_filing(&org.id, &f.id, reason, &now, &ctx.actor.user_id)
            .await?
        {
            return Err(AppError::stale());
        }
        audit(
            &mut uow,
            ctx,
            &org.id,
            Change {
                action: "gst_filing.removed",
                entity_id: Some(&f.id),
                before: Some(
                    serde_json::json!({"return": kind.id(), "period": period.code(), "arn": f.arn})
                        .to_string(),
                ),
                after: Some(serde_json::json!({ "reason": reason }).to_string()),
            },
            &now,
        )
        .await?;
        let view = self.filing_view(&mut uow, &org, &reg, period, kind).await?;
        uow.commit().await?;
        Ok(view)
    }

    /// The compliance pack (zip) of a filed return: the files frozen at
    /// filing, the acknowledgement, the filing record and its audit trail.
    pub async fn compliance_pack(
        &self,
        ctx: &RequestContext,
        registration_id: &str,
        period: &str,
        return_type: &str,
    ) -> AppResult<WorkpaperFile> {
        ctx.require(Permission::ViewBusiness)?;
        let period = Period::parse(period)?;
        let kind = ReturnKind::parse(return_type)?;
        let mut uow = self.store.begin().await?;
        let (org, reg) = self.filing_context(ctx, &mut uow, registration_id).await?;
        let filings = self.filed_returns(&mut uow, &org.id, Some(&reg.id)).await?;
        let f = filings
            .iter()
            .find(|f| f.return_type == kind.id() && f.period == period.code())
            .ok_or_else(|| {
                AppError::Conflict(format!(
                    "{} for {} isn't recorded as filed yet.",
                    kind.label(),
                    period.label()
                ))
            })?;
        let stored = uow.gst_filing_files(&f.id).await?;
        let mut files = Vec::new();
        for x in &stored {
            let bytes = uow
                .blob(&x.sha256)
                .await?
                .ok_or(AppError::NotFound("filed file"))?;
            let folder = if x.kind == "acknowledgement" {
                "acknowledgement"
            } else {
                "filed"
            };
            files.push((format!("{folder}/{}", x.file_name), bytes));
        }
        let trail: Vec<serde_json::Value> = uow
            .list_audit(&org.id, 100_000)
            .await?
            .into_iter()
            .filter(|a| a.entity_id.as_deref() == Some(f.id.as_str()))
            .map(|a| {
                serde_json::json!({
                    "at": a.occurred_at,
                    "actor": a.actor,
                    "action": a.action,
                    "before": a.before_json,
                    "after": a.after_json,
                })
            })
            .collect();
        files.push((
            "audit-trail.json".into(),
            serde_json::to_vec_pretty(&trail).unwrap_or_default(),
        ));
        let manifest = serde_json::json!({
            "app": "AccountsVU",
            "pack": "compliance",
            "return": kind.label(),
            "gstin": reg.gstin,
            "legalName": org.legal_name,
            "period": period.code(),
            "channel": f.channel,
            "arn": f.arn,
            "filedOn": f.filed_on,
            "note": f.note,
            "recordedAt": f.recorded_at,
            "recordedBy": f.recorded_by,
            "returnPack": f.return_pack_version,
            "rulePack": f.rule_pack_version,
            "summary": serde_json::from_str::<serde_json::Value>(&f.summary_json).unwrap_or_default(),
            "workpaperSha256": f.workpaper_sha256,
            "files": files.iter().map(|(n, b)| serde_json::json!({"name": n, "sha256": sha256_hex(b), "bytes": b.len()})).collect::<Vec<_>>(),
        });
        files.insert(
            0,
            (
                "manifest.json".into(),
                serde_json::to_vec_pretty(&manifest).unwrap_or_default(),
            ),
        );
        Ok(WorkpaperFile {
            file_name: format!(
                "{}_compliance_pack_{}_{:02}{}.zip",
                kind.label().replace('-', ""),
                reg.gstin,
                period.month,
                period.year
            ),
            bytes: zip_files(&files)?,
        })
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    fn filed(rt: &str, period: &str) -> GstFilingRecord {
        GstFilingRecord {
            id: format!("{rt}-{period}"),
            organisation_id: "o".into(),
            gst_registration_id: "r".into(),
            return_type: rt.into(),
            period: period.into(),
            status: "filed".into(),
            channel: "offline".into(),
            arn: "AA2710260000001".into(),
            filed_on: "2026-11-10".into(),
            note: None,
            return_pack_version: "x".into(),
            rule_pack_version: "x".into(),
            summary_json: "{}".into(),
            workpaper_sha256: String::new(),
            warning_count: 0,
            recorded_at: String::new(),
            recorded_by: String::new(),
            removed_reason: None,
            removed_at: None,
            removed_by: None,
        }
    }

    #[test]
    fn arn_and_locks() {
        assert_eq!(
            normalise_arn(" aa27 1026 0000 123 ").as_deref(),
            Some("AA271026000012".to_owned() + "3").as_deref()
        );
        assert_eq!(normalise_arn("AA27102600001"), None);
        assert_eq!(normalise_arn("AA27102600001-3"), None);

        let f = vec![filed("gstr1", "2026-10")];
        assert_eq!(
            lock_label(&f, "2026-10-15", "sales").as_deref(),
            Some("GSTR-1 for October 2026")
        );
        assert_eq!(lock_label(&f, "2026-10-15", "purchases"), None);
        assert_eq!(lock_label(&f, "2026-11-01", "sales"), None);

        let f = vec![filed("gstr3b", "2026-10"), filed("gstr3b", "2026-11")];
        assert_eq!(
            lock_label(&f, "2026-10-15", "purchases").as_deref(),
            Some("GSTR-3B for October 2026")
        );
        let oct = Period::parse("2026-10").unwrap();
        assert_eq!(first_open_3b(&f, oct).code(), "2026-12");
    }
}
