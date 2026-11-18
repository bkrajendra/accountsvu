//! Tax for a whole sales or purchase document (spec §5.2, §5.6, D11).
//!
//! Pure and deterministic: the same lines always give the same paise. Tax is
//! computed per line and rounded to the paisa; the grand total is optionally
//! rounded to the rupee with the difference reported as round-off.

use av_domain::{Money, TaxCategory};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{DocumentTotals, LineTax, SupplyKind, TaxRate, compute_line_tax, document_totals};

/// One line as entered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineInput {
    pub quantity: Decimal,
    pub unit_price: Money,
    /// Flat discount on the line, in the same terms as the price.
    pub discount: Money,
    pub category: TaxCategory,
    pub rate: TaxRate,
    pub cess: TaxRate,
}

/// How the document is taxed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Supply {
    pub kind: SupplyKind,
    /// Export or SEZ supply under LUT/bond: no tax is charged.
    pub zero_rated: bool,
    /// Prices already include GST; the taxable value is worked out of them.
    pub prices_include_tax: bool,
    /// A bill of supply never carries tax.
    pub tax_free_document: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputedLine {
    /// Quantity × price − discount, rounded to the paisa.
    pub amount: Money,
    pub tax: LineTax,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputedDocument {
    pub lines: Vec<ComputedLine>,
    pub totals: DocumentTotals,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LineError {
    #[error("quantity must be more than zero")]
    Quantity,
    #[error("price can't be negative")]
    Price,
    #[error("discount can't be negative or more than the line amount")]
    Discount,
}

/// Validate a line's numbers; returns its amount (qty × price − discount).
pub fn line_amount(l: &LineInput) -> Result<Money, LineError> {
    if l.quantity <= Decimal::ZERO {
        return Err(LineError::Quantity);
    }
    if l.unit_price.is_negative() {
        return Err(LineError::Price);
    }
    let gross = Money::new(l.quantity * l.unit_price.amount()).round_paise();
    if l.discount.is_negative() || l.discount.amount() > gross.amount() {
        return Err(LineError::Discount);
    }
    Ok(gross - l.discount)
}

/// Compute every line and the document totals. Lines that fail validation
/// are returned as errors by index; the caller decides how to report them.
pub fn compute_document(
    lines: &[LineInput],
    supply: Supply,
    round_to_rupee: bool,
) -> Result<ComputedDocument, Vec<(usize, LineError)>> {
    let mut errors = Vec::new();
    let mut out = Vec::with_capacity(lines.len());
    for (i, l) in lines.iter().enumerate() {
        match line_amount(l) {
            Ok(amount) => out.push(ComputedLine {
                amount,
                tax: line_tax(l, amount, supply),
            }),
            Err(e) => errors.push((i, e)),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let totals = document_totals(out.iter().map(|c| &c.tax), round_to_rupee);
    Ok(ComputedDocument { lines: out, totals })
}

fn line_tax(l: &LineInput, amount: Money, supply: Supply) -> LineTax {
    let charges_tax =
        l.category == TaxCategory::Taxable && !supply.zero_rated && !supply.tax_free_document;
    if !charges_tax {
        return LineTax {
            taxable_value: amount,
            ..Default::default()
        };
    }
    let taxable = if supply.prices_include_tax {
        // amount = taxable × (1 + rate + cess)
        let divisor = Decimal::from(10_000 + l.rate.0 + l.cess.0) / Decimal::from(10_000);
        Money::new(amount.amount() / divisor)
    } else {
        amount
    };
    compute_line_tax(taxable, l.rate, l.cess, supply.kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn line(qty: Decimal, price: Decimal, rate: u32) -> LineInput {
        LineInput {
            quantity: qty,
            unit_price: Money::new(price),
            discount: Money::ZERO,
            category: TaxCategory::Taxable,
            rate: TaxRate(rate),
            cess: TaxRate(0),
        }
    }

    const INTRA: Supply = Supply {
        kind: SupplyKind::IntraState,
        zero_rated: false,
        prices_include_tax: false,
        tax_free_document: false,
    };

    #[test]
    fn invoice_with_mixed_lines() {
        let mut exempt = line(dec!(10), dec!(40), 0);
        exempt.category = TaxCategory::Exempt;
        let mut discounted = line(dec!(2.5), dec!(199.99), 1800);
        discounted.discount = Money::new(dec!(50));
        let d = compute_document(
            &[line(dec!(1), dec!(1000), 1800), discounted, exempt],
            INTRA,
            true,
        )
        .unwrap();
        // 2.5 × 199.99 = 499.975 → 499.98; − 50 = 449.98; 9% = 40.4982 → 40.50
        assert_eq!(d.lines[1].amount.amount(), dec!(449.98));
        assert_eq!(d.lines[1].tax.cgst.amount(), dec!(40.50));
        assert_eq!(d.lines[2].tax.total_tax(), Money::ZERO);
        assert_eq!(d.totals.taxable_value.amount(), dec!(1849.98));
        // 1849.98 + 2 × (90 + 40.50) = 2110.98 → 2111.00
        assert_eq!(d.totals.grand_total.amount(), dec!(2111));
        assert_eq!(d.totals.round_off.amount(), dec!(0.02));
    }

    #[test]
    fn prices_including_tax() {
        let s = Supply {
            prices_include_tax: true,
            ..INTRA
        };
        let d = compute_document(&[line(dec!(1), dec!(118), 1800)], s, false).unwrap();
        assert_eq!(d.lines[0].tax.taxable_value.amount(), dec!(100));
        assert_eq!(d.totals.grand_total.amount(), dec!(118));
    }

    #[test]
    fn zero_rated_and_bill_of_supply_carry_no_tax() {
        for s in [
            Supply {
                zero_rated: true,
                ..INTRA
            },
            Supply {
                tax_free_document: true,
                ..INTRA
            },
        ] {
            let d = compute_document(&[line(dec!(1), dec!(500), 1800)], s, true).unwrap();
            assert_eq!(d.totals.grand_total.amount(), dec!(500));
        }
    }

    #[test]
    fn bad_lines_are_reported_by_index() {
        let mut over = line(dec!(1), dec!(10), 1800);
        over.discount = Money::new(dec!(11));
        let err = compute_document(
            &[line(dec!(0), dec!(1), 0), line(dec!(1), dec!(1), 0), over],
            INTRA,
            true,
        )
        .unwrap_err();
        assert_eq!(
            err,
            vec![(0, LineError::Quantity), (2, LineError::Discount)]
        );
    }
}
