//! Phase 1.6: ledgers and reports — trial balance, profit & loss, balance
//! sheet, general and party ledgers, journal register, receivables and
//! payables ageing — plus period lock, financial-year close and the business
//! logo printed on invoices.
//!
//! Every figure is computed from posted journal lines, so every row can drill
//! down to the ledger and from there to the source document.

use std::collections::BTreeMap;

use av_domain::{FinancialYear, new_id};
use av_persistence::records::{
    AccountTotalsRecord, BookLineRecord, BooksLockRecord, FyCloseRecord, LogoRecord,
};
use av_persistence::{
    AccountRepo, AttachmentRepo, LedgerQueries, PartyRepo, ReportRepo, Store, UnitOfWork,
};
use chrono::NaiveDate;
use serde::Serialize;

use crate::AppCore;
use crate::context::{Permission, RequestContext};
use crate::error::{AppError, AppResult};
use crate::posting::{PostLine, Posting, post};
use crate::purchases::sha256_hex;
use crate::setup::{Change, FinancialYearInfo, audit, json};

/// Source type of the year-end closing entry. Profit & loss leaves it out so
/// a closed year still reports its income and expenses.
const YEAR_CLOSE: &str = "year_close";
const JOURNAL_LIMIT: i64 = 1000;
const LOGO_MAX_BYTES: usize = 1024 * 1024;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrialBalanceRow {
    pub account_id: String,
    pub code: String,
    pub name: String,
    pub kind: String,
    pub system_role: Option<String>,
    pub opening_paise: i64,
    pub debit_paise: i64,
    pub credit_paise: i64,
    /// Debit balances positive.
    pub closing_paise: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrialBalanceView {
    pub from: Option<String>,
    pub to: Option<String>,
    pub rows: Vec<TrialBalanceRow>,
    pub opening_debit_paise: i64,
    pub opening_credit_paise: i64,
    pub debit_paise: i64,
    pub credit_paise: i64,
    pub closing_debit_paise: i64,
    pub closing_credit_paise: i64,
    pub balanced: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportLine {
    pub account_id: String,
    pub code: String,
    pub name: String,
    pub amount_paise: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportSection {
    pub key: &'static str,
    pub title: &'static str,
    pub lines: Vec<ReportLine>,
    pub total_paise: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfitAndLossView {
    pub from: String,
    pub to: String,
    /// Trading income, direct costs, other income, operating expenses.
    pub sections: Vec<ReportSection>,
    pub gross_profit_paise: i64,
    pub net_profit_paise: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceSheetView {
    pub as_at: String,
    pub assets: ReportSection,
    pub liabilities: ReportSection,
    pub equity: ReportSection,
    /// Income less expenses not yet closed into retained earnings.
    pub profit_paise: i64,
    pub total_assets_paise: i64,
    pub total_liabilities_and_equity_paise: i64,
    pub balanced: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerLineView {
    #[serde(flatten)]
    pub line: BookLineRecord,
    pub balance_paise: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerView {
    /// Account or party id.
    pub id: String,
    pub title: String,
    pub subtitle: String,
    /// `account` or `party`.
    pub ledger_type: &'static str,
    pub from: Option<String>,
    pub to: Option<String>,
    pub opening_paise: i64,
    pub debit_total_paise: i64,
    pub credit_total_paise: i64,
    pub closing_paise: i64,
    pub lines: Vec<LedgerLineView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalLineView {
    pub account_id: String,
    pub account_code: String,
    pub account_name: String,
    pub debit_paise: i64,
    pub credit_paise: i64,
    pub party_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntryView {
    pub entry_id: String,
    pub entry_date: String,
    pub source_type: String,
    pub source_id: Option<String>,
    pub narration: Option<String>,
    pub reverses_entry_id: Option<String>,
    pub posted_at: String,
    pub posted_by: Option<String>,
    pub total_paise: i64,
    pub lines: Vec<JournalLineView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalRegisterView {
    pub from: Option<String>,
    pub to: Option<String>,
    pub entries: Vec<JournalEntryView>,
    /// More entries exist than were returned; narrow the dates.
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgeingDocument {
    pub document_kind: String,
    pub id: String,
    pub number: Option<String>,
    pub doc_date: String,
    pub due_date: Option<String>,
    pub total_paise: i64,
    pub outstanding_paise: i64,
    /// Negative while not yet due.
    pub days_overdue: i64,
    pub bucket: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgeingParty {
    pub party_id: String,
    pub party_name: String,
    /// Not due, 1–30, 31–60, 61–90, over 90 days.
    pub buckets: [i64; 5],
    /// Opening balances and advances not tied to a document (advances negative).
    pub unallocated_paise: i64,
    pub total_paise: i64,
    pub documents: Vec<AgeingDocument>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgeingView {
    /// `receivables` or `payables`.
    pub kind: String,
    pub as_of: String,
    pub bucket_labels: [&'static str; 5],
    pub parties: Vec<AgeingParty>,
    pub buckets: [i64; 5],
    pub unallocated_paise: i64,
    pub total_paise: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YearStatus {
    #[serde(flatten)]
    pub year: FinancialYearInfo,
    /// `open`, `closed`
    pub status: &'static str,
    pub ended: bool,
    pub close: Option<FyCloseRecord>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BooksControlView {
    pub lock: Option<BooksLockRecord>,
    pub books_start_date: String,
    pub years: Vec<YearStatus>,
    pub history: Vec<FyCloseRecord>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogoContent {
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub bytes: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn date(s: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| AppError::Invalid(format!("expected YYYY-MM-DD, got {s:?}")))
}

fn opt_date(s: Option<&str>) -> AppResult<Option<String>> {
    match s.map(str::trim).filter(|s| !s.is_empty()) {
        Some(d) => Ok(Some(date(d)?.format("%Y-%m-%d").to_string())),
        None => Ok(None),
    }
}

fn iso(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn closing(t: &AccountTotalsRecord) -> i64 {
    t.opening_paise + t.debit_paise - t.credit_paise
}

fn line(t: &AccountTotalsRecord, amount: i64) -> ReportLine {
    ReportLine {
        account_id: t.account_id.clone(),
        code: t.code.clone(),
        name: t.name.clone(),
        amount_paise: amount,
    }
}

fn section(key: &'static str, title: &'static str, lines: Vec<ReportLine>) -> ReportSection {
    ReportSection {
        key,
        title,
        total_paise: lines.iter().map(|l| l.amount_paise).sum(),
        lines,
    }
}

/// Direct costs: purchases and the 5xxx cost-of-goods group.
fn is_direct_cost(t: &AccountTotalsRecord) -> bool {
    t.system_role.as_deref() == Some("purchases") || t.code.starts_with('5')
}

fn bucket_of(days_overdue: i64) -> usize {
    match days_overdue {
        d if d <= 0 => 0,
        1..=30 => 1,
        31..=60 => 2,
        61..=90 => 3,
        _ => 4,
    }
}

fn ledger_lines(opening: i64, rows: Vec<BookLineRecord>) -> (Vec<LedgerLineView>, i64, i64, i64) {
    let mut balance = opening;
    let mut dr = 0;
    let mut cr = 0;
    let lines = rows
        .into_iter()
        .map(|line| {
            dr += line.debit_paise;
            cr += line.credit_paise;
            balance += line.debit_paise - line.credit_paise;
            LedgerLineView {
                line,
                balance_paise: balance,
            }
        })
        .collect();
    (lines, dr, cr, balance)
}

/// The image type of a logo, from its first bytes (never trust the name).
fn logo_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase();
    if head.trim_start().starts_with("<svg") || (head.contains("<?xml") && head.contains("<svg")) {
        return Some("image/svg+xml");
    }
    None
}

/// An SVG logo must be a plain picture: no scripts, event handlers or
/// external references.
fn svg_is_safe(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    if [
        "<script",
        "javascript:",
        "<foreignobject",
        "href=\"http",
        "href='http",
    ]
    .iter()
    .any(|bad| text.contains(bad))
    {
        return false;
    }
    // Event handler attributes: whitespace, "on", letters, optional spaces, "=".
    let b = text.as_bytes();
    (0..b.len()).all(|i| {
        if !b[i].is_ascii_whitespace() || !text[i + 1..].starts_with("on") {
            return true;
        }
        let rest = &text[i + 3..];
        let name = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
        name == 0 || !rest[name..].trim_start().starts_with('=')
    })
}

// ---------------------------------------------------------------------------
// Use cases
// ---------------------------------------------------------------------------

impl<S: Store> AppCore<S> {
    pub async fn trial_balance(
        &self,
        ctx: &RequestContext,
        from: Option<&str>,
        to: Option<&str>,
    ) -> AppResult<TrialBalanceView> {
        ctx.require(Permission::ViewBusiness)?;
        let (from, to) = (opt_date(from)?, opt_date(to)?);
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let totals = uow
            .account_totals(&org.id, from.as_deref(), to.as_deref(), None)
            .await?;
        let rows: Vec<TrialBalanceRow> = totals
            .into_iter()
            .filter(|t| t.opening_paise != 0 || t.debit_paise != 0 || t.credit_paise != 0)
            .map(|t| TrialBalanceRow {
                closing_paise: closing(&t),
                account_id: t.account_id,
                code: t.code,
                name: t.name,
                kind: t.kind,
                system_role: t.system_role,
                opening_paise: t.opening_paise,
                debit_paise: t.debit_paise,
                credit_paise: t.credit_paise,
            })
            .collect();
        let pos = |f: fn(&TrialBalanceRow) -> i64| rows.iter().map(|r| f(r).max(0)).sum::<i64>();
        let neg = |f: fn(&TrialBalanceRow) -> i64| rows.iter().map(|r| (-f(r)).max(0)).sum::<i64>();
        let debit: i64 = rows.iter().map(|r| r.debit_paise).sum();
        let credit: i64 = rows.iter().map(|r| r.credit_paise).sum();
        let closing_debit = pos(|r| r.closing_paise);
        let closing_credit = neg(|r| r.closing_paise);
        Ok(TrialBalanceView {
            from,
            to,
            opening_debit_paise: pos(|r| r.opening_paise),
            opening_credit_paise: neg(|r| r.opening_paise),
            debit_paise: debit,
            credit_paise: credit,
            closing_debit_paise: closing_debit,
            closing_credit_paise: closing_credit,
            balanced: debit == credit && closing_debit == closing_credit,
            rows,
        })
    }

    pub async fn profit_and_loss(
        &self,
        ctx: &RequestContext,
        from: &str,
        to: &str,
    ) -> AppResult<ProfitAndLossView> {
        ctx.require(Permission::ViewBusiness)?;
        let (from, to) = (iso(date(from)?), iso(date(to)?));
        if from > to {
            return Err(AppError::Invalid(
                "the start date is after the end date".into(),
            ));
        }
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let totals = uow
            .account_totals(&org.id, Some(&from), Some(&to), Some(YEAR_CLOSE))
            .await?;
        let mut trading = Vec::new();
        let mut direct = Vec::new();
        let mut other = Vec::new();
        let mut operating = Vec::new();
        for t in &totals {
            let movement = t.debit_paise - t.credit_paise;
            if movement == 0 {
                continue;
            }
            match t.kind.as_str() {
                "income" if t.system_role.as_deref() == Some("sales") => {
                    trading.push(line(t, -movement));
                }
                "income" => other.push(line(t, -movement)),
                "expense" if is_direct_cost(t) => direct.push(line(t, movement)),
                "expense" => operating.push(line(t, movement)),
                _ => {}
            }
        }
        let sections = vec![
            section("trading_income", "Sales", trading),
            section("direct_costs", "Cost of sales", direct),
            section("other_income", "Other income", other),
            section("operating_expenses", "Operating expenses", operating),
        ];
        let gross = sections[0].total_paise - sections[1].total_paise;
        let net = gross + sections[2].total_paise - sections[3].total_paise;
        Ok(ProfitAndLossView {
            from,
            to,
            sections,
            gross_profit_paise: gross,
            net_profit_paise: net,
        })
    }

    pub async fn balance_sheet(
        &self,
        ctx: &RequestContext,
        as_at: &str,
    ) -> AppResult<BalanceSheetView> {
        ctx.require(Permission::ViewBusiness)?;
        let as_at = iso(date(as_at)?);
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let totals = uow
            .account_totals(&org.id, None, Some(&as_at), None)
            .await?;
        let mut assets = Vec::new();
        let mut liabilities = Vec::new();
        let mut equity = Vec::new();
        let mut profit = 0;
        for t in &totals {
            let bal = closing(t);
            match t.kind.as_str() {
                "asset" if bal != 0 => assets.push(line(t, bal)),
                "liability" if bal != 0 => liabilities.push(line(t, -bal)),
                "equity" if bal != 0 => equity.push(line(t, -bal)),
                "income" | "expense" => profit -= bal,
                _ => {}
            }
        }
        let assets = section("assets", "Assets", assets);
        let liabilities = section("liabilities", "Liabilities", liabilities);
        let equity = section("equity", "Capital and reserves", equity);
        let right = liabilities.total_paise + equity.total_paise + profit;
        Ok(BalanceSheetView {
            as_at,
            total_assets_paise: assets.total_paise,
            total_liabilities_and_equity_paise: right,
            balanced: assets.total_paise == right,
            assets,
            liabilities,
            equity,
            profit_paise: profit,
        })
    }

    pub async fn general_ledger(
        &self,
        ctx: &RequestContext,
        account_id: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> AppResult<LedgerView> {
        ctx.require(Permission::ViewBusiness)?;
        let (from, to) = (opt_date(from)?, opt_date(to)?);
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let account = uow
            .get_account(&org.id, account_id)
            .await?
            .ok_or(AppError::NotFound("account"))?;
        let opening = match from
            .as_deref()
            .map(date)
            .transpose()?
            .and_then(|d| d.pred_opt())
        {
            Some(before) => {
                uow.account_balance_paise(&org.id, &account.id, None, Some(&iso(before)))
                    .await?
            }
            None => 0,
        };
        let rows = uow
            .account_book(&org.id, &account.id, from.as_deref(), to.as_deref())
            .await?;
        let (lines, dr, cr, closing) = ledger_lines(opening, rows);
        Ok(LedgerView {
            id: account.id,
            title: account.name,
            subtitle: format!("{} · {}", account.code, account.kind),
            ledger_type: "account",
            from,
            to,
            opening_paise: opening,
            debit_total_paise: dr,
            credit_total_paise: cr,
            closing_paise: closing,
            lines,
        })
    }

    pub async fn party_ledger(
        &self,
        ctx: &RequestContext,
        party_id: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> AppResult<LedgerView> {
        ctx.require(Permission::ViewBusiness)?;
        let (from, to) = (opt_date(from)?, opt_date(to)?);
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let party = uow
            .get_party(&org.id, party_id)
            .await?
            .ok_or(AppError::NotFound("party"))?;
        let opening = match from
            .as_deref()
            .map(date)
            .transpose()?
            .and_then(|d| d.pred_opt())
        {
            Some(before) => {
                uow.party_balance_until(&org.id, &party.id, Some(&iso(before)))
                    .await?
            }
            None => 0,
        };
        let rows = uow
            .party_book(&org.id, &party.id, from.as_deref(), to.as_deref())
            .await?;
        let (lines, dr, cr, closing) = ledger_lines(opening, rows);
        Ok(LedgerView {
            id: party.id,
            subtitle: party.gstin.clone().unwrap_or_else(|| party.kind.clone()),
            title: party.name,
            ledger_type: "party",
            from,
            to,
            opening_paise: opening,
            debit_total_paise: dr,
            credit_total_paise: cr,
            closing_paise: closing,
            lines,
        })
    }

    pub async fn journal_register(
        &self,
        ctx: &RequestContext,
        from: Option<&str>,
        to: Option<&str>,
    ) -> AppResult<JournalRegisterView> {
        ctx.require(Permission::ViewBusiness)?;
        let (from, to) = (opt_date(from)?, opt_date(to)?);
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let rows = uow
            .journal_rows(&org.id, from.as_deref(), to.as_deref(), JOURNAL_LIMIT + 1)
            .await?;
        let mut entries: Vec<JournalEntryView> = Vec::new();
        for r in rows {
            if entries.last().is_none_or(|e| e.entry_id != r.entry_id) {
                entries.push(JournalEntryView {
                    entry_id: r.entry_id.clone(),
                    entry_date: r.entry_date.clone(),
                    source_type: r.source_type.clone(),
                    source_id: r.source_id.clone(),
                    narration: r.narration.clone(),
                    reverses_entry_id: r.reverses_entry_id.clone(),
                    posted_at: r.posted_at.clone(),
                    posted_by: r.posted_by.clone(),
                    total_paise: 0,
                    lines: Vec::new(),
                });
            }
            if let Some(e) = entries.last_mut() {
                e.total_paise += r.debit_paise;
                e.lines.push(JournalLineView {
                    account_id: r.account_id,
                    account_code: r.account_code,
                    account_name: r.account_name,
                    debit_paise: r.debit_paise,
                    credit_paise: r.credit_paise,
                    party_name: r.party_name,
                });
            }
        }
        let limit = usize::try_from(JOURNAL_LIMIT).unwrap_or(usize::MAX);
        let truncated = entries.len() > limit;
        entries.truncate(limit);
        Ok(JournalRegisterView {
            from,
            to,
            entries,
            truncated,
        })
    }

    /// Receivables (`receivables`) or payables (`payables`) by age as on a
    /// date: what each party owes on each open document, by days overdue,
    /// plus anything not tied to a document (opening balances, advances).
    pub async fn ageing(
        &self,
        ctx: &RequestContext,
        kind: &str,
        as_of: &str,
    ) -> AppResult<AgeingView> {
        ctx.require(Permission::ViewBusiness)?;
        let as_of_date = date(as_of)?;
        let as_of = iso(as_of_date);
        let (doc_kind, sign) = match kind {
            "receivables" => ("sales", 1),
            "payables" => ("purchase", -1),
            _ => return Err(AppError::Invalid("choose receivables or payables".into())),
        };
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let docs = uow.outstanding_documents(&org.id, doc_kind, &as_of).await?;
        let mut parties: BTreeMap<String, AgeingParty> = BTreeMap::new();
        for d in docs {
            let due = d
                .due_date
                .as_deref()
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                .or_else(|| NaiveDate::parse_from_str(&d.doc_date, "%Y-%m-%d").ok())
                .unwrap_or(as_of_date);
            let days = (as_of_date - due).num_days();
            let bucket = bucket_of(days);
            let p = parties
                .entry(d.party_id.clone())
                .or_insert_with(|| AgeingParty {
                    party_id: d.party_id.clone(),
                    party_name: d.party_name.clone(),
                    buckets: [0; 5],
                    unallocated_paise: 0,
                    total_paise: 0,
                    documents: Vec::new(),
                });
            p.buckets[bucket] += d.outstanding_paise;
            p.documents.push(AgeingDocument {
                document_kind: d.document_kind,
                id: d.id,
                number: d.number,
                doc_date: d.doc_date,
                due_date: d.due_date,
                total_paise: d.total_paise,
                outstanding_paise: d.outstanding_paise,
                days_overdue: days,
                bucket,
            });
        }
        // What the ledger says each party owes, less what the documents
        // explain: opening balances and unallocated advances.
        for party in uow.list_parties(&org.id).await? {
            let wanted = match (kind, party.kind.as_str()) {
                ("receivables", "supplier") | ("payables", "customer") => {
                    parties.contains_key(&party.id)
                }
                _ => true,
            };
            if !wanted {
                continue;
            }
            let balance = sign
                * uow
                    .party_balance_until(&org.id, &party.id, Some(&as_of))
                    .await?;
            let documents: i64 = parties.get(&party.id).map_or(0, |p| p.buckets.iter().sum());
            let unallocated = balance - documents;
            if unallocated == 0 && documents == 0 {
                continue;
            }
            // A party whose net balance runs the other way (e.g. a customer
            // in credit) only shows here if it also has open documents.
            if documents == 0 && unallocated < 0 {
                continue;
            }
            let p = parties
                .entry(party.id.clone())
                .or_insert_with(|| AgeingParty {
                    party_id: party.id.clone(),
                    party_name: party.name.clone(),
                    buckets: [0; 5],
                    unallocated_paise: 0,
                    total_paise: 0,
                    documents: Vec::new(),
                });
            p.unallocated_paise = unallocated;
        }
        let mut list: Vec<AgeingParty> = parties
            .into_values()
            .map(|mut p| {
                p.total_paise = p.buckets.iter().sum::<i64>() + p.unallocated_paise;
                p
            })
            .collect();
        list.sort_by(|a, b| {
            b.total_paise
                .cmp(&a.total_paise)
                .then(a.party_name.cmp(&b.party_name))
        });
        let mut buckets = [0; 5];
        for p in &list {
            for (i, b) in p.buckets.iter().enumerate() {
                buckets[i] += b;
            }
        }
        let unallocated = list.iter().map(|p| p.unallocated_paise).sum();
        Ok(AgeingView {
            kind: kind.to_owned(),
            as_of,
            bucket_labels: [
                "Not due",
                "1–30 days",
                "31–60 days",
                "61–90 days",
                "Over 90 days",
            ],
            total_paise: list.iter().map(|p| p.total_paise).sum(),
            buckets,
            unallocated_paise: unallocated,
            parties: list,
        })
    }

    // -- Period lock and year-end close ------------------------------------

    pub async fn books_control(&self, ctx: &RequestContext) -> AppResult<BooksControlView> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let lock = uow.get_books_lock(&org.id).await?;
        let history = uow.list_fy_closes(&org.id).await?;
        let today = self.today();
        let start_month = u32::try_from(org.fy_start_month).unwrap_or(4);
        let books_start = date(&org.books_start_date)?;
        let mut fy = FinancialYear::containing(books_start, start_month);
        let mut years = Vec::new();
        while fy.start_date() <= today {
            let info = FinancialYearInfo::from(fy);
            let close = history
                .iter()
                .find(|c| c.financial_year == info.code && c.status == "closed")
                .cloned();
            years.push(YearStatus {
                status: if close.is_some() { "closed" } else { "open" },
                ended: fy.end_date() < today,
                close,
                year: info,
            });
            fy = fy.next();
        }
        years.reverse();
        Ok(BooksControlView {
            lock,
            books_start_date: org.books_start_date,
            years,
            history,
        })
    }

    /// Lock the books up to a date, move the lock, or (with `None`) remove it.
    /// Moving the lock back reopens a period and needs a reason.
    pub async fn set_books_lock(
        &self,
        ctx: &RequestContext,
        until: Option<&str>,
        reason: Option<&str>,
    ) -> AppResult<BooksControlView> {
        ctx.require(Permission::ManageBusiness)?;
        let until = opt_date(until)?;
        let reason = reason.map(str::trim).filter(|r| !r.is_empty());
        {
            let mut uow = self.store.begin().await?;
            let org = self.require_org(ctx, &mut uow).await?;
            let before = uow.get_books_lock(&org.id).await?;
            let reopening = match (&before, &until) {
                (Some(b), Some(u)) => u < &b.locked_until,
                (Some(_), None) => true,
                _ => false,
            };
            if reopening && reason.is_none() {
                return Err(AppError::Validation(vec![crate::FieldError {
                    field: "reason".into(),
                    message: "Say why the period is being reopened.".into(),
                }]));
            }
            if let Some(u) = &until {
                if date(u)? > self.today() {
                    return Err(AppError::Validation(vec![crate::FieldError {
                        field: "lockedUntil".into(),
                        message: "You can't lock a date that hasn't happened yet.".into(),
                    }]));
                }
                // A closed year stays locked.
                let closes = uow.list_fy_closes(&org.id).await?;
                if let Some(c) = closes
                    .iter()
                    .filter(|c| c.status == "closed")
                    .find(|c| &c.end_date > u)
                {
                    return Err(AppError::Conflict(format!(
                        "{} is closed. Reopen the year before unlocking it.",
                        c.financial_year
                    )));
                }
            } else if uow
                .list_fy_closes(&org.id)
                .await?
                .iter()
                .any(|c| c.status == "closed")
            {
                return Err(AppError::Conflict(
                    "A closed year stays locked. Reopen it first.".into(),
                ));
            }
            let now = self.now();
            let after = match &until {
                Some(u) => {
                    let rec = BooksLockRecord {
                        organisation_id: org.id.clone(),
                        locked_until: u.clone(),
                        reason: reason.map(str::to_owned),
                        updated_at: now.clone(),
                        updated_by: ctx.actor.user_id.clone(),
                    };
                    uow.set_books_lock(&rec).await?;
                    Some(rec)
                }
                None => {
                    uow.clear_books_lock(&org.id).await?;
                    None
                }
            };
            let c = Change {
                action: if reopening {
                    "books_lock.reopen"
                } else {
                    "books_lock.lock"
                },
                entity_id: None,
                before: before.as_ref().and_then(json),
                after: after.as_ref().and_then(json),
            };
            audit(&mut uow, ctx, &org.id, c, &now).await?;
            uow.commit().await?;
        }
        self.books_control(ctx).await
    }

    /// Close a financial year: move every income and expense balance to
    /// retained earnings with one entry on the last day, then lock the year.
    pub async fn close_financial_year(
        &self,
        ctx: &RequestContext,
        code: &str,
    ) -> AppResult<BooksControlView> {
        ctx.require(Permission::ManageBusiness)?;
        {
            let mut uow = self.store.begin().await?;
            let org = self.require_org(ctx, &mut uow).await?;
            let start_month = u32::try_from(org.fy_start_month).unwrap_or(4);
            let fy = self.find_year(&org.books_start_date, start_month, code)?;
            if fy.end_date() >= self.today() {
                return Err(AppError::Conflict(format!(
                    "{} hasn't ended yet. A year can be closed from the day after it ends.",
                    fy.code()
                )));
            }
            let closes = uow.list_fy_closes(&org.id).await?;
            if closes
                .iter()
                .any(|c| c.financial_year == fy.code() && c.status == "closed")
            {
                return Err(AppError::Conflict(format!(
                    "{} is already closed.",
                    fy.code()
                )));
            }
            // Years close in order.
            let books_start = date(&org.books_start_date)?;
            let previous = FinancialYear::new(fy.start_year() - 1, start_month);
            if previous.end_date() >= books_start
                && !closes
                    .iter()
                    .any(|c| c.financial_year == previous.code() && c.status == "closed")
            {
                return Err(AppError::Conflict(format!(
                    "Close {} first; years close in order.",
                    previous.code()
                )));
            }
            if let Some(l) = uow.get_books_lock(&org.id).await?
                && l.locked_until >= iso(fy.end_date())
            {
                // The closing entry is dated the year's last day; a lock
                // beyond it would refuse it. Closing re-applies the lock.
                return Err(AppError::Conflict(format!(
                    "The books are locked up to {}. Move the lock before {} to close the year.",
                    l.locked_until,
                    iso(fy.end_date())
                )));
            }
            let (start, end) = (iso(fy.start_date()), iso(fy.end_date()));
            let totals = uow
                .account_totals(&org.id, Some(&start), Some(&end), Some(YEAR_CLOSE))
                .await?;
            let mut lines = Vec::new();
            let mut profit = 0;
            for t in totals
                .iter()
                .filter(|t| t.kind == "income" || t.kind == "expense")
            {
                let movement = t.debit_paise - t.credit_paise;
                if movement == 0 {
                    continue;
                }
                profit -= movement;
                lines.push(PostLine {
                    role: "",
                    account_id: Some(t.account_id.clone()),
                    debit_paise: (-movement).max(0),
                    credit_paise: movement.max(0),
                    party_id: None,
                });
            }
            let now = self.now();
            let id = new_id().to_string();
            let entry_id = if lines.is_empty() {
                None
            } else {
                lines.push(if profit >= 0 {
                    PostLine::credit("retained_earnings", profit, None)
                } else {
                    PostLine::debit("retained_earnings", -profit, None)
                });
                Some(
                    post(
                        &mut uow,
                        ctx,
                        Posting {
                            org_id: &org.id,
                            entry_date: &end,
                            source_type: YEAR_CLOSE,
                            source_id: Some(&id),
                            narration: Some(format!(
                                "Year-end close {} — profit to retained earnings",
                                fy.code()
                            )),
                            reverses_entry_id: None,
                            idempotency_key: Some(format!("fy:close:{id}")),
                            lines,
                        },
                        &now,
                    )
                    .await?,
                )
            };
            let rec = FyCloseRecord {
                id: id.clone(),
                organisation_id: org.id.clone(),
                financial_year: fy.code(),
                start_date: start,
                end_date: end.clone(),
                status: "closed".into(),
                entry_id,
                profit_paise: profit,
                closed_at: now.clone(),
                closed_by: ctx.actor.user_id.clone(),
                reopen_entry_id: None,
                reopen_reason: None,
                reopened_at: None,
                reopened_by: None,
            };
            uow.insert_fy_close(&rec).await?;
            let lock = uow.get_books_lock(&org.id).await?;
            if lock.as_ref().is_none_or(|l| l.locked_until < end) {
                uow.set_books_lock(&BooksLockRecord {
                    organisation_id: org.id.clone(),
                    locked_until: end,
                    reason: Some(format!("Year {} closed", fy.code())),
                    updated_at: now.clone(),
                    updated_by: ctx.actor.user_id.clone(),
                })
                .await?;
            }
            let c = Change {
                action: "financial_year.close",
                entity_id: Some(&id),
                before: None,
                after: json(&rec),
            };
            audit(&mut uow, ctx, &org.id, c, &now).await?;
            uow.commit().await?;
        }
        self.books_control(ctx).await
    }

    /// Reopen the latest closed year: unlock it and reverse its closing entry.
    pub async fn reopen_financial_year(
        &self,
        ctx: &RequestContext,
        code: &str,
        reason: &str,
    ) -> AppResult<BooksControlView> {
        ctx.require(Permission::ManageBusiness)?;
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(AppError::Validation(vec![crate::FieldError {
                field: "reason".into(),
                message: "Say why the year is being reopened.".into(),
            }]));
        }
        {
            let mut uow = self.store.begin().await?;
            let org = self.require_org(ctx, &mut uow).await?;
            let closes = uow.list_fy_closes(&org.id).await?;
            let Some(close) = closes
                .iter()
                .find(|c| c.financial_year == code && c.status == "closed")
                .cloned()
            else {
                return Err(AppError::Conflict(format!("{code} isn't closed.")));
            };
            if closes
                .iter()
                .any(|c| c.status == "closed" && c.start_date > close.start_date)
            {
                return Err(AppError::Conflict(
                    "Reopen the later years first; years reopen newest first.".into(),
                ));
            }
            let now = self.now();
            // Unlock from the year's start, then reverse the closing entry.
            let before_lock = uow.get_books_lock(&org.id).await?;
            let unlock_to = date(&close.start_date)?.pred_opt().map(iso);
            match unlock_to {
                Some(d) if d >= org.books_start_date => {
                    uow.set_books_lock(&BooksLockRecord {
                        organisation_id: org.id.clone(),
                        locked_until: d,
                        reason: Some(format!("Year {code} reopened: {reason}")),
                        updated_at: now.clone(),
                        updated_by: ctx.actor.user_id.clone(),
                    })
                    .await?;
                }
                _ => uow.clear_books_lock(&org.id).await?,
            }
            let reopen_entry_id = match &close.entry_id {
                Some(entry_id) => {
                    let rows = uow
                        .journal_rows(
                            &org.id,
                            Some(&close.end_date),
                            Some(&close.end_date),
                            100_000,
                        )
                        .await?;
                    let lines: Vec<PostLine> = rows
                        .iter()
                        .filter(|r| &r.entry_id == entry_id)
                        .map(|r| PostLine {
                            role: "",
                            account_id: Some(r.account_id.clone()),
                            debit_paise: r.credit_paise,
                            credit_paise: r.debit_paise,
                            party_id: None,
                        })
                        .collect();
                    Some(
                        post(
                            &mut uow,
                            ctx,
                            Posting {
                                org_id: &org.id,
                                entry_date: &close.end_date,
                                source_type: YEAR_CLOSE,
                                source_id: Some(&close.id),
                                narration: Some(format!("Year {code} reopened — {reason}")),
                                reverses_entry_id: Some(entry_id),
                                idempotency_key: Some(format!("fy:reopen:{}", close.id)),
                                lines,
                            },
                            &now,
                        )
                        .await?,
                    )
                }
                None => None,
            };
            let mut after = close.clone();
            after.status = "reopened".into();
            after.reopen_entry_id = reopen_entry_id;
            after.reopen_reason = Some(reason.to_owned());
            after.reopened_at = Some(now.clone());
            after.reopened_by = Some(ctx.actor.user_id.clone());
            uow.update_fy_close(&after).await?;
            let c = Change {
                action: "financial_year.reopen",
                entity_id: Some(&close.id),
                before: before_lock.as_ref().and_then(json),
                after: json(&after),
            };
            audit(&mut uow, ctx, &org.id, c, &now).await?;
            uow.commit().await?;
        }
        self.books_control(ctx).await
    }

    fn find_year(
        &self,
        books_start: &str,
        start_month: u32,
        code: &str,
    ) -> AppResult<FinancialYear> {
        let mut fy = FinancialYear::containing(date(books_start)?, start_month);
        let today = self.today();
        while fy.start_date() <= today {
            if fy.code() == code {
                return Ok(fy);
            }
            fy = fy.next();
        }
        Err(AppError::NotFound("financial year"))
    }

    // -- Logo ----------------------------------------------------------------

    /// Set the logo printed on invoices: PNG, JPEG or SVG up to 1 MB.
    pub async fn set_logo(
        &self,
        ctx: &RequestContext,
        file_name: &str,
        bytes: &[u8],
    ) -> AppResult<LogoContent> {
        ctx.require(Permission::ManageBusiness)?;
        if bytes.is_empty() {
            return Err(AppError::Invalid("the file is empty".into()));
        }
        if bytes.len() > LOGO_MAX_BYTES {
            return Err(AppError::Invalid("the logo must be 1 MB or smaller".into()));
        }
        let content_type = logo_type(bytes)
            .ok_or_else(|| AppError::Invalid("use a PNG, JPEG or SVG image for the logo".into()))?;
        if content_type == "image/svg+xml" && !svg_is_safe(bytes) {
            return Err(AppError::Invalid(
                "this SVG contains scripts or links; export it as a plain SVG or PNG".into(),
            ));
        }
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let before = uow.get_logo(&org.id).await?;
        let sha = sha256_hex(bytes);
        uow.put_blob(&sha, bytes).await?;
        let now = self.now();
        let rec = LogoRecord {
            organisation_id: org.id.clone(),
            sha256: sha,
            file_name: file_name.chars().take(200).collect(),
            content_type: content_type.into(),
            size_bytes: i64::try_from(bytes.len()).unwrap_or(i64::MAX),
            updated_at: now.clone(),
            updated_by: ctx.actor.user_id.clone(),
        };
        uow.set_logo(&rec).await?;
        let c = Change {
            action: "organisation_logo.update",
            entity_id: Some(&org.id),
            before: before.as_ref().and_then(json),
            after: json(&rec),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        uow.commit().await?;
        Ok(LogoContent {
            file_name: rec.file_name,
            content_type: rec.content_type,
            size_bytes: rec.size_bytes,
            bytes: bytes.to_vec(),
        })
    }

    pub async fn logo(&self, ctx: &RequestContext) -> AppResult<Option<LogoContent>> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let Some(org) = self.find_org(ctx, &mut uow).await? else {
            return Ok(None);
        };
        let Some(l) = uow.get_logo(&org.id).await? else {
            return Ok(None);
        };
        let bytes = uow
            .blob(&l.sha256)
            .await?
            .ok_or(AppError::NotFound("logo"))?;
        if sha256_hex(&bytes) != l.sha256 {
            return Err(AppError::Invalid(
                "the stored logo is damaged; upload it again".into(),
            ));
        }
        Ok(Some(LogoContent {
            file_name: l.file_name,
            content_type: l.content_type,
            size_bytes: l.size_bytes,
            bytes,
        }))
    }

    pub async fn remove_logo(&self, ctx: &RequestContext) -> AppResult<()> {
        ctx.require(Permission::ManageBusiness)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let before = uow.get_logo(&org.id).await?;
        uow.delete_logo(&org.id).await?;
        let now = self.now();
        let c = Change {
            action: "organisation_logo.remove",
            entity_id: Some(&org.id),
            before: before.as_ref().and_then(json),
            after: None,
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        uow.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_by_days_overdue() {
        assert_eq!(bucket_of(-5), 0);
        assert_eq!(bucket_of(0), 0);
        assert_eq!(bucket_of(1), 1);
        assert_eq!(bucket_of(30), 1);
        assert_eq!(bucket_of(31), 2);
        assert_eq!(bucket_of(90), 3);
        assert_eq!(bucket_of(91), 4);
    }

    #[test]
    fn logo_types_and_unsafe_svg() {
        assert_eq!(logo_type(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
        assert_eq!(logo_type(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(
            logo_type(b"<svg xmlns='http://www.w3.org/2000/svg'/>"),
            Some("image/svg+xml")
        );
        assert_eq!(logo_type(b"GIF89a"), None);
        assert!(svg_is_safe(
            b"<svg viewBox='0 0 10 10'><path d='M0 0h10'/></svg>"
        ));
        assert!(!svg_is_safe(b"<svg><script>alert(1)</script></svg>"));
        assert!(!svg_is_safe(b"<svg onload=\"x()\"></svg>"));
        assert!(svg_is_safe(b"<svg><text>Union Bank</text></svg>"));
    }
}
