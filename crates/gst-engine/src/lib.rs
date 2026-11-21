//! GST tax calculation (spec §5.6, D11).
//!
//! Per-line component split and document totals, the versioned rule pack
//! (`rules`), and document review: warnings and GSTR-1 placement (`checks`).
//! Return workpapers follow in later Phase 2 steps.

use av_domain::{Money, StateCode};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub mod checks;
pub mod invoice;
pub mod offset;
pub mod rules;

pub use rules::RulePack;

/// A GST rate in basis points (`1800` = 18%).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaxRate(pub u32);

impl TaxRate {
    pub const fn percent(p: u32) -> Self {
        TaxRate(p * 100)
    }
}

impl TaxRate {
    /// Whether the bundled rule pack knows this slab (on any date).
    pub fn is_known(self) -> bool {
        RulePack::bundled().is_known_rate(self.0)
    }
}

/// How the supply is taxed, derived from supplier state and place of supply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplyKind {
    /// Same state: CGST + SGST.
    IntraState,
    /// Same UT without legislature: CGST + UTGST.
    IntraUnionTerritory,
    /// Different states / UTs: IGST.
    InterState,
}

impl SupplyKind {
    pub fn determine(supplier_state: StateCode, place_of_supply: StateCode) -> Self {
        if supplier_state != place_of_supply {
            SupplyKind::InterState
        } else if supplier_state.levies_utgst() {
            SupplyKind::IntraUnionTerritory
        } else {
            SupplyKind::IntraState
        }
    }
}

/// Tax components for a single line, each rounded to the paisa.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineTax {
    pub taxable_value: Money,
    pub cgst: Money,
    pub sgst: Money,
    pub utgst: Money,
    pub igst: Money,
    pub cess: Money,
}

impl LineTax {
    pub fn total_tax(&self) -> Money {
        self.cgst + self.sgst + self.utgst + self.igst + self.cess
    }

    pub fn line_total(&self) -> Money {
        self.taxable_value + self.total_tax()
    }
}

/// Compute tax for one line. The taxable value is rounded to the paisa first;
/// each component is computed and rounded independently, so the two halves
/// of an intra-state split are always equal.
pub fn compute_line_tax(taxable: Money, rate: TaxRate, cess: TaxRate, kind: SupplyKind) -> LineTax {
    let taxable_value = taxable.round_paise();
    let mut t = LineTax {
        taxable_value,
        ..Default::default()
    };
    match kind {
        SupplyKind::InterState => t.igst = taxable_value.percent_bp(rate.0).round_paise(),
        SupplyKind::IntraState | SupplyKind::IntraUnionTerritory => {
            // Half the rate, exactly: 0.25% splits into 0.125% + 0.125%, so
            // never halve the basis points with integer division.
            let half =
                Money::new(taxable_value.amount() * Decimal::from(rate.0) / Decimal::from(20_000))
                    .round_paise();
            t.cgst = half;
            if kind == SupplyKind::IntraState {
                t.sgst = half;
            } else {
                t.utgst = half;
            }
        }
    }
    t.cess = taxable_value.percent_bp(cess.0).round_paise();
    t
}

/// Document totals with rupee round-off (D11).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentTotals {
    pub taxable_value: Money,
    pub cgst: Money,
    pub sgst: Money,
    pub utgst: Money,
    pub igst: Money,
    pub cess: Money,
    /// Posted to the Round-off ledger; may be negative.
    pub round_off: Money,
    pub grand_total: Money,
}

pub fn document_totals<'a>(
    lines: impl IntoIterator<Item = &'a LineTax>,
    round_to_rupee: bool,
) -> DocumentTotals {
    let mut d = DocumentTotals::default();
    for l in lines {
        d.taxable_value += l.taxable_value;
        d.cgst += l.cgst;
        d.sgst += l.sgst;
        d.utgst += l.utgst;
        d.igst += l.igst;
        d.cess += l.cess;
    }
    let exact = d.taxable_value + d.cgst + d.sgst + d.utgst + d.igst + d.cess;
    d.grand_total = if round_to_rupee {
        exact.round_rupee()
    } else {
        exact
    };
    d.round_off = d.grand_total - exact;
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn st(c: u8) -> StateCode {
        StateCode::new(c).unwrap()
    }

    #[test]
    fn determines_supply_kind() {
        assert_eq!(
            SupplyKind::determine(st(27), st(27)),
            SupplyKind::IntraState
        );
        assert_eq!(
            SupplyKind::determine(st(27), st(29)),
            SupplyKind::InterState
        );
        assert_eq!(
            SupplyKind::determine(st(4), st(4)),
            SupplyKind::IntraUnionTerritory
        );
    }

    #[test]
    fn intra_state_split() {
        let t = compute_line_tax(
            Money::from_rupees(1000),
            TaxRate::percent(18),
            TaxRate(0),
            SupplyKind::IntraState,
        );
        assert_eq!(t.cgst, Money::from_rupees(90));
        assert_eq!(t.sgst, Money::from_rupees(90));
        assert_eq!(t.igst, Money::ZERO);
        assert_eq!(t.line_total(), Money::from_rupees(1180));
    }

    #[test]
    fn inter_state_igst() {
        let t = compute_line_tax(
            Money::new(dec!(333.33)),
            TaxRate::percent(5),
            TaxRate(0),
            SupplyKind::InterState,
        );
        assert_eq!(t.igst.amount(), dec!(16.67));
        assert_eq!(t.cgst, Money::ZERO);
    }

    #[test]
    fn halves_always_equal() {
        let t = compute_line_tax(
            Money::new(dec!(10.01)),
            TaxRate::percent(5),
            TaxRate(0),
            SupplyKind::IntraState,
        );
        assert_eq!(t.cgst, t.sgst);
        assert_eq!(t.cgst.amount(), dec!(0.25));
    }

    #[test]
    fn fractional_rates_split_exactly() {
        // 0.25% on ₹10,000: 12.50 each half (not 12.00 from 12 bp).
        let t = compute_line_tax(
            Money::from_rupees(10_000),
            TaxRate(25),
            TaxRate(0),
            SupplyKind::IntraState,
        );
        assert_eq!(t.cgst.amount(), dec!(12.50));
        assert_eq!(t.sgst.amount(), dec!(12.50));
    }

    #[test]
    fn utgst_used_for_ut() {
        let t = compute_line_tax(
            Money::from_rupees(100),
            TaxRate::percent(18),
            TaxRate(0),
            SupplyKind::IntraUnionTerritory,
        );
        assert_eq!(t.utgst, Money::from_rupees(9));
        assert_eq!(t.sgst, Money::ZERO);
    }

    #[test]
    fn rupee_round_off() {
        let a = compute_line_tax(
            Money::new(dec!(99.40)),
            TaxRate::percent(18),
            TaxRate(0),
            SupplyKind::InterState,
        );
        // 99.40 + 17.89 = 117.29 → 117.00, round-off −0.29
        let d = document_totals([&a], true);
        assert_eq!(d.grand_total, Money::from_rupees(117));
        assert_eq!(d.round_off.amount(), dec!(-0.29));
        let d = document_totals([&a], false);
        assert_eq!(d.round_off, Money::ZERO);
    }
}
