//! Double-entry accounting core (spec §5.5).
//!
//! A [`JournalEntry`] can only be constructed if it balances, so an
//! unbalanced entry is unrepresentable past this boundary.

pub mod chart;

use av_domain::{Id, Money};
use serde::{Deserialize, Serialize};

/// Top-level account classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    Asset,
    Liability,
    Equity,
    Income,
    Expense,
}

impl AccountKind {
    /// Whether a debit increases the balance of this kind of account.
    pub fn is_debit_normal(self) -> bool {
        matches!(self, AccountKind::Asset | AccountKind::Expense)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            AccountKind::Asset => "asset",
            AccountKind::Liability => "liability",
            AccountKind::Equity => "equity",
            AccountKind::Income => "income",
            AccountKind::Expense => "expense",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalLine {
    pub account_id: Id,
    pub debit: Money,
    pub credit: Money,
    pub memo: Option<String>,
}

impl JournalLine {
    pub fn debit(account_id: Id, amount: Money) -> Self {
        JournalLine {
            account_id,
            debit: amount,
            credit: Money::ZERO,
            memo: None,
        }
    }

    pub fn credit(account_id: Id, amount: Money) -> Self {
        JournalLine {
            account_id,
            debit: Money::ZERO,
            credit: amount,
            memo: None,
        }
    }

    pub fn with_memo(mut self, memo: impl Into<String>) -> Self {
        self.memo = Some(memo.into());
        self
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum JournalError {
    #[error("a journal entry needs at least two lines")]
    TooFewLines,
    #[error("line {0} must have exactly one of debit or credit, and it must be positive")]
    InvalidLine(usize),
    #[error("line {0} is not rounded to the paisa")]
    Unrounded(usize),
    #[error("journal does not balance: debits {debits}, credits {credits}")]
    Unbalanced { debits: Money, credits: Money },
}

/// A balanced, validated journal entry. Posted entries are immutable;
/// corrections are made with [`JournalEntry::reversal`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    lines: Vec<JournalLine>,
}

impl JournalEntry {
    pub fn new(lines: Vec<JournalLine>) -> Result<Self, JournalError> {
        if lines.len() < 2 {
            return Err(JournalError::TooFewLines);
        }
        for (i, l) in lines.iter().enumerate() {
            let one_sided = l.debit.is_zero() != l.credit.is_zero();
            if !one_sided || l.debit.is_negative() || l.credit.is_negative() {
                return Err(JournalError::InvalidLine(i));
            }
            if l.debit != l.debit.round_paise() || l.credit != l.credit.round_paise() {
                return Err(JournalError::Unrounded(i));
            }
        }
        let debits: Money = lines.iter().map(|l| l.debit).sum();
        let credits: Money = lines.iter().map(|l| l.credit).sum();
        if debits != credits {
            return Err(JournalError::Unbalanced { debits, credits });
        }
        Ok(JournalEntry { lines })
    }

    pub fn lines(&self) -> &[JournalLine] {
        &self.lines
    }

    pub fn total(&self) -> Money {
        self.lines.iter().map(|l| l.debit).sum()
    }

    /// The mirror entry that cancels this one.
    pub fn reversal(&self) -> JournalEntry {
        let lines = self
            .lines
            .iter()
            .map(|l| JournalLine {
                account_id: l.account_id,
                debit: l.credit,
                credit: l.debit,
                memo: Some("Reversal".into()),
            })
            .collect();
        JournalEntry { lines }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use av_domain::new_id;
    use rust_decimal_macros::dec;

    #[test]
    fn sales_invoice_balances() {
        let (ar, sales, cgst, sgst) = (new_id(), new_id(), new_id(), new_id());
        let e = JournalEntry::new(vec![
            JournalLine::debit(ar, Money::from_rupees(1180)),
            JournalLine::credit(sales, Money::from_rupees(1000)),
            JournalLine::credit(cgst, Money::from_rupees(90)),
            JournalLine::credit(sgst, Money::from_rupees(90)),
        ])
        .unwrap();
        assert_eq!(e.total(), Money::from_rupees(1180));
    }

    #[test]
    fn rejects_unbalanced() {
        let err = JournalEntry::new(vec![
            JournalLine::debit(new_id(), Money::from_rupees(100)),
            JournalLine::credit(new_id(), Money::from_rupees(99)),
        ])
        .unwrap_err();
        assert!(matches!(err, JournalError::Unbalanced { .. }));
    }

    #[test]
    fn rejects_bad_lines() {
        let a = new_id();
        assert_eq!(
            JournalEntry::new(vec![JournalLine::debit(a, Money::from_rupees(1))]),
            Err(JournalError::TooFewLines)
        );
        let both = JournalLine {
            account_id: a,
            debit: Money::from_rupees(1),
            credit: Money::from_rupees(1),
            memo: None,
        };
        assert_eq!(
            JournalEntry::new(vec![both, JournalLine::credit(a, Money::ZERO)]),
            Err(JournalError::InvalidLine(0))
        );
        assert_eq!(
            JournalEntry::new(vec![
                JournalLine::debit(a, Money::new(dec!(1.005))),
                JournalLine::credit(a, Money::new(dec!(1.005))),
            ]),
            Err(JournalError::Unrounded(0))
        );
    }

    #[test]
    fn reversal_balances_and_swaps() {
        let (a, b) = (new_id(), new_id());
        let e = JournalEntry::new(vec![
            JournalLine::debit(a, Money::from_rupees(5)),
            JournalLine::credit(b, Money::from_rupees(5)),
        ])
        .unwrap();
        let r = e.reversal();
        assert_eq!(r.lines()[0].credit, Money::from_rupees(5));
        assert!(JournalEntry::new(r.lines().to_vec()).is_ok());
    }
}
