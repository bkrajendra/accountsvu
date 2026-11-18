//! GSTIN parsing and validation (format + mod-36 check digit).
//!
//! Layout (15 chars): `SS PPPPPPPPPP E Z C`
//! * `SS` – state code, `P…` – PAN of the holder, `E` – entity number,
//! * `Z` – default `Z`, `C` – check character.
//!
//! Validation here is structural only; it does not prove the GSTIN is active.

use crate::state::StateCode;
use serde::{Deserialize, Serialize};
use std::fmt;

const CHARSET: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GstinError {
    #[error("GSTIN must be 15 characters")]
    Length,
    #[error("GSTIN contains an invalid character")]
    Charset,
    #[error("GSTIN has an unknown state code")]
    State,
    #[error("GSTIN does not contain a valid PAN")]
    Pan,
    #[error("GSTIN check digit does not match (expected {expected})")]
    Checksum { expected: char },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Gstin(String);

impl Gstin {
    pub fn parse(raw: &str) -> Result<Self, GstinError> {
        let s = raw.trim().to_ascii_uppercase();
        let b = s.as_bytes();
        if b.len() != 15 {
            return Err(GstinError::Length);
        }
        if !b.iter().all(|c| CHARSET.contains(c)) {
            return Err(GstinError::Charset);
        }
        let state: u8 = s[0..2].parse().map_err(|_| GstinError::State)?;
        StateCode::new(state).map_err(|_| GstinError::State)?;

        let pan = &b[2..12];
        let pan_ok = pan[..5].iter().all(u8::is_ascii_uppercase)
            && pan[5..9].iter().all(u8::is_ascii_digit)
            && pan[9].is_ascii_uppercase();
        if !pan_ok {
            return Err(GstinError::Pan);
        }

        let expected = check_char(&b[..14]);
        if b[14] as char != expected {
            return Err(GstinError::Checksum { expected });
        }
        Ok(Gstin(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn state(&self) -> StateCode {
        StateCode::new(self.0[0..2].parse().expect("validated")).expect("validated")
    }

    pub fn pan(&self) -> &str {
        &self.0[2..12]
    }
}

/// GSTN mod-36 check character over the first 14 characters.
fn check_char(first14: &[u8]) -> char {
    let sum: u32 = first14
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let v = CHARSET
                .iter()
                .position(|x| x == c)
                .expect("charset checked") as u32;
            let p = v * if i % 2 == 0 { 1 } else { 2 };
            p / 36 + p % 36
        })
        .sum();
    CHARSET[((36 - sum % 36) % 36) as usize] as char
}

impl TryFrom<String> for Gstin {
    type Error = GstinError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Gstin::parse(&s)
    }
}
impl From<Gstin> for String {
    fn from(g: Gstin) -> String {
        g.0
    }
}
impl fmt::Display for Gstin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_check(first14: &str) -> String {
        format!("{first14}{}", check_char(first14.as_bytes()))
    }

    #[test]
    fn accepts_valid() {
        let g = Gstin::parse(&with_check("27AAPFU0939F1Z")).unwrap();
        assert_eq!(g.state().code(), 27);
        assert_eq!(g.pan(), "AAPFU0939F");
    }

    #[test]
    fn known_public_example() {
        // Widely published sample GSTIN.
        assert!(Gstin::parse("27AAPFU0939F1ZV").is_ok());
    }

    #[test]
    fn normalises_case_and_whitespace() {
        let raw = with_check("29ABCDE1234F1Z").to_lowercase();
        assert!(Gstin::parse(&format!("  {raw} ")).is_ok());
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(Gstin::parse("27AAPFU0939F1Z"), Err(GstinError::Length));
        assert_eq!(Gstin::parse("99AAPFU0939F1ZV"), Err(GstinError::State));
        assert_eq!(Gstin::parse("27AAPF00939F1ZV"), Err(GstinError::Pan));
        assert!(matches!(
            Gstin::parse("27AAPFU0939F1ZA"),
            Err(GstinError::Checksum { .. })
        ));
    }
}
