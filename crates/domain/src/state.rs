//! GST state / union-territory codes (first two digits of a GSTIN).

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct StateCode(u8);

/// (code, name, is_union_territory_without_legislature)
///
/// UTs *without* a legislature levy UTGST instead of SGST on intra-state
/// supplies. Delhi, Puducherry and Jammu & Kashmir have legislatures and
/// therefore levy SGST.
const STATES: &[(u8, &str, bool)] = &[
    (1, "Jammu and Kashmir", false),
    (2, "Himachal Pradesh", false),
    (3, "Punjab", false),
    (4, "Chandigarh", true),
    (5, "Uttarakhand", false),
    (6, "Haryana", false),
    (7, "Delhi", false),
    (8, "Rajasthan", false),
    (9, "Uttar Pradesh", false),
    (10, "Bihar", false),
    (11, "Sikkim", false),
    (12, "Arunachal Pradesh", false),
    (13, "Nagaland", false),
    (14, "Manipur", false),
    (15, "Mizoram", false),
    (16, "Tripura", false),
    (17, "Meghalaya", false),
    (18, "Assam", false),
    (19, "West Bengal", false),
    (20, "Jharkhand", false),
    (21, "Odisha", false),
    (22, "Chhattisgarh", false),
    (23, "Madhya Pradesh", false),
    (24, "Gujarat", false),
    (25, "Daman and Diu (legacy)", true),
    (26, "Dadra and Nagar Haveli and Daman and Diu", true),
    (27, "Maharashtra", false),
    (29, "Karnataka", false),
    (30, "Goa", false),
    (31, "Lakshadweep", true),
    (32, "Kerala", false),
    (33, "Tamil Nadu", false),
    (34, "Puducherry", false),
    (35, "Andaman and Nicobar Islands", true),
    (36, "Telangana", false),
    (37, "Andhra Pradesh", false),
    (38, "Ladakh", true),
    (96, "Other Country", false),
    (97, "Other Territory", true),
];

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("unknown GST state code: {0}")]
pub struct UnknownStateCode(pub u8);

impl StateCode {
    pub fn new(code: u8) -> Result<Self, UnknownStateCode> {
        if STATES.iter().any(|(c, _, _)| *c == code) {
            Ok(StateCode(code))
        } else {
            Err(UnknownStateCode(code))
        }
    }

    pub fn code(&self) -> u8 {
        self.0
    }

    pub fn name(&self) -> &'static str {
        self.entry().1
    }

    /// True when intra-state supplies attract UTGST rather than SGST.
    pub fn levies_utgst(&self) -> bool {
        self.entry().2
    }

    pub fn all() -> impl Iterator<Item = StateCode> {
        STATES.iter().map(|(c, _, _)| StateCode(*c))
    }

    fn entry(&self) -> &'static (u8, &'static str, bool) {
        STATES
            .iter()
            .find(|(c, _, _)| *c == self.0)
            .expect("StateCode is only constructed from known codes")
    }
}

impl TryFrom<u8> for StateCode {
    type Error = UnknownStateCode;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        StateCode::new(v)
    }
}

impl From<StateCode> for u8 {
    fn from(s: StateCode) -> u8 {
        s.0
    }
}

impl fmt::Display for StateCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}-{}", self.0, self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_and_unknown() {
        assert_eq!(StateCode::new(27).unwrap().name(), "Maharashtra");
        assert!(StateCode::new(28).is_err()); // retired code
        assert!(StateCode::new(0).is_err());
    }

    #[test]
    fn utgst_flags() {
        assert!(StateCode::new(4).unwrap().levies_utgst());
        assert!(!StateCode::new(7).unwrap().levies_utgst()); // Delhi has a legislature
        assert!(!StateCode::new(27).unwrap().levies_utgst());
    }
}
