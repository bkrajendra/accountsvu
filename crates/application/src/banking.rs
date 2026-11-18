//! Phase 1.5: cash and bank accounts, opening balances, receipts and payments
//! with allocation to invoices and bills, cash/bank books, bank statement
//! import and reconciliation.
//!
//! Receipts and payments post immediately (there are no drafts): a mistake is
//! corrected by cancelling, which posts a reversal and releases whatever the
//! voucher had settled. Statement lines never post anything by themselves —
//! they are matched to entries already in the books, or used to create a
//! receipt or payment that is matched in the same step.

use std::collections::HashMap;

use av_domain::{FinancialYear, new_id};
use av_persistence::records::{
    AccountOpeningRecord, AccountRecord, BookLineRecord, MoneyAccountRecord, MoneyVoucherRecord,
    OpenDocumentRecord, StatementLineRecord, VoucherAllocationRecord,
};
use av_persistence::{
    AccountRepo, BankingRepo, LedgerQueries, PartyRepo, SalesRepo, Store, UnitOfWork,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::AppCore;
use crate::context::{Permission, RequestContext};
use crate::error::{AppError, AppResult, Errors};
use crate::import::read_grid;
use crate::masters::parse_amount;
use crate::posting::{PostLine, Posting, post};
use crate::purchases::sha256_hex;
use crate::setup::{Change, audit, check_len, clean, json, parse_date};

/// Ledger roles that never take a direct opening balance or voucher line:
/// parties carry receivables/payables, and the balancing account is internal.
const PARTY_CONTROLLED: &[&str] = &["receivables", "payables", "opening_balance"];

const MODES: &[&str] = &[
    "cash", "upi", "neft", "rtgs", "imps", "cheque", "card", "other",
];

/// How close (in days) a statement line and a ledger entry must be to be
/// suggested as a match.
const MATCH_WINDOW_DAYS: i64 = 7;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoneyAccountInput {
    /// `cash` or `bank` (fixed once created).
    pub kind: String,
    pub name: String,
    pub bank_name: Option<String>,
    pub account_number: Option<String>,
    pub ifsc: Option<String>,
    /// Opening balance as at the books start date. Blank keeps it at zero.
    pub opening_balance: Option<String>,
    /// `debit` (money in the account, the usual case) or `credit` (overdraft).
    pub opening_balance_side: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoneyAccountView {
    #[serde(flatten)]
    pub account: MoneyAccountRecord,
    pub ledger_code: String,
    pub system_role: Option<String>,
    pub balance_paise: i64,
    pub opening_balance_paise: i64,
    pub unmatched_lines: i64,
    pub last_statement_date: Option<String>,
    pub statement_balance_paise: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpeningBalanceView {
    pub account_id: String,
    pub code: String,
    pub name: String,
    pub kind: String,
    pub system_role: Option<String>,
    pub amount_paise: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpeningBalanceInput {
    pub account_id: String,
    pub amount: Option<String>,
    /// `debit` or `credit`.
    pub side: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationInput {
    pub document_id: String,
    pub amount: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoucherInput {
    /// `receipt` or `payment`.
    pub voucher_type: String,
    pub voucher_date: String,
    pub money_account_id: String,
    pub party_id: Option<String>,
    pub counter_account_id: Option<String>,
    pub amount: String,
    pub mode: Option<String>,
    pub reference: Option<String>,
    pub narration: Option<String>,
    #[serde(default)]
    pub allocations: Vec<AllocationInput>,
    /// Match the new voucher to this bank statement line.
    pub statement_line_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoucherSummary {
    pub id: String,
    pub voucher_type: String,
    pub status: String,
    pub number: String,
    pub voucher_date: String,
    pub money_account_id: String,
    pub money_account_name: String,
    pub party_id: Option<String>,
    /// The party, or the other ledger account.
    pub counterparty: String,
    pub amount_paise: i64,
    pub mode: String,
    pub reference: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationView {
    pub document_kind: String,
    pub document_id: String,
    pub number: Option<String>,
    pub doc_date: Option<String>,
    pub amount_paise: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoucherView {
    #[serde(flatten)]
    pub voucher: MoneyVoucherRecord,
    pub money_account_name: String,
    pub money_account_kind: String,
    pub party_name: Option<String>,
    pub counter_account_name: Option<String>,
    pub allocations: Vec<AllocationView>,
    pub allocated_paise: i64,
    /// Not allocated to any document: an advance or on-account amount.
    pub on_account_paise: i64,
    pub statement_line_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenDocumentView {
    #[serde(flatten)]
    pub document: OpenDocumentRecord,
    pub outstanding_paise: i64,
}

/// A voucher's or document's settlement history line.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementView {
    pub voucher_id: String,
    pub voucher_type: String,
    pub number: String,
    pub voucher_date: String,
    pub amount_paise: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookLineView {
    #[serde(flatten)]
    pub line: BookLineRecord,
    /// Running balance after this line (debit positive).
    pub balance_paise: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoneyBookView {
    pub account: MoneyAccountView,
    pub from: Option<String>,
    pub to: Option<String>,
    pub opening_paise: i64,
    pub debit_total_paise: i64,
    pub credit_total_paise: i64,
    pub closing_paise: i64,
    pub lines: Vec<BookLineView>,
}

/// Which statement column feeds which field. Column indexes are 0-based;
/// `header_row` is the 0-based row of the column titles in the file grid.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatementMapping {
    pub header_row: usize,
    pub date: Option<usize>,
    pub description: Option<usize>,
    pub reference: Option<usize>,
    pub withdrawal: Option<usize>,
    pub deposit: Option<usize>,
    /// One signed amount column, or an amount with a separate Dr/Cr column.
    pub amount: Option<usize>,
    pub dr_cr: Option<usize>,
    pub balance: Option<usize>,
    /// `dmy` (Indian banks), `mdy` or `ymd`.
    pub date_order: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementFile {
    pub file_name: String,
    /// Every non-blank row of the first sheet, as text.
    pub rows: Vec<Vec<String>>,
    /// The saved mapping for this account if it still fits the file,
    /// otherwise a suggestion.
    pub mapping: StatementMapping,
    pub mapping_saved: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementImportInput {
    pub money_account_id: String,
    pub file_name: String,
    pub rows: Vec<Vec<String>>,
    pub mapping: StatementMapping,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedStatementLine {
    /// Row number in the file, 1-based, as a spreadsheet shows it.
    pub row: usize,
    pub line_date: String,
    pub description: String,
    pub reference: Option<String>,
    pub withdrawal_paise: i64,
    pub deposit_paise: i64,
    pub balance_paise: Option<i64>,
    pub duplicate: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedRow {
    pub row: usize,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementPreview {
    pub lines: Vec<ParsedStatementLine>,
    pub skipped: Vec<SkippedRow>,
    pub new_lines: usize,
    pub duplicates: usize,
    pub deposits_paise: i64,
    pub withdrawals_paise: i64,
    pub first_date: Option<String>,
    pub last_date: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementImportResult {
    pub batch: String,
    pub imported: usize,
    pub duplicates: usize,
    pub skipped: usize,
}

/// A ledger entry that could be the other side of a statement line.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchCandidate {
    pub entry_id: String,
    pub entry_date: String,
    pub source_type: String,
    pub source_id: Option<String>,
    pub narration: Option<String>,
    pub party_name: Option<String>,
    pub amount_paise: i64,
    /// Days between the statement line and the entry.
    pub day_gap: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementLineView {
    #[serde(flatten)]
    pub line: StatementLineRecord,
    pub matched: Option<MatchCandidate>,
    pub suggestions: Vec<MatchCandidate>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconciliationView {
    pub account: MoneyAccountView,
    pub lines: Vec<StatementLineView>,
    /// Entries in the books for this account that no statement line matches.
    pub unmatched_entries: Vec<MatchCandidate>,
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

fn side_sign(e: &mut Errors, field: &str, side: &Option<String>) -> i64 {
    match clean(side).as_deref().unwrap_or("debit") {
        "debit" => 1,
        "credit" => -1,
        _ => {
            e.add(field, "Choose debit or credit.");
            1
        }
    }
}

fn opening_eligible(a: &AccountRecord) -> bool {
    matches!(a.kind.as_str(), "asset" | "liability" | "equity")
        && !a
            .system_role
            .as_deref()
            .is_some_and(|r| PARTY_CONTROLLED.contains(&r))
}

fn counter_eligible(a: &AccountRecord) -> bool {
    a.is_active
        && !a
            .system_role
            .as_deref()
            .is_some_and(|r| PARTY_CONTROLLED.contains(&r) || r == "round_off")
}

fn voucher_prefix(voucher_type: &str) -> &'static str {
    if voucher_type == "receipt" {
        "RCT"
    } else {
        "PMT"
    }
}

/// Net effect of a book line: positive when money came in.
fn net(l: &BookLineRecord) -> i64 {
    l.debit_paise - l.credit_paise
}

fn date_of(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

fn candidate(l: &BookLineRecord, line_date: Option<NaiveDate>) -> MatchCandidate {
    let gap = match (line_date, date_of(&l.entry_date)) {
        (Some(a), Some(b)) => (b - a).num_days(),
        _ => 0,
    };
    MatchCandidate {
        entry_id: l.entry_id.clone(),
        entry_date: l.entry_date.clone(),
        source_type: l.source_type.clone(),
        source_id: l.source_id.clone(),
        narration: l.narration.clone(),
        party_name: l.party_name.clone(),
        amount_paise: net(l),
        day_gap: gap,
    }
}

fn normalise_header(h: &str) -> String {
    h.chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Parse a statement date in the given component order. Accepts `/`, `-`,
/// `.` or space separators, month names (`11-Nov-2026`), two-digit years,
/// a trailing time (`11/11/2026 03:14:53 PM`), ISO dates and Excel serial
/// day numbers.
pub(crate) fn parse_statement_date(raw: &str, order: &str) -> Option<NaiveDate> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    // ISO first: unambiguous whatever the order.
    if let Some(d) = s
        .get(..10)
        .and_then(|p| NaiveDate::parse_from_str(p, "%Y-%m-%d").ok())
    {
        return Some(d);
    }
    // Excel serial day number (1900 date system).
    let whole = s.split('.').next().unwrap_or(s);
    if whole.len() == 5
        && whole.bytes().all(|b| b.is_ascii_digit())
        && let Ok(n) = whole.parse::<i64>()
        && (20_000..80_000).contains(&n)
    {
        return NaiveDate::from_ymd_opt(1899, 12, 30)?
            .checked_add_signed(chrono::Duration::days(n));
    }
    let date_part = s.split_whitespace().next().unwrap_or(s);
    let parts: Vec<&str> = date_part
        .split(['/', '-', '.'])
        .filter(|p| !p.is_empty())
        .collect();
    let parts = if parts.len() == 3 {
        parts
    } else {
        // "11 Nov 2026"
        let sp: Vec<&str> = s.split_whitespace().take(3).collect();
        if sp.len() == 3 { sp } else { return None }
    };
    let (d, m, y) = match order {
        "mdy" => (parts[1], parts[0], parts[2]),
        "ymd" => (parts[2], parts[1], parts[0]),
        _ => (parts[0], parts[1], parts[2]),
    };
    let day: u32 = d.parse().ok()?;
    let month: u32 = m.parse().ok().or_else(|| month_from_name(m))?;
    let mut year: i32 = y.parse().ok()?;
    if y.len() == 2 {
        year += 2000;
    }
    NaiveDate::from_ymd_opt(year, month, day)
}

fn month_from_name(m: &str) -> Option<u32> {
    let m = m.get(..3)?.to_ascii_lowercase();
    let names = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    names
        .iter()
        .position(|n| *n == m)
        .and_then(|i| u32::try_from(i + 1).ok())
}

/// Parse a statement amount to signed paise. Handles `1,23,456.78`, `₹`,
/// `(500.00)` and `-500` as negatives, and `500 Dr` / `500 Cr` suffixes
/// (Dr negative). Blank, `-` and `0` are `None`.
pub(crate) fn parse_statement_amount(raw: &str) -> Option<i64> {
    let mut s = raw.trim().replace([',', '₹', ' '], "");
    if s.is_empty() || s == "-" {
        return None;
    }
    let mut sign = 1;
    let lower = s.to_ascii_lowercase();
    if let Some(stripped) = lower.strip_suffix("dr") {
        sign = -1;
        s = stripped.to_owned();
    } else if let Some(stripped) = lower.strip_suffix("cr") {
        s = stripped.to_owned();
    }
    if s.starts_with('(') && s.ends_with(')') {
        sign = -sign;
        s = s[1..s.len() - 1].to_owned();
    }
    if let Some(stripped) = s.strip_prefix('-') {
        sign = -sign;
        s = stripped.to_owned();
    }
    let p = av_domain::Money::parse(&s).ok()?.to_paise().ok()?;
    if p == 0 { None } else { Some(sign * p) }
}

/// Guess the header row and columns of a bank statement grid.
pub(crate) fn suggest_statement_mapping(rows: &[Vec<String>]) -> StatementMapping {
    let mut best: Option<(usize, StatementMapping, usize)> = None;
    for (i, row) in rows.iter().enumerate().take(40) {
        let mut m = StatementMapping {
            header_row: i,
            date_order: "dmy".into(),
            ..StatementMapping::default()
        };
        let mut score = 0;
        for (c, h) in row.iter().enumerate() {
            let n = normalise_header(h);
            if n.is_empty() {
                continue;
            }
            let has = |keys: &[&str]| keys.iter().any(|k| n.contains(k));
            if m.date.is_none()
                && has(&["date"])
                && !has(&["value", "posted"])
                && !has(&["chq", "cheque"])
            {
                m.date = Some(c);
                score += 2;
            } else if m.description.is_none()
                && has(&[
                    "description",
                    "narration",
                    "particular",
                    "remark",
                    "details",
                ])
            {
                m.description = Some(c);
                score += 2;
            } else if m.withdrawal.is_none() && has(&["withdrawal", "debit", "dramount"]) {
                m.withdrawal = Some(c);
                score += 2;
            } else if m.deposit.is_none() && has(&["deposit", "credit", "cramount"]) {
                m.deposit = Some(c);
                score += 2;
            } else if m.dr_cr.is_none()
                && (n == "crdr" || n == "drcr" || n == "type" || n == "txntype")
            {
                m.dr_cr = Some(c);
                score += 1;
            } else if m.balance.is_none() && has(&["balance"]) {
                m.balance = Some(c);
                score += 1;
            } else if m.amount.is_none() && has(&["amount"]) {
                m.amount = Some(c);
                score += 2;
            } else if m.reference.is_none()
                && has(&["chq", "cheque", "ref", "utr", "transactionid", "tranid"])
            {
                m.reference = Some(c);
                score += 1;
            }
        }
        // A value-date column alone still makes a date.
        if m.date.is_none() {
            m.date = row
                .iter()
                .position(|h| normalise_header(h).contains("date"));
            if m.date.is_some() {
                score += 1;
            }
        }
        let has_money = m.amount.is_some() || m.withdrawal.is_some() || m.deposit.is_some();
        if m.date.is_some() && has_money && best.as_ref().is_none_or(|b| score > b.2) {
            best = Some((i, m, score));
        }
    }
    let mut m = best.map(|b| b.1).unwrap_or_else(|| StatementMapping {
        date_order: "dmy".into(),
        ..StatementMapping::default()
    });
    // A Dr/Cr column only makes sense with a single amount column.
    if m.amount.is_none() {
        m.dr_cr = None;
    }
    // Day-first is the Indian norm; switch only when the data says otherwise.
    if let Some(dc) = m.date {
        let samples: Vec<&str> = rows
            .iter()
            .skip(m.header_row + 1)
            .filter_map(|r| r.get(dc).map(String::as_str))
            .take(50)
            .collect();
        let dmy = samples
            .iter()
            .filter(|s| parse_statement_date(s, "dmy").is_some())
            .count();
        let mdy = samples
            .iter()
            .filter(|s| parse_statement_date(s, "mdy").is_some())
            .count();
        if mdy > dmy {
            m.date_order = "mdy".into();
        }
    }
    m
}

fn cell(row: &[String], col: Option<usize>) -> &str {
    col.and_then(|c| row.get(c)).map_or("", |s| s.trim())
}

/// Turn the grid into statement lines using the mapping.
pub(crate) fn parse_statement(
    rows: &[Vec<String>],
    m: &StatementMapping,
) -> AppResult<(Vec<ParsedStatementLine>, Vec<SkippedRow>)> {
    if m.date.is_none() {
        return Err(AppError::Invalid("choose the date column".into()));
    }
    if m.description.is_none() {
        return Err(AppError::Invalid("choose the description column".into()));
    }
    if m.amount.is_none() && m.withdrawal.is_none() && m.deposit.is_none() {
        return Err(AppError::Invalid(
            "choose the withdrawal and deposit columns, or a single amount column".into(),
        ));
    }
    let mut lines = Vec::new();
    let mut skipped = Vec::new();
    for (i, row) in rows.iter().enumerate().skip(m.header_row + 1) {
        let row_no = i + 1;
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let Some(date) = parse_statement_date(cell(row, m.date), &m.date_order) else {
            skipped.push(SkippedRow {
                row: row_no,
                reason: "no date".into(),
            });
            continue;
        };
        let signed = if let Some(ac) = m.amount {
            let amt = parse_statement_amount(cell(row, Some(ac)));
            match (amt, m.dr_cr) {
                (Some(a), Some(_)) => {
                    let t = cell(row, m.dr_cr).to_ascii_lowercase();
                    if t.starts_with('d') || t.starts_with("wd") {
                        Some(-a.abs())
                    } else if t.starts_with('c') || t.starts_with("dep") {
                        Some(a.abs())
                    } else {
                        None
                    }
                }
                (a, None) => a,
                (None, _) => None,
            }
        } else {
            let w = parse_statement_amount(cell(row, m.withdrawal)).map(i64::abs);
            let d = parse_statement_amount(cell(row, m.deposit)).map(i64::abs);
            match (w, d) {
                (Some(w), None) => Some(-w),
                (None, Some(d)) => Some(d),
                (Some(w), Some(d)) => Some(d - w).filter(|v| *v != 0),
                (None, None) => None,
            }
        };
        let Some(signed) = signed else {
            skipped.push(SkippedRow {
                row: row_no,
                reason: "no amount".into(),
            });
            continue;
        };
        let description = cell(row, m.description);
        let reference = Some(cell(row, m.reference))
            .filter(|s| !s.is_empty() && *s != "-")
            .map(str::to_owned);
        lines.push(ParsedStatementLine {
            row: row_no,
            line_date: date.format("%Y-%m-%d").to_string(),
            description: if description.is_empty() {
                "(no description)".into()
            } else {
                description.chars().take(500).collect()
            },
            reference,
            withdrawal_paise: if signed < 0 { -signed } else { 0 },
            deposit_paise: signed.max(0),
            balance_paise: m
                .balance
                .and_then(|b| parse_statement_amount(cell(row, Some(b)))),
            duplicate: false,
        });
    }
    Ok((lines, skipped))
}

/// Stable identity of a statement line, so re-importing an overlapping
/// statement skips what is already there. Identical lines within one file
/// (two equal UPI payments on a day) are told apart by their occurrence.
fn fingerprints(lines: &[ParsedStatementLine]) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    lines
        .iter()
        .map(|l| {
            let desc: String = l
                .description
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase();
            let base = format!(
                "{}|{}|{}|{}|{}|",
                l.line_date,
                l.deposit_paise - l.withdrawal_paise,
                desc,
                l.reference.as_deref().unwrap_or(""),
                l.balance_paise.map(|b| b.to_string()).unwrap_or_default(),
            );
            let n = seen.entry(base.clone()).or_insert(0);
            *n += 1;
            sha256_hex(format!("{base}{n}").as_bytes())
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Use cases
// ---------------------------------------------------------------------------

impl<S: Store> AppCore<S> {
    async fn money_view(
        &self,
        uow: &mut S::Uow,
        org_id: &str,
        m: MoneyAccountRecord,
    ) -> AppResult<MoneyAccountView> {
        let ledger = uow.get_account(org_id, &m.account_id).await?;
        let balance = uow
            .account_balance_paise(org_id, &m.account_id, None, None)
            .await?;
        let opening = uow
            .get_opening(org_id, &m.account_id)
            .await?
            .map_or(0, |o| o.amount_paise);
        let lines = uow.list_statement_lines(org_id, &m.id).await?;
        let unmatched = lines.iter().filter(|l| l.status == "unmatched").count();
        // Lines come newest first; the latest line with a running balance is
        // the statement's closing balance.
        let last_date = lines.first().map(|l| l.line_date.clone());
        let statement_balance = lines.iter().find_map(|l| l.balance_paise);
        Ok(MoneyAccountView {
            ledger_code: ledger.as_ref().map(|a| a.code.clone()).unwrap_or_default(),
            system_role: ledger.and_then(|a| a.system_role),
            balance_paise: balance,
            opening_balance_paise: opening,
            unmatched_lines: i64::try_from(unmatched).unwrap_or(i64::MAX),
            last_statement_date: last_date,
            statement_balance_paise: statement_balance,
            account: m,
        })
    }

    async fn require_money_account(
        &self,
        uow: &mut S::Uow,
        org_id: &str,
        id: &str,
    ) -> AppResult<MoneyAccountRecord> {
        uow.get_money_account(org_id, id)
            .await?
            .ok_or_else(|| AppError::NotFound("cash or bank account"))
    }

    /// Cash and bank accounts with their book balances.
    pub async fn list_money_accounts(
        &self,
        ctx: &RequestContext,
    ) -> AppResult<Vec<MoneyAccountView>> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let Some(org) = self.find_org(ctx, &mut uow).await? else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for m in uow.list_money_accounts(&org.id).await? {
            out.push(self.money_view(&mut uow, &org.id, m).await?);
        }
        Ok(out)
    }

    pub async fn create_money_account(
        &self,
        ctx: &RequestContext,
        input: &MoneyAccountInput,
    ) -> AppResult<MoneyAccountView> {
        ctx.require(Permission::ManageMasters)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let mut e = Errors::new("");
        let kind = match input.kind.as_str() {
            k @ ("cash" | "bank") => k,
            _ => {
                e.add("kind", "Choose cash or bank.");
                "bank"
            }
        };
        let rec = validate_money_account(&mut e, input, kind);
        let sign = side_sign(&mut e, "openingBalanceSide", &input.opening_balance_side);
        let opening =
            parse_amount(&mut e, "openingBalance", &input.opening_balance, false).unwrap_or(0);
        if uow
            .money_account_name_taken(&org.id, &rec.name, None)
            .await?
        {
            e.add("name", "Another cash or bank account has this name.");
        }
        e.finish()?;

        let now = self.now();
        // Each money account gets its own ledger account: cash 1001–1009,
        // banks 1011–1099, next to the default ones.
        let accounts = uow.list_accounts(&org.id).await?;
        let range = if kind == "cash" {
            1001..1010
        } else {
            1011..1100
        };
        let code = range
            .map(|c: i32| c.to_string())
            .find(|c| !accounts.iter().any(|a| &a.code == c))
            .ok_or_else(|| {
                AppError::Invalid("no free account code left for a new account".into())
            })?;
        let ledger = AccountRecord {
            id: new_id().to_string(),
            organisation_id: org.id.clone(),
            code,
            name: rec.name.clone(),
            kind: "asset".into(),
            system_role: None,
            is_active: true,
            created_at: now.clone(),
        };
        uow.insert_account(&ledger).await?;
        let m = MoneyAccountRecord {
            id: new_id().to_string(),
            organisation_id: org.id.clone(),
            account_id: ledger.id.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
            ..rec
        };
        uow.insert_money_account(&m).await?;
        sync_account_opening(
            &mut uow,
            ctx,
            &org.id,
            &org.books_start_date,
            &ledger,
            sign * opening,
            &now,
        )
        .await?;
        let c = Change {
            action: "money_account.create",
            entity_id: Some(&m.id),
            before: None,
            after: json(&m),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        let view = self.money_view(&mut uow, &org.id, m).await?;
        uow.commit().await?;
        Ok(view)
    }

    pub async fn update_money_account(
        &self,
        ctx: &RequestContext,
        id: &str,
        input: &MoneyAccountInput,
        expected_version: i64,
    ) -> AppResult<MoneyAccountView> {
        ctx.require(Permission::ManageMasters)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let before = self.require_money_account(&mut uow, &org.id, id).await?;
        if before.row_version != expected_version {
            return Err(AppError::stale());
        }
        let mut e = Errors::new("");
        let rec = validate_money_account(&mut e, input, &before.kind);
        let sign = side_sign(&mut e, "openingBalanceSide", &input.opening_balance_side);
        let opening =
            parse_amount(&mut e, "openingBalance", &input.opening_balance, false).unwrap_or(0);
        if uow
            .money_account_name_taken(&org.id, &rec.name, Some(id))
            .await?
        {
            e.add("name", "Another cash or bank account has this name.");
        }
        let active = input.is_active.unwrap_or(before.is_active);
        if !active && before.is_active {
            let bal = uow
                .account_balance_paise(&org.id, &before.account_id, None, None)
                .await?;
            if bal != 0 {
                e.add(
                    "isActive",
                    "Bring the balance to zero (transfer it) before closing this account.",
                );
            }
        }
        e.finish()?;
        let now = self.now();
        let after = MoneyAccountRecord {
            id: before.id.clone(),
            organisation_id: before.organisation_id.clone(),
            account_id: before.account_id.clone(),
            kind: before.kind.clone(),
            statement_mapping: before.statement_mapping.clone(),
            is_active: active,
            row_version: before.row_version,
            created_at: before.created_at.clone(),
            updated_at: now.clone(),
            ..rec
        };
        if !uow.update_money_account(&after, expected_version).await? {
            return Err(AppError::stale());
        }
        if after.name != before.name {
            uow.rename_account(&org.id, &after.account_id, &after.name)
                .await?;
        }
        if let Some(ledger) = uow.get_account(&org.id, &after.account_id).await? {
            sync_account_opening(
                &mut uow,
                ctx,
                &org.id,
                &org.books_start_date,
                &ledger,
                sign * opening,
                &now,
            )
            .await?;
        }
        let c = Change {
            action: "money_account.update",
            entity_id: Some(id),
            before: json(&before),
            after: json(&after),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        let fresh = self.require_money_account(&mut uow, &org.id, id).await?;
        let view = self.money_view(&mut uow, &org.id, fresh).await?;
        uow.commit().await?;
        Ok(view)
    }

    /// Ledger accounts that can carry an opening balance, with their current one.
    pub async fn list_opening_balances(
        &self,
        ctx: &RequestContext,
    ) -> AppResult<Vec<OpeningBalanceView>> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let Some(org) = self.find_org(ctx, &mut uow).await? else {
            return Ok(Vec::new());
        };
        let openings: HashMap<String, i64> = uow
            .list_openings(&org.id)
            .await?
            .into_iter()
            .map(|o| (o.account_id, o.amount_paise))
            .collect();
        Ok(uow
            .list_accounts(&org.id)
            .await?
            .into_iter()
            .filter(opening_eligible)
            .map(|a| OpeningBalanceView {
                amount_paise: openings.get(&a.id).copied().unwrap_or(0),
                account_id: a.id,
                code: a.code,
                name: a.name,
                kind: a.kind,
                system_role: a.system_role,
            })
            .collect())
    }

    pub async fn set_opening_balance(
        &self,
        ctx: &RequestContext,
        input: &OpeningBalanceInput,
    ) -> AppResult<OpeningBalanceView> {
        ctx.require(Permission::ManageMasters)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let account = uow
            .get_account(&org.id, &input.account_id)
            .await?
            .ok_or_else(|| AppError::NotFound("account"))?;
        if !opening_eligible(&account) {
            return Err(AppError::Invalid(
                "This account's opening balance comes from parties or isn't a balance sheet account."
                    .into(),
            ));
        }
        let mut e = Errors::new("");
        let sign = side_sign(&mut e, "side", &input.side);
        let amount = parse_amount(&mut e, "amount", &input.amount, false).unwrap_or(0);
        e.finish()?;
        let now = self.now();
        let signed = sign * amount;
        sync_account_opening(
            &mut uow,
            ctx,
            &org.id,
            &org.books_start_date,
            &account,
            signed,
            &now,
        )
        .await?;
        uow.commit().await?;
        Ok(OpeningBalanceView {
            account_id: account.id,
            code: account.code,
            name: account.name,
            kind: account.kind,
            system_role: account.system_role,
            amount_paise: signed,
        })
    }

    /// Ledger accounts a receipt or payment can be booked against when it
    /// isn't with a party (bank charges, capital, GST, transfers).
    pub async fn voucher_accounts(&self, ctx: &RequestContext) -> AppResult<Vec<AccountRecord>> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let Some(org) = self.find_org(ctx, &mut uow).await? else {
            return Ok(Vec::new());
        };
        Ok(uow
            .list_accounts(&org.id)
            .await?
            .into_iter()
            .filter(counter_eligible)
            .collect())
    }

    /// Invoices (for a receipt) or credit bills (for a payment) of a party
    /// that still have something to settle, oldest first.
    pub async fn open_documents(
        &self,
        ctx: &RequestContext,
        party_id: &str,
        voucher_type: &str,
    ) -> AppResult<Vec<OpenDocumentView>> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let kind = if voucher_type == "payment" {
            "purchase"
        } else {
            "sales"
        };
        Ok(uow
            .open_documents(&org.id, party_id, kind)
            .await?
            .into_iter()
            .map(|d| OpenDocumentView {
                outstanding_paise: d.total_paise - d.paid_paise - d.adjusted_paise,
                document: d,
            })
            .collect())
    }

    pub async fn list_vouchers(&self, ctx: &RequestContext) -> AppResult<Vec<VoucherSummary>> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let Some(org) = self.find_org(ctx, &mut uow).await? else {
            return Ok(Vec::new());
        };
        let parties: HashMap<String, String> = uow
            .list_parties(&org.id)
            .await?
            .into_iter()
            .map(|p| (p.id, p.name))
            .collect();
        let accounts: HashMap<String, String> = uow
            .list_accounts(&org.id)
            .await?
            .into_iter()
            .map(|a| (a.id, a.name))
            .collect();
        let money: HashMap<String, String> = uow
            .list_money_accounts(&org.id)
            .await?
            .into_iter()
            .map(|m| (m.id, m.name))
            .collect();
        Ok(uow
            .list_vouchers(&org.id)
            .await?
            .into_iter()
            .map(|v| VoucherSummary {
                counterparty: v
                    .party_id
                    .as_ref()
                    .and_then(|p| parties.get(p))
                    .or_else(|| v.counter_account_id.as_ref().and_then(|a| accounts.get(a)))
                    .cloned()
                    .unwrap_or_default(),
                money_account_name: money.get(&v.money_account_id).cloned().unwrap_or_default(),
                id: v.id,
                voucher_type: v.voucher_type,
                status: v.status,
                number: v.number,
                voucher_date: v.voucher_date,
                money_account_id: v.money_account_id,
                party_id: v.party_id,
                amount_paise: v.amount_paise,
                mode: v.mode,
                reference: v.reference,
            })
            .collect())
    }

    async fn load_voucher(
        &self,
        uow: &mut S::Uow,
        org_id: &str,
        id: &str,
    ) -> AppResult<VoucherView> {
        let v = uow
            .get_voucher(org_id, id)
            .await?
            .ok_or_else(|| AppError::NotFound("receipt or payment"))?;
        let money = self
            .require_money_account(uow, org_id, &v.money_account_id)
            .await?;
        let party_name = match &v.party_id {
            Some(p) => uow.get_party(org_id, p).await?.map(|p| p.name),
            None => None,
        };
        let counter_account_name = match &v.counter_account_id {
            Some(a) => uow.get_account(org_id, a).await?.map(|a| a.name),
            None => None,
        };
        let mut allocations = Vec::new();
        for a in uow.voucher_allocations(&v.id).await? {
            let (number, doc_date) = if a.document_kind == "sales" {
                match uow.get_sales(org_id, &a.document_id).await? {
                    Some(d) => (d.number, Some(d.doc_date)),
                    None => (None, None),
                }
            } else {
                use av_persistence::PurchaseRepo;
                match uow.get_purchase(org_id, &a.document_id).await? {
                    Some(d) => (
                        d.supplier_invoice_number.or(d.number),
                        Some(d.supplier_invoice_date.unwrap_or(d.doc_date)),
                    ),
                    None => (None, None),
                }
            };
            allocations.push(AllocationView {
                document_kind: a.document_kind,
                document_id: a.document_id,
                number,
                doc_date,
                amount_paise: a.amount_paise,
            });
        }
        let allocated: i64 = allocations.iter().map(|a| a.amount_paise).sum();
        let statement_line_id = uow
            .statement_line_for_entry(org_id, &v.journal_entry_id)
            .await?
            .map(|l| l.id);
        Ok(VoucherView {
            money_account_name: money.name,
            money_account_kind: money.kind,
            party_name,
            counter_account_name,
            on_account_paise: if v.party_id.is_some() {
                v.amount_paise - allocated
            } else {
                0
            },
            allocated_paise: allocated,
            allocations,
            statement_line_id,
            voucher: v,
        })
    }

    pub async fn get_voucher(&self, ctx: &RequestContext, id: &str) -> AppResult<VoucherView> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        self.load_voucher(&mut uow, &org.id, id).await
    }

    /// Receipts and payments that settled a sales or purchase document.
    pub async fn document_settlements(
        &self,
        ctx: &RequestContext,
        document_kind: &str,
        document_id: &str,
    ) -> AppResult<Vec<SettlementView>> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        Ok(uow
            .document_allocations(&org.id, document_kind, document_id)
            .await?
            .into_iter()
            .map(|(v, amount)| SettlementView {
                voucher_id: v.id,
                voucher_type: v.voucher_type,
                number: v.number,
                voucher_date: v.voucher_date,
                amount_paise: amount,
            })
            .collect())
    }

    /// Record a receipt or payment: post it, settle the chosen documents and,
    /// when created from a bank statement line, reconcile that line.
    pub async fn create_voucher(
        &self,
        ctx: &RequestContext,
        input: &VoucherInput,
    ) -> AppResult<VoucherView> {
        ctx.require(Permission::PostDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let mut e = Errors::new("");
        let today = self.today();

        let voucher_type = match input.voucher_type.as_str() {
            t @ ("receipt" | "payment") => t,
            _ => {
                e.add("voucherType", "Choose receipt or payment.");
                "receipt"
            }
        };
        let date = parse_date(&mut e, "voucherDate", &input.voucher_date);
        if let Some(d) = date {
            if d > today {
                e.add("voucherDate", "The date can't be in the future.");
            }
            if date_of(&org.books_start_date).is_some_and(|s| d < s) {
                e.add("voucherDate", "The date is before your books start.");
            }
        }
        let money = uow
            .get_money_account(&org.id, &input.money_account_id)
            .await?;
        match &money {
            None => e.add("moneyAccountId", "Choose a cash or bank account."),
            Some(m) if !m.is_active => e.add("moneyAccountId", "This account is closed."),
            _ => {}
        }
        let amount = match parse_amount(&mut e, "amount", &Some(input.amount.clone()), false) {
            Some(0) => {
                e.add("amount", "Enter an amount.");
                None
            }
            None if clean(&Some(input.amount.clone())).is_none() => {
                e.add("amount", "Enter an amount.");
                None
            }
            a => a,
        };
        let mode = if money.as_ref().is_some_and(|m| m.kind == "cash") {
            "cash".to_owned()
        } else {
            match clean(&input.mode) {
                Some(m) if MODES.contains(&m.as_str()) => m,
                Some(_) => {
                    e.add("mode", "Choose how the money moved.");
                    "other".into()
                }
                None => "neft".into(),
            }
        };
        let reference = clean(&input.reference);
        check_len(&mut e, "reference", &reference, 100);
        let narration = clean(&input.narration);
        check_len(&mut e, "narration", &narration, 500);

        // Exactly one of party / other account.
        let party_id = clean(&input.party_id);
        let counter_id = clean(&input.counter_account_id);
        let mut party = None;
        let mut counter = None;
        match (&party_id, &counter_id) {
            (Some(_), Some(_)) => e.add("partyId", "Choose a party or an account, not both."),
            (None, None) => e.add("partyId", "Choose who the money is from or to."),
            (Some(p), None) => match uow.get_party(&org.id, p).await? {
                Some(p) => party = Some(p),
                None => e.add("partyId", "Choose a party."),
            },
            (None, Some(a)) => match uow.get_account(&org.id, a).await? {
                Some(a) if counter_eligible(&a) => {
                    if money.as_ref().is_some_and(|m| m.account_id == a.id) {
                        e.add(
                            "counterAccountId",
                            "Choose a different account from the one the money moves through.",
                        );
                    }
                    counter = Some(a);
                }
                _ => e.add("counterAccountId", "Choose an account."),
            },
        }

        // Allocations: only to this party's open documents, never more than
        // is outstanding on each, never more than the voucher.
        let mut allocations: Vec<(OpenDocumentRecord, i64)> = Vec::new();
        if !input.allocations.is_empty() {
            if let Some(p) = &party {
                let kind = if voucher_type == "payment" {
                    "purchase"
                } else {
                    "sales"
                };
                let open = uow.open_documents(&org.id, &p.id, kind).await?;
                for (i, a) in input.allocations.iter().enumerate() {
                    let field = format!("allocations.{i}.amount");
                    let Some(amt) = parse_amount(&mut e, &field, &Some(a.amount.clone()), false)
                    else {
                        continue;
                    };
                    if amt == 0 {
                        continue;
                    }
                    let Some(doc) = open.iter().find(|d| d.id == a.document_id) else {
                        e.add(
                            &field,
                            "This document is already settled or isn't this party's.",
                        );
                        continue;
                    };
                    if allocations.iter().any(|(d, _)| d.id == doc.id) {
                        e.add(&field, "This document is listed twice.");
                        continue;
                    }
                    let outstanding = doc.total_paise - doc.paid_paise - doc.adjusted_paise;
                    if amt > outstanding {
                        e.add(&field, "More than is outstanding on this document.");
                        continue;
                    }
                    allocations.push((doc.clone(), amt));
                }
                let total: i64 = allocations.iter().map(|(_, a)| a).sum();
                if amount.is_some_and(|a| total > a) {
                    e.add(
                        "allocations",
                        "The amounts settled add up to more than the total.",
                    );
                }
            } else {
                e.add(
                    "allocations",
                    "Only money from or to a party can settle documents.",
                );
            }
        }

        // Statement line to reconcile with, if any.
        let mut statement_line = None;
        if let Some(line_id) = clean(&input.statement_line_id) {
            match uow.get_statement_line(&org.id, &line_id).await? {
                Some(l) if l.status == "unmatched" => {
                    let signed = l.deposit_paise - l.withdrawal_paise;
                    let expected = if voucher_type == "receipt" {
                        amount
                    } else {
                        amount.map(|a| -a)
                    };
                    if Some(l.money_account_id.as_str()) != money.as_ref().map(|m| m.id.as_str()) {
                        e.add(
                            "moneyAccountId",
                            "The statement line belongs to another account.",
                        );
                    } else if expected != Some(signed) {
                        e.add(
                            "amount",
                            "The amount and direction must match the statement line.",
                        );
                    }
                    statement_line = Some(l);
                }
                _ => e.add(
                    "statementLineId",
                    "This statement line is already reconciled.",
                ),
            }
        }
        e.finish()?;
        let (Some(date), Some(money), Some(amount)) = (date, money, amount) else {
            return Err(AppError::Invalid("incomplete receipt or payment".into()));
        };

        let fy =
            FinancialYear::containing(date, u32::try_from(org.fy_start_month).unwrap_or(4)).code();
        let seq = uow.next_voucher_seq(&org.id, voucher_type, &fy).await?;
        let number = format!("{}/{fy}/{seq:04}", voucher_prefix(voucher_type));
        let now = self.now();
        let id = new_id().to_string();
        let voucher_date = date.format("%Y-%m-%d").to_string();

        let money_line = |debit: bool| PostLine {
            role: "",
            account_id: Some(money.account_id.clone()),
            debit_paise: if debit { amount } else { 0 },
            credit_paise: if debit { 0 } else { amount },
            party_id: None,
        };
        let other_line = |debit: bool| match (&party, &counter) {
            (Some(p), _) => {
                let role = if voucher_type == "receipt" {
                    "receivables"
                } else {
                    "payables"
                };
                if debit {
                    PostLine::debit(role, amount, Some(&p.id))
                } else {
                    PostLine::credit(role, amount, Some(&p.id))
                }
            }
            (_, Some(a)) => PostLine {
                role: "",
                account_id: Some(a.id.clone()),
                debit_paise: if debit { amount } else { 0 },
                credit_paise: if debit { 0 } else { amount },
                party_id: None,
            },
            _ => unreachable!("validated above"),
        };
        let lines = if voucher_type == "receipt" {
            vec![money_line(true), other_line(false)]
        } else {
            vec![other_line(true), money_line(false)]
        };
        let who = party
            .as_ref()
            .map(|p| p.name.clone())
            .or_else(|| counter.as_ref().map(|a| a.name.clone()))
            .unwrap_or_default();
        let entry_id = post(
            &mut uow,
            ctx,
            Posting {
                org_id: &org.id,
                entry_date: &voucher_date,
                source_type: voucher_type,
                source_id: Some(&id),
                narration: Some(narration.clone().unwrap_or_else(|| {
                    if voucher_type == "receipt" {
                        format!("{number} — received from {who}")
                    } else {
                        format!("{number} — paid to {who}")
                    }
                })),
                reverses_entry_id: None,
                idempotency_key: Some(format!("voucher:post:{id}")),
                lines,
            },
            &now,
        )
        .await?;

        let v = MoneyVoucherRecord {
            id: id.clone(),
            organisation_id: org.id.clone(),
            voucher_type: voucher_type.into(),
            status: "posted".into(),
            number,
            financial_year: fy,
            seq,
            voucher_date,
            money_account_id: money.id.clone(),
            party_id: party.as_ref().map(|p| p.id.clone()),
            counter_account_id: counter.as_ref().map(|a| a.id.clone()),
            amount_paise: amount,
            mode,
            reference,
            narration,
            journal_entry_id: entry_id.clone(),
            cancel_entry_id: None,
            cancel_reason: None,
            row_version: 1,
            created_at: now.clone(),
            updated_at: now.clone(),
            cancelled_at: None,
        };
        uow.insert_voucher(&v).await?;
        for (doc, amt) in &allocations {
            uow.insert_allocation(&VoucherAllocationRecord {
                id: new_id().to_string(),
                voucher_id: id.clone(),
                document_kind: doc.document_kind.clone(),
                document_id: doc.id.clone(),
                amount_paise: *amt,
            })
            .await?;
            if !uow
                .adjust_paid(&org.id, &doc.document_kind, &doc.id, *amt)
                .await?
            {
                return Err(AppError::stale());
            }
        }
        if let Some(mut l) = statement_line {
            l.status = "matched".into();
            l.journal_entry_id = Some(entry_id);
            l.matched_at = Some(now.clone());
            l.matched_by = Some(ctx.actor.user_id.clone());
            uow.update_statement_match(&l).await?;
        }
        let c = Change {
            action: if voucher_type == "receipt" {
                "money_voucher.receipt"
            } else {
                "money_voucher.payment"
            },
            entity_id: Some(&id),
            before: None,
            after: json(&v),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        let view = self.load_voucher(&mut uow, &org.id, &id).await?;
        uow.commit().await?;
        Ok(view)
    }

    /// Cancel a receipt or payment: post the reversal and release the
    /// documents it settled.
    pub async fn cancel_voucher(
        &self,
        ctx: &RequestContext,
        id: &str,
        reason: &str,
        expected_version: i64,
    ) -> AppResult<VoucherView> {
        ctx.require(Permission::PostDocuments)?;
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(AppError::Invalid("give a reason for cancelling".into()));
        }
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let before = uow
            .get_voucher(&org.id, id)
            .await?
            .ok_or_else(|| AppError::NotFound("receipt or payment"))?;
        if before.status != "posted" {
            return Err(AppError::Conflict("This is already cancelled.".into()));
        }
        if before.row_version != expected_version {
            return Err(AppError::stale());
        }
        if uow
            .statement_line_for_entry(&org.id, &before.journal_entry_id)
            .await?
            .is_some()
        {
            return Err(AppError::Conflict(
                "This is reconciled with a bank statement line. Unmatch it first.".into(),
            ));
        }
        let now = self.now();
        // Reverse exactly what was posted.
        let original = journal_lines_of(&mut uow, &org.id, &before).await?;
        let entry_id = post(
            &mut uow,
            ctx,
            Posting {
                org_id: &org.id,
                entry_date: &before.voucher_date,
                source_type: &before.voucher_type,
                source_id: Some(&before.id),
                narration: Some(format!("Cancellation of {} — {reason}", before.number)),
                reverses_entry_id: Some(&before.journal_entry_id),
                idempotency_key: Some(format!("voucher:cancel:{}", before.id)),
                lines: original.iter().map(PostLine::mirrored).collect(),
            },
            &now,
        )
        .await?;
        for a in uow.voucher_allocations(&before.id).await? {
            uow.adjust_paid(&org.id, &a.document_kind, &a.document_id, -a.amount_paise)
                .await?;
        }
        let mut after = before.clone();
        after.status = "cancelled".into();
        after.cancel_entry_id = Some(entry_id);
        after.cancel_reason = Some(reason.to_owned());
        after.cancelled_at = Some(now.clone());
        after.updated_at.clone_from(&now);
        if !uow.update_voucher(&after, expected_version).await? {
            return Err(AppError::stale());
        }
        let c = Change {
            action: "money_voucher.cancel",
            entity_id: Some(id),
            before: json(&before),
            after: json(&after),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        let view = self.load_voucher(&mut uow, &org.id, id).await?;
        uow.commit().await?;
        Ok(view)
    }

    /// Cash book or bank book: every entry through the account in a period,
    /// with opening, running and closing balances.
    pub async fn money_book(
        &self,
        ctx: &RequestContext,
        money_account_id: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> AppResult<MoneyBookView> {
        ctx.require(Permission::ViewBusiness)?;
        for d in [from, to].into_iter().flatten() {
            if date_of(d).is_none() {
                return Err(AppError::Invalid(format!("expected YYYY-MM-DD, got {d:?}")));
            }
        }
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let m = self
            .require_money_account(&mut uow, &org.id, money_account_id)
            .await?;
        let opening = match from.and_then(date_of).and_then(|d| d.pred_opt()) {
            Some(before) => {
                let b = before.format("%Y-%m-%d").to_string();
                uow.account_balance_paise(&org.id, &m.account_id, None, Some(&b))
                    .await?
            }
            None => 0,
        };
        let rows = uow.account_book(&org.id, &m.account_id, from, to).await?;
        let mut balance = opening;
        let mut dr = 0;
        let mut cr = 0;
        let mut lines = Vec::with_capacity(rows.len());
        for r in rows {
            // Show each entry's net effect on one side.
            let n = net(&r);
            let mut line = r;
            line.debit_paise = n.max(0);
            line.credit_paise = (-n).max(0);
            dr += line.debit_paise;
            cr += line.credit_paise;
            balance += n;
            lines.push(BookLineView {
                line,
                balance_paise: balance,
            });
        }
        let account = self.money_view(&mut uow, &org.id, m).await?;
        Ok(MoneyBookView {
            account,
            from: from.map(str::to_owned),
            to: to.map(str::to_owned),
            opening_paise: opening,
            debit_total_paise: dr,
            credit_total_paise: cr,
            closing_paise: balance,
            lines,
        })
    }

    // -- Bank statements ----------------------------------------------------

    /// Read a statement file and propose (or reuse) its column mapping.
    pub async fn read_statement_file(
        &self,
        ctx: &RequestContext,
        money_account_id: &str,
        file_name: &str,
        bytes: &[u8],
    ) -> AppResult<StatementFile> {
        ctx.require(Permission::PrepareDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let m = self
            .require_money_account(&mut uow, &org.id, money_account_id)
            .await?;
        let rows = read_grid(file_name, bytes)?;
        let saved = m
            .statement_mapping
            .as_deref()
            .and_then(|s| serde_json::from_str::<StatementMapping>(s).ok())
            .filter(|s| {
                rows.get(s.header_row).is_some()
                    && parse_statement(&rows, s).is_ok_and(|(l, _)| !l.is_empty())
            });
        Ok(StatementFile {
            file_name: file_name.to_owned(),
            mapping_saved: saved.is_some(),
            mapping: saved.unwrap_or_else(|| suggest_statement_mapping(&rows)),
            rows,
        })
    }

    pub async fn preview_statement(
        &self,
        ctx: &RequestContext,
        input: &StatementImportInput,
    ) -> AppResult<StatementPreview> {
        ctx.require(Permission::PrepareDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let m = self
            .require_money_account(&mut uow, &org.id, &input.money_account_id)
            .await?;
        let (mut lines, skipped) = parse_statement(&input.rows, &input.mapping)?;
        let existing: std::collections::HashSet<String> = uow
            .list_statement_lines(&org.id, &m.id)
            .await?
            .into_iter()
            .map(|l| l.fingerprint)
            .collect();
        let fps = fingerprints(&lines);
        for (l, fp) in lines.iter_mut().zip(fps) {
            l.duplicate = existing.contains(&fp);
        }
        Ok(summarise(lines, skipped))
    }

    /// Import a statement: store new lines, skip ones already imported, and
    /// remember the mapping for this account's next statement.
    pub async fn import_statement(
        &self,
        ctx: &RequestContext,
        input: &StatementImportInput,
    ) -> AppResult<StatementImportResult> {
        ctx.require(Permission::PrepareDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let m = self
            .require_money_account(&mut uow, &org.id, &input.money_account_id)
            .await?;
        if m.kind != "bank" {
            return Err(AppError::Invalid(
                "statements are imported into bank accounts".into(),
            ));
        }
        let (lines, skipped) = parse_statement(&input.rows, &input.mapping)?;
        if lines.is_empty() {
            return Err(AppError::Invalid(
                "no transactions found with this column mapping".into(),
            ));
        }
        let now = self.now();
        let batch = new_id().to_string();
        let mut imported = 0;
        for (l, fp) in lines.iter().zip(fingerprints(&lines)) {
            let rec = StatementLineRecord {
                id: new_id().to_string(),
                organisation_id: org.id.clone(),
                money_account_id: m.id.clone(),
                line_date: l.line_date.clone(),
                description: l.description.clone(),
                reference: l.reference.clone(),
                withdrawal_paise: l.withdrawal_paise,
                deposit_paise: l.deposit_paise,
                balance_paise: l.balance_paise,
                fingerprint: fp,
                import_batch: batch.clone(),
                source_file: Some(input.file_name.chars().take(200).collect()),
                status: "unmatched".into(),
                journal_entry_id: None,
                matched_at: None,
                matched_by: None,
                created_at: now.clone(),
            };
            if uow.insert_statement_line(&rec).await? {
                imported += 1;
            }
        }
        let mapping = serde_json::to_string(&input.mapping).ok();
        if mapping != m.statement_mapping {
            let mut updated = m.clone();
            updated.statement_mapping = mapping;
            updated.updated_at.clone_from(&now);
            if !uow.update_money_account(&updated, m.row_version).await? {
                return Err(AppError::stale());
            }
        }
        let result = StatementImportResult {
            batch: batch.clone(),
            imported,
            duplicates: lines.len() - imported,
            skipped: skipped.len(),
        };
        let c = Change {
            action: "bank_statement.import",
            entity_id: Some(&m.id),
            before: None,
            after: Some(format!(
                "{{\"file\":{},\"batch\":\"{batch}\",\"imported\":{imported},\"duplicates\":{}}}",
                serde_json::to_string(&input.file_name).unwrap_or_default(),
                result.duplicates
            )),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        uow.commit().await?;
        Ok(result)
    }

    /// Statement lines of a bank account with match suggestions.
    pub async fn reconciliation(
        &self,
        ctx: &RequestContext,
        money_account_id: &str,
    ) -> AppResult<ReconciliationView> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let m = self
            .require_money_account(&mut uow, &org.id, money_account_id)
            .await?;
        let book = uow.account_book(&org.id, &m.account_id, None, None).await?;
        let by_entry: HashMap<&str, &BookLineRecord> =
            book.iter().map(|b| (b.entry_id.as_str(), b)).collect();
        let lines = uow.list_statement_lines(&org.id, &m.id).await?;
        let free: Vec<&BookLineRecord> = book
            .iter()
            .filter(|b| b.statement_line_id.is_none() && net(b) != 0)
            .collect();
        let mut views = Vec::with_capacity(lines.len());
        for l in lines {
            let ld = date_of(&l.line_date);
            let matched = l
                .journal_entry_id
                .as_deref()
                .and_then(|e| by_entry.get(e))
                .map(|b| candidate(b, ld));
            let suggestions = if l.status == "unmatched" {
                let signed = l.deposit_paise - l.withdrawal_paise;
                let mut s: Vec<MatchCandidate> = free
                    .iter()
                    .filter(|b| net(b) == signed)
                    .map(|b| candidate(b, ld))
                    .filter(|c| c.day_gap.abs() <= MATCH_WINDOW_DAYS)
                    .collect();
                s.sort_by_key(|c| c.day_gap.abs());
                s.truncate(5);
                s
            } else {
                Vec::new()
            };
            views.push(StatementLineView {
                line: l,
                matched,
                suggestions,
            });
        }
        let unmatched_entries = free.iter().rev().map(|b| candidate(b, None)).collect();
        let account = self.money_view(&mut uow, &org.id, m).await?;
        Ok(ReconciliationView {
            account,
            lines: views,
            unmatched_entries,
        })
    }

    /// Reconcile a statement line with an entry already in the books.
    pub async fn match_statement_line(
        &self,
        ctx: &RequestContext,
        line_id: &str,
        entry_id: &str,
    ) -> AppResult<()> {
        ctx.require(Permission::PostDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let mut l = uow
            .get_statement_line(&org.id, line_id)
            .await?
            .ok_or_else(|| AppError::NotFound("statement line"))?;
        if l.status != "unmatched" {
            return Err(AppError::Conflict(
                "This line is already reconciled or ignored.".into(),
            ));
        }
        let m = self
            .require_money_account(&mut uow, &org.id, &l.money_account_id)
            .await?;
        let entry = uow
            .account_book(&org.id, &m.account_id, None, None)
            .await?
            .into_iter()
            .find(|b| b.entry_id == entry_id)
            .ok_or_else(|| {
                AppError::Invalid("That entry doesn't touch this bank account.".into())
            })?;
        if entry.statement_line_id.is_some() {
            return Err(AppError::Conflict(
                "That entry is already matched to another statement line.".into(),
            ));
        }
        if net(&entry) != l.deposit_paise - l.withdrawal_paise {
            return Err(AppError::Invalid(
                "The entry's amount or direction differs from the statement line.".into(),
            ));
        }
        let now = self.now();
        l.status = "matched".into();
        l.journal_entry_id = Some(entry_id.to_owned());
        l.matched_at = Some(now.clone());
        l.matched_by = Some(ctx.actor.user_id.clone());
        uow.update_statement_match(&l).await?;
        let c = Change {
            action: "bank_statement_line.match",
            entity_id: Some(line_id),
            before: None,
            after: json(&l),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        uow.commit().await?;
        Ok(())
    }

    /// Undo a match, or un-ignore a line.
    pub async fn unmatch_statement_line(
        &self,
        ctx: &RequestContext,
        line_id: &str,
    ) -> AppResult<()> {
        self.set_line_state(ctx, line_id, "unmatched").await
    }

    /// Mark a line as not needing an entry (e.g. a reversed transaction).
    pub async fn ignore_statement_line(
        &self,
        ctx: &RequestContext,
        line_id: &str,
    ) -> AppResult<()> {
        self.set_line_state(ctx, line_id, "ignored").await
    }

    async fn set_line_state(
        &self,
        ctx: &RequestContext,
        line_id: &str,
        state: &str,
    ) -> AppResult<()> {
        ctx.require(Permission::PostDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let mut l = uow
            .get_statement_line(&org.id, line_id)
            .await?
            .ok_or_else(|| AppError::NotFound("statement line"))?;
        if state == "ignored" && l.status != "unmatched" {
            return Err(AppError::Conflict(
                "Only unreconciled lines can be ignored.".into(),
            ));
        }
        let before = json(&l);
        let now = self.now();
        l.status = state.into();
        l.journal_entry_id = None;
        l.matched_at = (state == "ignored").then(|| now.clone());
        l.matched_by = (state == "ignored").then(|| ctx.actor.user_id.clone());
        uow.update_statement_match(&l).await?;
        let c = Change {
            action: if state == "ignored" {
                "bank_statement_line.ignore"
            } else {
                "bank_statement_line.unmatch"
            },
            entity_id: Some(line_id),
            before,
            after: json(&l),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        uow.commit().await?;
        Ok(())
    }

    /// Remove the lines of one import that aren't reconciled yet.
    pub async fn undo_statement_import(
        &self,
        ctx: &RequestContext,
        money_account_id: &str,
        batch: &str,
    ) -> AppResult<u64> {
        ctx.require(Permission::PrepareDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let m = self
            .require_money_account(&mut uow, &org.id, money_account_id)
            .await?;
        let n = uow.delete_statement_batch(&org.id, &m.id, batch).await?;
        let now = self.now();
        let c = Change {
            action: "bank_statement.undo_import",
            entity_id: Some(&m.id),
            before: None,
            after: Some(format!("{{\"batch\":\"{batch}\",\"removed\":{n}}}")),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        uow.commit().await?;
        Ok(n)
    }
}

fn summarise(lines: Vec<ParsedStatementLine>, skipped: Vec<SkippedRow>) -> StatementPreview {
    let fresh: Vec<&ParsedStatementLine> = lines.iter().filter(|l| !l.duplicate).collect();
    StatementPreview {
        new_lines: fresh.len(),
        duplicates: lines.len() - fresh.len(),
        deposits_paise: fresh.iter().map(|l| l.deposit_paise).sum(),
        withdrawals_paise: fresh.iter().map(|l| l.withdrawal_paise).sum(),
        first_date: lines.iter().map(|l| l.line_date.clone()).min(),
        last_date: lines.iter().map(|l| l.line_date.clone()).max(),
        lines,
        skipped,
    }
}

fn validate_money_account(
    e: &mut Errors,
    input: &MoneyAccountInput,
    kind: &str,
) -> MoneyAccountRecord {
    let name = input.name.trim().to_owned();
    if name.is_empty() {
        e.add("name", "Give the account a name.");
    }
    let name_opt = Some(name.clone());
    check_len(e, "name", &name_opt, 80);
    let bank = |v: &Option<String>| if kind == "bank" { clean(v) } else { None };
    let bank_name = bank(&input.bank_name);
    check_len(e, "bankName", &bank_name, 80);
    let account_number = bank(&input.account_number).map(|s| s.replace(' ', ""));
    if account_number
        .as_ref()
        .is_some_and(|n| n.len() > 20 || !n.chars().all(|c| c.is_ascii_alphanumeric()))
    {
        e.add(
            "accountNumber",
            "Enter the account number without spaces or symbols.",
        );
    }
    let ifsc = bank(&input.ifsc).map(|s| s.to_ascii_uppercase());
    if let Some(i) = &ifsc {
        let b = i.as_bytes();
        let ok = b.len() == 11
            && b[..4].iter().all(u8::is_ascii_alphabetic)
            && b[4] == b'0'
            && b[5..].iter().all(u8::is_ascii_alphanumeric);
        if !ok {
            e.add("ifsc", "An IFSC looks like HDFC0001234.");
        }
    }
    MoneyAccountRecord {
        id: String::new(),
        organisation_id: String::new(),
        account_id: String::new(),
        kind: kind.into(),
        name,
        bank_name,
        account_number,
        ifsc,
        statement_mapping: None,
        is_active: input.is_active.unwrap_or(true),
        row_version: 1,
        created_at: String::new(),
        updated_at: String::new(),
    }
}

/// The posted lines of a voucher, rebuilt from the voucher itself (they are
/// fully determined by it), for the cancellation reversal.
async fn journal_lines_of<U: UnitOfWork>(
    uow: &mut U,
    org_id: &str,
    v: &MoneyVoucherRecord,
) -> AppResult<Vec<PostLine>> {
    let money = uow
        .get_money_account(org_id, &v.money_account_id)
        .await?
        .ok_or_else(|| AppError::NotFound("cash or bank account"))?;
    let amount = v.amount_paise;
    let receipt = v.voucher_type == "receipt";
    let money_line = PostLine {
        role: "",
        account_id: Some(money.account_id),
        debit_paise: if receipt { amount } else { 0 },
        credit_paise: if receipt { 0 } else { amount },
        party_id: None,
    };
    let other = match (&v.party_id, &v.counter_account_id) {
        (Some(p), _) => {
            if receipt {
                PostLine::credit("receivables", amount, Some(p))
            } else {
                PostLine::debit("payables", amount, Some(p))
            }
        }
        (_, Some(a)) => PostLine {
            role: "",
            account_id: Some(a.clone()),
            debit_paise: if receipt { 0 } else { amount },
            credit_paise: if receipt { amount } else { 0 },
            party_id: None,
        },
        _ => return Err(AppError::Invalid("voucher has no other side".into())),
    };
    Ok(if receipt {
        vec![money_line, other]
    } else {
        vec![other, money_line]
    })
}

/// Bring an account's posted opening balance in line with `amount` (debit
/// positive): reverse the previous opening entry, post the new one.
pub(crate) async fn sync_account_opening<U: UnitOfWork>(
    uow: &mut U,
    ctx: &RequestContext,
    org_id: &str,
    books_start: &str,
    account: &AccountRecord,
    amount: i64,
    now: &str,
) -> AppResult<()> {
    let before = uow.get_opening(org_id, &account.id).await?;
    let old = before.as_ref().map_or(0, |o| o.amount_paise);
    if old == amount {
        return Ok(());
    }
    let lines = |amt: i64| {
        let mine = PostLine {
            role: "",
            account_id: Some(account.id.clone()),
            debit_paise: amt.max(0),
            credit_paise: (-amt).max(0),
            party_id: None,
        };
        let other = if amt > 0 {
            PostLine::credit("opening_balance", amt, None)
        } else {
            PostLine::debit("opening_balance", -amt, None)
        };
        vec![mine, other]
    };
    if let Some(b) = &before
        && let Some(old_id) = &b.entry_id
    {
        post(
            uow,
            ctx,
            Posting {
                org_id,
                entry_date: books_start,
                source_type: "account_opening_balance",
                source_id: Some(&account.id),
                narration: Some(format!("Reversal of opening balance — {}", account.name)),
                reverses_entry_id: Some(old_id),
                idempotency_key: None,
                lines: lines(b.amount_paise)
                    .iter()
                    .map(PostLine::mirrored)
                    .collect(),
            },
            now,
        )
        .await?;
    }
    let entry_id = if amount == 0 {
        None
    } else {
        Some(
            post(
                uow,
                ctx,
                Posting {
                    org_id,
                    entry_date: books_start,
                    source_type: "account_opening_balance",
                    source_id: Some(&account.id),
                    narration: Some(format!("Opening balance — {}", account.name)),
                    reverses_entry_id: None,
                    idempotency_key: None,
                    lines: lines(amount),
                },
                now,
            )
            .await?,
        )
    };
    let rec = AccountOpeningRecord {
        account_id: account.id.clone(),
        organisation_id: org_id.to_owned(),
        amount_paise: amount,
        entry_id,
        updated_at: now.to_owned(),
    };
    uow.upsert_opening(&rec).await?;
    let c = Change {
        action: "account.opening_balance",
        entity_id: Some(&account.id),
        before: before.as_ref().and_then(json),
        after: json(&rec),
    };
    audit(uow, ctx, org_id, c, now).await?;
    Ok(())
}

/// Create the cash and bank money accounts for a new business's default
/// ledgers (existing businesses get them from migration 0006).
pub(crate) async fn seed_money_accounts<U: UnitOfWork>(
    uow: &mut U,
    org_id: &str,
    now: &str,
) -> AppResult<()> {
    for a in uow.list_accounts(org_id).await? {
        let Some(role) = a.system_role.as_deref() else {
            continue;
        };
        if role != "cash" && role != "bank" {
            continue;
        }
        uow.insert_money_account(&MoneyAccountRecord {
            id: new_id().to_string(),
            organisation_id: org_id.to_owned(),
            account_id: a.id.clone(),
            kind: role.into(),
            name: a.name.clone(),
            bank_name: None,
            account_number: None,
            ifsc: None,
            statement_mapping: None,
            is_active: true,
            row_version: 1,
            created_at: now.to_owned(),
            updated_at: now.to_owned(),
        })
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: &[&[&str]]) -> Vec<Vec<String>> {
        rows.iter()
            .map(|r| r.iter().map(|c| (*c).to_owned()).collect())
            .collect()
    }

    #[test]
    fn dates_in_bank_formats() {
        let d = |s, o| parse_statement_date(s, o).map(|d| d.to_string());
        assert_eq!(d("11/11/2026", "dmy").as_deref(), Some("2026-11-11"));
        assert_eq!(
            d("18/11/2026 08:30:42 AM ", "dmy").as_deref(),
            Some("2026-11-18")
        );
        assert_eq!(d("05-Nov-2026", "dmy").as_deref(), Some("2026-11-05"));
        assert_eq!(d("05 Nov 26", "dmy").as_deref(), Some("2026-11-05"));
        assert_eq!(d("2026-11-05", "dmy").as_deref(), Some("2026-11-05"));
        assert_eq!(d("11/05/2026", "mdy").as_deref(), Some("2026-11-05"));
        assert_eq!(d("46337", "dmy").as_deref(), Some("2026-11-11"));
        assert_eq!(d("31/02/2026", "dmy"), None);
        assert_eq!(d("Legends", "dmy"), None);
    }

    #[test]
    fn amounts_in_bank_formats() {
        assert_eq!(parse_statement_amount("1,23,456.78"), Some(12_345_678));
        assert_eq!(parse_statement_amount("(500.00)"), Some(-50_000));
        assert_eq!(parse_statement_amount("500.00 Dr"), Some(-50_000));
        assert_eq!(parse_statement_amount("500 Cr"), Some(50_000));
        assert_eq!(parse_statement_amount("-"), None);
        assert_eq!(parse_statement_amount("0.00"), None);
        assert_eq!(parse_statement_amount(""), None);
    }

    /// The layout of an ICICI "detailed statement" export: title rows above
    /// the header, one amount column and a separate Cr/Dr column.
    #[test]
    fn icici_style_statement() {
        let rows = grid(&[
            &["DETAILED STATEMENT", "", "", "", "", "", "", "", ""],
            &[
                "Transactions List - THINGSVU (INR) - 0000",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
            ],
            &[
                "No.",
                "Transaction ID",
                "Value Date",
                "Txn Posted Date",
                "ChequeNo.",
                "Description",
                "Cr/Dr",
                "Transaction Amount(INR)",
                "Available Balance(INR)",
            ],
            &[
                "1",
                "S10786543",
                "11/11/2026",
                "11/11/2026 03:14:53 PM",
                "-",
                "MMT/IMPS/P2P",
                "CR",
                "1",
                "1",
            ],
            &[
                "2",
                "M335040",
                "17/11/2026",
                "18/11/2026 08:30:42 AM",
                "-",
                "UPI/IN",
                "CR",
                "10100",
                "10101",
            ],
            &[
                "3",
                "M335041",
                "18/11/2026",
                "18/11/2026 09:00:00 AM",
                "-",
                "CHARGES",
                "DR",
                "118",
                "9983",
            ],
        ]);
        let m = suggest_statement_mapping(&rows);
        assert_eq!(m.header_row, 2);
        assert_eq!(m.description, Some(5));
        assert_eq!(m.amount, Some(7));
        assert_eq!(m.dr_cr, Some(6));
        assert_eq!(m.balance, Some(8));
        assert!(m.date == Some(3) || m.date == Some(2));
        let (lines, skipped) = parse_statement(&rows, &m).unwrap();
        assert!(skipped.is_empty());
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1].deposit_paise, 1_010_000);
        assert_eq!(lines[2].withdrawal_paise, 11_800);
        assert_eq!(lines[2].balance_paise, Some(998_300));
    }

    #[test]
    fn withdrawal_deposit_columns_and_footer() {
        let rows = grid(&[
            &[
                "Date",
                "Narration",
                "Chq./Ref.No.",
                "Withdrawal Amt.",
                "Deposit Amt.",
                "Closing Balance",
            ],
            &["01/11/26", "NEFT-ACME", "UTR1", "", "5,000.00", "5,000.00"],
            &["02/11/26", "ATM", "", "1,000.00", "", "4,000.00"],
            &["", "", "", "", "", ""],
            &["Statement summary", "", "", "", "", ""],
        ]);
        let m = suggest_statement_mapping(&rows);
        assert_eq!(
            (m.withdrawal, m.deposit, m.reference),
            (Some(3), Some(4), Some(2))
        );
        let (lines, skipped) = parse_statement(&rows, &m).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(skipped.len(), 1);
        assert_eq!(lines[0].reference.as_deref(), Some("UTR1"));
        assert_eq!(lines[1].withdrawal_paise, 100_000);
    }

    #[test]
    fn identical_lines_get_distinct_fingerprints() {
        let l = ParsedStatementLine {
            row: 1,
            line_date: "2026-11-01".into(),
            description: "UPI".into(),
            reference: None,
            withdrawal_paise: 100,
            deposit_paise: 0,
            balance_paise: None,
            duplicate: false,
        };
        let f = fingerprints(&[l.clone(), l]);
        assert_ne!(f[0], f[1]);
    }
}
