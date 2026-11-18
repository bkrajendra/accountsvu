//! Exact INR money (spec §5.5, D10, D11).
//!
//! * Domain arithmetic uses [`rust_decimal::Decimal`] — never binary floats.
//! * Persistence stores integer **paise** (`i64`).
//! * Rounding to the paisa and to the rupee is half-up (away from zero).

use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MoneyError {
    #[error("amount has more than 2 decimal places: {0}")]
    TooPrecise(Decimal),
    #[error("amount is out of range for storage")]
    Overflow,
    #[error("invalid amount: {0}")]
    Parse(String),
}

/// An INR amount held at full decimal precision.
///
/// Intermediate results (e.g. `taxable * rate`) may carry more than two
/// decimals; call [`Money::round_paise`] at the defined rounding points.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Money(Decimal);

impl Money {
    pub const ZERO: Money = Money(Decimal::ZERO);

    pub fn new(value: Decimal) -> Self {
        Money(value)
    }

    pub fn from_paise(paise: i64) -> Self {
        Money(Decimal::new(paise, 2))
    }

    /// Whole rupees, e.g. `Money::from_rupees(1500)`.
    pub fn from_rupees(rupees: i64) -> Self {
        Money(Decimal::from(rupees))
    }

    pub fn parse(s: &str) -> Result<Self, MoneyError> {
        s.trim()
            .replace(',', "")
            .parse::<Decimal>()
            .map(Money)
            .map_err(|_| MoneyError::Parse(s.to_owned()))
    }

    pub fn amount(&self) -> Decimal {
        self.0
    }

    /// Round half-up to the paisa (2 dp).
    pub fn round_paise(self) -> Self {
        Money(
            self.0
                .round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero),
        )
    }

    /// Round half-up to the nearest rupee (used for invoice grand totals, D11).
    pub fn round_rupee(self) -> Self {
        Money(
            self.0
                .round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero),
        )
    }

    /// Convert to integer paise for storage. Fails if the value is not
    /// already rounded to the paisa — rounding must be an explicit decision.
    pub fn to_paise(self) -> Result<i64, MoneyError> {
        if self.0.normalize().scale() > 2 {
            return Err(MoneyError::TooPrecise(self.0));
        }
        let scaled = self.0 * Decimal::ONE_HUNDRED;
        i64::try_from(scaled.trunc()).map_err(|_| MoneyError::Overflow)
    }

    /// Multiply by a rate given in basis points (`1800` = 18%). Not rounded.
    pub fn percent_bp(self, basis_points: u32) -> Self {
        Money(self.0 * Decimal::from(basis_points) / Decimal::from(10_000))
    }

    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    pub fn is_negative(&self) -> bool {
        self.0.is_sign_negative() && !self.0.is_zero()
    }

    pub fn abs(self) -> Self {
        Money(self.0.abs())
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "₹{}", self.round_paise().0)
    }
}

impl Add for Money {
    type Output = Money;
    fn add(self, rhs: Money) -> Money {
        Money(self.0 + rhs.0)
    }
}
impl Sub for Money {
    type Output = Money;
    fn sub(self, rhs: Money) -> Money {
        Money(self.0 - rhs.0)
    }
}
impl Neg for Money {
    type Output = Money;
    fn neg(self) -> Money {
        Money(-self.0)
    }
}
impl AddAssign for Money {
    fn add_assign(&mut self, rhs: Money) {
        self.0 += rhs.0;
    }
}
impl SubAssign for Money {
    fn sub_assign(&mut self, rhs: Money) {
        self.0 -= rhs.0;
    }
}
impl Sum for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Money {
        iter.fold(Money::ZERO, |a, b| a + b)
    }
}
impl<'a> Sum<&'a Money> for Money {
    fn sum<I: Iterator<Item = &'a Money>>(iter: I) -> Money {
        iter.copied().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn paise_round_trip() {
        let m = Money::from_paise(123_456);
        assert_eq!(m.amount(), dec!(1234.56));
        assert_eq!(m.to_paise().unwrap(), 123_456);
        assert_eq!(Money::from_paise(-5).to_paise().unwrap(), -5);
    }

    #[test]
    fn refuses_unrounded_storage() {
        let m = Money::new(dec!(10.005));
        assert!(matches!(m.to_paise(), Err(MoneyError::TooPrecise(_))));
        assert_eq!(m.round_paise().to_paise().unwrap(), 1001);
    }

    #[test]
    fn trailing_zeros_are_not_extra_precision() {
        assert_eq!(Money::new(dec!(10.5000)).to_paise().unwrap(), 1050);
    }

    #[test]
    fn half_up_rounding() {
        assert_eq!(Money::new(dec!(2.345)).round_paise().amount(), dec!(2.35));
        assert_eq!(Money::new(dec!(-2.345)).round_paise().amount(), dec!(-2.35));
        assert_eq!(Money::new(dec!(99.50)).round_rupee().amount(), dec!(100));
        assert_eq!(Money::new(dec!(99.49)).round_rupee().amount(), dec!(99));
    }

    #[test]
    fn basis_points() {
        let tax = Money::from_rupees(1000).percent_bp(1800);
        assert_eq!(tax.amount(), dec!(180));
        let tax = Money::new(dec!(333.33)).percent_bp(250).round_paise();
        assert_eq!(tax.amount(), dec!(8.33));
    }

    #[test]
    fn parse_with_grouping() {
        assert_eq!(
            Money::parse("1,23,456.70").unwrap().amount(),
            dec!(123456.70)
        );
        assert!(Money::parse("abc").is_err());
    }

    #[test]
    fn serialises_as_string() {
        let json = serde_json::to_string(&Money::from_paise(1050)).unwrap();
        assert_eq!(json, "\"10.50\"");
    }
}
