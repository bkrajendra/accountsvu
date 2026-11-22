//! GSTR-2B reconciliation (Phase 2.4): matches the purchase register against
//! the supplier documents in GSTR-2B.
//!
//! Rules, in order:
//! 1. **Duplicate** — a second 2B document with the same supplier GSTIN, kind
//!    and number (numbers compared ignoring case, punctuation and leading
//!    zeros) as an earlier one.
//! 2. **Exact** — same GSTIN, kind and number; date equal and taxable value
//!    and every tax head within the tolerance.
//! 3. **Mismatch** — same GSTIN, kind and number, but the date or an amount
//!    differs; the differences are listed.
//! 4. **Likely** — same GSTIN and kind, number differs, taxable value and
//!    every head within the tolerance, and either the same date or the same
//!    trailing digits in the number.
//! 5. Whatever is left is **missing in 2B** (books) or **missing in books**
//!    (2B).
//!
//! A match never decides eligibility: input tax credit still follows the
//! review on each purchase line.

/// A document on either side, amounts in paise.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Doc {
    pub gstin: String,
    /// `invoice`, `credit_note` or `debit_note`, from the supplier's side.
    pub kind: String,
    pub number: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    pub taxable: i64,
    pub igst: i64,
    pub cgst: i64,
    pub sgst: i64,
    pub cess: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchKind {
    Exact,
    Likely,
    Mismatch,
}

/// A field that differs between the books and 2B.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    /// `number`, `date`, `taxable`, `igst`, `cgst`, `sgst` or `cess`.
    pub field: &'static str,
    pub books: String,
    pub portal: String,
    /// Portal minus books, for amounts.
    pub delta_paise: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pair {
    pub book: usize,
    pub portal: usize,
    pub kind: MatchKind,
    pub differences: Vec<Difference>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub pairs: Vec<Pair>,
    /// Book documents with no 2B partner.
    pub missing_in_portal: Vec<usize>,
    /// 2B documents with no book partner.
    pub missing_in_books: Vec<usize>,
    /// 2B documents repeating an earlier one: (duplicate, first).
    pub duplicates: Vec<(usize, usize)>,
}

/// Document numbers compared ignoring case, punctuation and leading zeros
/// in each run of digits: `INV/0042` and `inv-42` are the same number.
pub fn normalise_number(n: &str) -> String {
    runs(n).concat()
}

/// Letter and digit runs, upper-cased, digit runs without leading zeros;
/// punctuation separates runs.
fn runs(n: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_digits = false;
    let flush = |cur: &mut String, digits: bool, out: &mut Vec<String>| {
        if cur.is_empty() {
            return;
        }
        if digits {
            let t = cur.trim_start_matches('0');
            out.push(if t.is_empty() { "0".into() } else { t.into() });
        } else {
            out.push(cur.clone());
        }
        cur.clear();
    };
    for c in n.chars() {
        if !c.is_ascii_alphanumeric() {
            flush(&mut cur, cur_digits, &mut out);
            continue;
        }
        let d = c.is_ascii_digit();
        if d != cur_digits {
            flush(&mut cur, cur_digits, &mut out);
            cur_digits = d;
        }
        cur.push(c.to_ascii_uppercase());
    }
    flush(&mut cur, cur_digits, &mut out);
    out
}

/// The last run of digits, without leading zeros (`S1/0042` → `42`).
fn trailing_digits(n: &str) -> Option<String> {
    runs(n)
        .into_iter()
        .rev()
        .find(|r| r.bytes().all(|b| b.is_ascii_digit()))
}

fn amounts(d: &Doc) -> [(&'static str, i64); 5] {
    [
        ("taxable", d.taxable),
        ("igst", d.igst),
        ("cgst", d.cgst),
        ("sgst", d.sgst),
        ("cess", d.cess),
    ]
}

fn differences(b: &Doc, p: &Doc, tolerance: i64) -> Vec<Difference> {
    let mut out = Vec::new();
    if normalise_number(&b.number) != normalise_number(&p.number) {
        out.push(Difference {
            field: "number",
            books: b.number.clone(),
            portal: p.number.clone(),
            delta_paise: None,
        });
    }
    if b.date != p.date {
        out.push(Difference {
            field: "date",
            books: b.date.clone(),
            portal: p.date.clone(),
            delta_paise: None,
        });
    }
    for ((field, bv), (_, pv)) in amounts(b).into_iter().zip(amounts(p)) {
        if (pv - bv).abs() > tolerance {
            out.push(Difference {
                field,
                books: bv.to_string(),
                portal: pv.to_string(),
                delta_paise: Some(pv - bv),
            });
        }
    }
    out
}

fn amounts_agree(b: &Doc, p: &Doc, tolerance: i64) -> bool {
    amounts(b)
        .into_iter()
        .zip(amounts(p))
        .all(|((_, bv), (_, pv))| (pv - bv).abs() <= tolerance)
}

fn day_gap(a: &str, b: &str) -> i64 {
    let parse = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
    match (parse(a), parse(b)) {
        (Some(a), Some(b)) => (a - b).num_days().abs(),
        _ => i64::MAX,
    }
}

/// Matches `books` against `portal`; `tolerance` is the largest difference
/// (in paise, per amount) still treated as equal.
pub fn reconcile(books: &[Doc], portal: &[Doc], tolerance: i64) -> Outcome {
    let key = |d: &Doc| {
        (
            d.gstin.to_ascii_uppercase(),
            d.kind.clone(),
            normalise_number(&d.number),
        )
    };
    let mut out = Outcome::default();
    let mut book_used = vec![false; books.len()];
    let mut portal_done = vec![false; portal.len()];

    // 1. Duplicates within 2B.
    for i in 0..portal.len() {
        if let Some(first) = (0..i).find(|&j| {
            !out.duplicates.iter().any(|d| d.0 == j) && key(&portal[j]) == key(&portal[i])
        }) {
            out.duplicates.push((i, first));
            portal_done[i] = true;
        }
    }

    // 2–3. Same number: exact or mismatch.
    for (pi, p) in portal.iter().enumerate() {
        if portal_done[pi] {
            continue;
        }
        let k = key(p);
        if let Some(bi) = (0..books.len()).find(|&bi| !book_used[bi] && key(&books[bi]) == k) {
            let diffs = differences(&books[bi], p, tolerance);
            out.pairs.push(Pair {
                book: bi,
                portal: pi,
                kind: if diffs.is_empty() {
                    MatchKind::Exact
                } else {
                    MatchKind::Mismatch
                },
                differences: diffs,
            });
            book_used[bi] = true;
            portal_done[pi] = true;
        }
    }

    // 4. Likely: same supplier and kind, amounts agree, and the same date or
    // the same trailing digits. The closest date wins.
    for (pi, p) in portal.iter().enumerate() {
        if portal_done[pi] {
            continue;
        }
        let digits = trailing_digits(&p.number);
        let best = (0..books.len())
            .filter(|&bi| {
                let b = &books[bi];
                !book_used[bi]
                    && b.gstin.eq_ignore_ascii_case(&p.gstin)
                    && b.kind == p.kind
                    && amounts_agree(b, p, tolerance)
                    && (b.date == p.date
                        || (digits.is_some() && trailing_digits(&b.number) == digits))
            })
            .min_by_key(|&bi| day_gap(&books[bi].date, &p.date));
        if let Some(bi) = best {
            out.pairs.push(Pair {
                book: bi,
                portal: pi,
                kind: MatchKind::Likely,
                differences: differences(&books[bi], p, tolerance),
            });
            book_used[bi] = true;
            portal_done[pi] = true;
        }
    }

    out.missing_in_books = (0..portal.len()).filter(|&i| !portal_done[i]).collect();
    out.missing_in_portal = (0..books.len()).filter(|&i| !book_used[i]).collect();
    out.pairs.sort_by_key(|p| p.portal);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(gstin: &str, kind: &str, number: &str, date: &str, taxable: i64, igst: i64) -> Doc {
        Doc {
            gstin: gstin.into(),
            kind: kind.into(),
            number: number.into(),
            date: date.into(),
            taxable,
            igst,
            ..Doc::default()
        }
    }

    #[test]
    fn numbers_ignore_case_punctuation_and_leading_zeros() {
        assert_eq!(normalise_number("INV/0042"), normalise_number("inv-42"));
        assert_eq!(normalise_number("S1/001"), "S11");
        assert_eq!(normalise_number("A-0"), "A0");
        assert_ne!(normalise_number("INV/42"), normalise_number("INV/420"));
        assert_eq!(trailing_digits("S1/0042").as_deref(), Some("42"));
    }

    #[test]
    fn sorts_every_kind_of_outcome() {
        let g = "29BBBBB1111B1Z5";
        let books = vec![
            doc(g, "invoice", "S1/001", "2026-04-02", 1_000_000, 180_000), // exact
            doc(g, "invoice", "S1/002", "2026-04-03", 50_000, 9_000),      // mismatch (tax)
            doc(g, "invoice", "77", "2026-04-10", 200_000, 36_000),        // likely (digits)
            doc(g, "invoice", "S1/009", "2026-04-28", 10_000, 1_800),      // missing in 2B
            doc(g, "credit_note", "DN-1", "2026-04-20", 100_000, 18_000),  // likely (date)
        ];
        let portal = vec![
            doc(g, "invoice", "s1-1", "2026-04-02", 1_000_050, 180_000),
            doc(g, "invoice", "S1/002", "2026-04-03", 50_000, 9_900),
            doc(g, "invoice", "S1/077", "2026-04-11", 200_000, 36_000),
            doc(g, "invoice", "S1/001", "2026-04-02", 1_000_000, 180_000), // duplicate
            doc(g, "invoice", "X-5", "2026-04-05", 5_000, 900),            // missing in books
            doc(g, "credit_note", "CN-7", "2026-04-20", 100_000, 18_000),
        ];
        let o = reconcile(&books, &portal, 100);
        assert_eq!(o.duplicates, vec![(3, 0)]);
        let kinds: Vec<_> = o.pairs.iter().map(|p| (p.portal, p.book, p.kind)).collect();
        assert_eq!(
            kinds,
            vec![
                (0, 0, MatchKind::Exact),
                (1, 1, MatchKind::Mismatch),
                (2, 2, MatchKind::Likely),
                (5, 4, MatchKind::Likely),
            ]
        );
        let m = &o.pairs[1].differences;
        assert_eq!(m.len(), 1);
        assert_eq!((m[0].field, m[0].delta_paise), ("igst", Some(900)));
        assert_eq!(o.missing_in_books, vec![4]);
        assert_eq!(o.missing_in_portal, vec![3]);
    }

    #[test]
    fn a_different_supplier_never_matches() {
        let b = vec![doc(
            "29AAAAA0000A1Z5",
            "invoice",
            "1",
            "2026-04-01",
            100,
            18,
        )];
        let p = vec![doc(
            "27AAAAA0000A1Z5",
            "invoice",
            "1",
            "2026-04-01",
            100,
            18,
        )];
        let o = reconcile(&b, &p, 100);
        assert!(o.pairs.is_empty());
        assert_eq!(
            (o.missing_in_books.len(), o.missing_in_portal.len()),
            (1, 1)
        );
    }
}
