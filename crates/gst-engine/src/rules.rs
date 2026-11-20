//! Versioned GST rule pack (spec §5.6, D12).
//!
//! The pack is data, not code: rate slabs and HSN/SAC rates with
//! effective dates, return thresholds, and the notification each comes from.
//! A copy ships inside the app (`rules/rule-pack.json`); a newer pack can be
//! loaded with [`RulePack::from_json`] without changing the engine. Every
//! document records the pack version it was computed with, so historic tax
//! is never recomputed under new rules.

use std::sync::OnceLock;

use av_domain::{ItemKind, TaxCategory};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

const BUNDLED: &str = include_str!("../rules/rule-pack.json");

/// The pack format this engine understands.
pub const FORMAT: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub detail: Option<String>,
}

/// A rate slab and the dates it can be charged on (inclusive).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateRule {
    pub bp: u32,
    pub label: String,
    #[serde(default)]
    pub from: Option<NaiveDate>,
    #[serde(default)]
    pub to: Option<NaiveDate>,
    #[serde(default)]
    pub note: Option<String>,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatedRate {
    pub bp: u32,
    /// Exempt or nil-rated entries carry no tax; default taxable.
    #[serde(default = "taxable")]
    pub category: TaxCategory,
    #[serde(default)]
    pub from: Option<NaiveDate>,
    #[serde(default)]
    pub to: Option<NaiveDate>,
    pub source: String,
}

fn taxable() -> TaxCategory {
    TaxCategory::Taxable
}

/// Rates for an HSN/SAC heading. The code matches every longer code that
/// starts with it (`8415` covers `84151010`); the longest match wins.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HsnRule {
    pub code: String,
    pub kind: ItemKind,
    pub description: String,
    /// The rate depends on value, packaging or recipient: suggest it, but
    /// don't warn when a different rate is used.
    #[serde(default)]
    pub conditional: bool,
    pub rates: Vec<DatedRate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Threshold {
    pub key: String,
    pub paise: i64,
    #[serde(default)]
    pub from: Option<NaiveDate>,
    #[serde(default)]
    pub to: Option<NaiveDate>,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulePack {
    pub format: u32,
    pub version: String,
    pub published: NaiveDate,
    #[serde(default)]
    pub note: Option<String>,
    pub sources: Vec<Source>,
    pub rates: Vec<RateRule>,
    pub thresholds: Vec<Threshold>,
    pub hsn: Vec<HsnRule>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PackError {
    #[error("the rule pack isn't valid JSON: {0}")]
    Json(String),
    #[error("rule pack format {0} needs a newer version of AccountsVU")]
    Format(u32),
    #[error("rule pack problem: {0}")]
    Invalid(String),
}

/// Whether a rate may be charged on a date.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum RateStatus {
    InForce,
    /// A known slab, but not on this date.
    NotInForce {
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    },
    Unknown,
}

fn covers(from: Option<NaiveDate>, to: Option<NaiveDate>, d: NaiveDate) -> bool {
    from.is_none_or(|f| d >= f) && to.is_none_or(|t| d <= t)
}

fn overlaps(
    a: (Option<NaiveDate>, Option<NaiveDate>),
    b: (Option<NaiveDate>, Option<NaiveDate>),
) -> bool {
    let start = |x: Option<NaiveDate>| x.unwrap_or(NaiveDate::MIN);
    let end = |x: Option<NaiveDate>| x.unwrap_or(NaiveDate::MAX);
    start(a.0) <= end(b.1) && start(b.0) <= end(a.1)
}

impl RulePack {
    /// The pack bundled with this build.
    pub fn bundled() -> &'static RulePack {
        static PACK: OnceLock<RulePack> = OnceLock::new();
        PACK.get_or_init(|| RulePack::from_json(BUNDLED).expect("bundled rule pack is valid"))
    }

    pub fn from_json(json: &str) -> Result<RulePack, PackError> {
        let pack: RulePack =
            serde_json::from_str(json).map_err(|e| PackError::Json(e.to_string()))?;
        if pack.format != FORMAT {
            return Err(PackError::Format(pack.format));
        }
        pack.validate()?;
        Ok(pack)
    }

    fn validate(&self) -> Result<(), PackError> {
        let bad = |m: String| Err(PackError::Invalid(m));
        let known_source = |id: &str| self.sources.iter().any(|s| s.id == id);
        for r in &self.rates {
            if !known_source(&r.source) {
                return bad(format!(
                    "rate {} cites unknown source {}",
                    r.label, r.source
                ));
            }
        }
        for h in &self.hsn {
            if h.code.len() < 2 || h.code.len() > 8 || !h.code.bytes().all(|b| b.is_ascii_digit()) {
                return bad(format!("HSN/SAC code {:?} must be 2 to 8 digits", h.code));
            }
            if self.hsn.iter().filter(|o| o.code == h.code).count() > 1 {
                return bad(format!("HSN/SAC {} is listed twice", h.code));
            }
            if h.rates.is_empty() {
                return bad(format!("HSN/SAC {} has no rates", h.code));
            }
            for (i, r) in h.rates.iter().enumerate() {
                if !known_source(&r.source) {
                    return bad(format!(
                        "HSN/SAC {} cites unknown source {}",
                        h.code, r.source
                    ));
                }
                if r.category == TaxCategory::Taxable
                    && r.bp > 0
                    && !self.rates.iter().any(|s| s.bp == r.bp)
                {
                    return bad(format!("HSN/SAC {} uses a rate that isn't a slab", h.code));
                }
                if r.category != TaxCategory::Taxable && r.bp != 0 {
                    return bad(format!(
                        "HSN/SAC {}: exempt or nil entries carry no rate",
                        h.code
                    ));
                }
                if h.rates[i + 1..]
                    .iter()
                    .any(|o| overlaps((r.from, r.to), (o.from, o.to)))
                {
                    return bad(format!("HSN/SAC {} has overlapping dates", h.code));
                }
            }
        }
        for t in &self.thresholds {
            let same = self.thresholds.iter().filter(|o| o.key == t.key);
            if same
                .filter(|o| overlaps((t.from, t.to), (o.from, o.to)))
                .count()
                > 1
            {
                return bad(format!("threshold {} has overlapping dates", t.key));
            }
        }
        Ok(())
    }

    pub fn source(&self, id: &str) -> Option<&Source> {
        self.sources.iter().find(|s| s.id == id)
    }

    pub fn rate_status(&self, bp: u32, date: NaiveDate) -> RateStatus {
        let slabs: Vec<&RateRule> = self.rates.iter().filter(|r| r.bp == bp).collect();
        if slabs.is_empty() {
            RateStatus::Unknown
        } else if slabs.iter().any(|r| covers(r.from, r.to, date)) {
            RateStatus::InForce
        } else {
            RateStatus::NotInForce {
                from: slabs[0].from,
                to: slabs[0].to,
            }
        }
    }

    pub fn is_known_rate(&self, bp: u32) -> bool {
        self.rates.iter().any(|r| r.bp == bp)
    }

    pub fn rate_rule(&self, bp: u32) -> Option<&RateRule> {
        self.rates.iter().find(|r| r.bp == bp)
    }

    /// Slabs that can be charged on `date`, lowest first.
    pub fn rates_on(&self, date: NaiveDate) -> Vec<&RateRule> {
        let mut v: Vec<&RateRule> = self
            .rates
            .iter()
            .filter(|r| covers(r.from, r.to, date))
            .collect();
        v.sort_by_key(|r| r.bp);
        v
    }

    /// The most specific entry whose code starts the given HSN/SAC.
    pub fn hsn_rule(&self, code: &str) -> Option<&HsnRule> {
        let code = code.trim();
        self.hsn
            .iter()
            .filter(|h| code.starts_with(h.code.as_str()))
            .max_by_key(|h| h.code.len())
    }

    /// The pack's rate for an HSN/SAC on a date.
    pub fn hsn_rate(&self, code: &str, date: NaiveDate) -> Option<(&HsnRule, &DatedRate)> {
        let rule = self.hsn_rule(code)?;
        let rate = rule.rates.iter().find(|r| covers(r.from, r.to, date))?;
        Some((rule, rate))
    }

    /// By code (digits: prefix either way) or by words in the description.
    pub fn search_hsn(&self, query: &str, limit: usize) -> Vec<&HsnRule> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        if q.bytes().all(|b| b.is_ascii_digit()) {
            let mut hits: Vec<&HsnRule> = self
                .hsn
                .iter()
                .filter(|h| h.code.starts_with(&q) || q.starts_with(h.code.as_str()))
                .collect();
            hits.sort_by(|a, b| a.code.cmp(&b.code));
            hits.truncate(limit);
            return hits;
        }
        // Words of 3+ letters, matched on word starts ("soap" finds "Soap",
        // "cond" finds "conditioners"); more matching words rank higher.
        let words: Vec<&str> = q.split_whitespace().filter(|w| w.len() >= 3).collect();
        let score = |h: &HsnRule| {
            let desc = h.description.to_lowercase();
            let starts: Vec<&str> = desc
                .split(|c: char| !c.is_alphanumeric())
                .filter(|s| !s.is_empty())
                .collect();
            words
                .iter()
                .filter(|w| starts.iter().any(|s| s.starts_with(*w)))
                .count()
        };
        let mut hits: Vec<(usize, &HsnRule)> = self
            .hsn
            .iter()
            .map(|h| (score(h), h))
            .filter(|(s, _)| *s > 0)
            .collect();
        hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.code.cmp(&b.1.code)));
        hits.into_iter().take(limit).map(|(_, h)| h).collect()
    }

    pub fn threshold(&self, key: &str, date: NaiveDate) -> Option<i64> {
        self.thresholds
            .iter()
            .find(|t| t.key == key && covers(t.from, t.to, date))
            .map(|t| t.paise)
    }
}

/// "22 Sep 2025".
pub fn short_date(d: NaiveDate) -> String {
    d.format("%-d %b %Y").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn bundled_pack_loads() {
        let p = RulePack::bundled();
        assert_eq!(p.format, FORMAT);
        assert!(!p.version.is_empty());
        assert!(p.hsn.len() > 30);
    }

    #[test]
    fn rates_follow_the_september_2025_change() {
        let p = RulePack::bundled();
        let before = d("2025-09-21");
        let after = d("2025-09-22");
        assert_eq!(p.rate_status(1200, before), RateStatus::InForce);
        assert!(matches!(
            p.rate_status(1200, after),
            RateStatus::NotInForce { .. }
        ));
        assert!(matches!(
            p.rate_status(4000, before),
            RateStatus::NotInForce { .. }
        ));
        assert_eq!(p.rate_status(4000, after), RateStatus::InForce);
        assert_eq!(p.rate_status(1800, after), RateStatus::InForce);
        assert_eq!(p.rate_status(1300, after), RateStatus::Unknown);
        let now: Vec<u32> = p.rates_on(after).iter().map(|r| r.bp).collect();
        assert!(now.contains(&500) && now.contains(&4000) && !now.contains(&1200));
    }

    #[test]
    fn hsn_longest_prefix_and_dates() {
        let p = RulePack::bundled();
        let (rule, rate) = p.hsn_rate("84151010", d("2026-04-01")).unwrap();
        assert_eq!(rule.code, "8415");
        assert_eq!(rate.bp, 1800);
        assert_eq!(p.hsn_rate("8415", d("2025-01-10")).unwrap().1.bp, 2800);
        // 220210 is more specific than nothing; 2202 alone isn't listed.
        assert_eq!(p.hsn_rule("22021010").unwrap().code, "220210");
        assert!(p.hsn_rule("2202").is_none());
        assert!(p.hsn_rule("1234").is_none());
    }

    #[test]
    fn search_by_code_or_words() {
        let p = RulePack::bundled();
        assert_eq!(p.search_hsn("841", 5)[0].code, "8415");
        assert_eq!(p.search_hsn("air cond", 5)[0].code, "8415");
        // Extra words don't hide a match; best match first.
        assert_eq!(p.search_hsn("bath soap", 5)[0].code, "3401");
        assert_eq!(p.search_hsn("Split AC 1.5 ton", 5).len(), 0);
        assert!(p.search_hsn("", 5).is_empty());
        assert!(p.search_hsn("a b", 5).is_empty());
    }

    #[test]
    fn b2cl_threshold_changed_in_august_2024() {
        let p = RulePack::bundled();
        assert_eq!(
            p.threshold("b2clInvoiceValue", d("2024-07-31")),
            Some(25_000_000)
        );
        assert_eq!(
            p.threshold("b2clInvoiceValue", d("2024-08-01")),
            Some(10_000_000)
        );
    }

    #[test]
    fn rejects_broken_packs() {
        let mut v: serde_json::Value = serde_json::from_str(BUNDLED).unwrap();
        v["format"] = 2.into();
        assert_eq!(
            RulePack::from_json(&v.to_string()),
            Err(PackError::Format(2))
        );
        let mut v: serde_json::Value = serde_json::from_str(BUNDLED).unwrap();
        v["hsn"][0]["rates"][0]["source"] = "NOPE".into();
        assert!(matches!(
            RulePack::from_json(&v.to_string()),
            Err(PackError::Invalid(_))
        ));
        let mut v: serde_json::Value = serde_json::from_str(BUNDLED).unwrap();
        v["hsn"][1]["rates"][1]["from"] = "2025-01-01".into();
        assert!(matches!(
            RulePack::from_json(&v.to_string()),
            Err(PackError::Invalid(_))
        ));
        assert!(matches!(RulePack::from_json("{"), Err(PackError::Json(_))));
    }
}
