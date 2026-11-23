//! Phase 2.3: the GSTR-3B workpaper — outward and reverse-charge liability
//! (3.1, 3.2), input tax credit (4) with reversals including the rule 42
//! common-credit proposal, exempt inward supplies (5), and a payment
//! proposal (6.1) that uses credit in the rule 88A order.
//!
//! Outward figures come from the same issued sales documents as GSTR-1, so
//! the two returns can be cross-checked line for line. ITC comes from the
//! review status of every taxed purchase line dated in the month: pending
//! lines are never claimed. Credit carried forward is chained month by month
//! from the books start date, as AccountsVU's estimate of the electronic
//! credit ledger.

use std::collections::{BTreeMap, HashMap};

use av_gst_engine::SupplyKind;
use av_gst_engine::checks::{Gstr1Section, Recipient};
use av_gst_engine::offset::{Heads, OffsetProposal, propose};
use av_persistence::records::{
    PurchaseDocumentRecord, PurchaseLineRecord, RegistrationRecord, SalesDocumentRecord,
};
use av_persistence::{PurchaseRepo, Store};
use chrono::NaiveDate;
use serde::Serialize;

use crate::AppCore;
use crate::context::{Permission, RequestContext};
use crate::error::{AppError, AppResult};
use crate::returns::{Period, ReturnException, ReturnSales, advance_rows, build, iso, state_name};
use crate::sales::{gstr1_of, kind_of, recipient_of};

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

/// One row of a GSTR-3B table.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub key: &'static str,
    pub label: &'static str,
    pub taxable_paise: i64,
    pub tax: Heads,
}

/// Table 3.2: inter-state supplies to unregistered and composition persons.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterStateRow {
    pub place_of_supply: i64,
    pub place_name: String,
    pub unregistered_taxable_paise: i64,
    pub unregistered_igst_paise: i64,
    pub composition_taxable_paise: i64,
    pub composition_igst_paise: i64,
}

/// Table 5: exempt, nil-rated and non-GST inward supplies.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InwardRow {
    pub key: &'static str,
    pub label: &'static str,
    pub inter_state_paise: i64,
    pub intra_state_paise: i64,
}

/// Rule 42 proposal for the month (provisional; trued up annually).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule42 {
    pub common_lines: usize,
    /// C2: eligible ITC on lines marked common.
    pub common_credit: Heads,
    /// E: exempt, nil-rated and non-GST outward supplies.
    pub exempt_turnover_paise: i64,
    /// F: total outward turnover (taxable value, without tax).
    pub total_turnover_paise: i64,
    /// D1 = C2 × E ÷ F, per head.
    pub reversal: Heads,
}

/// A purchase line as it enters table 4.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItcLine {
    pub document_id: String,
    pub doc_type: String,
    pub supplier_invoice: Option<String>,
    pub doc_date: String,
    pub party_name: String,
    pub gstin: Option<String>,
    pub description: String,
    /// `4A(2)`, `4A(3)`, `4A(5)` or `pending`.
    pub table: &'static str,
    pub status: String,
    pub reason: Option<String>,
    pub common: bool,
    pub reverse_charge: bool,
    pub tax: Heads,
    pub eligible: Heads,
    /// The document's month when the ITC was reviewed after that month's
    /// GSTR-3B was filed and is claimed here instead.
    pub moved_from: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossCheckRow {
    pub label: &'static str,
    pub gstr1_paise: i64,
    pub gstr3b_paise: i64,
    pub difference_paise: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gstr3bView {
    pub registration_id: String,
    pub gstin: String,
    pub state_name: String,
    pub period: String,
    pub label: String,
    pub from: String,
    pub to: String,
    pub due_date: String,
    pub status: &'static str,
    /// 3.1 (a)–(e).
    pub outward: Vec<Row>,
    pub inter_state: Vec<InterStateRow>,
    /// 4A (1)–(5).
    pub itc_available: Vec<Row>,
    /// 4B (1)–(2).
    pub itc_reversed: Vec<Row>,
    /// 4C.
    pub itc_net: Heads,
    /// 4D (1)–(2).
    pub itc_other: Vec<Row>,
    pub inward_exempt: Vec<InwardRow>,
    pub rule42: Rule42,
    /// Forward-charge liability: 3.1 (a) + (b).
    pub liability: Heads,
    /// Reverse-charge liability: 3.1 (d), paid in cash.
    pub reverse_charge: Heads,
    pub credit_brought_forward: Heads,
    /// Credit brought forward plus net ITC (negative heads moved to liability).
    pub credit_available: Heads,
    pub offset: OffsetProposal,
    pub cross_check: Vec<CrossCheckRow>,
    pub itc_lines: Vec<ItcLine>,
    pub pending_itc: Heads,
    pub exceptions: Vec<ReturnException>,
}

// ---------------------------------------------------------------------------
// Building
// ---------------------------------------------------------------------------

/// A workpaper file ready to save.
#[derive(Clone, Debug)]
pub struct WorkpaperFile {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

pub(crate) type Purchase = (PurchaseDocumentRecord, Vec<PurchaseLineRecord>);

fn heads_of_sales(l: &av_persistence::records::SalesLineRecord) -> Heads {
    Heads {
        igst: l.igst_paise,
        cgst: l.cgst_paise,
        sgst: l.sgst_paise + l.utgst_paise,
        cess: l.cess_paise,
    }
}

pub(crate) fn heads_of(l: &PurchaseLineRecord) -> Heads {
    Heads {
        igst: l.igst_paise,
        cgst: l.cgst_paise,
        sgst: l.sgst_paise + l.utgst_paise,
        cess: l.cess_paise,
    }
}

fn scale(h: Heads, sign: i64) -> Heads {
    Heads {
        igst: sign * h.igst,
        cgst: sign * h.cgst,
        sgst: sign * h.sgst,
        cess: sign * h.cess,
    }
}

/// `part` of `h` split across the heads in proportion, the last non-zero
/// head taking the rounding (as the ITC posting does).
pub(crate) fn split(h: Heads, part: i64) -> Heads {
    let total = h.total();
    if total == 0 {
        return Heads::default();
    }
    let part = part.clamp(0, total);
    let vals = [h.igst, h.cgst, h.sgst, h.cess];
    let last = vals.iter().rposition(|v| *v != 0).unwrap_or(0);
    let mut out = [0i64; 4];
    let mut given = 0;
    for (i, v) in vals.iter().enumerate() {
        out[i] = if i == last {
            part - given
        } else {
            v * part / total
        };
        given += out[i];
    }
    Heads {
        igst: out[0],
        cgst: out[1],
        sgst: out[2],
        cess: out[3],
    }
}

/// `h × num ÷ den`, rounded half up per head.
fn ratio(h: Heads, num: i64, den: i64) -> Heads {
    if den <= 0 || num <= 0 {
        return Heads::default();
    }
    let f = |v: i64| {
        let x = i128::from(v) * i128::from(num.min(den));
        let d = i128::from(den);
        i64::try_from((x + d / 2) / d).unwrap_or(0)
    };
    Heads {
        igst: f(h.igst),
        cgst: f(h.cgst),
        sgst: f(h.sgst),
        cess: f(h.cess),
    }
}

fn add_row(rows: &mut [Row], key: &str, taxable: i64, tax: Heads) {
    if let Some(r) = rows.iter_mut().find(|r| r.key == key) {
        r.taxable_paise += taxable;
        r.tax = r.tax.plus(tax);
    }
}

fn row(key: &'static str, label: &'static str) -> Row {
    Row {
        key,
        label,
        ..Default::default()
    }
}

fn exception(severity: &'static str, code: &'static str, message: String) -> ReturnException {
    ReturnException {
        severity,
        code,
        message,
        document_id: None,
        number: None,
    }
}

fn rupees(p: i64) -> String {
    av_domain::Money::from_paise(p).to_string()
}

/// Whether a sales document (or, for a credit note, its invoice) is a
/// zero-rated supply: an export or a supply to an SEZ.
fn zero_rated(d: &SalesDocumentRecord, original: Option<&SalesDocumentRecord>) -> bool {
    let target = original.unwrap_or(d);
    matches!(
        gstr1_of(target, &[], None).section,
        Some(
            Gstr1Section::ExpWpay
                | Gstr1Section::ExpWopay
                | Gstr1Section::SezWpay
                | Gstr1Section::SezWopay
        )
    )
}

pub(crate) fn build_3b(
    reg: &RegistrationRecord,
    sales: &ReturnSales,
    purchases: &[Purchase],
    period: Period,
    brought_forward: Heads,
    today: NaiveDate,
) -> Gstr3bView {
    let (from, to) = (iso(period.first()), iso(period.last()));
    let ours = |id: &Option<String>, date: &str| {
        id.as_deref() == Some(reg.id.as_str()) && date >= from.as_str() && date <= to.as_str()
    };
    let by_id: HashMap<&str, &SalesDocumentRecord> =
        sales.iter().map(|(d, _)| (d.id.as_str(), d)).collect();
    let mut exceptions = Vec::new();

    // 3.1 and 3.2 from issued sales.
    let mut outward = vec![
        row(
            "a",
            "(a) Outward taxable supplies (other than zero-rated, nil-rated and exempted)",
        ),
        row("b", "(b) Outward taxable supplies (zero-rated)"),
        row("c", "(c) Other outward supplies (nil-rated, exempted)"),
        row("d", "(d) Inward supplies (liable to reverse charge)"),
        row("e", "(e) Non-GST outward supplies"),
    ];
    // Tax on advances (GSTR-1 11A less 11B) is part of 3.1(a).
    for r in advance_rows(reg, &sales.advances, period) {
        let sign = if r.table == "11A" { 1 } else { -1 };
        add_row(
            &mut outward,
            "a",
            sign * r.totals.taxable_paise,
            scale(
                Heads {
                    igst: r.totals.igst_paise,
                    cgst: r.totals.cgst_paise,
                    sgst: r.totals.sgst_paise,
                    cess: r.totals.cess_paise,
                },
                sign,
            ),
        );
    }
    let mut inter: BTreeMap<i64, InterStateRow> = BTreeMap::new();
    for (d, lines) in sales
        .iter()
        .filter(|(d, _)| d.status == "issued" && ours(&d.gst_registration_id, &d.doc_date))
    {
        let original = d
            .original_document_id
            .as_deref()
            .and_then(|id| by_id.get(id).copied());
        let sign = if d.doc_type == "credit_note" { -1 } else { 1 };
        let zero = zero_rated(d, original);
        let recipient = recipient_of(d);
        let composition = d.party_gst_type == "composition";
        let inter_state = kind_of(d) == SupplyKind::InterState;
        for l in lines {
            let taxable = sign * l.taxable_paise;
            if l.tax_category == "taxable" && d.doc_type != "bill_of_supply" {
                let tax = scale(heads_of_sales(l), sign);
                add_row(&mut outward, if zero { "b" } else { "a" }, taxable, tax);
                if !zero && inter_state && (recipient == Recipient::Unregistered || composition) {
                    let r = inter
                        .entry(d.place_of_supply)
                        .or_insert_with(|| InterStateRow {
                            place_of_supply: d.place_of_supply,
                            place_name: state_name(d.place_of_supply),
                            ..Default::default()
                        });
                    if composition {
                        r.composition_taxable_paise += taxable;
                        r.composition_igst_paise += tax.igst;
                    } else {
                        r.unregistered_taxable_paise += taxable;
                        r.unregistered_igst_paise += tax.igst;
                    }
                }
            } else if l.tax_category == "non_gst" {
                add_row(&mut outward, "e", taxable, Heads::default());
            } else {
                add_row(&mut outward, "c", taxable, Heads::default());
            }
        }
    }

    // Table 4, 3.1(d) and table 5 from recorded purchases.
    let mut available = vec![
        row("1", "(1) Import of goods"),
        row("2", "(2) Import of services"),
        row(
            "3",
            "(3) Inward supplies liable to reverse charge (other than 1 & 2)",
        ),
        row("4", "(4) Inward supplies from ISD"),
        row("5", "(5) All other ITC"),
    ];
    let mut reversed = vec![
        row(
            "1",
            "(1) As per rules 38, 42 & 43 of CGST Rules and section 17(5)",
        ),
        row("2", "(2) Others"),
    ];
    let itc_other = vec![
        row(
            "1",
            "(1) ITC reclaimed which was reversed under 4B(2) earlier",
        ),
        row(
            "2",
            "(2) Ineligible ITC under section 16(4) & ITC restricted due to PoS rules",
        ),
    ];
    let mut inward = [
        InwardRow {
            key: "exempt",
            label: "From a supplier under composition scheme, exempt and nil-rated supply",
            ..Default::default()
        },
        InwardRow {
            key: "non_gst",
            label: "Non-GST supply",
            ..Default::default()
        },
    ];
    let mut itc_lines = Vec::new();
    let mut pending = Heads::default();
    let mut pending_lines = 0;
    let mut common = Heads::default();
    let mut common_lines = 0;
    let mut unclaimable = 0;
    let code = period.code();
    for (d, lines) in purchases.iter().filter(|(d, _)| {
        d.status == "recorded" && d.gst_registration_id.as_deref() == Some(reg.id.as_str())
    }) {
        let in_month = ours(&d.gst_registration_id, &d.doc_date);
        let doc_month = d.doc_date.get(..7).unwrap_or_default();
        // ITC reviewed after the month's GSTR-3B was filed is claimed later.
        if !in_month
            && !lines
                .iter()
                .any(|l| l.itc_period.as_deref() == Some(code.as_str()))
        {
            continue;
        }
        let sign = if d.doc_type == "debit_note" { -1 } else { 1 };
        let gst_type = d.party_gst_type.as_deref().unwrap_or("unregistered");
        let overseas = gst_type == "overseas";
        let has_gstin = d.party_gstin.is_some();
        let inter_state = d.supply_kind == "inter_state";
        for l in lines {
            let tax = heads_of(l);
            let claim_month = l.itc_period.as_deref().unwrap_or(doc_month);
            if !in_month {
                if claim_month != code {
                    continue;
                }
            } else if d.reverse_charge && l.tax_category == "taxable" {
                add_row(&mut outward, "d", sign * l.taxable_paise, scale(tax, sign));
            }
            let exempt_col = match l.tax_category.as_str() {
                "non_gst" => Some(1),
                "nil_rated" | "exempt" => Some(0),
                _ if gst_type == "composition" && tax.total() == 0 => Some(0),
                _ => None,
            };
            if let Some(c) = exempt_col.filter(|_| in_month) {
                let amount = sign * l.taxable_paise;
                if inter_state {
                    inward[c].inter_state_paise += amount;
                } else {
                    inward[c].intra_state_paise += amount;
                }
            }
            if tax.total() == 0 || l.itc_status == "na" || claim_month != code {
                continue;
            }
            if !has_gstin && !d.reverse_charge {
                unclaimable += 1;
                continue;
            }
            let table = if d.reverse_charge && overseas {
                "2"
            } else if d.reverse_charge {
                "3"
            } else {
                "5"
            };
            let eligible = match l.itc_status.as_str() {
                "eligible" => tax,
                "partial" => split(tax, l.itc_eligible_paise),
                _ => Heads::default(),
            };
            let label = if l.itc_status == "pending" {
                pending = pending.plus(scale(tax, sign));
                pending_lines += 1;
                "pending"
            } else {
                add_row(&mut available, table, 0, scale(tax, sign));
                add_row(&mut reversed, "1", 0, scale(tax.minus(eligible), sign));
                if l.itc_common {
                    common = common.plus(scale(eligible, sign));
                    common_lines += 1;
                }
                match table {
                    "2" => "4A(2)",
                    "3" => "4A(3)",
                    _ => "4A(5)",
                }
            };
            itc_lines.push(ItcLine {
                document_id: d.id.clone(),
                doc_type: d.doc_type.clone(),
                supplier_invoice: d
                    .supplier_invoice_number
                    .clone()
                    .or_else(|| d.number.clone()),
                doc_date: d.doc_date.clone(),
                party_name: d.party_name.clone().unwrap_or_default(),
                gstin: d.party_gstin.clone(),
                description: l.description.clone(),
                table: label,
                status: l.itc_status.clone(),
                reason: l.itc_reason.clone(),
                common: l.itc_common,
                reverse_charge: d.reverse_charge,
                tax: scale(tax, sign),
                eligible: scale(eligible, sign),
                moved_from: (claim_month != doc_month).then(|| doc_month.to_owned()),
            });
        }
    }

    // Rule 42: reverse the exempt share of common credit.
    let total_of = |k: &str| {
        outward
            .iter()
            .find(|r| r.key == k)
            .map_or(0, |r| r.taxable_paise)
    };
    let exempt_turnover = total_of("c") + total_of("e");
    let total_turnover = total_of("a") + total_of("b") + exempt_turnover;
    let rule42_reversal = ratio(common.floor(), exempt_turnover, total_turnover);
    if rule42_reversal.total() != 0 {
        add_row(&mut reversed, "1", 0, rule42_reversal);
    }
    let rule42 = Rule42 {
        common_lines,
        common_credit: common,
        exempt_turnover_paise: exempt_turnover,
        total_turnover_paise: total_turnover,
        reversal: rule42_reversal,
    };

    let gross = available
        .iter()
        .fold(Heads::default(), |h, r| h.plus(r.tax));
    let rev = reversed.iter().fold(Heads::default(), |h, r| h.plus(r.tax));
    let net = gross.minus(rev);

    // 6.1: liability and credit. A head whose credit runs negative (more
    // reversed than available, with what was brought forward) adds to the
    // liability instead.
    let tax_of = |k: &str| {
        outward
            .iter()
            .find(|r| r.key == k)
            .map_or(Heads::default(), |r| r.tax)
    };
    let mut liability = tax_of("a").plus(tax_of("b"));
    let reverse_charge = tax_of("d");
    let mut credit = brought_forward.plus(net);
    let negative = Heads::default().minus(credit).floor();
    liability = liability.plus(negative);
    credit = credit.floor();
    let offset = propose(liability, credit, reverse_charge);

    // Cross-check against GSTR-1 (same documents, built independently).
    let g1 = build(reg, sales, period, today).view;
    let nil_g1: i64 = g1
        .nil
        .iter()
        .map(|n| n.nil_paise + n.exempt_paise + n.non_gst_paise)
        .sum();
    let ab = |f: fn(&Row) -> i64| f(&outward[0]) + f(&outward[1]);
    let cross: Vec<CrossCheckRow> = [
        (
            "Taxable value (3.1 a + b)",
            g1.totals.taxable_paise + g1.advance_totals.taxable_paise,
            ab(|r| r.taxable_paise),
        ),
        (
            "IGST",
            g1.totals.igst_paise + g1.advance_totals.igst_paise,
            ab(|r| r.tax.igst),
        ),
        (
            "CGST",
            g1.totals.cgst_paise + g1.advance_totals.cgst_paise,
            ab(|r| r.tax.cgst),
        ),
        (
            "SGST/UTGST",
            g1.totals.sgst_paise + g1.advance_totals.sgst_paise,
            ab(|r| r.tax.sgst),
        ),
        (
            "Cess",
            g1.totals.cess_paise + g1.advance_totals.cess_paise,
            ab(|r| r.tax.cess),
        ),
        (
            "Nil-rated, exempt and non-GST (3.1 c + e)",
            nil_g1,
            exempt_turnover,
        ),
    ]
    .into_iter()
    .map(|(label, a, b)| CrossCheckRow {
        label,
        gstr1_paise: a,
        gstr3b_paise: b,
        difference_paise: b - a,
    })
    .collect();
    for c in &cross {
        if c.difference_paise != 0 {
            let tax_row = !c.label.starts_with("Nil");
            exceptions.push(exception(
                if tax_row { "error" } else { "warning" },
                if tax_row {
                    "gstr1_mismatch"
                } else {
                    "gstr1_nil_mismatch"
                },
                format!(
                    "{}: GSTR-1 shows {}, GSTR-3B {} (difference {}).{}",
                    c.label,
                    rupees(c.gstr1_paise),
                    rupees(c.gstr3b_paise),
                    rupees(c.difference_paise),
                    if tax_row {
                        ""
                    } else {
                        " Exempt lines on export invoices are reported in table 6A of GSTR-1."
                    }
                ),
            ));
        }
    }
    let g1_errors = g1
        .exceptions
        .iter()
        .filter(|e| e.severity == "error")
        .count();
    if g1_errors > 0 {
        exceptions.push(exception(
            "warning",
            "gstr1_errors",
            format!("GSTR-1 for this month has {g1_errors} to fix; outward figures here follow the same documents."),
        ));
    }
    if pending_lines > 0 {
        exceptions.push(exception(
            "warning",
            "itc_pending",
            format!(
                "{pending_lines} purchase {} with {} of GST {} awaiting ITC review, so not claimed. Review them in Purchases to claim.",
                if pending_lines == 1 { "line" } else { "lines" },
                rupees(pending.total()),
                if pending_lines == 1 { "is" } else { "are" }
            ),
        ));
    }
    if unclaimable > 0 {
        exceptions.push(exception(
            "warning",
            "itc_unregistered",
            format!("{unclaimable} purchase lines carry GST from a supplier without a GSTIN; they are left out of table 4."),
        ));
    }
    let drafts = purchases
        .iter()
        .filter(|(d, _)| d.status == "draft" && ours(&d.gst_registration_id, &d.doc_date))
        .count();
    if drafts > 0 {
        exceptions.push(exception(
            "warning",
            "purchase_draft",
            format!(
                "{drafts} purchase {} dated this month {} not recorded, so {} not in this return.",
                if drafts == 1 { "draft" } else { "drafts" },
                if drafts == 1 { "is" } else { "are" },
                if drafts == 1 { "it is" } else { "they are" }
            ),
        ));
    }
    if rule42_reversal.total() > 0 {
        exceptions.push(exception(
            "warning",
            "rule42",
            format!(
                "Rule 42: {} of common credit is reversed in 4B(1) for exempt supplies ({} of {} turnover). This is the monthly figure; it is trued up for the year.",
                rupees(rule42_reversal.total()),
                rupees(exempt_turnover),
                rupees(total_turnover)
            ),
        ));
    }
    if negative.total() > 0 {
        exceptions.push(exception(
            "warning",
            "negative_itc",
            format!(
                "More credit is reversed than is available; {} is added to the tax payable.",
                rupees(negative.total())
            ),
        ));
    }
    exceptions.sort_by_key(|e| if e.severity == "error" { 0 } else { 1 });

    Gstr3bView {
        registration_id: reg.id.clone(),
        gstin: reg.gstin.clone(),
        state_name: state_name(reg.state_code),
        period: period.code(),
        label: period.label(),
        from,
        to,
        due_date: iso(period.gstr3b_due()),
        status: period.status_by(today, period.gstr3b_due()),
        outward,
        inter_state: inter.into_values().collect(),
        itc_available: available,
        itc_reversed: reversed,
        itc_net: net,
        itc_other,
        inward_exempt: inward.to_vec(),
        rule42,
        liability,
        reverse_charge,
        credit_brought_forward: brought_forward,
        credit_available: credit,
        offset,
        cross_check: cross,
        itc_lines,
        pending_itc: pending,
        exceptions,
    }
}

/// GSTR-3B for each month from `first` to `last`, chaining the credit
/// carried forward from one month to the next.
pub(crate) fn build_3b_series(
    reg: &RegistrationRecord,
    sales: &ReturnSales,
    purchases: &[Purchase],
    first: Period,
    last: Period,
    today: NaiveDate,
) -> Vec<Gstr3bView> {
    let mut out = Vec::new();
    let mut carry = Heads::default();
    let mut p = first;
    while p.first() <= last.first() {
        let v = build_3b(reg, sales, purchases, p, carry, today);
        carry = v.offset.carry_forward;
        out.push(v);
        p = p.next();
    }
    out
}

// ---------------------------------------------------------------------------
// Use cases
// ---------------------------------------------------------------------------

impl<S: Store> AppCore<S> {
    pub(crate) async fn load_return_purchases(
        &self,
        uow: &mut S::Uow,
        org_id: &str,
    ) -> AppResult<Vec<Purchase>> {
        let mut out = Vec::new();
        for d in uow.list_purchases(org_id).await? {
            let lines = if d.status == "recorded" {
                uow.purchase_lines(&d.id).await?
            } else {
                Vec::new()
            };
            out.push((d, lines));
        }
        Ok(out)
    }

    /// First month the credit chain starts from: the books start date.
    pub(crate) fn chain_start(books_start: &str, fallback: NaiveDate) -> Period {
        Period::of(NaiveDate::parse_from_str(books_start, "%Y-%m-%d").unwrap_or(fallback))
    }

    /// The GSTR-3B workpaper as an Excel workbook.
    pub async fn gstr3b_xlsx(
        &self,
        ctx: &RequestContext,
        registration_id: &str,
        period: &str,
    ) -> AppResult<WorkpaperFile> {
        let v = self.gstr3b(ctx, registration_id, period).await?;
        let bytes = crate::workpaper_xlsx::gstr3b_workbook(&v)
            .map_err(|e| AppError::Invalid(format!("couldn't build the workbook: {e}")))?;
        let (year, month) = v.period.split_once('-').unwrap_or_default();
        Ok(WorkpaperFile {
            file_name: format!("GSTR3B_workpaper_{}_{month}{year}.xlsx", v.gstin),
            bytes,
        })
    }

    /// The GSTR-3B workpaper of a GSTIN for a month (`YYYY-MM`).
    pub async fn gstr3b(
        &self,
        ctx: &RequestContext,
        registration_id: &str,
        period: &str,
    ) -> AppResult<Gstr3bView> {
        ctx.require(Permission::ViewBusiness)?;
        let period = Period::parse(period)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let (_, reg) = self
            .return_registration(&mut uow, &org.id, Some(registration_id))
            .await?;
        let reg = reg.ok_or(AppError::NotFound("GST registration"))?;
        let sales = self.load_return_docs(&mut uow, &org.id).await?;
        let purchases = self.load_return_purchases(&mut uow, &org.id).await?;
        let first = Self::chain_start(&org.books_start_date, period.first());
        let first = if first.first() > period.first() {
            period
        } else {
            first
        };
        let series = build_3b_series(&reg, &sales, &purchases, first, period, self.today());
        series
            .into_iter()
            .last()
            .ok_or(AppError::NotFound("return period"))
    }
}
