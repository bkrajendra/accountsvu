//! GST portal file formats (Phase 2.2).
//!
//! Each adapter turns a neutral, already-computed return (amounts in paise,
//! rates in basis points) into the file the GST portal or its offline tool
//! accepts. The adapters do no tax logic: placement and totals come from the
//! application layer, so a format change never changes a figure.
//!
//! Numbers are written as exact decimal text (`1234.50`, `18`, `2.5`); they
//! never pass through binary floating point.

pub mod gstr1;
pub mod gstr2b;

use serde::{Serialize, Serializer};
use serde_json::value::RawValue;

/// Paise written as rupees with two decimals.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Amount(pub i64);

impl Serialize for Amount {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        raw(&rupees(self.0), s)
    }
}

/// A rate in basis points written as a plain percentage (`18`, `2.5`, `0.25`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rate(pub u32);

impl Serialize for Rate {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        raw(&percent(self.0), s)
    }
}

/// A decimal quantity already formatted as text (e.g. `"12.5"`), written as
/// a JSON number.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Quantity(pub String);

impl Serialize for Quantity {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let t = self.0.trim();
        let ok = !t.is_empty()
            && t.trim_start_matches('-')
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.')
            && t.matches('.').count() <= 1
            && !t.ends_with('.')
            && !t.trim_start_matches('-').starts_with('.');
        raw(if ok { t } else { "0" }, s)
    }
}

fn raw<S: Serializer>(text: &str, s: S) -> Result<S::Ok, S::Error> {
    RawValue::from_string(text.to_owned())
        .map_err(serde::ser::Error::custom)?
        .serialize(s)
}

pub fn rupees(paise: i64) -> String {
    let sign = if paise < 0 { "-" } else { "" };
    let p = paise.unsigned_abs();
    format!("{sign}{}.{:02}", p / 100, p % 100)
}

pub fn percent(bp: u32) -> String {
    let (w, f) = (bp / 100, bp % 100);
    match f {
        0 => w.to_string(),
        f if f % 10 == 0 => format!("{w}.{}", f / 10),
        f => format!("{w}.{f:02}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_exact_text() {
        assert_eq!(rupees(123_450), "1234.50");
        assert_eq!(rupees(-5), "-0.05");
        assert_eq!(rupees(0), "0.00");
        assert_eq!(percent(1800), "18");
        assert_eq!(percent(250), "2.5");
        assert_eq!(percent(25), "0.25");
        let v = serde_json::to_string(&(Amount(100), Rate(150), Quantity("2.5".into()))).unwrap();
        assert_eq!(v, "[1.00,1.5,2.5]");
        let bad = serde_json::to_string(&Quantity("1e9".into())).unwrap();
        assert_eq!(bad, "0");
    }
}
