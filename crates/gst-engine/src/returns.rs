//! Versioned return pack (Phase 2.5).
//!
//! Which returns there are, when they fall due, what filing them locks, and
//! every check a return can raise with its severity: an `error` blocks the
//! filing pack and recording the return as filed; a `warning` doesn't. Like
//! the rule pack it is data shipped with the app (`rules/return-pack.json`),
//! and each filing records the pack version it was validated with.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const BUNDLED: &str = include_str!("../rules/return-pack.json");

/// The pack format this engine understands.
pub const FORMAT: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReturnDef {
    pub id: String,
    pub label: String,
    /// Day of the following month the return is due.
    pub due_day: u32,
    /// Document families a filed return locks: `sales`, `purchases`.
    pub locks: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckDef {
    pub code: String,
    pub returns: Vec<String>,
    pub severity: Severity,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReturnPack {
    pub format: u32,
    pub version: String,
    pub returns: Vec<ReturnDef>,
    pub checks: Vec<CheckDef>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReturnPackError {
    #[error("the return pack isn't valid JSON: {0}")]
    Json(String),
    #[error("return pack format {0} isn't supported")]
    Format(u32),
    #[error("the return pack is invalid: {0}")]
    Invalid(String),
}

impl ReturnPack {
    /// The pack bundled with this build.
    pub fn bundled() -> &'static ReturnPack {
        static PACK: OnceLock<ReturnPack> = OnceLock::new();
        PACK.get_or_init(|| ReturnPack::from_json(BUNDLED).expect("bundled return pack is valid"))
    }

    pub fn from_json(json: &str) -> Result<ReturnPack, ReturnPackError> {
        let pack: ReturnPack =
            serde_json::from_str(json).map_err(|e| ReturnPackError::Json(e.to_string()))?;
        if pack.format != FORMAT {
            return Err(ReturnPackError::Format(pack.format));
        }
        pack.validate()?;
        Ok(pack)
    }

    fn validate(&self) -> Result<(), ReturnPackError> {
        let bad = |m: String| Err(ReturnPackError::Invalid(m));
        if self.version.trim().is_empty() {
            return bad("version is empty".into());
        }
        for (i, r) in self.returns.iter().enumerate() {
            if !(1..=28).contains(&r.due_day) {
                return bad(format!("{}: due day {} isn't 1–28", r.id, r.due_day));
            }
            if self.returns[..i].iter().any(|o| o.id == r.id) {
                return bad(format!("return {} is listed twice", r.id));
            }
            if let Some(l) = r
                .locks
                .iter()
                .find(|l| !matches!(l.as_str(), "sales" | "purchases"))
            {
                return bad(format!("{}: unknown lock {l}", r.id));
            }
        }
        for (i, c) in self.checks.iter().enumerate() {
            if c.returns.is_empty() {
                return bad(format!("check {} applies to no return", c.code));
            }
            if let Some(r) = c.returns.iter().find(|r| self.return_def(r).is_none()) {
                return bad(format!("check {}: unknown return {r}", c.code));
            }
            let dup = self.checks[..i]
                .iter()
                .any(|o| o.code == c.code && o.returns.iter().any(|r| c.returns.contains(r)));
            if dup {
                return bad(format!("check {} is listed twice", c.code));
            }
        }
        Ok(())
    }

    pub fn return_def(&self, id: &str) -> Option<&ReturnDef> {
        self.returns.iter().find(|r| r.id == id)
    }

    /// The check `code` raised by return `return_id`, if the pack knows it.
    pub fn check(&self, return_id: &str, code: &str) -> Option<&CheckDef> {
        self.checks
            .iter()
            .find(|c| c.code == code && c.returns.iter().any(|r| r == return_id))
    }

    /// Severity of a check; unknown checks block, so a new check can't slip
    /// through as a warning by accident.
    pub fn severity(&self, return_id: &str, code: &str) -> Severity {
        self.check(return_id, code)
            .map_or(Severity::Error, |c| c.severity)
    }

    /// Whether a filed `return_id` locks the document family (`sales`, `purchases`).
    pub fn locks(&self, return_id: &str, family: &str) -> bool {
        self.return_def(return_id)
            .is_some_and(|r| r.locks.iter().any(|l| l == family))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_pack_loads() {
        let p = ReturnPack::bundled();
        assert_eq!(p.return_def("gstr1").map(|r| r.due_day), Some(11));
        assert_eq!(p.return_def("gstr3b").map(|r| r.due_day), Some(20));
        assert_eq!(p.severity("gstr1", "missing_hsn"), Severity::Error);
        assert_eq!(p.severity("gstr3b", "itc_pending"), Severity::Warning);
        assert_eq!(p.severity("gstr3b", "no_such_check"), Severity::Error);
        assert!(p.locks("gstr1", "sales"));
        assert!(!p.locks("gstr1", "purchases"));
        assert!(p.locks("gstr3b", "purchases"));
    }

    #[test]
    fn invalid_packs_are_refused() {
        let base = r#"{"format":1,"version":"x","returns":[{"id":"gstr1","label":"GSTR-1","dueDay":11,"locks":["sales"]}],"checks":[CHECKS]}"#;
        let with = |c: &str| base.replace("CHECKS", c);
        assert!(ReturnPack::from_json(&with("")).is_ok());
        assert!(matches!(
            ReturnPack::from_json(&with(
                r#"{"code":"a","returns":["gstr9"],"severity":"error","title":"t"}"#
            )),
            Err(ReturnPackError::Invalid(_))
        ));
        assert!(matches!(
            ReturnPack::from_json(&with(
                r#"{"code":"a","returns":["gstr1"],"severity":"error","title":"t"},{"code":"a","returns":["gstr1"],"severity":"warning","title":"t"}"#
            )),
            Err(ReturnPackError::Invalid(_))
        ));
        assert!(matches!(
            ReturnPack::from_json(
                &base
                    .replace("\"format\":1", "\"format\":2")
                    .replace("CHECKS", "")
            ),
            Err(ReturnPackError::Format(2))
        ));
    }
}
