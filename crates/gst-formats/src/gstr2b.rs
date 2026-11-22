//! GSTR-2B import (Phase 2.4): reads the JSON statement downloaded from the
//! GST portal (Returns → GSTR-2B → Download → JSON).
//!
//! Only the sections that can be matched against the purchase register are
//! turned into lines: B2B invoices, B2B credit/debit notes and their
//! amendments (`b2b`, `b2ba`, `cdnr`, `cdnra`). ISD credit and imports
//! (`isd`, `impg`, `impgsez`) are counted and reported, not matched.
//!
//! Amounts arrive as JSON numbers in rupees. They are converted to paise from
//! their decimal text, never through floating-point arithmetic.

use serde_json::{Map, Value};

pub const ADAPTER_VERSION: &str = "gstr2b-json/1";

/// What kind of supplier document a line is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocKind {
    Invoice,
    CreditNote,
    DebitNote,
}

impl DocKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Invoice => "invoice",
            Self::CreditNote => "credit_note",
            Self::DebitNote => "debit_note",
        }
    }
}

/// One supplier document from the statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// `b2b`, `b2ba`, `cdnr` or `cdnra`.
    pub section: &'static str,
    pub supplier_gstin: String,
    pub supplier_name: String,
    pub kind: DocKind,
    pub number: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// Document value as the supplier reported it.
    pub value_paise: i64,
    pub taxable_paise: i64,
    pub igst_paise: i64,
    pub cgst_paise: i64,
    pub sgst_paise: i64,
    pub cess_paise: i64,
    /// Two-digit state code, when given.
    pub place_of_supply: Option<String>,
    pub reverse_charge: bool,
    /// The portal's "ITC available" flag (`itcavl`).
    pub itc_available: bool,
    /// The portal's reason when ITC is not available (`rsn`).
    pub itc_reason: Option<String>,
    /// Supplier's GSTR-1/IFF filing date, `YYYY-MM-DD`.
    pub supplier_filed_on: Option<String>,
    /// Supplier's return period, `YYYY-MM`.
    pub supplier_period: Option<String>,
    /// For amendments: the original document number.
    pub original_number: Option<String>,
}

/// A parsed statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statement {
    /// Recipient GSTIN (ours).
    pub gstin: String,
    /// Return period, `YYYY-MM`.
    pub period: String,
    /// Generation date, `YYYY-MM-DD`, when given.
    pub generated_on: Option<String>,
    pub version: Option<String>,
    pub lines: Vec<Line>,
    /// Documents in sections that are not reconciled (ISD, imports), by section.
    pub skipped: Vec<(String, usize)>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("this isn't a GSTR-2B JSON file: {0}")]
    NotGstr2b(String),
    #[error("{section} document {index}: {problem}")]
    Line {
        section: String,
        index: usize,
        problem: String,
    },
}

/// Parses a GSTR-2B JSON download.
pub fn parse(bytes: &[u8]) -> Result<Statement, ParseError> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let root: Value = serde_json::from_slice(bytes)
        .map_err(|e| ParseError::NotGstr2b(format!("not valid JSON ({e})")))?;
    // The download wraps everything in `data`; some tools strip it.
    let data = root.get("data").unwrap_or(&root);
    let data = data
        .as_object()
        .ok_or_else(|| ParseError::NotGstr2b("expected a JSON object".into()))?;
    let gstin = text(data, "gstin")
        .ok_or_else(|| ParseError::NotGstr2b("the recipient GSTIN (gstin) is missing".into()))?
        .to_uppercase();
    let rtnprd = text(data, "rtnprd")
        .ok_or_else(|| ParseError::NotGstr2b("the return period (rtnprd) is missing".into()))?;
    let period = period_of(&rtnprd).ok_or_else(|| {
        ParseError::NotGstr2b(format!("the return period \"{rtnprd}\" isn't MMYYYY"))
    })?;
    let docdata = data
        .get("docdata")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            ParseError::NotGstr2b("the document details (docdata) are missing".into())
        })?;

    let mut lines = Vec::new();
    let mut skipped = Vec::new();
    for (section, list_key) in [
        ("b2b", "inv"),
        ("b2ba", "inv"),
        ("cdnr", "nt"),
        ("cdnra", "nt"),
    ] {
        let Some(suppliers) = docdata.get(section).and_then(Value::as_array) else {
            continue;
        };
        let mut index = 0;
        for s in suppliers {
            let Some(s) = s.as_object() else { continue };
            let ctin = text(s, "ctin").unwrap_or_default().to_uppercase();
            let name = text(s, "trdnm").unwrap_or_default();
            let filed = text(s, "supfildt").and_then(|d| date_of(&d));
            let sup_period = text(s, "supprd").and_then(|p| period_of(&p));
            for d in s
                .get(list_key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                index += 1;
                let Some(d) = d.as_object() else { continue };
                let line =
                    doc_line(section, d, &ctin, &name, &filed, &sup_period).map_err(|problem| {
                        ParseError::Line {
                            section: section.to_uppercase(),
                            index,
                            problem,
                        }
                    })?;
                lines.push(line);
            }
        }
    }
    for (k, v) in docdata {
        if matches!(k.as_str(), "b2b" | "b2ba" | "cdnr" | "cdnra") {
            continue;
        }
        let n = match v {
            Value::Array(a) => a
                .iter()
                .map(|e| {
                    e.as_object()
                        .and_then(|o| o.values().find_map(|v| v.as_array().map(Vec::len)))
                        .unwrap_or(1)
                })
                .sum(),
            _ => 0,
        };
        if n > 0 {
            skipped.push((k.to_uppercase(), n));
        }
    }
    Ok(Statement {
        gstin,
        period,
        generated_on: text(data, "gendt").and_then(|d| date_of(&d)),
        version: text(data, "version"),
        lines,
        skipped,
    })
}

fn doc_line(
    section: &'static str,
    d: &Map<String, Value>,
    ctin: &str,
    name: &str,
    filed: &Option<String>,
    sup_period: &Option<String>,
) -> Result<Line, String> {
    let notes = section.starts_with("cdn");
    let number = text(d, if notes { "ntnum" } else { "inum" })
        .ok_or_else(|| "the document number is missing".to_string())?;
    let raw_date = text(d, "dt").ok_or_else(|| format!("{number}: the date is missing"))?;
    let date = date_of(&raw_date)
        .ok_or_else(|| format!("{number}: the date \"{raw_date}\" isn't DD-MM-YYYY"))?;
    let kind = if notes {
        match text(d, "typ").as_deref() {
            Some("C") => DocKind::CreditNote,
            Some("D") => DocKind::DebitNote,
            other => return Err(format!("{number}: unknown note type {other:?}")),
        }
    } else {
        DocKind::Invoice
    };
    let amount = |key: &str| -> Result<i64, String> {
        match d.get(key) {
            None | Some(Value::Null) => Ok(0),
            Some(v) => paise(v).ok_or_else(|| format!("{number}: {key} isn't an amount")),
        }
    };
    // Invoice-level totals are given; older files only carry item rows.
    let items = d.get("items").and_then(Value::as_array);
    let sum_items = |key: &str| -> Result<i64, String> {
        let mut t = 0i64;
        for it in items.into_iter().flatten() {
            if let Some(v) = it.get(key) {
                t += paise(v).ok_or_else(|| format!("{number}: item {key} isn't an amount"))?;
            }
        }
        Ok(t)
    };
    let head = |key: &str| -> Result<i64, String> {
        if d.contains_key(key) {
            amount(key)
        } else {
            sum_items(key)
        }
    };
    Ok(Line {
        section,
        supplier_gstin: ctin.to_string(),
        supplier_name: name.to_string(),
        kind,
        number: number.trim().to_string(),
        date,
        value_paise: amount("val")?,
        taxable_paise: head("txval")?,
        igst_paise: head("igst")?,
        cgst_paise: head("cgst")?,
        sgst_paise: head("sgst")?,
        cess_paise: head("cess")?,
        place_of_supply: text(d, "pos"),
        reverse_charge: text(d, "rev").is_some_and(|r| r.eq_ignore_ascii_case("Y")),
        itc_available: !text(d, "itcavl").is_some_and(|r| r.eq_ignore_ascii_case("N")),
        itc_reason: text(d, "rsn").filter(|r| !r.is_empty()),
        supplier_filed_on: filed.clone(),
        supplier_period: sup_period.clone(),
        original_number: text(d, if notes { "ontnum" } else { "oinum" }),
    })
}

fn text(o: &Map<String, Value>, key: &str) -> Option<String> {
    match o.get(key)? {
        Value::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// `MMYYYY` → `YYYY-MM`.
fn period_of(p: &str) -> Option<String> {
    let p = p.trim();
    if p.len() != 6 || !p.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (m, y) = p.split_at(2);
    let month: u32 = m.parse().ok()?;
    (1..=12).contains(&month).then(|| format!("{y}-{m}"))
}

/// `DD-MM-YYYY` (or `DD/MM/YYYY`) → `YYYY-MM-DD`.
fn date_of(d: &str) -> Option<String> {
    let parts: Vec<&str> = d.trim().split(['-', '/']).collect();
    let [dd, mm, yyyy] = parts.as_slice() else {
        return None;
    };
    let day: u32 = dd.parse().ok()?;
    let month: u32 = mm.parse().ok()?;
    let year: i32 = yyyy.parse().ok()?;
    chrono::NaiveDate::from_ymd_opt(year, month, day).map(|d| d.format("%Y-%m-%d").to_string())
}

/// A JSON number (or numeric string) in rupees → paise, rounded half away
/// from zero at two decimals.
fn paise(v: &Value) -> Option<i64> {
    let s = match v {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.trim().to_string(),
        _ => return None,
    };
    decimal_paise(&s)
}

fn decimal_paise(s: &str) -> Option<i64> {
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    // Exponent forms (`1e3`) only appear for whole numbers in practice.
    if let Some((m, e)) = s.split_once(['e', 'E']) {
        let exp: i32 = e.parse().ok()?;
        let base = decimal_paise(m)?;
        let v = if exp >= 0 {
            base.checked_mul(10i64.checked_pow(exp.unsigned_abs())?)?
        } else {
            let div = 10i64.checked_pow(exp.unsigned_abs())?;
            (base + div / 2) / div
        };
        return Some(if neg { -v } else { v });
    }
    let (whole, frac) = s.split_once('.').unwrap_or((s, ""));
    if whole.is_empty() && frac.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !frac.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let w: i64 = if whole.is_empty() {
        0
    } else {
        whole.parse().ok()?
    };
    let mut f = frac.bytes().take(3).map(|b| i64::from(b - b'0'));
    let d1 = f.next().unwrap_or(0);
    let d2 = f.next().unwrap_or(0);
    let d3 = f.next().unwrap_or(0);
    let mut v = w.checked_mul(100)?.checked_add(d1 * 10 + d2)?;
    if d3 >= 5 {
        v += 1;
    }
    Some(if neg { -v } else { v })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_become_paise_exactly() {
        assert_eq!(decimal_paise("11800"), Some(1_180_000));
        assert_eq!(decimal_paise("0.1"), Some(10));
        assert_eq!(decimal_paise("1234.5"), Some(123_450));
        assert_eq!(decimal_paise("1234.567"), Some(123_457));
        assert_eq!(decimal_paise("-90.05"), Some(-9005));
        assert_eq!(decimal_paise("1e3"), Some(100_000));
        assert_eq!(decimal_paise("abc"), None);
        assert_eq!(date_of("05-04-2026").as_deref(), Some("2026-04-05"));
        assert_eq!(date_of("31-02-2026"), None);
        assert_eq!(period_of("042026").as_deref(), Some("2026-04"));
        assert_eq!(period_of("132026"), None);
    }

    #[test]
    fn parses_a_portal_statement() {
        let json = br#"{"chksum":"x","data":{"gstin":"27aaaaa0000a1z5","rtnprd":"042026","version":"1.0","gendt":"14-05-2026",
          "docdata":{
            "b2b":[{"ctin":"29BBBBB1111B1Z5","trdnm":"Supplier One","supfildt":"11-05-2026","supprd":"042026",
               "inv":[{"inum":"S1/001","typ":"R","dt":"02-04-2026","val":11800,"pos":"27","rev":"N","itcavl":"Y","rsn":"",
                       "txval":10000,"igst":1800,"cgst":0,"sgst":0,"cess":0},
                      {"inum":"S1/002","typ":"R","dt":"03-04-2026","val":590.5,"pos":"27","rev":"Y","itcavl":"N","rsn":"P",
                       "items":[{"num":1,"rt":18,"txval":500.42,"igst":90.08,"cess":0}]}]}],
            "cdnr":[{"ctin":"29BBBBB1111B1Z5","trdnm":"Supplier One","supfildt":"11-05-2026","supprd":"042026",
               "nt":[{"ntnum":"CN-7","typ":"C","suptyp":"R","dt":"20-04-2026","val":1180,"pos":"27","rev":"N","itcavl":"Y",
                      "txval":1000,"igst":180,"cgst":0,"sgst":0,"cess":0}]}],
            "impg":[{"refdt":"01-04-2026","boenum":"123","txval":100,"igst":18}]
          }}}"#;
        let s = parse(json).unwrap();
        assert_eq!(s.gstin, "27AAAAA0000A1Z5");
        assert_eq!(s.period, "2026-04");
        assert_eq!(s.generated_on.as_deref(), Some("2026-05-14"));
        assert_eq!(s.lines.len(), 3);
        let a = &s.lines[0];
        assert_eq!(
            (a.kind, a.number.as_str(), a.date.as_str()),
            (DocKind::Invoice, "S1/001", "2026-04-02")
        );
        assert_eq!(
            (a.taxable_paise, a.igst_paise, a.value_paise),
            (1_000_000, 180_000, 1_180_000)
        );
        let b = &s.lines[1];
        assert_eq!((b.taxable_paise, b.igst_paise), (50_042, 9008));
        assert!(b.reverse_charge && !b.itc_available);
        assert_eq!(b.itc_reason.as_deref(), Some("P"));
        assert_eq!(s.lines[2].kind, DocKind::CreditNote);
        assert_eq!(s.lines[2].supplier_period.as_deref(), Some("2026-04"));
        assert_eq!(s.skipped, vec![("IMPG".to_string(), 1)]);
    }

    #[test]
    fn rejects_other_files() {
        assert!(matches!(parse(b"[1,2]"), Err(ParseError::NotGstr2b(_))));
        assert!(matches!(
            parse(br#"{"data":{"gstin":"X","rtnprd":"042026"}}"#),
            Err(ParseError::NotGstr2b(_))
        ));
        let bad = br#"{"data":{"gstin":"X","rtnprd":"042026","docdata":{"b2b":[{"ctin":"Y","inv":[{"inum":"1","dt":"2026-04-01"}]}]}}}"#;
        assert!(matches!(parse(bad), Err(ParseError::Line { .. })));
    }
}
