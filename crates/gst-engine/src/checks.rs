//! Tax review of a document against the rule pack (spec §5.6): warnings the
//! user should look at, and where an outward document lands in GSTR-1.
//!
//! Warnings never block saving. Blocking checks (an unknown rate, a missing
//! HSN/SAC on a registered business) stay in the use cases.

use av_domain::{StateCode, TaxCategory};
use chrono::NaiveDate;
use serde::Serialize;

use crate::SupplyKind;
use crate::rules::{RateStatus, RulePack, short_date};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaxWarning {
    pub code: &'static str,
    /// The input it is about, e.g. `lines.2.gstRate`, or `placeOfSupply`.
    pub field: String,
    pub message: String,
}

/// One document line as the checks see it.
#[derive(Clone, Copy, Debug)]
pub struct LineFacts<'a> {
    pub hsn: Option<&'a str>,
    pub category: TaxCategory,
    pub rate_bp: u32,
}

fn pct(pack: &RulePack, bp: u32) -> String {
    pack.rate_rule(bp)
        .map(|r| r.label.clone())
        .unwrap_or_else(|| format!("{}%", f64_free_percent(bp)))
}

/// Basis points as a percentage string without floating point.
fn f64_free_percent(bp: u32) -> String {
    let (w, f) = (bp / 100, bp % 100);
    match f {
        0 => w.to_string(),
        f if f % 10 == 0 => format!("{w}.{}", f / 10),
        f => format!("{w}.{f:02}"),
    }
}

/// Rate and HSN/SAC checks for one line on the document date.
pub fn line_warnings(
    pack: &RulePack,
    date: NaiveDate,
    index: usize,
    line: LineFacts<'_>,
) -> Vec<TaxWarning> {
    let mut out = Vec::new();
    let field = |name: &str| format!("lines.{index}.{name}");
    let hsn = line
        .hsn
        .and_then(|code| pack.hsn_rate(code, date))
        .filter(|(rule, _)| !rule.conditional);
    let hsn_text = |code: &str, desc: &str| format!("HSN/SAC {code} ({desc})");

    if line.category == TaxCategory::Taxable
        && let RateStatus::NotInForce { from, to } = pack.rate_status(line.rate_bp, date)
    {
        let when = match (from, to) {
            (_, Some(t)) if date > t => format!("only up to {}", short_date(t)),
            (Some(f), _) => format!("only from {}", short_date(f)),
            _ => "on other dates".into(),
        };
        let mut message = format!("{} GST applies {when}.", pct(pack, line.rate_bp));
        if let Some((rule, rate)) = &hsn
            && rate.category == TaxCategory::Taxable
        {
            message.push_str(&format!(
                " The rule pack has {} for {}.",
                pct(pack, rate.bp),
                hsn_text(&rule.code, &rule.description)
            ));
        }
        out.push(TaxWarning {
            code: "rate_not_in_force",
            field: field("gstRate"),
            message,
        });
        return out;
    }

    let Some((rule, rate)) = hsn else {
        return out;
    };
    let since = rate
        .from
        .map(|f| format!(" from {}", short_date(f)))
        .unwrap_or_default();
    let what = hsn_text(&rule.code, &rule.description);
    match (line.category, rate.category) {
        (TaxCategory::Taxable, TaxCategory::Taxable) if rate.bp != line.rate_bp => {
            out.push(TaxWarning {
                code: "rate_differs",
                field: field("gstRate"),
                message: format!(
                    "{what} is {}{since} in the rule pack; this line uses {}. Check the rate.",
                    pct(pack, rate.bp),
                    pct(pack, line.rate_bp)
                ),
            });
        }
        (TaxCategory::Taxable, c) if c != TaxCategory::Taxable => out.push(TaxWarning {
            code: "hsn_not_taxable",
            field: field("taxCategory"),
            message: format!(
                "{what} is {} in the rule pack. Check whether GST applies.",
                category_words(c)
            ),
        }),
        (c, TaxCategory::Taxable) if c == TaxCategory::Exempt || c == TaxCategory::NilRated => out
            .push(TaxWarning {
                code: "hsn_taxable",
                field: field("taxCategory"),
                message: format!(
                    "{what} is taxed at {}{since} in the rule pack, but this line is marked {}.",
                    pct(pack, rate.bp),
                    category_words(c)
                ),
            }),
        _ => {}
    }
    out
}

fn category_words(c: TaxCategory) -> &'static str {
    match c {
        TaxCategory::Taxable => "taxable",
        TaxCategory::Exempt => "exempt",
        TaxCategory::NilRated => "nil-rated",
        TaxCategory::NonGst => "outside GST",
    }
}

/// A registered customer's GSTIN state differs from the place of supply.
/// Often right (goods delivered elsewhere, services at a site), so a warning.
pub fn place_of_supply_warning(
    place_of_supply: StateCode,
    recipient_gstin_state: Option<StateCode>,
) -> Option<TaxWarning> {
    let home = recipient_gstin_state?;
    (home != place_of_supply).then(|| TaxWarning {
        code: "place_of_supply",
        field: "placeOfSupply".into(),
        message: format!(
            "Place of supply is {} ({:02}), but the customer's GSTIN is registered in {} ({:02}). Check that this is right.",
            place_of_supply.name(),
            place_of_supply.code(),
            home.name(),
            home.code()
        ),
    })
}

// ---------------------------------------------------------------------------
// GSTR-1 placement
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Gstr1Section {
    B2b,
    B2cl,
    B2cs,
    ExpWpay,
    ExpWopay,
    SezWpay,
    SezWopay,
    Cdnr,
    Cdnur,
}

impl Gstr1Section {
    pub fn label(self) -> &'static str {
        match self {
            Gstr1Section::B2b => "B2B (table 4A)",
            Gstr1Section::B2cl => "B2C large (table 5)",
            Gstr1Section::B2cs => "B2C small (table 7)",
            Gstr1Section::ExpWpay => "Export with payment of tax (table 6A)",
            Gstr1Section::ExpWopay => "Export without payment of tax (table 6A)",
            Gstr1Section::SezWpay => "SEZ with payment of tax (table 6B)",
            Gstr1Section::SezWopay => "SEZ without payment of tax (table 6B)",
            Gstr1Section::Cdnr => "Credit/debit notes, registered (table 9B)",
            Gstr1Section::Cdnur => "Credit/debit notes, unregistered (table 9B)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutwardDoc {
    Invoice,
    BillOfSupply,
    CreditNote,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recipient {
    Registered,
    Sez,
    Unregistered,
    Overseas,
}

#[derive(Clone, Copy, Debug)]
pub struct OutwardFacts {
    pub doc: OutwardDoc,
    pub date: NaiveDate,
    pub recipient: Recipient,
    pub kind: SupplyKind,
    pub zero_rated: bool,
    /// Invoice value (grand total) in paise.
    pub total_paise: i64,
    pub has_taxable_lines: bool,
    pub has_other_lines: bool,
    /// Where the original invoice of a credit note was reported.
    pub original_section: Option<Gstr1Section>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gstr1Placement {
    /// Table for the taxable (or zero-rated) part; none for a document with
    /// only exempt, nil-rated or non-GST lines.
    pub section: Option<Gstr1Section>,
    /// Exempt, nil-rated and non-GST lines go to table 8.
    pub nil_table: bool,
}

pub fn gstr1_placement(pack: &RulePack, f: &OutwardFacts) -> Gstr1Placement {
    let export = f.recipient == Recipient::Overseas;
    let taxable_part = f.has_taxable_lines && f.doc != OutwardDoc::BillOfSupply;
    let nil_table = !export
        && (f.has_other_lines || (f.doc == OutwardDoc::BillOfSupply && f.has_taxable_lines));
    let section = if f.doc == OutwardDoc::CreditNote {
        match f.recipient {
            Recipient::Registered | Recipient::Sez => Some(Gstr1Section::Cdnr),
            // Notes against B2C small invoices are netted in table 7.
            _ if f.original_section == Some(Gstr1Section::B2cs) => Some(Gstr1Section::B2cs),
            _ if f.original_section.is_none() && !taxable_part => None,
            _ => Some(Gstr1Section::Cdnur),
        }
    } else if !taxable_part && !export {
        None
    } else {
        Some(match f.recipient {
            Recipient::Overseas if f.zero_rated => Gstr1Section::ExpWopay,
            Recipient::Overseas => Gstr1Section::ExpWpay,
            Recipient::Sez if f.zero_rated => Gstr1Section::SezWopay,
            Recipient::Sez => Gstr1Section::SezWpay,
            Recipient::Registered => Gstr1Section::B2b,
            Recipient::Unregistered => {
                let limit = pack
                    .threshold("b2clInvoiceValue", f.date)
                    .unwrap_or(i64::MAX);
                if f.kind == SupplyKind::InterState && f.total_paise > limit {
                    Gstr1Section::B2cl
                } else {
                    Gstr1Section::B2cs
                }
            }
        })
    };
    Gstr1Placement { section, nil_table }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn pack() -> &'static RulePack {
        RulePack::bundled()
    }
    fn taxable(hsn: Option<&str>, bp: u32) -> LineFacts<'_> {
        LineFacts {
            hsn,
            category: TaxCategory::Taxable,
            rate_bp: bp,
        }
    }

    #[test]
    fn withdrawn_rate_after_the_change() {
        let w = line_warnings(pack(), d("2025-10-01"), 0, taxable(Some("84151010"), 2800));
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].code, "rate_not_in_force");
        assert_eq!(w[0].field, "lines.0.gstRate");
        assert!(
            w[0].message.contains("only up to 21 Sep 2025"),
            "{}",
            w[0].message
        );
        assert!(w[0].message.contains("18%"), "{}", w[0].message);
        // Same line before the change is fine.
        assert!(line_warnings(pack(), d("2025-09-01"), 0, taxable(Some("8415"), 2800)).is_empty());
        // 40% before it existed.
        let w = line_warnings(pack(), d("2025-06-01"), 3, taxable(None, 4000));
        assert!(
            w[0].message.contains("only from 22 Sep 2025"),
            "{}",
            w[0].message
        );
    }

    #[test]
    fn rate_differs_from_hsn() {
        let w = line_warnings(pack(), d("2026-01-15"), 1, taxable(Some("3401"), 1800));
        assert_eq!(w[0].code, "rate_differs");
        assert!(
            w[0].message.contains("is 5% from 22 Sep 2025"),
            "{}",
            w[0].message
        );
        assert!(line_warnings(pack(), d("2026-01-15"), 1, taxable(Some("3401"), 500)).is_empty());
        // Conditional headings never warn on rate.
        assert!(line_warnings(pack(), d("2026-01-15"), 1, taxable(Some("8703"), 4000)).is_empty());
        // Unknown HSN: nothing to compare.
        assert!(line_warnings(pack(), d("2026-01-15"), 1, taxable(Some("1234"), 1800)).is_empty());
    }

    #[test]
    fn category_mismatch() {
        let exempt = LineFacts {
            hsn: Some("9983"),
            category: TaxCategory::Exempt,
            rate_bp: 0,
        };
        let w = line_warnings(pack(), d("2026-01-15"), 0, exempt);
        assert_eq!(w[0].code, "hsn_taxable");
        assert_eq!(w[0].field, "lines.0.taxCategory");
        // Non-GST lines are the user's call.
        let non_gst = LineFacts {
            category: TaxCategory::NonGst,
            ..exempt
        };
        assert!(line_warnings(pack(), d("2026-01-15"), 0, non_gst).is_empty());
    }

    #[test]
    fn place_of_supply_check() {
        let mh = StateCode::new(27).unwrap();
        let ka = StateCode::new(29).unwrap();
        assert!(place_of_supply_warning(mh, Some(mh)).is_none());
        assert!(place_of_supply_warning(mh, None).is_none());
        let w = place_of_supply_warning(ka, Some(mh)).unwrap();
        assert!(
            w.message.contains("Karnataka (29)") && w.message.contains("(27)"),
            "{}",
            w.message
        );
    }

    fn facts(recipient: Recipient, kind: SupplyKind, total_rupees: i64) -> OutwardFacts {
        OutwardFacts {
            doc: OutwardDoc::Invoice,
            date: d("2026-01-15"),
            recipient,
            kind,
            zero_rated: false,
            total_paise: total_rupees * 100,
            has_taxable_lines: true,
            has_other_lines: false,
            original_section: None,
        }
    }

    #[test]
    fn gstr1_sections() {
        let p = pack();
        let place = |f: OutwardFacts| gstr1_placement(p, &f).section;
        use Gstr1Section::*;
        assert_eq!(
            place(facts(Recipient::Registered, SupplyKind::IntraState, 500)),
            Some(B2b)
        );
        assert_eq!(
            place(facts(
                Recipient::Unregistered,
                SupplyKind::InterState,
                100_001
            )),
            Some(B2cl)
        );
        // Exactly the limit stays small; intra-state is always small.
        assert_eq!(
            place(facts(
                Recipient::Unregistered,
                SupplyKind::InterState,
                100_000
            )),
            Some(B2cs)
        );
        assert_eq!(
            place(facts(
                Recipient::Unregistered,
                SupplyKind::IntraState,
                900_000
            )),
            Some(B2cs)
        );
        // Before Aug 2024 the limit was ₹2.5 lakh.
        let old = OutwardFacts {
            date: d("2024-05-01"),
            ..facts(Recipient::Unregistered, SupplyKind::InterState, 200_000)
        };
        assert_eq!(place(old), Some(B2cs));
        let lut = OutwardFacts {
            zero_rated: true,
            ..facts(Recipient::Overseas, SupplyKind::InterState, 5_000)
        };
        assert_eq!(place(lut), Some(ExpWopay));
        assert_eq!(
            place(facts(Recipient::Overseas, SupplyKind::InterState, 5_000)),
            Some(ExpWpay)
        );
        let sez = OutwardFacts {
            zero_rated: true,
            ..facts(Recipient::Sez, SupplyKind::InterState, 5_000)
        };
        assert_eq!(place(sez), Some(SezWopay));
    }

    #[test]
    fn gstr1_credit_notes_and_nil() {
        let p = pack();
        let cn = |recipient, original| OutwardFacts {
            doc: OutwardDoc::CreditNote,
            original_section: original,
            ..facts(recipient, SupplyKind::InterState, 1_000)
        };
        use Gstr1Section::*;
        assert_eq!(
            gstr1_placement(p, &cn(Recipient::Registered, Some(B2b))).section,
            Some(Cdnr)
        );
        assert_eq!(
            gstr1_placement(p, &cn(Recipient::Unregistered, Some(B2cl))).section,
            Some(Cdnur)
        );
        assert_eq!(
            gstr1_placement(p, &cn(Recipient::Unregistered, Some(B2cs))).section,
            Some(B2cs)
        );

        let mixed = OutwardFacts {
            has_other_lines: true,
            ..facts(Recipient::Registered, SupplyKind::IntraState, 500)
        };
        assert_eq!(
            gstr1_placement(p, &mixed),
            Gstr1Placement {
                section: Some(B2b),
                nil_table: true
            }
        );
        let bos = OutwardFacts {
            doc: OutwardDoc::BillOfSupply,
            ..facts(Recipient::Unregistered, SupplyKind::IntraState, 500)
        };
        assert_eq!(
            gstr1_placement(p, &bos),
            Gstr1Placement {
                section: None,
                nil_table: true
            }
        );
    }
}
