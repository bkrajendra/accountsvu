//! Posting journal entries (spec §5.5). Every posting is validated as a
//! balanced [`JournalEntry`] before it reaches storage, and corrections are
//! new reversal entries — posted lines are never edited.

use av_accounting::{JournalEntry, JournalLine};
use av_domain::{Id, Money, new_id};
use av_persistence::JournalRepo;
use av_persistence::records::{JournalEntryRecord, JournalLineRecord};

use crate::context::RequestContext;
use crate::error::{AppError, AppResult};

/// One line to post: account by system role (or by id, when `account_id` is
/// set), signed side, optional party.
#[derive(Clone, Debug)]
pub(crate) struct PostLine {
    pub role: &'static str,
    /// A specific account (already validated as belonging to the organisation).
    pub account_id: Option<String>,
    pub debit_paise: i64,
    pub credit_paise: i64,
    pub party_id: Option<String>,
}

impl PostLine {
    pub fn debit(role: &'static str, paise: i64, party_id: Option<&str>) -> Self {
        PostLine {
            role,
            account_id: None,
            debit_paise: paise,
            credit_paise: 0,
            party_id: party_id.map(str::to_owned),
        }
    }
    pub fn credit(role: &'static str, paise: i64, party_id: Option<&str>) -> Self {
        PostLine {
            role,
            account_id: None,
            debit_paise: 0,
            credit_paise: paise,
            party_id: party_id.map(str::to_owned),
        }
    }
    /// Debit a specific account by id.
    pub fn debit_account(account_id: &str, paise: i64) -> Self {
        PostLine {
            role: "",
            account_id: Some(account_id.to_owned()),
            debit_paise: paise,
            credit_paise: 0,
            party_id: None,
        }
    }
    /// The same line on the opposite side, for a reversal.
    pub fn mirrored(&self) -> Self {
        PostLine {
            debit_paise: self.credit_paise,
            credit_paise: self.debit_paise,
            ..self.clone()
        }
    }
}

pub(crate) struct Posting<'a> {
    pub org_id: &'a str,
    pub entry_date: &'a str,
    pub source_type: &'a str,
    pub source_id: Option<&'a str>,
    pub narration: Option<String>,
    pub reverses_entry_id: Option<&'a str>,
    /// Unique per organisation (enforced by the database): the same business
    /// event can never post twice, e.g. `sales:issue:<id>`.
    pub idempotency_key: Option<String>,
    pub lines: Vec<PostLine>,
}

/// Validate and insert a journal entry; returns its id.
pub(crate) async fn post(
    uow: &mut (impl JournalRepo + Send),
    ctx: &RequestContext,
    p: Posting<'_>,
    now: &str,
) -> AppResult<String> {
    // A locked period takes no postings (the database enforces it too).
    if let Some(until) = uow.books_locked_until(p.org_id).await?
        && p.entry_date <= until.as_str()
    {
        return Err(AppError::Conflict(format!(
            "The books are locked up to {until}. Reopen the period in Settings to post on {}.",
            p.entry_date
        )));
    }
    let mut p = p;
    p.lines
        .retain(|l| l.debit_paise != 0 || l.credit_paise != 0);
    let mut resolved = Vec::with_capacity(p.lines.len());
    for l in &p.lines {
        if let Some(id) = &l.account_id {
            resolved.push(id.clone());
            continue;
        }
        let account = uow
            .account_id_by_role(p.org_id, l.role)
            .await?
            .ok_or_else(|| {
                AppError::Invalid(format!("the chart of accounts has no '{}' account", l.role))
            })?;
        resolved.push(account);
    }

    // Balance and shape checks live in the accounting crate.
    let check: Vec<JournalLine> = p
        .lines
        .iter()
        .zip(&resolved)
        .map(|(l, account)| {
            let id = account.parse::<Id>().unwrap_or_default();
            if l.debit_paise > 0 {
                JournalLine::debit(id, Money::from_paise(l.debit_paise))
            } else {
                JournalLine::credit(id, Money::from_paise(l.credit_paise))
            }
        })
        .collect();
    JournalEntry::new(check).map_err(|e| AppError::Invalid(e.to_string()))?;

    let entry_id = new_id().to_string();
    let entry = JournalEntryRecord {
        id: entry_id.clone(),
        organisation_id: p.org_id.to_owned(),
        entry_date: p.entry_date.to_owned(),
        source_type: p.source_type.to_owned(),
        source_id: p.source_id.map(str::to_owned),
        narration: p.narration,
        reverses_entry_id: p.reverses_entry_id.map(str::to_owned),
        idempotency_key: p.idempotency_key,
        posted_at: now.to_owned(),
        posted_by: Some(ctx.actor.user_id.clone()),
    };
    let lines: Vec<JournalLineRecord> = p
        .lines
        .into_iter()
        .zip(resolved)
        .enumerate()
        .map(|(i, (l, account_id))| JournalLineRecord {
            id: new_id().to_string(),
            journal_entry_id: entry_id.clone(),
            line_no: i64::try_from(i + 1).unwrap_or(i64::MAX),
            account_id,
            debit_paise: l.debit_paise,
            credit_paise: l.credit_paise,
            memo: None,
            party_id: l.party_id,
        })
        .collect();
    uow.insert_journal(&entry, &lines).await?;
    Ok(entry_id)
}
