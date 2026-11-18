//! Financial years. India's FY runs April–March, but the start month is an
//! organisation setting (spec §5.1).

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FinancialYear {
    start_year: i32,
    start_month: u32,
}

impl FinancialYear {
    /// `start_month` is 1–12; panics otherwise (validated at the edges).
    pub fn new(start_year: i32, start_month: u32) -> Self {
        assert!((1..=12).contains(&start_month), "start month must be 1-12");
        FinancialYear {
            start_year,
            start_month,
        }
    }

    /// The financial year containing `date`.
    pub fn containing(date: NaiveDate, start_month: u32) -> Self {
        let year = if date.month() >= start_month {
            date.year()
        } else {
            date.year() - 1
        };
        FinancialYear::new(year, start_month)
    }

    pub fn start_date(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.start_year, self.start_month, 1).expect("valid month")
    }

    /// Inclusive last day.
    pub fn end_date(&self) -> NaiveDate {
        self.next()
            .start_date()
            .pred_opt()
            .expect("not the minimum date")
    }

    pub fn contains(&self, date: NaiveDate) -> bool {
        date >= self.start_date() && date <= self.end_date()
    }

    pub fn next(&self) -> Self {
        FinancialYear::new(self.start_year + 1, self.start_month)
    }

    pub fn start_year(&self) -> i32 {
        self.start_year
    }

    /// Storage/label code, e.g. `2026-27` (or `2026` for a calendar-year FY).
    pub fn code(&self) -> String {
        if self.start_month == 1 {
            self.start_year.to_string()
        } else {
            format!(
                "{}-{:02}",
                self.start_year,
                (self.start_year + 1).rem_euclid(100)
            )
        }
    }

    /// Short form used in document numbers, e.g. `26-27`.
    pub fn short_code(&self) -> String {
        if self.start_month == 1 {
            format!("{:02}", self.start_year.rem_euclid(100))
        } else {
            format!(
                "{:02}-{:02}",
                self.start_year.rem_euclid(100),
                (self.start_year + 1).rem_euclid(100)
            )
        }
    }
}

impl fmt::Display for FinancialYear {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FY {}", self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn april_start() {
        let fy = FinancialYear::containing(d(2026, 11, 17), 4);
        assert_eq!(fy.code(), "2026-27");
        assert_eq!(fy.short_code(), "26-27");
        assert_eq!(fy.start_date(), d(2026, 4, 1));
        assert_eq!(fy.end_date(), d(2027, 3, 31));
        assert_eq!(FinancialYear::containing(d(2027, 3, 31), 4), fy);
        assert_eq!(FinancialYear::containing(d(2027, 4, 1), 4), fy.next());
    }

    #[test]
    fn century_rollover() {
        assert_eq!(FinancialYear::new(2099, 4).code(), "2099-00");
    }

    #[test]
    fn calendar_year() {
        let fy = FinancialYear::containing(d(2026, 6, 1), 1);
        assert_eq!(fy.code(), "2026");
        assert_eq!(fy.end_date(), d(2026, 12, 31));
    }
}
