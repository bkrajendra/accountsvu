//! Permanent Account Number (PAN) — format validation only.
//!
//! Layout: 5 letters, 4 digits, 1 letter. The 4th letter encodes the holder
//! type (P individual, C company, F firm/LLP, H HUF, …).

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("PAN must be 5 letters, 4 digits and 1 letter (e.g. ABCPE1234F)")]
pub struct PanError;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Pan(String);

impl Pan {
    pub fn parse(raw: &str) -> Result<Self, PanError> {
        let s = raw.trim().to_ascii_uppercase();
        let b = s.as_bytes();
        let ok = b.len() == 10
            && b[..5].iter().all(u8::is_ascii_uppercase)
            && b[5..9].iter().all(u8::is_ascii_digit)
            && b[9].is_ascii_uppercase();
        if ok { Ok(Pan(s)) } else { Err(PanError) }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Holder-type letter (4th character).
    pub fn holder_type(&self) -> char {
        self.0.as_bytes()[3] as char
    }
}

impl TryFrom<String> for Pan {
    type Error = PanError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Pan::parse(&s)
    }
}
impl From<Pan> for String {
    fn from(p: Pan) -> String {
        p.0
    }
}
impl fmt::Display for Pan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_normalises() {
        let p = Pan::parse(" abcpe1234f ").unwrap();
        assert_eq!(p.as_str(), "ABCPE1234F");
        assert_eq!(p.holder_type(), 'P');
    }

    #[test]
    fn rejects_bad_shapes() {
        for bad in ["", "ABCPE1234", "ABCP31234F", "ABCPE12345", "1BCPE1234F"] {
            assert!(Pan::parse(bad).is_err(), "{bad}");
        }
    }
}
