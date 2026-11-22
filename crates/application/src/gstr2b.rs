//! Phase 2.4: GSTR-2B import and reconciliation with the purchase register.
//!
//! A statement is imported per GSTIN and month from the portal's JSON
//! download. Reconciliation runs on demand from the saved statement, the
//! recorded purchases and the reviewer's decisions, so it always reflects
//! the books as they are now.
//!
//! Which purchases take part in a month:
//! * purchases from a supplier with a GSTIN, recorded against the GSTIN,
//!   whose supplier invoice date (or recorded date) falls in the month;
//! * earlier purchases not found in any other imported statement — they can
//!   still match a supplier who filed late, and are listed as missing only
//!   when a reviewer deferred them from an earlier month;
//! * purchases found in another month's statement are left out here and say
//!   where they were found.
//!
//! Matching never claims credit: input tax credit still follows the review
//! on each purchase line (spec: do not auto-claim merely because an invoice
//! matches).

use std::collections::{BTreeMap, HashMap, HashSet};

use av_domain::new_id;
use av_gst_engine::offset::Heads;
use av_gst_engine::recon::{self, Doc, MatchKind};
use av_gst_formats::gstr2b as fmt;
use av_persistence::records::{
    Gstr2bDecisionRecord, Gstr2bLineRecord, Gstr2bStatementRecord, PurchaseDocumentRecord,
    PurchaseLineRecord, RegistrationRecord,
};
use av_persistence::{Gstr2bRepo, Store, UnitOfWork};
use serde::{Deserialize, Serialize};

use crate::AppCore;
use crate::context::{Permission, RequestContext};
use crate::error::{AppError, AppResult};
use crate::gstr3b::{heads_of, split};
use crate::purchases::sha256_hex;
use crate::returns::{Period, iso};
use crate::setup::{Change, audit};

/// Largest difference per amount still treated as equal (₹1, for rounding).
pub const TOLERANCE_PAISE: i64 = 100;

const MAX_FILE_BYTES: usize = 20 * 1024 * 1024;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Amounts {
    pub taxable_paise: i64,
    pub igst_paise: i64,
    pub cgst_paise: i64,
    pub sgst_paise: i64,
    pub cess_paise: i64,
    pub tax_paise: i64,
}

impl Amounts {
    fn of(d: &Doc) -> Self {
        Amounts {
            taxable_paise: d.taxable,
            igst_paise: d.igst,
            cgst_paise: d.cgst,
            sgst_paise: d.sgst,
            cess_paise: d.cess,
            tax_paise: d.igst + d.cgst + d.sgst + d.cess,
        }
    }

    fn add(&mut self, o: &Amounts, sign: i64) {
        self.taxable_paise += sign * o.taxable_paise;
        self.igst_paise += sign * o.igst_paise;
        self.cgst_paise += sign * o.cgst_paise;
        self.sgst_paise += sign * o.sgst_paise;
        self.cess_paise += sign * o.cess_paise;
        self.tax_paise += sign * o.tax_paise;
    }
}

/// The purchase-register side of a row.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookSide {
    pub purchase_id: String,
    /// `bill`, `expense` or `debit_note`.
    pub doc_type: String,
    pub number: String,
    pub date: String,
    pub amounts: Amounts,
    /// Lines whose ITC review is still pending.
    pub itc_pending_lines: usize,
    /// ITC claimable on this document after review.
    pub itc_claimable_paise: i64,
    /// Dated before the month (carried in).
    pub carried: bool,
}

/// The GSTR-2B side of a row.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalSide {
    pub section: String,
    pub number: String,
    pub date: String,
    pub value_paise: i64,
    pub amounts: Amounts,
    pub itc_available: bool,
    pub itc_reason: Option<String>,
    pub reverse_charge: bool,
    pub supplier_filed_on: Option<String>,
    pub supplier_period: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DifferenceView {
    pub field: &'static str,
    pub books: String,
    pub portal: String,
    pub delta_paise: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconRow {
    /// Stable key for decisions.
    pub key: String,
    /// `exact`, `likely`, `mismatch`, `missing_in_2b`, `missing_in_books` or `duplicate`.
    pub status: &'static str,
    /// `accepted` or `deferred` (this month's decision).
    pub decision: Option<String>,
    pub note: Option<String>,
    /// Still needs a decision.
    pub needs_review: bool,
    pub supplier_gstin: String,
    pub supplier_name: String,
    /// Supplier's view: `invoice`, `credit_note` or `debit_note`.
    pub kind: String,
    pub books: Option<BookSide>,
    pub portal: Option<PortalSide>,
    pub differences: Vec<DifferenceView>,
    /// For a purchase missing here: the month whose 2B lists it.
    pub found_in: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusSummary {
    pub status: &'static str,
    pub label: &'static str,
    pub count: usize,
    pub needs_review: usize,
    pub books: Amounts,
    pub portal: Amounts,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementInfo {
    pub file_name: String,
    pub imported_at: String,
    pub generated_on: Option<String>,
    pub document_count: usize,
    pub skipped_summary: Option<String>,
}

/// ITC in 2B against ITC claimed in the books for the same documents.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItcCompare {
    /// 2B documents marked ITC available (credit notes reduce it).
    pub portal_available_paise: i64,
    /// 2B documents marked ITC not available.
    pub portal_not_available_paise: i64,
    /// Claimable in the books after review, on documents in this month.
    pub books_claimable_paise: i64,
    /// Books documents with ITC claimed but not in this 2B.
    pub books_claimed_not_in_2b_paise: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gstr2bView {
    pub registration_id: String,
    pub gstin: String,
    pub period: String,
    pub label: String,
    pub statement: Option<StatementInfo>,
    pub rows: Vec<ReconRow>,
    pub summary: Vec<StatusSummary>,
    pub needs_review: usize,
    pub itc: ItcCompare,
    pub tolerance_paise: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gstr2bImportResult {
    pub period: String,
    pub document_count: usize,
    pub skipped_summary: Option<String>,
    /// True when a statement for the month was replaced.
    pub replaced: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gstr2bDecisionInput {
    pub row_key: String,
    /// `accepted`, `deferred`, or `None` to clear.
    pub decision: Option<String>,
    pub note: Option<String>,
}

pub struct ReconFile {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Pure reconciliation
// ---------------------------------------------------------------------------

/// A recorded purchase ready for matching.
pub(crate) struct BookDoc {
    rec: PurchaseDocumentRecord,
    doc: Doc,
    itc_pending_lines: usize,
    itc_claimable_paise: i64,
}

fn book_doc(d: PurchaseDocumentRecord, lines: &[PurchaseLineRecord]) -> Option<BookDoc> {
    let gstin = d.party_gstin.clone().filter(|g| !g.trim().is_empty())?;
    if d.status != "recorded" {
        return None;
    }
    let (kind, number) = match d.doc_type.as_str() {
        "bill" | "expense" => (
            "invoice",
            d.supplier_invoice_number
                .clone()
                .or_else(|| d.number.clone())?,
        ),
        // Our debit note is the supplier's credit note; its number is ours.
        "debit_note" => ("credit_note", d.number.clone()?),
        _ => return None,
    };
    let date = d
        .supplier_invoice_date
        .clone()
        .unwrap_or_else(|| d.doc_date.clone());
    let mut pending = 0;
    let mut claimable = 0;
    for l in lines {
        let tax: Heads = heads_of(l);
        if tax.total() == 0 {
            continue;
        }
        match l.itc_status.as_str() {
            "pending" => pending += 1,
            "eligible" => claimable += tax.total(),
            "partial" => claimable += split(tax, l.itc_eligible_paise).total(),
            _ => {}
        }
    }
    let doc = Doc {
        gstin: gstin.trim().to_uppercase(),
        kind: kind.into(),
        number,
        date,
        taxable: d.taxable_paise,
        igst: d.igst_paise,
        cgst: d.cgst_paise,
        sgst: d.sgst_paise + d.utgst_paise,
        cess: d.cess_paise,
    };
    Some(BookDoc {
        rec: d,
        doc,
        itc_pending_lines: pending,
        itc_claimable_paise: claimable,
    })
}

fn portal_doc(l: &Gstr2bLineRecord) -> Doc {
    Doc {
        gstin: l.supplier_gstin.to_uppercase(),
        kind: l.doc_kind.clone(),
        number: l.doc_number.clone(),
        date: l.doc_date.clone(),
        taxable: l.taxable_paise,
        igst: l.igst_paise,
        cgst: l.cgst_paise,
        sgst: l.sgst_paise,
        cess: l.cess_paise,
    }
}

fn doc_key(d: &Doc) -> String {
    format!(
        "2b:{}|{}|{}",
        d.gstin.to_uppercase(),
        d.kind,
        recon::normalise_number(&d.number)
    )
}

fn book_key(id: &str) -> String {
    format!("book:{id}")
}

/// Everything reconciliation needs, loaded once.
pub(crate) struct ReconData {
    pub(crate) books: Vec<BookDoc>,
    pub(crate) statements: Vec<(Gstr2bStatementRecord, Vec<Gstr2bLineRecord>)>,
    pub(crate) decisions: Vec<Gstr2bDecisionRecord>,
}

const STATUSES: [(&str, &str); 6] = [
    ("exact", "Exact match"),
    ("likely", "Likely match"),
    ("mismatch", "Mismatch"),
    ("missing_in_2b", "Missing in 2B"),
    ("missing_in_books", "Missing in books"),
    ("duplicate", "Duplicate in 2B"),
];

pub(crate) struct Reconciled {
    pub(crate) rows: Vec<ReconRow>,
    pub(crate) statement: Option<StatementInfo>,
}

pub(crate) fn reconcile_period(data: &ReconData, period: Period) -> Reconciled {
    let code = period.code();
    let first = iso(period.first());
    let last = iso(period.last());
    let this = data.statements.iter().find(|(s, _)| s.period == code);
    let lines: &[Gstr2bLineRecord] = this.map(|(_, l)| l.as_slice()).unwrap_or(&[]);

    // Where else each document key appears in 2B.
    let mut found_elsewhere: HashMap<String, String> = HashMap::new();
    for (s, ls) in &data.statements {
        if s.period == code {
            continue;
        }
        for l in ls {
            found_elsewhere
                .entry(doc_key(&portal_doc(l)))
                .or_insert_with(|| s.period.clone());
        }
    }
    // Purchases accepted as matched in another month.
    let accepted_elsewhere: HashSet<&str> = data
        .decisions
        .iter()
        .filter(|d| d.period != code && d.decision == "accepted")
        .filter_map(|d| d.purchase_id.as_deref())
        .collect();
    // Latest earlier decision per book key.
    let mut earlier_book: BTreeMap<&str, &Gstr2bDecisionRecord> = BTreeMap::new();
    for d in data.decisions.iter().filter(|d| d.period < code) {
        if d.row_key.starts_with("book:") {
            earlier_book.insert(d.row_key.as_str(), d);
        }
    }
    let this_decisions: HashMap<&str, &Gstr2bDecisionRecord> = data
        .decisions
        .iter()
        .filter(|d| d.period == code)
        .map(|d| (d.row_key.as_str(), d))
        .collect();

    // The pool of purchases for this month.
    let mut pool: Vec<&BookDoc> = Vec::new();
    let mut found_in: HashMap<&str, String> = HashMap::new();
    for b in &data.books {
        let date = b.doc.date.as_str();
        if date > last.as_str() {
            continue;
        }
        if let Some(p) = found_elsewhere.get(&doc_key(&b.doc)) {
            if date >= first.as_str() {
                found_in.insert(b.rec.id.as_str(), p.clone());
            }
            continue;
        }
        if accepted_elsewhere.contains(b.rec.id.as_str()) {
            continue;
        }
        pool.push(b);
    }
    let book_docs: Vec<Doc> = pool.iter().map(|b| b.doc.clone()).collect();
    let portal_docs: Vec<Doc> = lines.iter().map(portal_doc).collect();
    let outcome = recon::reconcile(&book_docs, &portal_docs, TOLERANCE_PAISE);

    let book_side = |b: &BookDoc| BookSide {
        purchase_id: b.rec.id.clone(),
        doc_type: b.rec.doc_type.clone(),
        number: b.doc.number.clone(),
        date: b.doc.date.clone(),
        amounts: Amounts::of(&b.doc),
        itc_pending_lines: b.itc_pending_lines,
        itc_claimable_paise: b.itc_claimable_paise,
        carried: b.doc.date < first,
    };
    let portal_side = |l: &Gstr2bLineRecord| PortalSide {
        section: l.section.clone(),
        number: l.doc_number.clone(),
        date: l.doc_date.clone(),
        value_paise: l.value_paise,
        amounts: Amounts::of(&portal_doc(l)),
        itc_available: l.itc_available,
        itc_reason: l.itc_reason.clone(),
        reverse_charge: l.reverse_charge,
        supplier_filed_on: l.supplier_filed_on.clone(),
        supplier_period: l.supplier_period.clone(),
    };
    let decided = |key: &str, purchase: Option<&str>| {
        this_decisions
            .get(key)
            .filter(|d| purchase.is_none() || d.purchase_id.as_deref() == purchase)
            .map(|d| (d.decision.clone(), d.note.clone()))
    };

    let mut rows = Vec::new();
    for p in &outcome.pairs {
        let b = pool[p.book];
        let l = &lines[p.portal];
        let key = doc_key(&portal_docs[p.portal]);
        let status = match p.kind {
            MatchKind::Exact => "exact",
            MatchKind::Likely => "likely",
            MatchKind::Mismatch => "mismatch",
        };
        let d = decided(&key, Some(&b.rec.id));
        rows.push(ReconRow {
            needs_review: status != "exact" && d.is_none(),
            decision: d.as_ref().map(|d| d.0.clone()),
            note: d.and_then(|d| d.1),
            key,
            status,
            supplier_gstin: l.supplier_gstin.clone(),
            supplier_name: if l.supplier_name.is_empty() {
                b.rec.party_name.clone().unwrap_or_default()
            } else {
                l.supplier_name.clone()
            },
            kind: l.doc_kind.clone(),
            books: Some(book_side(b)),
            portal: Some(portal_side(l)),
            differences: p
                .differences
                .iter()
                .map(|d| DifferenceView {
                    field: d.field,
                    books: d.books.clone(),
                    portal: d.portal.clone(),
                    delta_paise: d.delta_paise,
                })
                .collect(),
            found_in: None,
        });
    }
    for &(i, _) in &outcome.duplicates {
        let l = &lines[i];
        let key = format!("{}#{}", doc_key(&portal_docs[i]), l.line_no);
        let d = decided(&key, None);
        rows.push(portal_only(l, key, "duplicate", d, portal_side(l)));
    }
    for &i in &outcome.missing_in_books {
        let l = &lines[i];
        let key = doc_key(&portal_docs[i]);
        let d = decided(&key, None);
        rows.push(portal_only(l, key, "missing_in_books", d, portal_side(l)));
    }
    for &i in &outcome.missing_in_portal {
        let b = pool[i];
        let key = book_key(&b.rec.id);
        let carried = b.doc.date < first;
        // An earlier purchase shows only if it was deferred to a later month.
        if carried
            && !earlier_book
                .get(key.as_str())
                .is_some_and(|d| d.decision == "deferred")
        {
            continue;
        }
        let d = decided(&key, None);
        rows.push(book_only(b, key, d, book_side(b), None));
    }
    for b in &data.books {
        if let Some(p) = found_in.get(b.rec.id.as_str()) {
            let key = book_key(&b.rec.id);
            let mut row = book_only(b, key, None, book_side(b), Some(p.clone()));
            row.needs_review = false;
            rows.push(row);
        }
    }
    rows.sort_by(|a, b| {
        let date = |r: &ReconRow| {
            r.portal
                .as_ref()
                .map(|p| p.date.clone())
                .or_else(|| r.books.as_ref().map(|b| b.date.clone()))
                .unwrap_or_default()
        };
        (a.supplier_name.to_lowercase(), date(a)).cmp(&(b.supplier_name.to_lowercase(), date(b)))
    });

    let statement = this.map(|(s, l)| StatementInfo {
        file_name: s.file_name.clone(),
        imported_at: s.imported_at.clone(),
        generated_on: s.generated_on.clone(),
        document_count: l.len(),
        skipped_summary: s.skipped_summary.clone(),
    });
    Reconciled { rows, statement }
}

fn portal_only(
    l: &Gstr2bLineRecord,
    key: String,
    status: &'static str,
    d: Option<(String, Option<String>)>,
    side: PortalSide,
) -> ReconRow {
    ReconRow {
        needs_review: d.is_none(),
        decision: d.as_ref().map(|d| d.0.clone()),
        note: d.and_then(|d| d.1),
        key,
        status,
        supplier_gstin: l.supplier_gstin.clone(),
        supplier_name: l.supplier_name.clone(),
        kind: l.doc_kind.clone(),
        books: None,
        portal: Some(side),
        differences: Vec::new(),
        found_in: None,
    }
}

fn book_only(
    b: &BookDoc,
    key: String,
    d: Option<(String, Option<String>)>,
    side: BookSide,
    found_in: Option<String>,
) -> ReconRow {
    ReconRow {
        needs_review: d.is_none(),
        decision: d.as_ref().map(|d| d.0.clone()),
        note: d.and_then(|d| d.1),
        key,
        status: "missing_in_2b",
        supplier_gstin: b.doc.gstin.clone(),
        supplier_name: b.rec.party_name.clone().unwrap_or_default(),
        kind: b.doc.kind.clone(),
        books: Some(side),
        portal: None,
        differences: Vec::new(),
        found_in,
    }
}

fn kind_sign(kind: &str) -> i64 {
    if kind == "credit_note" { -1 } else { 1 }
}

fn summarise(rows: &[ReconRow]) -> (Vec<StatusSummary>, ItcCompare) {
    let mut out: Vec<StatusSummary> = STATUSES
        .iter()
        .map(|&(status, label)| StatusSummary {
            status,
            label,
            count: 0,
            needs_review: 0,
            books: Amounts::default(),
            portal: Amounts::default(),
        })
        .collect();
    let mut itc = ItcCompare::default();
    for r in rows {
        let sign = kind_sign(&r.kind);
        if let Some(s) = out.iter_mut().find(|s| s.status == r.status) {
            s.count += 1;
            s.needs_review += usize::from(r.needs_review);
            if let Some(b) = &r.books {
                s.books.add(&b.amounts, sign);
            }
            if let Some(p) = &r.portal {
                s.portal.add(&p.amounts, sign);
            }
        }
        if r.status == "duplicate" || r.found_in.is_some() {
            continue;
        }
        if let Some(p) = &r.portal {
            if p.itc_available {
                itc.portal_available_paise += sign * p.amounts.tax_paise;
            } else {
                itc.portal_not_available_paise += sign * p.amounts.tax_paise;
            }
        }
        if let Some(b) = &r.books {
            itc.books_claimable_paise += sign * b.itc_claimable_paise;
            if r.portal.is_none() {
                itc.books_claimed_not_in_2b_paise += sign * b.itc_claimable_paise;
            }
        }
    }
    (out, itc)
}

// ---------------------------------------------------------------------------
// Use cases
// ---------------------------------------------------------------------------

impl<S: Store> AppCore<S> {
    pub(crate) async fn load_recon_data(
        &self,
        uow: &mut S::Uow,
        org_id: &str,
        reg: &RegistrationRecord,
    ) -> AppResult<ReconData> {
        let mut books = Vec::new();
        for (d, lines) in self.load_return_purchases(uow, org_id).await? {
            if d.gst_registration_id.as_deref() != Some(reg.id.as_str()) {
                continue;
            }
            if let Some(b) = book_doc(d, &lines) {
                books.push(b);
            }
        }
        let mut statements = Vec::new();
        for s in uow.list_gstr2b_statements(org_id, &reg.id).await? {
            let lines = uow.gstr2b_lines(&s.id).await?;
            statements.push((s, lines));
        }
        let decisions = uow.list_gstr2b_decisions(org_id, &reg.id).await?;
        Ok(ReconData {
            books,
            statements,
            decisions,
        })
    }

    /// Reconciliation of a GSTIN's purchases with its GSTR-2B for a month
    /// (`YYYY-MM`). Works without a statement too: every purchase is then
    /// missing in 2B.
    pub async fn gstr2b(
        &self,
        ctx: &RequestContext,
        registration_id: &str,
        period: &str,
    ) -> AppResult<Gstr2bView> {
        ctx.require(Permission::ViewBusiness)?;
        let period = Period::parse(period)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let (_, reg) = self
            .return_registration(&mut uow, &org.id, Some(registration_id))
            .await?;
        let reg = reg.ok_or(AppError::NotFound("GST registration"))?;
        let data = self.load_recon_data(&mut uow, &org.id, &reg).await?;
        Ok(view_of(&reg, period, reconcile_period(&data, period)))
    }

    /// Imports a GSTR-2B JSON download for the GSTIN, replacing an earlier
    /// import of the same month. Decisions are kept.
    pub async fn import_gstr2b(
        &self,
        ctx: &RequestContext,
        registration_id: &str,
        file_name: &str,
        bytes: &[u8],
    ) -> AppResult<Gstr2bImportResult> {
        ctx.require(Permission::PostDocuments)?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(AppError::Invalid("the file is larger than 20 MB".into()));
        }
        let lower = file_name.to_lowercase();
        if lower.ends_with(".xlsx") || lower.ends_with(".xls") || lower.ends_with(".zip") {
            return Err(AppError::Invalid(
                "choose the JSON download of GSTR-2B (Excel and ZIP files aren't read yet; unzip the download first)".into(),
            ));
        }
        let parsed = fmt::parse(bytes).map_err(|e| AppError::Invalid(e.to_string()))?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let (_, reg) = self
            .return_registration(&mut uow, &org.id, Some(registration_id))
            .await?;
        let reg = reg.ok_or(AppError::NotFound("GST registration"))?;
        if !parsed.gstin.eq_ignore_ascii_case(&reg.gstin) {
            return Err(AppError::Invalid(format!(
                "this GSTR-2B is for {}, not {}",
                parsed.gstin, reg.gstin
            )));
        }
        let period = Period::parse(&parsed.period)?;
        let replaced = uow
            .get_gstr2b_statement(&org.id, &reg.id, &period.code())
            .await?
            .is_some();
        let now = self.now();
        let skipped = (!parsed.skipped.is_empty()).then(|| {
            parsed
                .skipped
                .iter()
                .map(|(s, n)| format!("{s} {n}"))
                .collect::<Vec<_>>()
                .join(", ")
        });
        let sha = sha256_hex(bytes);
        let statement = Gstr2bStatementRecord {
            id: new_id().to_string(),
            organisation_id: org.id.clone(),
            gst_registration_id: reg.id.clone(),
            period: period.code(),
            gstin: parsed.gstin.clone(),
            generated_on: parsed.generated_on.clone(),
            format_version: parsed.version.clone(),
            adapter_version: fmt::ADAPTER_VERSION.into(),
            file_name: file_name.into(),
            sha256: sha.clone(),
            skipped_summary: skipped.clone(),
            imported_at: now.clone(),
            imported_by: Some(ctx.actor.user_id.clone()),
        };
        let lines: Vec<Gstr2bLineRecord> = parsed
            .lines
            .iter()
            .enumerate()
            .map(|(i, l)| Gstr2bLineRecord {
                id: new_id().to_string(),
                statement_id: statement.id.clone(),
                line_no: i64::try_from(i + 1).unwrap_or(i64::MAX),
                section: l.section.into(),
                supplier_gstin: l.supplier_gstin.clone(),
                supplier_name: l.supplier_name.clone(),
                doc_kind: l.kind.as_str().into(),
                doc_number: l.number.clone(),
                doc_date: l.date.clone(),
                value_paise: l.value_paise,
                taxable_paise: l.taxable_paise,
                igst_paise: l.igst_paise,
                cgst_paise: l.cgst_paise,
                sgst_paise: l.sgst_paise,
                cess_paise: l.cess_paise,
                place_of_supply: l.place_of_supply.clone(),
                reverse_charge: l.reverse_charge,
                itc_available: l.itc_available,
                itc_reason: l.itc_reason.clone(),
                supplier_filed_on: l.supplier_filed_on.clone(),
                supplier_period: l.supplier_period.clone(),
                original_number: l.original_number.clone(),
            })
            .collect();
        uow.replace_gstr2b_statement(&statement, &lines).await?;
        let after = serde_json::json!({
            "gstin": reg.gstin,
            "period": period.code(),
            "documents": lines.len(),
            "format": fmt::ADAPTER_VERSION,
            "fileName": file_name,
            "sha256": sha,
            "replaced": replaced,
        });
        audit(
            &mut uow,
            ctx,
            &org.id,
            Change {
                action: "gstr2b.imported",
                entity_id: Some(&statement.id),
                before: None,
                after: Some(after.to_string()),
            },
            &now,
        )
        .await?;
        uow.commit().await?;
        Ok(Gstr2bImportResult {
            period: period.code(),
            document_count: lines.len(),
            skipped_summary: skipped,
            replaced,
        })
    }

    /// Accepts or defers a reconciliation row, or clears its decision.
    pub async fn decide_gstr2b(
        &self,
        ctx: &RequestContext,
        registration_id: &str,
        period: &str,
        input: Gstr2bDecisionInput,
    ) -> AppResult<Gstr2bView> {
        ctx.require(Permission::PostDocuments)?;
        let period = Period::parse(period)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let (_, reg) = self
            .return_registration(&mut uow, &org.id, Some(registration_id))
            .await?;
        let reg = reg.ok_or(AppError::NotFound("GST registration"))?;
        let data = self.load_recon_data(&mut uow, &org.id, &reg).await?;
        let rec = reconcile_period(&data, period);
        let row = rec
            .rows
            .iter()
            .find(|r| r.key == input.row_key && r.found_in.is_none())
            .ok_or(AppError::NotFound("reconciliation row"))?;
        let now = self.now();
        let before = row.decision.clone();
        match input.decision.as_deref() {
            None => {
                uow.delete_gstr2b_decision(&org.id, &reg.id, &period.code(), &row.key)
                    .await?;
            }
            Some(d @ ("accepted" | "deferred")) => {
                if row.status == "exact" {
                    return Err(AppError::Invalid("an exact match needs no decision".into()));
                }
                if d == "deferred" && row.status == "duplicate" {
                    return Err(AppError::Invalid(
                        "a duplicate can only be accepted (acknowledged)".into(),
                    ));
                }
                let note = input
                    .note
                    .as_deref()
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map(|n| n.chars().take(500).collect::<String>());
                uow.upsert_gstr2b_decision(&Gstr2bDecisionRecord {
                    id: new_id().to_string(),
                    organisation_id: org.id.clone(),
                    gst_registration_id: reg.id.clone(),
                    period: period.code(),
                    row_key: row.key.clone(),
                    purchase_id: row.books.as_ref().map(|b| b.purchase_id.clone()),
                    decision: d.into(),
                    note,
                    decided_at: now.clone(),
                    decided_by: Some(ctx.actor.user_id.clone()),
                })
                .await?;
            }
            Some(_) => return Err(AppError::Invalid("choose accept or defer".into())),
        }
        let after = serde_json::json!({
            "period": period.code(),
            "row": row.key,
            "status": row.status,
            "decision": input.decision,
        });
        audit(
            &mut uow,
            ctx,
            &org.id,
            Change {
                action: if input.decision.is_some() {
                    "gstr2b.decided"
                } else {
                    "gstr2b.cleared"
                },
                entity_id: row.books.as_ref().map(|b| b.purchase_id.as_str()),
                before: before.map(|b| serde_json::json!({ "decision": b }).to_string()),
                after: Some(after.to_string()),
            },
            &now,
        )
        .await?;
        let data = self.load_recon_data(&mut uow, &org.id, &reg).await?;
        uow.commit().await?;
        Ok(view_of(&reg, period, reconcile_period(&data, period)))
    }

    /// The reconciliation and purchase register as an Excel workbook.
    pub async fn gstr2b_xlsx(
        &self,
        ctx: &RequestContext,
        registration_id: &str,
        period: &str,
    ) -> AppResult<ReconFile> {
        let v = self.gstr2b(ctx, registration_id, period).await?;
        let bytes = crate::workpaper_xlsx::gstr2b_workbook(&v)
            .map_err(|e| AppError::Invalid(format!("couldn't build the workbook: {e}")))?;
        let (year, month) = v.period.split_once('-').unwrap_or_default();
        Ok(ReconFile {
            file_name: format!("GSTR2B_reconciliation_{}_{month}{year}.xlsx", v.gstin),
            bytes,
        })
    }
}

pub(crate) fn view_of(reg: &RegistrationRecord, period: Period, r: Reconciled) -> Gstr2bView {
    let (summary, itc) = summarise(&r.rows);
    Gstr2bView {
        registration_id: reg.id.clone(),
        gstin: reg.gstin.clone(),
        period: period.code(),
        label: period.label(),
        needs_review: r.rows.iter().filter(|r| r.needs_review).count(),
        statement: r.statement,
        rows: r.rows,
        summary,
        itc,
        tolerance_paise: TOLERANCE_PAISE,
    }
}
