//! Phase 2.2: the return centre and the GSTR-1 workpaper.
//!
//! Everything is computed from issued sales documents of one GSTIN, placed
//! by the same rule-pack logic the invoice editor shows (`gstr1_of`), so the
//! table an invoice says it lands in is the table it is reported in. Every
//! row keeps the document ids it came from for drill-down.
//!
//! Monthly periods only; quarterly (QRMP) filers see monthly periods until
//! QRMP support lands.

use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;

use av_domain::masters::is_valid_uqc;
use av_domain::{FinancialYear, Gstin, StateCode, TaxCategory};
use av_gst_engine::RulePack;
use av_gst_engine::SupplyKind;
use av_gst_engine::checks::{
    Gstr1Section, LineFacts, Recipient, line_warnings, place_of_supply_warning,
};
use av_gst_formats::gstr1::{
    self as fmt, B2csRow, DocumentNature, DocumentSeries, Gstr1Return, HsnRow, Invoice,
    InvoiceType, NilRow, NilSupply, Note, NoteType, RateItem, UnregisteredType,
};
use av_persistence::records::{RegistrationRecord, SalesDocumentRecord, SalesLineRecord};
use av_persistence::{RegistrationRepo, SalesRepo, Store, UnitOfWork};
use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::AppCore;
use crate::context::{Permission, RequestContext};
use crate::error::{AppError, AppResult};
use crate::purchases::sha256_hex;
use crate::sales::{gstr1_of, kind_of, recipient_of, ymd};
use crate::setup::{Change, audit};

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaxTotals {
    pub taxable_paise: i64,
    pub igst_paise: i64,
    pub cgst_paise: i64,
    /// SGST and UTGST.
    pub sgst_paise: i64,
    pub cess_paise: i64,
    pub tax_paise: i64,
}

impl TaxTotals {
    fn add(&mut self, i: &RateItem, sign: i64) {
        self.taxable_paise += sign * i.taxable_paise;
        self.igst_paise += sign * i.igst_paise;
        self.cgst_paise += sign * i.cgst_paise;
        self.sgst_paise += sign * i.sgst_paise;
        self.cess_paise += sign * i.cess_paise;
        self.tax_paise = self.igst_paise + self.cgst_paise + self.sgst_paise + self.cess_paise;
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrationOption {
    pub id: String,
    pub gstin: String,
    pub state_name: String,
    pub filing_frequency: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodSummary {
    /// `YYYY-MM`.
    pub period: String,
    pub label: String,
    pub from: String,
    pub to: String,
    pub gstr1_due: String,
    /// `open` (month not over), `due` (before the due date), `past_due`.
    pub status: &'static str,
    pub document_count: usize,
    pub draft_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub output_tax: TaxTotals,
    pub gstr3b_due: String,
    pub gstr3b_status: &'static str,
    /// Net ITC (4C) claimed in the month.
    pub net_itc_paise: i64,
    /// Proposed cash payment (6.1), reverse charge included.
    pub cash_payable_paise: i64,
    /// A GSTR-2B statement has been imported for the month.
    pub gstr2b_imported: bool,
    /// Reconciliation rows still needing a decision (0 until imported).
    pub gstr2b_needs_review: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReturnCentreView {
    pub registrations: Vec<RegistrationOption>,
    pub registration_id: Option<String>,
    pub financial_year: String,
    pub fy_start: String,
    pub fy_end: String,
    pub previous_year: Option<String>,
    pub next_year: Option<String>,
    /// Quarterly (QRMP) filer shown monthly for now.
    pub quarterly: bool,
    /// Newest first.
    pub periods: Vec<PeriodSummary>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionSummary {
    pub key: &'static str,
    pub title: &'static str,
    pub table: &'static str,
    pub count: usize,
    pub totals: TaxTotals,
    /// Invoice/note value, or the table's amount for table 8.
    pub value_paise: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReturnDocRow {
    pub id: String,
    /// `b2b`, `b2cl`, `exp`, `cdnr` or `cdnur`.
    pub section: &'static str,
    pub doc_type: String,
    pub number: String,
    pub doc_date: String,
    pub party_name: String,
    pub gstin: Option<String>,
    pub place_of_supply: i64,
    pub place_name: String,
    /// `Regular`, `SEZ with payment`, …, or export type.
    pub kind_label: &'static str,
    pub reverse_charge: bool,
    pub value_paise: i64,
    pub totals: TaxTotals,
    pub original_number: Option<String>,
    pub original_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocLink {
    pub id: String,
    pub number: String,
    pub doc_type: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct B2csView {
    pub place_of_supply: i64,
    pub place_name: String,
    pub intra_state: bool,
    pub rate_bp: u32,
    pub totals: TaxTotals,
    pub documents: Vec<DocLink>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NilView {
    pub supply: &'static str,
    pub label: &'static str,
    pub nil_paise: i64,
    pub exempt_paise: i64,
    pub non_gst_paise: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HsnView {
    /// `b2b` or `b2c`.
    pub group: &'static str,
    pub hsn: String,
    pub description: String,
    pub uqc: String,
    pub quantity: String,
    pub rate_bp: u32,
    pub totals: TaxTotals,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocSeriesView {
    pub label: &'static str,
    pub from: String,
    pub to: String,
    pub total: u32,
    pub cancelled: u32,
    pub net: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReturnException {
    /// `error` (the portal would reject or the figure is wrong) or `warning`.
    pub severity: &'static str,
    pub code: &'static str,
    pub message: String,
    pub document_id: Option<String>,
    pub number: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gstr1View {
    pub registration_id: String,
    pub gstin: String,
    pub state_name: String,
    pub period: String,
    pub label: String,
    pub from: String,
    pub to: String,
    pub due_date: String,
    pub status: &'static str,
    pub rule_pack_version: String,
    pub format_version: &'static str,
    pub sections: Vec<SectionSummary>,
    /// Net output tax: invoices less credit notes.
    pub totals: TaxTotals,
    pub documents: Vec<ReturnDocRow>,
    pub b2cs: Vec<B2csView>,
    pub nil: Vec<NilView>,
    pub hsn: Vec<HsnView>,
    pub series: Vec<DocSeriesView>,
    pub exceptions: Vec<ReturnException>,
}

/// The portal file, ready to save.
#[derive(Clone, Debug)]
pub struct Gstr1File {
    pub file_name: String,
    pub json: String,
    pub error_count: usize,
}

// ---------------------------------------------------------------------------
// Periods
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Period {
    pub(crate) year: i32,
    pub(crate) month: u32,
}

impl Period {
    pub(crate) fn parse(s: &str) -> AppResult<Period> {
        let bad = || AppError::Invalid("choose a return period".into());
        let (y, m) = s.trim().split_once('-').ok_or_else(bad)?;
        let year: i32 = y.parse().map_err(|_| bad())?;
        let month: u32 = m.parse().map_err(|_| bad())?;
        if !(1..=12).contains(&month) || !(2017..=2100).contains(&year) {
            return Err(bad());
        }
        Ok(Period { year, month })
    }

    pub(crate) fn of(d: NaiveDate) -> Period {
        Period {
            year: d.year(),
            month: d.month(),
        }
    }

    pub(crate) fn code(self) -> String {
        format!("{}-{:02}", self.year, self.month)
    }

    pub(crate) fn first(self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.year, self.month, 1).expect("valid month")
    }

    pub(crate) fn next(self) -> Period {
        if self.month == 12 {
            Period {
                year: self.year + 1,
                month: 1,
            }
        } else {
            Period {
                year: self.year,
                month: self.month + 1,
            }
        }
    }

    pub(crate) fn last(self) -> NaiveDate {
        self.next()
            .first()
            .pred_opt()
            .expect("not the minimum date")
    }

    pub(crate) fn label(self) -> String {
        self.first().format("%B %Y").to_string()
    }

    /// GSTR-1 (monthly) is due on the 11th of the following month.
    pub(crate) fn gstr1_due(self) -> NaiveDate {
        self.next().first().with_day(11).expect("11th exists")
    }

    pub(crate) fn status(self, today: NaiveDate) -> &'static str {
        self.status_by(today, self.gstr1_due())
    }

    /// GSTR-3B (monthly) is due on the 20th of the following month.
    pub(crate) fn gstr3b_due(self) -> NaiveDate {
        self.next().first().with_day(20).expect("20th exists")
    }

    pub(crate) fn status_by(self, today: NaiveDate, due: NaiveDate) -> &'static str {
        if today <= self.last() {
            "open"
        } else if today <= due {
            "due"
        } else {
            "past_due"
        }
    }
}

pub(crate) fn iso(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub(crate) fn state_name(code: i64) -> String {
    u8::try_from(code)
        .ok()
        .and_then(|c| StateCode::new(c).ok())
        .map_or_else(
            || {
                if code == 96 {
                    "Outside India".to_owned()
                } else {
                    format!("State {code:02}")
                }
            },
            |s| s.name().to_owned(),
        )
}

// ---------------------------------------------------------------------------
// Building the workpaper
// ---------------------------------------------------------------------------

pub(crate) type Doc = (SalesDocumentRecord, Vec<SalesLineRecord>);

/// Taxable lines grouped by rate (SGST and UTGST together).
fn rate_items(lines: &[SalesLineRecord]) -> Vec<RateItem> {
    let mut by_rate: BTreeMap<i64, RateItem> = BTreeMap::new();
    for l in lines.iter().filter(|l| l.tax_category == "taxable") {
        let e = by_rate.entry(l.gst_rate_bp).or_insert_with(|| RateItem {
            rate_bp: u32::try_from(l.gst_rate_bp).unwrap_or(0),
            ..Default::default()
        });
        e.taxable_paise += l.taxable_paise;
        e.igst_paise += l.igst_paise;
        e.cgst_paise += l.cgst_paise;
        e.sgst_paise += l.sgst_paise + l.utgst_paise;
        e.cess_paise += l.cess_paise;
    }
    by_rate.into_values().collect()
}

fn totals_of(items: &[RateItem]) -> TaxTotals {
    let mut t = TaxTotals::default();
    for i in items {
        t.add(i, 1);
    }
    t
}

fn section_key(s: Gstr1Section) -> &'static str {
    match s {
        Gstr1Section::B2b | Gstr1Section::SezWpay | Gstr1Section::SezWopay => "b2b",
        Gstr1Section::B2cl => "b2cl",
        Gstr1Section::B2cs => "b2cs",
        Gstr1Section::ExpWpay | Gstr1Section::ExpWopay => "exp",
        Gstr1Section::Cdnr => "cdnr",
        Gstr1Section::Cdnur => "cdnur",
    }
}

fn invoice_type(s: Option<Gstr1Section>, d: &SalesDocumentRecord) -> InvoiceType {
    match s {
        Some(Gstr1Section::SezWpay) => InvoiceType::SezWithPayment,
        Some(Gstr1Section::SezWopay) => InvoiceType::SezWithoutPayment,
        _ if recipient_of(d) == Recipient::Sez => {
            if d.zero_rated {
                InvoiceType::SezWithoutPayment
            } else {
                InvoiceType::SezWithPayment
            }
        }
        _ => InvoiceType::Regular,
    }
}

fn kind_label(section: Gstr1Section, t: InvoiceType) -> &'static str {
    match section {
        Gstr1Section::ExpWpay => "Export with payment",
        Gstr1Section::ExpWopay => "Export under LUT",
        Gstr1Section::B2cl => "B2C large",
        _ => match t {
            InvoiceType::Regular => "Regular",
            InvoiceType::SezWithPayment => "SEZ with payment",
            InvoiceType::SezWithoutPayment => "SEZ under LUT",
            InvoiceType::DeemedExport => "Deemed export",
        },
    }
}

#[derive(Default)]
struct HsnAcc {
    description: String,
    quantity: Decimal,
    item: RateItem,
}

#[derive(Default)]
struct SeriesAcc {
    numbers: Vec<(String, String)>,
    cancelled: u32,
}

pub(crate) struct Built {
    pub(crate) view: Gstr1View,
    pub(crate) portal: Gstr1Return,
}

pub(crate) fn build(
    reg: &RegistrationRecord,
    docs: &[Doc],
    period: Period,
    today: NaiveDate,
) -> Built {
    let pack = RulePack::bundled();
    let (from, to) = (iso(period.first()), iso(period.last()));
    let by_id: HashMap<&str, &SalesDocumentRecord> =
        docs.iter().map(|(d, _)| (d.id.as_str(), d)).collect();
    let mut in_period: Vec<&Doc> = docs
        .iter()
        .filter(|(d, _)| {
            d.gst_registration_id.as_deref() == Some(reg.id.as_str())
                && d.doc_date >= from
                && d.doc_date <= to
        })
        .collect();
    in_period.sort_by(|a, b| {
        (a.0.doc_date.as_str(), a.0.number.as_deref())
            .cmp(&(b.0.doc_date.as_str(), b.0.number.as_deref()))
    });

    let mut portal = Gstr1Return {
        gstin: reg.gstin.clone(),
        year: period.year,
        month: period.month,
        ..Default::default()
    };
    let mut rows: Vec<ReturnDocRow> = Vec::new();
    let mut exceptions: Vec<ReturnException> = Vec::new();
    let mut b2cs: BTreeMap<(i64, u32, bool), (RateItem, Vec<DocLink>)> = BTreeMap::new();
    let mut nil: BTreeMap<NilSupply, NilRow> = BTreeMap::new();
    let mut hsn: BTreeMap<(&'static str, String, String, u32), HsnAcc> = BTreeMap::new();
    let mut series: BTreeMap<(DocumentNature, String), SeriesAcc> = BTreeMap::new();
    let mut exports = false;

    for (d, lines) in in_period {
        let number = d.number.clone().unwrap_or_default();
        let link = |sev: &'static str, code: &'static str, message: String| ReturnException {
            severity: sev,
            code,
            message,
            document_id: Some(d.id.clone()),
            number: d.number.clone(),
        };
        if d.status == "draft" {
            exceptions.push(link(
                "warning",
                "draft",
                format!(
                    "Draft for {} dated {} isn't issued, so it isn't in this return.",
                    d.party_name,
                    ymd(&d.doc_date).format("%-d %b %Y")
                ),
            ));
            continue;
        }
        // Table 13 counts issued and cancelled documents alike.
        let nature = if d.doc_type == "credit_note" {
            DocumentNature::CreditNotes
        } else {
            DocumentNature::OutwardInvoices
        };
        let s = series
            .entry((nature, d.series_id.clone().unwrap_or_default()))
            .or_default();
        s.numbers.push((
            d.issued_at.clone().unwrap_or_else(|| d.doc_date.clone()),
            number.clone(),
        ));
        if d.status == "cancelled" {
            s.cancelled += 1;
            continue;
        }

        let original = d
            .original_document_id
            .as_deref()
            .and_then(|id| by_id.get(id).copied());
        let placement = gstr1_of(d, lines, original);
        let credit = d.doc_type == "credit_note";
        let sign = if credit { -1 } else { 1 };
        let recipient = recipient_of(d);
        let registered = matches!(recipient, Recipient::Registered | Recipient::Sez);
        let intra = kind_of(d) != SupplyKind::InterState;
        let items = rate_items(lines);
        let inv_type = invoice_type(placement.section, d);

        // Checks the return depends on.
        if let Some(s) = placement.section
            && matches!(
                s,
                Gstr1Section::B2b
                    | Gstr1Section::SezWpay
                    | Gstr1Section::SezWopay
                    | Gstr1Section::Cdnr
            )
            && !d
                .party_gstin
                .as_deref()
                .is_some_and(|g| Gstin::parse(g).is_ok())
        {
            exceptions.push(link(
                "error",
                "missing_gstin",
                format!(
                    "{} {number} goes to a registered-party table, but {} has no valid GSTIN.",
                    doc_word(d),
                    d.party_name
                ),
            ));
        }
        let rate_date = ymd(original.map_or(d.doc_date.as_str(), |o| o.doc_date.as_str()));
        let mut points: Vec<String> = Vec::new();
        if recipient != Recipient::Overseas {
            let pos = u8::try_from(d.place_of_supply)
                .ok()
                .and_then(|c| StateCode::new(c).ok());
            let home = d
                .party_gstin
                .as_deref()
                .and_then(|g| Gstin::parse(g).ok())
                .map(|g| g.state());
            if let Some(w) = pos.and_then(|p| place_of_supply_warning(p, home)) {
                points.push(w.message);
            }
        }
        for (i, l) in lines.iter().enumerate() {
            let category = TaxCategory::parse(&l.tax_category).unwrap_or(TaxCategory::Taxable);
            let code = l
                .hsn_sac
                .as_deref()
                .map(str::trim)
                .filter(|c| !c.is_empty());
            if code.is_none() {
                exceptions.push(link(
                    "error",
                    "missing_hsn",
                    format!(
                        "Line {} of {} {number} has no HSN/SAC code; table 12 needs one.",
                        i + 1,
                        doc_word(d)
                    ),
                ));
            }
            points.extend(
                line_warnings(
                    pack,
                    rate_date,
                    i,
                    LineFacts {
                        hsn: code,
                        category,
                        rate_bp: u32::try_from(l.gst_rate_bp).unwrap_or(0),
                    },
                )
                .into_iter()
                .map(|w| w.message),
            );

            // Table 8 (not for exports, which carry every line in table 6A).
            if placement.nil_table && recipient != Recipient::Overseas {
                let as_exempt = d.doc_type == "bill_of_supply" && category == TaxCategory::Taxable;
                if category != TaxCategory::Taxable || as_exempt {
                    let supply = match (registered, intra) {
                        (true, false) => NilSupply::InterRegistered,
                        (true, true) => NilSupply::IntraRegistered,
                        (false, false) => NilSupply::InterUnregistered,
                        (false, true) => NilSupply::IntraUnregistered,
                    };
                    let n = nil.entry(supply).or_insert(NilRow {
                        supply,
                        nil_paise: 0,
                        exempt_paise: 0,
                        non_gst_paise: 0,
                    });
                    let amount = sign * l.taxable_paise;
                    match category {
                        TaxCategory::NilRated => n.nil_paise += amount,
                        TaxCategory::NonGst => n.non_gst_paise += amount,
                        _ => n.exempt_paise += amount,
                    }
                }
            }

            // Table 12: every line with a code.
            if let Some(code) = code {
                let service = code.starts_with("99");
                let unit = l
                    .unit
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or_default()
                    .to_uppercase();
                let uqc = if service {
                    "NA".to_owned()
                } else if is_valid_uqc(&unit) {
                    unit
                } else {
                    "OTH".to_owned()
                };
                let rate = if l.tax_category == "taxable" && d.doc_type != "bill_of_supply" {
                    u32::try_from(l.gst_rate_bp).unwrap_or(0)
                } else {
                    0
                };
                let group = if registered { "b2b" } else { "b2c" };
                let h = hsn
                    .entry((group, code.to_owned(), uqc, rate))
                    .or_insert_with(|| HsnAcc {
                        description: pack
                            .hsn_rule(code)
                            .map_or_else(|| l.description.clone(), |r| r.description.clone()),
                        ..Default::default()
                    });
                if !service {
                    let q = Decimal::from_str(&l.quantity).unwrap_or_default();
                    h.quantity += if credit { -q } else { q };
                }
                h.item.rate_bp = rate;
                h.item.taxable_paise += sign * l.taxable_paise;
                h.item.igst_paise += sign * l.igst_paise;
                h.item.cgst_paise += sign * l.cgst_paise;
                h.item.sgst_paise += sign * (l.sgst_paise + l.utgst_paise);
                h.item.cess_paise += sign * l.cess_paise;
            }
        }
        if !points.is_empty() {
            let more = points.len() - 1;
            let mut message = format!("{} {number}: {}", doc_word(d), points[0]);
            if more > 0 {
                message.push_str(&format!(" (+{more} more)"));
            }
            exceptions.push(link("warning", "tax_check", message));
        }

        let Some(section) = placement.section else {
            continue;
        };
        let date = ymd(&d.doc_date);
        let pos = u8::try_from(d.place_of_supply).unwrap_or(0);
        let doc_items = if matches!(section, Gstr1Section::ExpWpay | Gstr1Section::ExpWopay) {
            // Exports report every line; zero-rated lines at the line's rate.
            let mut by_rate: BTreeMap<i64, RateItem> = BTreeMap::new();
            for l in lines {
                let e = by_rate.entry(l.gst_rate_bp).or_insert_with(|| RateItem {
                    rate_bp: u32::try_from(l.gst_rate_bp).unwrap_or(0),
                    ..Default::default()
                });
                e.taxable_paise += l.taxable_paise;
                e.igst_paise += l.igst_paise;
                e.cess_paise += l.cess_paise;
            }
            by_rate.into_values().collect()
        } else {
            items
        };

        if section == Gstr1Section::B2cs {
            for i in &doc_items {
                let (item, docs) = b2cs
                    .entry((d.place_of_supply, i.rate_bp, intra))
                    .or_insert_with(|| {
                        (
                            RateItem {
                                rate_bp: i.rate_bp,
                                ..Default::default()
                            },
                            Vec::new(),
                        )
                    });
                item.taxable_paise += sign * i.taxable_paise;
                item.igst_paise += sign * i.igst_paise;
                item.cgst_paise += sign * i.cgst_paise;
                item.sgst_paise += sign * i.sgst_paise;
                item.cess_paise += sign * i.cess_paise;
                if !docs.iter().any(|x| x.id == d.id) {
                    docs.push(DocLink {
                        id: d.id.clone(),
                        number: number.clone(),
                        doc_type: d.doc_type.clone(),
                    });
                }
            }
            continue;
        }

        let ctin = registered.then(|| d.party_gstin.clone()).flatten();
        rows.push(ReturnDocRow {
            id: d.id.clone(),
            section: section_key(section),
            doc_type: d.doc_type.clone(),
            number: number.clone(),
            doc_date: d.doc_date.clone(),
            party_name: d.party_name.clone(),
            gstin: ctin.clone(),
            place_of_supply: d.place_of_supply,
            place_name: state_name(d.place_of_supply),
            kind_label: kind_label(section, inv_type),
            reverse_charge: d.reverse_charge,
            value_paise: d.total_paise,
            totals: totals_of(&doc_items),
            original_number: original.and_then(|o| o.number.clone()),
            original_id: original.map(|o| o.id.clone()),
        });
        match section {
            Gstr1Section::Cdnr | Gstr1Section::Cdnur => {
                let unregistered_type = (section == Gstr1Section::Cdnur).then(|| {
                    match original.map(|o| gstr1_of(o, &[], None).section) {
                        Some(Some(Gstr1Section::ExpWpay)) => UnregisteredType::ExportWithPayment,
                        Some(Some(Gstr1Section::ExpWopay)) => {
                            UnregisteredType::ExportWithoutPayment
                        }
                        _ => UnregisteredType::B2cl,
                    }
                });
                let target = if section == Gstr1Section::Cdnr {
                    &mut portal.cdnr
                } else {
                    &mut portal.cdnur
                };
                target.push(Note {
                    ctin,
                    note_type: NoteType::Credit,
                    number: number.clone(),
                    date,
                    value_paise: d.total_paise,
                    pos,
                    reverse_charge: d.reverse_charge,
                    intra_state: intra,
                    invoice_type: inv_type,
                    unregistered_type,
                    items: doc_items,
                });
            }
            _ => {
                let export = matches!(section, Gstr1Section::ExpWpay | Gstr1Section::ExpWopay);
                exports |= export;
                let inv = Invoice {
                    ctin,
                    number: number.clone(),
                    date,
                    value_paise: d.total_paise,
                    pos,
                    reverse_charge: d.reverse_charge,
                    intra_state: intra,
                    invoice_type: inv_type,
                    export_with_payment: section == Gstr1Section::ExpWpay,
                    items: doc_items,
                };
                match section {
                    Gstr1Section::B2cl => portal.b2cl.push(inv),
                    _ if export => portal.exp.push(inv),
                    _ => portal.b2b.push(inv),
                }
            }
        }
    }

    if exports {
        exceptions.push(ReturnException {
            severity: "warning",
            code: "shipping_bill",
            message: "Exports of goods need the shipping bill number, date and port code. AccountsVU doesn't record them yet; add them on the portal before filing.".into(),
            document_id: None,
            number: None,
        });
    }
    for (group, code, uqc, rate) in hsn.keys() {
        if uqc == "OTH" {
            exceptions.push(ReturnException {
                severity: "warning",
                code: "uqc",
                message: format!(
                    "HSN {code} at {}% ({group}) has lines without a standard unit, so it is reported in OTH (others).",
                    av_gst_formats::percent(*rate)
                ),
                document_id: None,
                number: None,
            });
        }
    }

    // Assemble the tables.
    portal.b2cs = b2cs
        .iter()
        .filter(|(_, (i, _))| {
            *i != RateItem {
                rate_bp: i.rate_bp,
                ..Default::default()
            }
        })
        .map(|((pos, _, intra), (item, _))| B2csRow {
            pos: u8::try_from(*pos).unwrap_or(0),
            intra_state: *intra,
            item: *item,
        })
        .collect();
    portal.nil = nil.values().copied().collect();
    for ((group, code, uqc, _), h) in &hsn {
        let row = HsnRow {
            hsn: code.clone(),
            description: h.description.clone(),
            uqc: uqc.clone(),
            quantity: h.quantity.normalize().to_string(),
            item: h.item,
        };
        if *group == "b2b" {
            portal.hsn_b2b.push(row);
        } else {
            portal.hsn_b2c.push(row);
        }
    }
    let mut series_view = Vec::new();
    for ((nature, _), mut s) in series {
        s.numbers.sort();
        let total = u32::try_from(s.numbers.len()).unwrap_or(u32::MAX);
        let from = s.numbers.first().map(|n| n.1.clone()).unwrap_or_default();
        let to = s.numbers.last().map(|n| n.1.clone()).unwrap_or_default();
        series_view.push(DocSeriesView {
            label: match nature {
                DocumentNature::CreditNotes => "Credit notes",
                DocumentNature::DebitNotes => "Debit notes",
                DocumentNature::OutwardInvoices => "Invoices for outward supply",
            },
            from: from.clone(),
            to: to.clone(),
            total,
            cancelled: s.cancelled,
            net: total.saturating_sub(s.cancelled),
        });
        portal.documents.push(DocumentSeries {
            nature,
            from,
            to,
            total,
            cancelled: s.cancelled,
        });
    }

    // Section summaries and the net liability.
    let sum_rows = |key: &str| {
        let mut t = TaxTotals::default();
        let mut value = 0;
        let mut count = 0;
        for r in rows.iter().filter(|r| r.section == key) {
            t.taxable_paise += r.totals.taxable_paise;
            t.igst_paise += r.totals.igst_paise;
            t.cgst_paise += r.totals.cgst_paise;
            t.sgst_paise += r.totals.sgst_paise;
            t.cess_paise += r.totals.cess_paise;
            t.tax_paise += r.totals.tax_paise;
            value += r.value_paise;
            count += 1;
        }
        (count, t, value)
    };
    let mut sections = Vec::new();
    let mut net = TaxTotals::default();
    for (key, title, table) in [
        ("b2b", "B2B, SEZ and deemed exports", "4A, 4B, 6B, 6C"),
        ("b2cl", "B2C large", "5"),
        ("exp", "Exports", "6A"),
    ] {
        let (count, totals, value) = sum_rows(key);
        net = add_totals(net, totals, 1);
        sections.push(SectionSummary {
            key,
            title,
            table,
            count,
            totals,
            value_paise: value,
        });
    }
    let b2cs_totals = b2cs.values().fold(TaxTotals::default(), |mut t, (i, _)| {
        t.add(i, 1);
        t
    });
    net = add_totals(net, b2cs_totals, 1);
    sections.push(SectionSummary {
        key: "b2cs",
        title: "B2C small",
        table: "7",
        count: b2cs.values().map(|(_, d)| d.len()).sum(),
        totals: b2cs_totals,
        value_paise: 0,
    });
    let nil_total: i64 = portal
        .nil
        .iter()
        .map(|n| n.nil_paise + n.exempt_paise + n.non_gst_paise)
        .sum();
    sections.push(SectionSummary {
        key: "nil",
        title: "Nil-rated, exempt and non-GST",
        table: "8",
        count: portal.nil.len(),
        totals: TaxTotals::default(),
        value_paise: nil_total,
    });
    for (key, title) in [
        ("cdnr", "Credit notes, registered"),
        ("cdnur", "Credit notes, unregistered"),
    ] {
        let (count, totals, value) = sum_rows(key);
        net = add_totals(net, totals, -1);
        sections.push(SectionSummary {
            key,
            title,
            table: "9B",
            count,
            totals,
            value_paise: value,
        });
    }
    let hsn_totals = hsn.values().fold(TaxTotals::default(), |mut t, h| {
        t.add(&h.item, 1);
        t
    });
    sections.push(SectionSummary {
        key: "hsn",
        title: "HSN summary",
        table: "12",
        count: hsn.len(),
        totals: hsn_totals,
        value_paise: 0,
    });
    sections.push(SectionSummary {
        key: "docs",
        title: "Documents issued",
        table: "13",
        count: series_view.iter().map(|s| s.total as usize).sum(),
        totals: TaxTotals::default(),
        value_paise: 0,
    });

    // Tax in table 12 must match the tax in the other tables.
    if hsn_totals.tax_paise != net.tax_paise && exceptions.iter().all(|e| e.code != "missing_hsn") {
        exceptions.push(ReturnException {
            severity: "error",
            code: "hsn_mismatch",
            message: format!(
                "Tax in the HSN summary ({}) differs from the tax in the other tables ({}).",
                av_gst_formats::rupees(hsn_totals.tax_paise),
                av_gst_formats::rupees(net.tax_paise)
            ),
            document_id: None,
            number: None,
        });
    }
    exceptions.sort_by_key(|e| if e.severity == "error" { 0 } else { 1 });

    let view = Gstr1View {
        registration_id: reg.id.clone(),
        gstin: reg.gstin.clone(),
        state_name: state_name(reg.state_code),
        period: period.code(),
        label: period.label(),
        from,
        to,
        due_date: iso(period.gstr1_due()),
        status: period.status(today),
        rule_pack_version: pack.version.clone(),
        format_version: fmt::ADAPTER_VERSION,
        sections,
        totals: net,
        documents: rows,
        b2cs: b2cs
            .into_iter()
            .map(|((pos, rate, intra), (item, documents))| B2csView {
                place_of_supply: pos,
                place_name: state_name(pos),
                intra_state: intra,
                rate_bp: rate,
                totals: totals_of(&[item]),
                documents,
            })
            .collect(),
        nil: portal
            .nil
            .iter()
            .map(|n| NilView {
                supply: match n.supply {
                    NilSupply::InterRegistered => "inter_registered",
                    NilSupply::IntraRegistered => "intra_registered",
                    NilSupply::InterUnregistered => "inter_unregistered",
                    NilSupply::IntraUnregistered => "intra_unregistered",
                },
                label: match n.supply {
                    NilSupply::InterRegistered => "Inter-state to registered persons",
                    NilSupply::IntraRegistered => "Intra-state to registered persons",
                    NilSupply::InterUnregistered => "Inter-state to unregistered persons",
                    NilSupply::IntraUnregistered => "Intra-state to unregistered persons",
                },
                nil_paise: n.nil_paise,
                exempt_paise: n.exempt_paise,
                non_gst_paise: n.non_gst_paise,
            })
            .collect(),
        hsn: hsn
            .into_iter()
            .map(|((group, code, uqc, rate), h)| HsnView {
                group,
                hsn: code,
                description: h.description,
                uqc,
                quantity: h.quantity.normalize().to_string(),
                rate_bp: rate,
                totals: totals_of(&[h.item]),
            })
            .collect(),
        series: series_view,
        exceptions,
    };
    Built { view, portal }
}

pub(crate) fn add_totals(mut a: TaxTotals, b: TaxTotals, sign: i64) -> TaxTotals {
    a.taxable_paise += sign * b.taxable_paise;
    a.igst_paise += sign * b.igst_paise;
    a.cgst_paise += sign * b.cgst_paise;
    a.sgst_paise += sign * b.sgst_paise;
    a.cess_paise += sign * b.cess_paise;
    a.tax_paise += sign * b.tax_paise;
    a
}

fn doc_word(d: &SalesDocumentRecord) -> &'static str {
    match d.doc_type.as_str() {
        "credit_note" => "Credit note",
        "bill_of_supply" => "Bill of supply",
        _ => "Invoice",
    }
}

// ---------------------------------------------------------------------------
// Use cases
// ---------------------------------------------------------------------------

impl<S: Store> AppCore<S> {
    pub(crate) async fn load_return_docs(
        &self,
        uow: &mut S::Uow,
        org_id: &str,
    ) -> AppResult<Vec<Doc>> {
        let mut out = Vec::new();
        for d in uow.list_sales(org_id).await? {
            let lines = if d.status == "issued" {
                uow.sales_lines(&d.id).await?
            } else {
                Vec::new()
            };
            out.push((d, lines));
        }
        Ok(out)
    }

    pub(crate) async fn return_registration(
        &self,
        uow: &mut S::Uow,
        org_id: &str,
        id: Option<&str>,
    ) -> AppResult<(Vec<RegistrationRecord>, Option<RegistrationRecord>)> {
        let regs: Vec<RegistrationRecord> = uow
            .list_registrations(org_id)
            .await?
            .into_iter()
            .filter(|r| r.registration_type != "composition")
            .collect();
        let chosen = match id {
            Some(id) => Some(
                regs.iter()
                    .find(|r| r.id == id)
                    .cloned()
                    .ok_or_else(|| AppError::NotFound("GST registration"))?,
            ),
            None => regs
                .iter()
                .find(|r| r.status == "active")
                .or_else(|| regs.first())
                .cloned(),
        };
        Ok((regs, chosen))
    }

    /// Monthly GSTR-1 periods of one financial year for a GSTIN, newest first.
    pub async fn return_centre(
        &self,
        ctx: &RequestContext,
        registration_id: Option<&str>,
        financial_year: Option<&str>,
    ) -> AppResult<ReturnCentreView> {
        ctx.require(Permission::ViewBusiness)?;
        let today = self.today();
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let start_month = u32::try_from(org.fy_start_month).unwrap_or(4);
        let fy = match financial_year {
            Some(code) => {
                let y: i32 = code
                    .get(..4)
                    .and_then(|y| y.parse().ok())
                    .ok_or_else(|| AppError::Invalid("choose a financial year".into()))?;
                FinancialYear::new(y, start_month)
            }
            None => FinancialYear::containing(today, start_month),
        };
        let (regs, reg) = self
            .return_registration(&mut uow, &org.id, registration_id)
            .await?;
        let books_start = NaiveDate::parse_from_str(&org.books_start_date, "%Y-%m-%d")
            .unwrap_or_else(|_| fy.start_date());
        let first = Period::of(fy.start_date().max(books_start));
        let last = Period::of(fy.end_date().min(today));
        let mut periods = Vec::new();
        if let Some(reg) = &reg {
            let docs = self.load_return_docs(&mut uow, &org.id).await?;
            let purchases = self.load_return_purchases(&mut uow, &org.id).await?;
            let chain = Self::chain_start(&org.books_start_date, first.first());
            let series = crate::gstr3b::build_3b_series(reg, &docs, &purchases, chain, last, today);
            let recon = self.load_recon_data(&mut uow, &org.id, reg).await?;
            let mut p = first;
            while p.first() <= last.first() && p.first() <= fy.end_date() {
                let built = build(reg, &docs, p, today);
                let v = built.view;
                let three_b = series.iter().find(|x| x.period == v.period);
                let two_b = recon
                    .statements
                    .iter()
                    .any(|(s, _)| s.period == v.period)
                    .then(|| crate::gstr2b::reconcile_period(&recon, p));
                let in_period = docs.iter().filter(|(d, _)| {
                    d.gst_registration_id.as_deref() == Some(reg.id.as_str())
                        && d.doc_date >= v.from
                        && d.doc_date <= v.to
                });
                let (mut count, mut drafts) = (0, 0);
                for (d, _) in in_period {
                    if d.status == "draft" {
                        drafts += 1;
                    } else if d.status == "issued" {
                        count += 1;
                    }
                }
                periods.push(PeriodSummary {
                    period: v.period,
                    label: v.label,
                    from: v.from,
                    to: v.to,
                    gstr1_due: v.due_date,
                    status: v.status,
                    document_count: count,
                    draft_count: drafts,
                    error_count: v
                        .exceptions
                        .iter()
                        .filter(|e| e.severity == "error")
                        .count(),
                    warning_count: v
                        .exceptions
                        .iter()
                        .filter(|e| e.severity != "error")
                        .count(),
                    output_tax: v.totals,
                    gstr3b_due: iso(p.gstr3b_due()),
                    gstr3b_status: p.status_by(today, p.gstr3b_due()),
                    net_itc_paise: three_b.map_or(0, |x| x.itc_net.total()),
                    cash_payable_paise: three_b.map_or(0, |x| x.offset.cash_total),
                    gstr2b_imported: two_b.is_some(),
                    gstr2b_needs_review: two_b
                        .map_or(0, |r| r.rows.iter().filter(|r| r.needs_review).count()),
                });
                p = p.next();
            }
        }
        periods.reverse();
        let prev = FinancialYear::new(fy.start_year() - 1, start_month);
        Ok(ReturnCentreView {
            registrations: regs
                .iter()
                .map(|r| RegistrationOption {
                    id: r.id.clone(),
                    gstin: r.gstin.clone(),
                    state_name: state_name(r.state_code),
                    filing_frequency: r.filing_frequency.clone(),
                })
                .collect(),
            registration_id: reg.as_ref().map(|r| r.id.clone()),
            quarterly: reg
                .as_ref()
                .is_some_and(|r| r.filing_frequency == "quarterly"),
            financial_year: fy.code(),
            fy_start: iso(fy.start_date()),
            fy_end: iso(fy.end_date()),
            previous_year: (prev.end_date() >= books_start).then(|| prev.code()),
            next_year: (fy.next().start_date() <= today).then(|| fy.next().code()),
            periods,
        })
    }

    /// The GSTR-1 workpaper of a GSTIN for a month (`YYYY-MM`).
    pub async fn gstr1(
        &self,
        ctx: &RequestContext,
        registration_id: &str,
        period: &str,
    ) -> AppResult<Gstr1View> {
        ctx.require(Permission::ViewBusiness)?;
        let period = Period::parse(period)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let (_, reg) = self
            .return_registration(&mut uow, &org.id, Some(registration_id))
            .await?;
        let reg = reg.ok_or_else(|| AppError::NotFound("GST registration"))?;
        let docs = self.load_return_docs(&mut uow, &org.id).await?;
        Ok(build(&reg, &docs, period, self.today()).view)
    }

    /// The GSTR-1 as portal JSON. Recorded in the audit log.
    pub async fn gstr1_json(
        &self,
        ctx: &RequestContext,
        registration_id: &str,
        period: &str,
    ) -> AppResult<Gstr1File> {
        ctx.require(Permission::ViewBusiness)?;
        let period = Period::parse(period)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let (_, reg) = self
            .return_registration(&mut uow, &org.id, Some(registration_id))
            .await?;
        let reg = reg.ok_or_else(|| AppError::NotFound("GST registration"))?;
        let docs = self.load_return_docs(&mut uow, &org.id).await?;
        let built = build(&reg, &docs, period, self.today());
        let json = fmt::to_json(&built.portal);
        let error_count = built
            .view
            .exceptions
            .iter()
            .filter(|e| e.severity == "error")
            .count();
        let now = self.now();
        let after = serde_json::json!({
            "gstin": reg.gstin,
            "period": period.code(),
            "format": fmt::ADAPTER_VERSION,
            "rulePack": built.view.rule_pack_version,
            "sha256": sha256_hex(json.as_bytes()),
            "errors": error_count,
        });
        audit(
            &mut uow,
            ctx,
            &org.id,
            Change {
                action: "gstr1.exported",
                entity_id: Some(&reg.id),
                before: None,
                after: Some(after.to_string()),
            },
            &now,
        )
        .await?;
        uow.commit().await?;
        Ok(Gstr1File {
            file_name: format!(
                "GSTR1_{}_{:02}{}.json",
                reg.gstin, period.month, period.year
            ),
            json,
            error_count,
        })
    }
}
