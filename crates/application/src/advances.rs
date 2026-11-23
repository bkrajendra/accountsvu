//! Advances, refunds and GST on advances (Phase 2.6).
//!
//! Money a party pays before there is an invoice or bill to settle stays on
//! the voucher as an advance ("on account"). Later it is adjusted against a
//! document, or refunded. A refund is a voucher in the other direction that
//! settles the party's own side of the ledger (a payment back to a customer
//! debits receivables).
//!
//! For services, tax is due when an advance is received (section 13): the
//! receipt records the tax inside the advance (GSTR-1 table 11A) and posts
//! it (Dr GST paid on advances, Cr output tax). When the advance is adjusted
//! against an invoice, or refunded, that part of the tax is taken back (Dr
//! output tax, Cr GST paid on advances; table 11B), since the invoice
//! charges it again. Goods are exempt from tax on advances (notification
//! 66/2017-CT), so tax on an advance is something the user turns on.

use std::collections::HashMap;

use av_domain::{Money, StateCode, new_id};
use av_gst_engine::rules::RateStatus;
use av_gst_engine::{RulePack, SupplyKind, TaxRate, tax_in_gross};
use av_persistence::records::{
    AdvanceAdjustmentRecord, AdvanceTaxRecord, MoneyVoucherRecord, OpenDocumentRecord,
    RegistrationRecord, VoucherAllocationRecord,
};
use av_persistence::{BankingRepo, PartyRepo, RegistrationRepo, SalesRepo, Store, UnitOfWork};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::AppCore;
use crate::banking::{AllocationInput, VoucherView};
use crate::context::{Permission, RequestContext};
use crate::error::{AppError, AppResult, Errors};
use crate::masters::parse_amount;
use crate::posting::{PostLine, Posting, post};
use crate::returns::state_name;
use crate::setup::{Change, audit, parse_date};

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

/// Tax on the part of a receipt left as an advance.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvanceTaxInput {
    pub registration_id: String,
    pub place_of_supply: i64,
    pub rate_bp: u32,
}

/// Adjust what is left of an advance against later invoices or bills.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustAdvanceInput {
    pub date: String,
    pub allocations: Vec<AllocationInput>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvanceTaxView {
    pub registration_id: String,
    pub gstin: String,
    pub place_of_supply: i64,
    pub place_name: String,
    pub rate_bp: i64,
    pub gross_paise: i64,
    pub taxable_paise: i64,
    pub tax_paise: i64,
    /// Tax not yet taken back by adjustments or refunds.
    pub remaining_tax_paise: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvanceAdjustmentView {
    pub id: String,
    /// `invoice` or `refund`.
    pub reason: String,
    pub adjust_date: String,
    pub gross_paise: i64,
    pub tax_paise: i64,
    pub status: String,
}

/// A receipt (or payment) of a party with something still on account.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartyAdvanceView {
    pub voucher_id: String,
    pub number: String,
    pub voucher_date: String,
    pub amount_paise: i64,
    pub on_account_paise: i64,
    /// GSTIN the advance was taxed under, if tax was paid on it.
    pub taxed_registration_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Tax parts
// ---------------------------------------------------------------------------

/// An advance (or part of one) split into taxable value and tax heads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Parts {
    pub gross: i64,
    pub taxable: i64,
    pub igst: i64,
    pub cgst: i64,
    pub sgst: i64,
    pub utgst: i64,
    pub cess: i64,
}

impl Parts {
    pub(crate) fn tax(&self) -> i64 {
        self.igst + self.cgst + self.sgst + self.utgst + self.cess
    }
    fn minus(self, o: Parts) -> Parts {
        Parts {
            gross: self.gross - o.gross,
            taxable: self.taxable - o.taxable,
            igst: self.igst - o.igst,
            cgst: self.cgst - o.cgst,
            sgst: self.sgst - o.sgst,
            utgst: self.utgst - o.utgst,
            cess: self.cess - o.cess,
        }
    }
    pub(crate) fn plus(self, o: Parts) -> Parts {
        self.minus(o.negate())
    }
    fn negate(self) -> Parts {
        Parts::default().minus(self)
    }
    pub(crate) fn of_tax(t: &AdvanceTaxRecord) -> Parts {
        Parts {
            gross: t.gross_paise,
            taxable: t.taxable_paise,
            igst: t.igst_paise,
            cgst: t.cgst_paise,
            sgst: t.sgst_paise,
            utgst: t.utgst_paise,
            cess: t.cess_paise,
        }
    }
    pub(crate) fn of_adjustment(a: &AdvanceAdjustmentRecord) -> Parts {
        Parts {
            gross: a.gross_paise,
            taxable: a.taxable_paise,
            igst: a.igst_paise,
            cgst: a.cgst_paise,
            sgst: a.sgst_paise,
            utgst: a.utgst_paise,
            cess: a.cess_paise,
        }
    }
}

fn paise(m: Money) -> i64 {
    m.to_paise().unwrap_or(0)
}

/// The tax inside `gross` at `rate`.
pub(crate) fn parts_in(gross: i64, rate_bp: u32, kind: SupplyKind) -> Parts {
    let t = tax_in_gross(Money::from_paise(gross), TaxRate(rate_bp), kind);
    Parts {
        gross,
        taxable: paise(t.taxable_value),
        igst: paise(t.igst),
        cgst: paise(t.cgst),
        sgst: paise(t.sgst),
        utgst: paise(t.utgst),
        cess: paise(t.cess),
    }
}

/// What is still untaken of a taxed advance.
pub(crate) fn remaining(tax: &AdvanceTaxRecord, adjustments: &[AdvanceAdjustmentRecord]) -> Parts {
    adjustments
        .iter()
        .filter(|a| a.voucher_id == tax.voucher_id && a.status == "posted")
        .fold(Parts::of_tax(tax), |left, a| {
            left.minus(Parts::of_adjustment(a))
        })
}

/// The part of a taxed advance that `gross` uses up: in proportion, and the
/// whole remainder when it uses up the rest, so nothing is left over from
/// rounding.
pub(crate) fn take(left: Parts, gross: i64, rate_bp: u32, kind: SupplyKind) -> Parts {
    if gross >= left.gross {
        return left;
    }
    let mut p = parts_in(gross, rate_bp, kind);
    p.igst = p.igst.min(left.igst);
    p.cgst = p.cgst.min(left.cgst);
    p.sgst = p.sgst.min(left.sgst);
    p.utgst = p.utgst.min(left.utgst);
    p.cess = p.cess.min(left.cess);
    p.taxable = gross - p.tax();
    p
}

/// Lines posting the tax on an advance: Dr GST paid on advances, Cr output
/// tax. Their mirror image takes it back.
pub(crate) fn tax_lines(p: &Parts) -> Vec<PostLine> {
    if p.tax() == 0 {
        return Vec::new();
    }
    let mut lines = vec![PostLine::debit("gst_on_advances", p.tax(), None)];
    for (role, amount) in [
        ("output_igst", p.igst),
        ("output_cgst", p.cgst),
        ("output_sgst", p.sgst),
        ("output_utgst", p.utgst),
        ("output_cess", p.cess),
    ] {
        if amount != 0 {
            lines.push(PostLine::credit(role, amount, None));
        }
    }
    lines
}

pub(crate) fn supply_kind(reg: &RegistrationRecord, place_of_supply: i64) -> SupplyKind {
    match (
        u8::try_from(reg.state_code)
            .ok()
            .and_then(|c| StateCode::new(c).ok()),
        u8::try_from(place_of_supply)
            .ok()
            .and_then(|c| StateCode::new(c).ok()),
    ) {
        (Some(s), Some(p)) => SupplyKind::determine(s, p),
        _ => SupplyKind::InterState,
    }
}

/// The ledger side a party voucher settles: receipts credit receivables and
/// payments debit payables, unless they are refunds, which work on the
/// party's other side (a refund to a customer debits receivables).
pub(crate) fn party_role(voucher_type: &str, purpose: &str) -> &'static str {
    match (voucher_type, purpose) {
        ("receipt", "refund") | ("payment", "settlement") => "payables",
        _ => "receivables",
    }
}

/// Validate tax on an advance and work it out.
pub(crate) fn advance_tax(
    e: &mut Errors,
    input: &AdvanceTaxInput,
    regs: &[RegistrationRecord],
    date: Option<NaiveDate>,
    gross: i64,
) -> Option<(RegistrationRecord, Parts)> {
    let reg = regs.iter().find(|r| r.id == input.registration_id);
    match reg {
        None => e.add(
            "advanceTax.registrationId",
            "Choose the GSTIN the advance was received under.",
        ),
        Some(r) if r.registration_type == "composition" => e.add(
            "advanceTax.registrationId",
            "A composition registration doesn't pay tax on advances.",
        ),
        Some(r) if r.status != "active" => {
            e.add("advanceTax.registrationId", "This GSTIN isn't active.");
        }
        _ => {}
    }
    if u8::try_from(input.place_of_supply)
        .ok()
        .and_then(|c| StateCode::new(c).ok())
        .is_none()
    {
        e.add("advanceTax.placeOfSupply", "Choose the place of supply.");
    }
    let pack = RulePack::bundled();
    match date.map(|d| pack.rate_status(input.rate_bp, d)) {
        Some(RateStatus::InForce) => {}
        Some(RateStatus::NotInForce { .. }) => {
            e.add(
                "advanceTax.rateBp",
                "This rate isn't in force on the receipt date.",
            );
        }
        _ => e.add("advanceTax.rateBp", "Choose a GST rate."),
    }
    if gross <= 0 {
        e.add(
            "advanceTax",
            "Nothing is left as an advance to pay tax on: the whole amount settles invoices.",
        );
    }
    let reg = reg?.clone();
    if !e.is_empty() || gross <= 0 {
        return None;
    }
    let kind = supply_kind(&reg, input.place_of_supply);
    Some((reg, parts_in(gross, input.rate_bp, kind)))
}

/// One movement of tax on advances for a GSTIN: received (table 11A) or
/// adjusted (table 11B).
#[derive(Clone, Debug)]
pub(crate) struct AdvanceEvent {
    pub registration_id: String,
    pub date: String,
    pub received: bool,
    pub place_of_supply: i64,
    pub rate_bp: u32,
    pub parts: Parts,
    pub voucher_id: String,
    pub number: String,
}

pub(crate) fn tax_record(
    voucher_id: &str,
    org_id: &str,
    reg_id: &str,
    input: &AdvanceTaxInput,
    p: &Parts,
) -> AdvanceTaxRecord {
    AdvanceTaxRecord {
        voucher_id: voucher_id.to_owned(),
        organisation_id: org_id.to_owned(),
        gst_registration_id: reg_id.to_owned(),
        place_of_supply: input.place_of_supply,
        rate_bp: i64::from(input.rate_bp),
        gross_paise: p.gross,
        taxable_paise: p.taxable,
        igst_paise: p.igst,
        cgst_paise: p.cgst,
        sgst_paise: p.sgst,
        utgst_paise: p.utgst,
        cess_paise: p.cess,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn adjustment_record(
    org_id: &str,
    voucher_id: &str,
    reason: &str,
    allocation_id: Option<String>,
    refund_voucher_id: Option<String>,
    date: &str,
    p: &Parts,
    entry_id: &str,
    now: &str,
) -> AdvanceAdjustmentRecord {
    AdvanceAdjustmentRecord {
        id: new_id().to_string(),
        organisation_id: org_id.to_owned(),
        voucher_id: voucher_id.to_owned(),
        reason: reason.to_owned(),
        allocation_id,
        refund_voucher_id,
        adjust_date: date.to_owned(),
        gross_paise: p.gross,
        taxable_paise: p.taxable,
        igst_paise: p.igst,
        cgst_paise: p.cgst,
        sgst_paise: p.sgst,
        utgst_paise: p.utgst,
        cess_paise: p.cess,
        journal_entry_id: entry_id.to_owned(),
        status: "posted".into(),
        cancel_entry_id: None,
        created_at: now.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// Use cases
// ---------------------------------------------------------------------------

/// What a voucher still holds on account, with its tax if it was taxed.
pub(crate) struct AdvanceState {
    pub on_account: i64,
    pub refunded: i64,
    pub refunds: Vec<MoneyVoucherRecord>,
    pub tax: Option<AdvanceTaxRecord>,
    pub adjustments: Vec<AdvanceAdjustmentRecord>,
}

impl<S: Store> AppCore<S> {
    pub(crate) async fn advance_state(
        &self,
        uow: &mut S::Uow,
        org_id: &str,
        v: &MoneyVoucherRecord,
    ) -> AppResult<AdvanceState> {
        let allocated: i64 = uow
            .voucher_allocations(&v.id)
            .await?
            .iter()
            .map(|a| a.amount_paise)
            .sum();
        let refunds: Vec<MoneyVoucherRecord> = uow
            .list_vouchers(org_id)
            .await?
            .into_iter()
            .filter(|r| r.refund_of_id.as_deref() == Some(v.id.as_str()) && r.status == "posted")
            .collect();
        let refunded: i64 = refunds.iter().map(|r| r.amount_paise).sum();
        let tax = uow
            .list_advance_taxes(org_id)
            .await?
            .into_iter()
            .find(|t| t.voucher_id == v.id);
        let adjustments = uow
            .list_advance_adjustments(org_id)
            .await?
            .into_iter()
            .filter(|a| a.voucher_id == v.id)
            .collect();
        let on_account = if v.party_id.is_some() && v.purpose == "settlement" {
            v.amount_paise - allocated - refunded
        } else {
            0
        };
        Ok(AdvanceState {
            on_account,
            refunded,
            refunds,
            tax,
            adjustments,
        })
    }

    pub(crate) async fn advance_views(
        &self,
        uow: &mut S::Uow,
        org_id: &str,
        state: &AdvanceState,
    ) -> AppResult<(Option<AdvanceTaxView>, Vec<AdvanceAdjustmentView>)> {
        let Some(t) = &state.tax else {
            return Ok((None, Vec::new()));
        };
        let gstin = uow
            .list_registrations(org_id)
            .await?
            .into_iter()
            .find(|r| r.id == t.gst_registration_id)
            .map(|r| r.gstin)
            .unwrap_or_default();
        let left = remaining(t, &state.adjustments);
        let view = AdvanceTaxView {
            registration_id: t.gst_registration_id.clone(),
            gstin,
            place_of_supply: t.place_of_supply,
            place_name: state_name(t.place_of_supply),
            rate_bp: t.rate_bp,
            gross_paise: t.gross_paise,
            taxable_paise: t.taxable_paise,
            tax_paise: Parts::of_tax(t).tax(),
            remaining_tax_paise: left.tax(),
        };
        let adjustments = state
            .adjustments
            .iter()
            .map(|a| AdvanceAdjustmentView {
                id: a.id.clone(),
                reason: a.reason.clone(),
                adjust_date: a.adjust_date.clone(),
                gross_paise: a.gross_paise,
                tax_paise: Parts::of_adjustment(a).tax(),
                status: a.status.clone(),
            })
            .collect();
        Ok((Some(view), adjustments))
    }

    /// Receipts (for `receipt`) or payments of a party with something still
    /// on account, oldest first: what can be adjusted against a document.
    pub async fn party_advances(
        &self,
        ctx: &RequestContext,
        party_id: &str,
        voucher_type: &str,
    ) -> AppResult<Vec<PartyAdvanceView>> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let mut vouchers: Vec<MoneyVoucherRecord> = uow
            .list_vouchers(&org.id)
            .await?
            .into_iter()
            .filter(|v| {
                v.party_id.as_deref() == Some(party_id)
                    && v.voucher_type == voucher_type
                    && v.status == "posted"
                    && v.purpose == "settlement"
            })
            .collect();
        vouchers.sort_by(|a, b| a.voucher_date.cmp(&b.voucher_date));
        let mut out = Vec::new();
        for v in vouchers {
            let s = self.advance_state(&mut uow, &org.id, &v).await?;
            if s.on_account > 0 {
                out.push(PartyAdvanceView {
                    voucher_id: v.id,
                    number: v.number,
                    voucher_date: v.voucher_date,
                    amount_paise: v.amount_paise,
                    on_account_paise: s.on_account,
                    taxed_registration_id: s.tax.map(|t| t.gst_registration_id),
                });
            }
        }
        Ok(out)
    }

    /// Adjust what is left of an advance against the party's open invoices
    /// (receipts) or bills (payments). A taxed advance takes back the tax on
    /// the part adjusted.
    pub async fn adjust_advance(
        &self,
        ctx: &RequestContext,
        voucher_id: &str,
        input: &AdjustAdvanceInput,
    ) -> AppResult<VoucherView> {
        ctx.require(Permission::PrepareDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let v = uow
            .get_voucher(&org.id, voucher_id)
            .await?
            .ok_or(AppError::NotFound("receipt or payment"))?;
        let Some(party_id) = v.party_id.clone() else {
            return Err(AppError::Invalid(
                "only money from or to a party can settle documents".into(),
            ));
        };
        if v.status != "posted" {
            return Err(AppError::Conflict("This is cancelled.".into()));
        }
        if v.purpose != "settlement" {
            return Err(AppError::Conflict(
                "A refund can't settle documents.".into(),
            ));
        }
        let state = self.advance_state(&mut uow, &org.id, &v).await?;
        let mut e = Errors::new("");
        let today = self.today();
        let date = parse_date(&mut e, "date", &input.date);
        if let Some(d) = date {
            if d > today {
                e.add("date", "The date can't be in the future.");
            }
            if NaiveDate::parse_from_str(&v.voucher_date, "%Y-%m-%d").is_ok_and(|vd| d < vd) {
                e.add("date", "The date can't be before the receipt or payment.");
            }
        }
        let kind = if v.voucher_type == "payment" {
            "purchase"
        } else {
            "sales"
        };
        let open = uow.open_documents(&org.id, &party_id, kind).await?;
        let mut chosen: Vec<(OpenDocumentRecord, i64)> = Vec::new();
        for (i, a) in input.allocations.iter().enumerate() {
            let field = format!("allocations.{i}.amount");
            let Some(amt) = parse_amount(&mut e, &field, &Some(a.amount.clone()), false) else {
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
            if chosen.iter().any(|(d, _)| d.id == doc.id) {
                e.add(&field, "This document is listed twice.");
                continue;
            }
            if amt > doc.total_paise - doc.paid_paise - doc.adjusted_paise {
                e.add(&field, "More than is outstanding on this document.");
                continue;
            }
            if date.is_some_and(|d| d.format("%Y-%m-%d").to_string() < doc.doc_date) {
                e.add(&field, "The document is dated after the adjustment date.");
                continue;
            }
            chosen.push((doc.clone(), amt));
        }
        let total: i64 = chosen.iter().map(|(_, a)| a).sum();
        if chosen.is_empty() && !!e.is_empty() {
            e.add(
                "allocations",
                "Enter an amount against at least one document.",
            );
        }
        if total > state.on_account {
            e.add(
                "allocations",
                "That adds up to more than is left of the advance.",
            );
        }
        // A taxed advance only adjusts against invoices of the same GSTIN.
        let mut reg = None;
        if let Some(t) = &state.tax {
            reg = uow
                .list_registrations(&org.id)
                .await?
                .into_iter()
                .find(|r| r.id == t.gst_registration_id);
            for (i, (doc, _)) in chosen.iter().enumerate() {
                let d = uow.get_sales(&org.id, &doc.id).await?;
                if d.and_then(|d| d.gst_registration_id).as_deref()
                    != Some(t.gst_registration_id.as_str())
                {
                    e.add(
                        &format!("allocations.{i}.amount"),
                        "Tax on this advance was paid under another GSTIN; adjust it against that GSTIN's invoices.",
                    );
                }
            }
        }
        e.finish()?;
        let date = date.ok_or_else(|| AppError::Invalid("choose a date".into()))?;
        let date = date.format("%Y-%m-%d").to_string();
        if let (Some(t), Some(_)) = (&state.tax, &reg)
            && let Some(lock) = self
                .gst_lock(
                    &mut uow,
                    &org.id,
                    Some(&t.gst_registration_id),
                    &date,
                    "sales",
                )
                .await?
        {
            return Err(AppError::Conflict(format!(
                "{lock} is filed, so tax on advances can't be adjusted in that month. Use a date in an open month."
            )));
        }

        let now = self.now();
        let mut left = state.tax.as_ref().map(|t| remaining(t, &state.adjustments));
        for (doc, amt) in &chosen {
            let allocation_id = new_id().to_string();
            uow.insert_allocation(&VoucherAllocationRecord {
                id: allocation_id.clone(),
                voucher_id: v.id.clone(),
                document_kind: doc.document_kind.clone(),
                document_id: doc.id.clone(),
                amount_paise: *amt,
                allocated_on: Some(date.clone()),
                allocated_by: Some(ctx.actor.user_id.clone()),
            })
            .await?;
            if !uow
                .adjust_paid(&org.id, &doc.document_kind, &doc.id, *amt)
                .await?
            {
                return Err(AppError::stale());
            }
            if let (Some(t), Some(r), Some(l)) = (&state.tax, &reg, left.as_mut()) {
                let kind = supply_kind(r, t.place_of_supply);
                let part = take(*l, *amt, u32::try_from(t.rate_bp).unwrap_or(0), kind);
                *l = l.minus(part);
                let lines: Vec<PostLine> =
                    tax_lines(&part).iter().map(PostLine::mirrored).collect();
                let entry_id = if lines.is_empty() {
                    None
                } else {
                    Some(
                        post(
                            &mut uow,
                            ctx,
                            Posting {
                                org_id: &org.id,
                                entry_date: &date,
                                source_type: "advance_adjustment",
                                source_id: Some(&v.id),
                                narration: Some(format!(
                                    "Tax on advance {} adjusted against {}",
                                    v.number,
                                    doc.number.clone().unwrap_or_default()
                                )),
                                reverses_entry_id: None,
                                idempotency_key: Some(format!("advance:adjust:{allocation_id}")),
                                lines,
                            },
                            &now,
                        )
                        .await?,
                    )
                };
                if let Some(entry_id) = entry_id {
                    uow.insert_advance_adjustment(&adjustment_record(
                        &org.id,
                        &v.id,
                        "invoice",
                        Some(allocation_id),
                        None,
                        &date,
                        &part,
                        &entry_id,
                        &now,
                    ))
                    .await?;
                }
            }
        }
        let after = serde_json::json!({
            "voucher": v.number,
            "date": date,
            "allocations": chosen
                .iter()
                .map(|(d, a)| serde_json::json!({"document": d.number, "amountPaise": a}))
                .collect::<Vec<_>>(),
        });
        audit(
            &mut uow,
            ctx,
            &org.id,
            Change {
                action: "money_voucher.adjusted",
                entity_id: Some(&v.id),
                before: None,
                after: Some(after.to_string()),
            },
            &now,
        )
        .await?;
        let view = self.load_voucher(&mut uow, &org.id, &v.id).await?;
        uow.commit().await?;
        Ok(view)
    }

    /// Tax on advances as GSTR-1 table 11A/11B movements, for every GSTIN.
    pub(crate) async fn load_advance_events(
        &self,
        uow: &mut S::Uow,
        org_id: &str,
    ) -> AppResult<Vec<AdvanceEvent>> {
        let taxes = uow.list_advance_taxes(org_id).await?;
        if taxes.is_empty() {
            return Ok(Vec::new());
        }
        let vouchers: HashMap<String, MoneyVoucherRecord> = uow
            .list_vouchers(org_id)
            .await?
            .into_iter()
            .map(|v| (v.id.clone(), v))
            .collect();
        let by_voucher: HashMap<&str, &AdvanceTaxRecord> =
            taxes.iter().map(|t| (t.voucher_id.as_str(), t)).collect();
        let mut out = Vec::new();
        for t in &taxes {
            let Some(v) = vouchers.get(&t.voucher_id).filter(|v| v.status == "posted") else {
                continue;
            };
            out.push(AdvanceEvent {
                registration_id: t.gst_registration_id.clone(),
                date: v.voucher_date.clone(),
                received: true,
                place_of_supply: t.place_of_supply,
                rate_bp: u32::try_from(t.rate_bp).unwrap_or(0),
                parts: Parts::of_tax(t),
                voucher_id: v.id.clone(),
                number: v.number.clone(),
            });
        }
        for a in uow.list_advance_adjustments(org_id).await? {
            if a.status != "posted" {
                continue;
            }
            let (Some(t), Some(v)) = (
                by_voucher.get(a.voucher_id.as_str()),
                vouchers.get(&a.voucher_id),
            ) else {
                continue;
            };
            out.push(AdvanceEvent {
                registration_id: t.gst_registration_id.clone(),
                date: a.adjust_date.clone(),
                received: false,
                place_of_supply: t.place_of_supply,
                rate_bp: u32::try_from(t.rate_bp).unwrap_or(0),
                parts: Parts::of_adjustment(&a),
                voucher_id: v.id.clone(),
                number: v.number.clone(),
            });
        }
        Ok(out)
    }

    /// Party record check for refunds: a refund goes back to a customer
    /// (payment) or comes back from a supplier (receipt).
    pub(crate) async fn refund_party_ok(
        &self,
        uow: &mut S::Uow,
        org_id: &str,
        party_id: &str,
        voucher_type: &str,
    ) -> AppResult<bool> {
        let Some(p) = uow.get_party(org_id, party_id).await? else {
            return Ok(false);
        };
        Ok(match voucher_type {
            "payment" => p.kind != "supplier",
            _ => p.kind != "customer",
        })
    }
}

/// Sum of parts, for tests and totals.
#[allow(dead_code)]
pub(crate) fn total(parts: &[Parts]) -> Parts {
    parts.iter().fold(Parts::default(), |t, p| t.plus(*p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjustments_use_up_the_tax_exactly() {
        let all = parts_in(100_100, 1800, SupplyKind::IntraState);
        assert_eq!(all.gross, all.taxable + all.tax());
        let mut left = all;
        let mut used = Vec::new();
        for gross in [33_333, 33_333, 40_000] {
            let p = take(left, gross, 1800, SupplyKind::IntraState);
            left = left.minus(p);
            used.push(p);
        }
        assert_eq!(left, Parts::default(), "the last adjustment takes the rest");
        assert_eq!(total(&used), all);
        assert!(used.iter().all(|p| p.gross == p.taxable + p.tax()));
    }

    #[test]
    fn refunds_settle_the_other_side() {
        assert_eq!(party_role("receipt", "settlement"), "receivables");
        assert_eq!(party_role("payment", "settlement"), "payables");
        assert_eq!(party_role("payment", "refund"), "receivables");
        assert_eq!(party_role("receipt", "refund"), "payables");
    }
}
