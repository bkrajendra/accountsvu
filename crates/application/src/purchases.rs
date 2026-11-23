//! Phase 1.4: purchases — supplier bills, debit notes and expenses — with a
//! reviewable input tax credit (ITC) status on every taxed line, and file
//! attachments.
//!
//! Lifecycle: draft → recorded → (cancelled). Recording validates, freezes a
//! tax snapshot and posts one balanced entry. ITC is never inferred from a
//! GSTIN or a tax amount: each line is `pending`, `eligible`, `ineligible` or
//! `partial`, and changing it later posts an adjusting entry (spec §5.3).

use std::collections::HashMap;

use av_domain::masters::validate_hsn_sac;
use av_domain::{
    DocumentType, FinancialYear, ItemKind, Money, PartyGstType, PartyKind, SeriesFormat, StateCode,
    TaxCategory, new_id,
};
use av_gst_engine::checks::{LineFacts, TaxWarning, line_warnings};
use av_gst_engine::invoice::{LineInput, Supply, compute_document, line_amount};
use av_gst_engine::{RulePack, SupplyKind, TaxRate, document_totals};
use av_persistence::records::{
    AccountRecord, AttachmentRecord, OrganisationRecord, PurchaseDocumentRecord, PurchaseLineRecord,
};
use av_persistence::{
    AccountRepo, AttachmentRepo, ItemRepo, PartyRepo, PurchaseRepo, RegistrationRepo, SeriesRepo,
    Store, UnitOfWork,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::AppCore;
use crate::context::{Permission, RequestContext};
use crate::error::{AppError, AppResult, Errors};
use crate::masters::{parse_amount, parse_rate};
use crate::posting::{PostLine, Posting, post};
use crate::sales::{LineCalc, TotalsCalc, capital, kind_str, parse_quantity, to_paise};
use crate::setup::{Change, audit, check_len, clean, json};
use crate::tax_override::{self, TaxOverrideInput};

const MAX_ATTACHMENT_BYTES: usize = 5 * 1024 * 1024;

/// Accounts a purchase line may be booked to: expenses and non-system assets
/// plus inventory and fixed assets. Tax, cash/bank and party control
/// accounts are excluded.
fn bookable(a: &AccountRecord) -> bool {
    a.is_active
        && match a.kind.as_str() {
            // Round-off is posted only by the system.
            "expense" => a.system_role.as_deref() != Some("round_off"),
            "asset" => matches!(
                a.system_role.as_deref(),
                None | Some("inventory" | "fixed_assets")
            ),
            _ => false,
        }
}

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseLineInput {
    #[serde(default)]
    pub item_id: Option<String>,
    /// The expense or asset account the line is booked to.
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub hsn_sac: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    pub quantity: String,
    pub unit_price: String,
    #[serde(default)]
    pub discount: Option<String>,
    pub tax_category: String,
    #[serde(default)]
    pub gst_rate: Option<String>,
    #[serde(default)]
    pub cess_rate: Option<String>,
    /// `pending` (default), `eligible`, `ineligible` or `partial`.
    #[serde(default)]
    pub itc_status: Option<String>,
    #[serde(default)]
    pub itc_reason: Option<String>,
    /// Partial ITC: the tax amount (rupees) that is claimable.
    #[serde(default)]
    pub itc_eligible: Option<String>,
    /// Common credit (rule 42): used for both taxable and exempt supplies.
    #[serde(default)]
    pub itc_common: Option<bool>,
    /// Tax entered by hand, e.g. as on the supplier's invoice.
    #[serde(default)]
    pub tax_override: Option<TaxOverrideInput>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseDocumentInput {
    /// `bill`, `debit_note` or `expense`.
    pub doc_type: String,
    #[serde(default)]
    pub party_id: Option<String>,
    #[serde(default)]
    pub supplier_invoice_number: Option<String>,
    #[serde(default)]
    pub supplier_invoice_date: Option<String>,
    /// Date recorded / goods received.
    pub doc_date: String,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub gst_registration_id: Option<String>,
    /// `credit` (default), `cash` or `bank`.
    #[serde(default)]
    pub payment_mode: Option<String>,
    #[serde(default)]
    pub reverse_charge: bool,
    #[serde(default)]
    pub prices_include_tax: bool,
    #[serde(default)]
    pub original_document_id: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub lines: Vec<PurchaseLineInput>,
    /// Why any line's tax was entered by hand.
    #[serde(default)]
    pub tax_override_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseStatus {
    Draft,
    Unpaid,
    Overdue,
    PartPaid,
    Paid,
    /// Debit notes once recorded.
    Recorded,
    Cancelled,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseSummary {
    #[serde(flatten)]
    pub document: PurchaseDocumentRecord,
    pub display_status: PurchaseStatus,
    /// Lines whose ITC still needs a decision.
    pub itc_pending_lines: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseRef {
    pub id: String,
    pub doc_type: String,
    pub number: Option<String>,
    pub supplier_invoice_number: Option<String>,
    pub doc_date: String,
    pub status: String,
    pub total_paise: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseDocumentView {
    #[serde(flatten)]
    pub document: PurchaseDocumentRecord,
    pub display_status: PurchaseStatus,
    pub lines: Vec<PurchaseLineRecord>,
    pub original: Option<PurchaseRef>,
    pub debit_notes: Vec<PurchaseRef>,
    pub debited_paise: i64,
    pub attachments: Vec<AttachmentRecord>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseCalculation {
    pub supply_kind: Option<SupplyKind>,
    pub lines: Vec<Option<LineCalc>>,
    pub totals: TotalsCalc,
    /// Tax the business pays itself under reverse charge.
    pub reverse_charge_tax_paise: i64,
    pub number_preview: Option<String>,
    /// Rate checks against the rule pack; never blocking.
    pub warnings: Vec<TaxWarning>,
    pub rule_pack_version: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentContent {
    pub file_name: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItcReviewInput {
    pub status: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub eligible: Option<String>,
    /// Common credit (rule 42); only kept for eligible or partial ITC.
    #[serde(default)]
    pub common: Option<bool>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn tax_of(l: &PurchaseLineRecord) -> i64 {
    l.cgst_paise + l.sgst_paise + l.utgst_paise + l.igst_paise + l.cess_paise
}

/// Where a line's tax goes, by ITC status. Partial ITC is split across the
/// tax components in proportion; the rest becomes part of the cost.
fn itc_allocation(l: &PurchaseLineRecord) -> Vec<PostLine> {
    let tax = tax_of(l);
    if tax == 0 {
        return Vec::new();
    }
    let comps = [
        ("input_cgst", l.cgst_paise),
        ("input_sgst", l.sgst_paise),
        ("input_utgst", l.utgst_paise),
        ("input_igst", l.igst_paise),
        ("input_cess", l.cess_paise),
    ];
    match l.itc_status.as_str() {
        "eligible" => comps
            .iter()
            .map(|(r, p)| PostLine::debit(r, *p, None))
            .collect(),
        "pending" => vec![PostLine::debit("itc_pending", tax, None)],
        "partial" => {
            let eligible = l.itc_eligible_paise.clamp(0, tax);
            let mut out = Vec::new();
            let mut given = 0;
            let last = comps.iter().rposition(|(_, p)| *p > 0).unwrap_or(0);
            for (i, (role, p)) in comps.iter().enumerate() {
                let share = if i == last {
                    eligible - given
                } else {
                    p * eligible / tax
                };
                given += share;
                out.push(PostLine::debit(role, share, None));
            }
            out.push(PostLine::debit_account(&l.account_id, tax - eligible));
            out
        }
        _ => vec![PostLine::debit_account(&l.account_id, tax)],
    }
}

fn signed(doc: &PurchaseDocumentRecord, lines: Vec<PostLine>) -> Vec<PostLine> {
    if doc.doc_type == "debit_note" {
        lines.iter().map(PostLine::mirrored).collect()
    } else {
        lines
    }
}

/// The ledger effect of a purchase: Dr cost/asset + allocated tax = Cr
/// supplier (or cash/bank) [+ Cr reverse-charge tax] ± round-off.
fn purchase_post_lines(d: &PurchaseDocumentRecord, lines: &[PurchaseLineRecord]) -> Vec<PostLine> {
    let mut out = Vec::new();
    for l in lines {
        out.push(PostLine::debit_account(&l.account_id, l.taxable_paise));
        out.extend(itc_allocation(l));
    }
    let party = d.party_id.as_deref();
    out.push(match d.payment_mode.as_str() {
        "cash" => PostLine::credit("cash", d.total_paise, None),
        "bank" => PostLine::credit("bank", d.total_paise, None),
        _ => PostLine::credit("payables", d.total_paise, party),
    });
    if d.reverse_charge {
        let tax = d.cgst_paise + d.sgst_paise + d.utgst_paise + d.igst_paise + d.cess_paise;
        out.push(PostLine::credit("rcm_payable", tax, None));
    }
    match d.round_off_paise {
        r if r > 0 => out.push(PostLine::debit("round_off", r, None)),
        r if r < 0 => out.push(PostLine::credit("round_off", -r, None)),
        _ => {}
    }
    signed(d, out)
}

/// `debited_paise`: recorded debit notes against the bill, which reduce what
/// is still owed just as payments do.
pub(crate) fn purchase_status(
    d: &PurchaseDocumentRecord,
    debited_paise: i64,
    today: NaiveDate,
) -> PurchaseStatus {
    let settled = d.paid_paise + debited_paise >= d.total_paise;
    match d.status.as_str() {
        "draft" => PurchaseStatus::Draft,
        "cancelled" => PurchaseStatus::Cancelled,
        _ if d.doc_type == "debit_note" => PurchaseStatus::Recorded,
        _ if settled && d.paid_paise > 0 => PurchaseStatus::Paid,
        _ if settled => PurchaseStatus::Recorded,
        _ if d.paid_paise > 0 => PurchaseStatus::PartPaid,
        _ if d
            .due_date
            .as_deref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .is_some_and(|due| due < today) =>
        {
            PurchaseStatus::Overdue
        }
        _ => PurchaseStatus::Unpaid,
    }
}

fn pref(d: &PurchaseDocumentRecord) -> PurchaseRef {
    PurchaseRef {
        id: d.id.clone(),
        doc_type: d.doc_type.clone(),
        number: d.number.clone(),
        supplier_invoice_number: d.supplier_invoice_number.clone(),
        doc_date: d.doc_date.clone(),
        status: d.status.clone(),
        total_paise: d.total_paise,
    }
}

fn parse_date(e: &mut Errors, field: &str, s: &str) -> Option<NaiveDate> {
    match NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d") {
        Ok(d) => Some(d),
        Err(_) => {
            e.add(field, "Enter a valid date.");
            None
        }
    }
}

pub(crate) fn content_type_for(file_name: &str) -> Option<&'static str> {
    let lower = file_name.to_ascii_lowercase();
    let ext = lower.rsplit('.').next()?;
    Some(match ext {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "csv" => "text/csv",
        _ => return None,
    })
}

/// Lowercase hex SHA-256 of a file.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

struct ItcDecision {
    status: &'static str,
    reason: Option<String>,
    eligible_paise: i64,
}

/// Validate an ITC decision for a line with `tax` paise of GST.
fn decide_itc(
    e: &mut Errors,
    prefix: &str,
    status: Option<&str>,
    reason: Option<String>,
    eligible: &Option<String>,
    tax: i64,
    claimable: Result<(), &'static str>,
) -> ItcDecision {
    if tax == 0 {
        return ItcDecision {
            status: "na",
            reason: None,
            eligible_paise: 0,
        };
    }
    let status = match status
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("pending")
    {
        "pending" => "pending",
        "eligible" => "eligible",
        "ineligible" => "ineligible",
        "partial" => "partial",
        _ => {
            e.add(&format!("{prefix}itcStatus"), "Choose the ITC status.");
            "pending"
        }
    };
    if let (Err(why), "eligible" | "partial") = (claimable, status) {
        e.add(&format!("{prefix}itcStatus"), why);
    }
    let reason = reason.and_then(|r| clean(&Some(r)));
    if matches!(status, "ineligible" | "partial") && reason.is_none() {
        e.add(
            &format!("{prefix}itcReason"),
            "Say why, e.g. blocked credit under section 17(5).",
        );
    }
    check_len(e, &format!("{prefix}itcReason"), &reason, 300);
    let mut eligible_paise = 0;
    match status {
        "eligible" => eligible_paise = tax,
        "partial" => match parse_amount(e, &format!("{prefix}itcEligible"), eligible, false) {
            Some(p) if p > 0 && p < tax => eligible_paise = p,
            Some(_) => e.add(
                &format!("{prefix}itcEligible"),
                format!(
                    "Enter an amount between ₹0 and the line's GST of {}.",
                    Money::from_paise(tax)
                ),
            ),
            None => e.add(
                &format!("{prefix}itcEligible"),
                "Enter the claimable amount.",
            ),
        },
        _ => {}
    }
    ItcDecision {
        status,
        reason,
        eligible_paise,
    }
}

struct Prepared {
    doc: PurchaseDocumentRecord,
    lines: Vec<PurchaseLineRecord>,
    reverse_charge_tax: i64,
    warnings: Vec<TaxWarning>,
}

impl<S: Store> AppCore<S> {
    /// Validate a purchase and compute its tax. Returns field errors and, when
    /// valid, the records ready to save.
    async fn prepare_purchase(
        &self,
        uow: &mut S::Uow,
        org: &OrganisationRecord,
        input: &PurchaseDocumentInput,
        self_id: Option<&str>,
        ctx: Option<&RequestContext>,
    ) -> AppResult<(Errors, Option<Prepared>)> {
        let mut e = Errors::new("");
        let today = self.today();
        let regs: Vec<_> = uow
            .list_registrations(&org.id)
            .await?
            .into_iter()
            .filter(|r| r.status == "active")
            .collect();
        let accounts: HashMap<String, AccountRecord> = uow
            .list_accounts(&org.id)
            .await?
            .into_iter()
            .map(|a| (a.id.clone(), a))
            .collect();

        let doc_type = match input.doc_type.trim() {
            t @ ("bill" | "debit_note" | "expense") => Some(t),
            _ => {
                e.add("docType", "Choose bill, debit note or expense.");
                None
            }
        };
        let is_dn = doc_type == Some("debit_note");

        // Debit note: the original bill decides the supplier and tax setup.
        let mut original = None;
        if is_dn {
            if regs.is_empty() {
                e.add("docType", "Debit notes need a GST registration.");
            }
            match clean(&input.original_document_id) {
                None => e.add(
                    "originalDocumentId",
                    "Choose the bill this debit note is for.",
                ),
                Some(id) => match uow.get_purchase(&org.id, &id).await? {
                    Some(o)
                        if o.status == "recorded"
                            && o.doc_type == "bill"
                            && o.party_id.is_some() =>
                    {
                        original = Some(o)
                    }
                    Some(_) => e.add(
                        "originalDocumentId",
                        "Debit notes can only be raised against a recorded supplier bill.",
                    ),
                    None => e.add("originalDocumentId", "That bill wasn't found."),
                },
            }
            let reason = clean(&input.reason);
            if reason.is_none() {
                e.add("reason", "Say why, e.g. goods returned to the supplier.");
            }
            check_len(&mut e, "reason", &reason, 300);
        }

        let registration = if let Some(o) = &original {
            regs.iter()
                .find(|r| Some(&r.id) == o.gst_registration_id.as_ref())
                .cloned()
        } else if let Some(id) = clean(&input.gst_registration_id) {
            let r = regs.iter().find(|r| r.id == id).cloned();
            if r.is_none() {
                e.add("gstRegistrationId", "Choose an active GSTIN.");
            }
            r
        } else {
            regs.first().cloned()
        };

        // Payment mode.
        let payment_mode = if is_dn {
            "credit"
        } else {
            match clean(&input.payment_mode).as_deref().unwrap_or("credit") {
                "credit" => "credit",
                "cash" => "cash",
                "bank" => "bank",
                _ => {
                    e.add("paymentMode", "Choose on credit, cash or bank.");
                    "credit"
                }
            }
        };

        // Supplier.
        let party_id = original
            .as_ref()
            .and_then(|o| o.party_id.clone())
            .or_else(|| clean(&input.party_id));
        let party = match &party_id {
            Some(id) => {
                let p = uow.get_party(&org.id, id).await?;
                match &p {
                    None => e.add("partyId", "That supplier wasn't found."),
                    Some(p) if !p.is_active && original.is_none() => {
                        e.add("partyId", "This party is inactive.")
                    }
                    Some(p) if PartyKind::parse(&p.kind) == Some(PartyKind::Customer) => {
                        e.add("partyId", "This party is set up as a customer only. Change it to Both to buy from them.");
                    }
                    _ => {}
                }
                p
            }
            None => {
                if doc_type == Some("bill") || payment_mode == "credit" {
                    e.add("partyId", "Choose the supplier.");
                }
                None
            }
        };

        // Supplier's invoice reference.
        let supplier_invoice_number =
            clean(&input.supplier_invoice_number).map(|s| s.to_uppercase());
        check_len(
            &mut e,
            "supplierInvoiceNumber",
            &supplier_invoice_number,
            30,
        );
        let supplier_invoice_date = clean(&input.supplier_invoice_date)
            .and_then(|s| parse_date(&mut e, "supplierInvoiceDate", &s));
        if doc_type == Some("bill") {
            if supplier_invoice_number.is_none() {
                e.add(
                    "supplierInvoiceNumber",
                    "Enter the supplier's invoice number.",
                );
            }
            if supplier_invoice_date.is_none() && clean(&input.supplier_invoice_date).is_none() {
                e.add("supplierInvoiceDate", "Enter the supplier's invoice date.");
            }
        }

        // Dates.
        let doc_date = parse_date(&mut e, "docDate", &input.doc_date);
        if let Some(d) = doc_date {
            if let Ok(start) = NaiveDate::parse_from_str(&org.books_start_date, "%Y-%m-%d")
                && d < start
            {
                e.add(
                    "docDate",
                    format!(
                        "Can't be before your books start ({}).",
                        start.format("%d %b %Y")
                    ),
                );
            }
            if d > today {
                e.add("docDate", "Can't be a future date.");
            }
            if supplier_invoice_date.is_some_and(|s| s > d) {
                e.add(
                    "supplierInvoiceDate",
                    "The supplier's invoice can't be dated after you received it.",
                );
            }
            if let Some(o) = &original
                && NaiveDate::parse_from_str(&o.doc_date, "%Y-%m-%d").is_ok_and(|od| d < od)
            {
                e.add("docDate", "A debit note can't be dated before its bill.");
            }
        }
        let fy_of = |d: NaiveDate| {
            FinancialYear::containing(d, u32::try_from(org.fy_start_month).unwrap_or(4))
        };
        let financial_year = supplier_invoice_date.or(doc_date).map(|d| fy_of(d).code());
        if let (Some(p), Some(no), Some(fy), Some("bill" | "expense")) =
            (&party, &supplier_invoice_number, &financial_year, doc_type)
            && let Some(dup) = uow
                .find_supplier_invoice(&org.id, &p.id, no, fy, self_id)
                .await?
        {
            e.add(
                "supplierInvoiceNumber",
                format!(
                    "This invoice from {} is already recorded (on {}).",
                    p.name, dup.doc_date
                ),
            );
        }
        let due_date = if payment_mode != "credit" || is_dn {
            None
        } else {
            match clean(&input.due_date) {
                Some(s) => parse_date(&mut e, "dueDate", &s),
                None => supplier_invoice_date.or(doc_date).map(|d| {
                    let days = party
                        .as_ref()
                        .and_then(|p| p.payment_terms_days)
                        .unwrap_or(0);
                    d + chrono::Days::new(u64::try_from(days).unwrap_or(0))
                }),
            }
        };

        // Tax setup: our state is the place of supply for what we receive.
        let our_state = registration
            .as_ref()
            .map_or(org.state_code, |r| r.state_code);
        let supplier_state = party.as_ref().map_or(our_state, |p| p.state_code);
        let supply_kind = (|| {
            Some(SupplyKind::determine(
                StateCode::new(u8::try_from(supplier_state).ok()?).ok()?,
                StateCode::new(u8::try_from(our_state).ok()?).ok()?,
            ))
        })();
        let reverse_charge = original
            .as_ref()
            .map_or(input.reverse_charge, |o| o.reverse_charge);
        let prices_include_tax = original
            .as_ref()
            .map_or(input.prices_include_tax, |o| o.prices_include_tax);
        if reverse_charge && registration.is_none() {
            e.add(
                "reverseCharge",
                "Reverse charge applies to GST-registered businesses.",
            );
        }
        // Whether ITC can be claimed on this document at all.
        let supplier_has_gstin = party.as_ref().is_some_and(|p| {
            p.gstin.is_some()
                && matches!(
                    PartyGstType::parse(&p.gst_type),
                    Some(PartyGstType::Regular | PartyGstType::Sez)
                )
        });
        let claimable: Result<(), &'static str> = if registration.is_none() {
            Err("Your business isn't GST-registered, so input tax can't be claimed.")
        } else if !supplier_has_gstin && !reverse_charge {
            Err("Input tax can only be claimed on a registered supplier's tax invoice.")
        } else {
            Ok(())
        };

        // Lines.
        if input.lines.is_empty() {
            e.add("lines", "Add at least one line.");
        }
        let mut engine_lines = Vec::new();
        let mut records = Vec::new();
        let mut decisions = Vec::new();
        for (i, l) in input.lines.iter().enumerate() {
            let f = |n: &str| format!("lines.{i}.{n}");
            let mut le = Errors::new("");
            let item = match clean(&l.item_id) {
                Some(id) => {
                    let it = uow.get_item(&org.id, &id).await?;
                    if it.is_none() {
                        le.add(&f("itemId"), "That item wasn't found.");
                    }
                    it
                }
                None => None,
            };
            let description = clean(&Some(l.description.clone()))
                .or_else(|| item.as_ref().map(|i| i.name.clone()));
            if description.is_none() {
                le.add(&f("description"), "Describe the line or pick an item.");
            }
            check_len(&mut le, &f("description"), &description, 300);
            match accounts.get(l.account_id.trim()) {
                Some(a) if bookable(a) => {}
                Some(_) => le.add(
                    &f("accountId"),
                    "Book purchases to an expense or asset account.",
                ),
                None => le.add(&f("accountId"), "Choose what this is for."),
            }
            let category = TaxCategory::parse(l.tax_category.trim());
            if category.is_none() {
                le.add(&f("taxCategory"), "Choose how GST applies.");
            }
            let rate = match parse_rate(&mut le, &f("gstRate"), &l.gst_rate) {
                Some(bp) if category == Some(TaxCategory::Taxable) && TaxRate(bp).is_known() => bp,
                Some(bp) if category == Some(TaxCategory::Taxable) => {
                    let _ = bp;
                    le.add(&f("gstRate"), "Choose a GST rate.");
                    0
                }
                Some(bp) if bp > 0 => {
                    le.add(
                        &f("gstRate"),
                        "Exempt, nil-rated and non-GST lines carry no GST.",
                    );
                    0
                }
                None if category == Some(TaxCategory::Taxable) => {
                    le.add(&f("gstRate"), "Choose a GST rate.");
                    0
                }
                _ => 0,
            };
            let cess = parse_rate(&mut le, &f("cessRate"), &l.cess_rate).unwrap_or(0);
            let hsn = clean(&l.hsn_sac).or_else(|| item.as_ref().and_then(|i| i.hsn_sac.clone()));
            if let Some(h) = &hsn {
                let kind = item
                    .as_ref()
                    .and_then(|i| ItemKind::parse(&i.kind))
                    .unwrap_or(if h.starts_with("99") {
                        ItemKind::Service
                    } else {
                        ItemKind::Goods
                    });
                if let Err(err) = validate_hsn_sac(h, kind) {
                    le.add(&f("hsnSac"), format!("{err}."));
                }
            }
            let quantity = parse_quantity(&mut le, &f("quantity"), &l.quantity);
            if clean(&Some(l.unit_price.clone())).is_none() {
                le.add(&f("unitPrice"), "Enter a price.");
            }
            let price = parse_amount(&mut le, &f("unitPrice"), &Some(l.unit_price.clone()), false);
            let discount = parse_amount(&mut le, &f("discount"), &l.discount, false).unwrap_or(0);
            if let (Some(q), Some(p), Some(c)) = (quantity, price, category) {
                let li = LineInput {
                    quantity: q,
                    unit_price: Money::from_paise(p),
                    discount: Money::from_paise(discount),
                    category: c,
                    rate: TaxRate(rate),
                    cess: TaxRate(cess),
                };
                if line_amount(&li).is_err() {
                    le.add(
                        &f("discount"),
                        "The discount can't be more than the line amount.",
                    );
                } else if le.is_empty() {
                    engine_lines.push(li);
                    decisions.push((
                        i,
                        l.itc_status.clone(),
                        l.itc_reason.clone(),
                        l.itc_eligible.clone(),
                    ));
                    records.push(PurchaseLineRecord {
                        id: new_id().to_string(),
                        document_id: String::new(),
                        line_no: i64::try_from(i + 1).unwrap_or(0),
                        item_id: item.as_ref().map(|i| i.id.clone()),
                        account_id: l.account_id.trim().to_owned(),
                        description: description.clone().unwrap_or_default(),
                        hsn_sac: hsn.clone(),
                        unit: clean(&l.unit).or_else(|| item.as_ref().map(|i| i.unit.clone())),
                        quantity: q.to_string(),
                        unit_price_paise: p,
                        discount_paise: discount,
                        tax_category: c.as_str().into(),
                        gst_rate_bp: i64::from(rate),
                        cess_rate_bp: i64::from(cess),
                        amount_paise: 0,
                        taxable_paise: 0,
                        cgst_paise: 0,
                        sgst_paise: 0,
                        utgst_paise: 0,
                        igst_paise: 0,
                        cess_paise: 0,
                        total_paise: 0,
                        itc_status: "na".into(),
                        itc_reason: None,
                        itc_eligible_paise: 0,
                        itc_reviewed_at: None,
                        itc_reviewed_by: None,
                        itc_common: l.itc_common.unwrap_or(false),
                        itc_period: None,
                        computed_tax_paise: None,
                    });
                }
            }
            e.absorb(le);
        }
        let notes = clean(&input.notes);
        check_len(&mut e, "notes", &notes, 1000);

        let (Some(t), Some(d), Some(kind)) = (doc_type, doc_date, supply_kind) else {
            return Ok((e, None));
        };
        if !e.is_empty() {
            return Ok((e, None));
        }
        // An unregistered business is charged GST but can't charge or claim it;
        // a supplier without a GSTIN can't charge it at all (outside reverse charge).
        let supply = Supply {
            kind,
            zero_rated: false,
            prices_include_tax,
            tax_free_document: false,
        };
        let computed = compute_document(&engine_lines, supply, false).map_err(|errs| {
            AppError::Invalid(format!("line {} could not be computed", errs[0].0 + 1))
        })?;
        let now = self.now();
        let mut oe = Errors::new("");
        let taxes: Vec<_> = computed
            .lines
            .iter()
            .zip(&decisions)
            .map(|(c, (i, ..))| {
                let entered = input.lines.get(*i).and_then(|l| l.tax_override.as_ref());
                tax_override::apply(&mut oe, *i, entered, c.tax, kind)
            })
            .collect();
        for (((rec, c), (i, status, reason, eligible)), (tax, calculated)) in records
            .iter_mut()
            .zip(&computed.lines)
            .zip(&decisions)
            .zip(&taxes)
        {
            rec.amount_paise = to_paise(c.amount);
            rec.taxable_paise = to_paise(tax.taxable_value);
            rec.cgst_paise = to_paise(tax.cgst);
            rec.sgst_paise = to_paise(tax.sgst);
            rec.utgst_paise = to_paise(tax.utgst);
            rec.igst_paise = to_paise(tax.igst);
            rec.cess_paise = to_paise(tax.cess);
            rec.total_paise = to_paise(tax.line_total());
            rec.computed_tax_paise = *calculated;
            let dec = decide_itc(
                &mut e,
                &format!("lines.{i}."),
                status.as_deref(),
                reason.clone(),
                eligible,
                tax_of(rec),
                claimable,
            );
            rec.itc_status = dec.status.into();
            rec.itc_reason = dec.reason;
            rec.itc_eligible_paise = dec.eligible_paise;
            rec.itc_common &= matches!(dec.status, "eligible" | "partial");
            if dec.status != "na" && dec.status != "pending" {
                rec.itc_reviewed_at = Some(now.clone());
            }
        }
        let tt = document_totals(taxes.iter().map(|(t, _)| t), false);
        let changed = records.iter().any(|l| l.computed_tax_paise.is_some());
        let (override_reason, override_by, override_at) = match ctx {
            Some(ctx) => {
                e.absorb(oe);
                tax_override::stamp(&mut e, ctx, changed, &input.tax_override_reason, &now)
            }
            None => (None, None, None),
        };
        let tax_total = to_paise(tt.cgst + tt.sgst + tt.utgst + tt.igst + tt.cess);
        let taxable = to_paise(tt.taxable_value);
        let base = if reverse_charge {
            taxable
        } else {
            taxable + tax_total
        };
        let total = if org.round_to_rupee {
            to_paise(Money::from_paise(base).round_rupee())
        } else {
            base
        };
        if total <= 0 {
            e.add("lines", "The total must be more than zero.");
        }
        if let Some(o) = &original {
            let already: i64 = uow
                .debit_notes_for(&org.id, &o.id)
                .await?
                .iter()
                .filter(|c| c.status == "recorded" && Some(c.id.as_str()) != self_id)
                .map(|c| c.total_paise)
                .sum();
            let left = o.total_paise - already;
            if total > left {
                e.add(
                    "lines",
                    format!(
                        "Debit notes can't exceed the bill. {} is left.",
                        Money::from_paise(left)
                    ),
                );
            }
        }
        let doc = PurchaseDocumentRecord {
            id: String::new(),
            organisation_id: org.id.clone(),
            doc_type: t.into(),
            status: "draft".into(),
            gst_registration_id: registration.as_ref().map(|r| r.id.clone()),
            series_id: None,
            number: None,
            financial_year,
            supplier_invoice_number,
            supplier_invoice_date: supplier_invoice_date.map(|d| d.format("%Y-%m-%d").to_string()),
            doc_date: d.format("%Y-%m-%d").to_string(),
            due_date: due_date.map(|d| d.format("%Y-%m-%d").to_string()),
            party_id: party.as_ref().map(|p| p.id.clone()),
            party_name: party.as_ref().map(|p| p.name.clone()),
            party_gstin: party.as_ref().and_then(|p| p.gstin.clone()),
            party_gst_type: party.as_ref().map(|p| p.gst_type.clone()),
            place_of_supply: our_state,
            supplier_state,
            supply_kind: kind_str(kind).into(),
            reverse_charge,
            prices_include_tax,
            payment_mode: payment_mode.into(),
            original_document_id: original.as_ref().map(|o| o.id.clone()),
            reason: if is_dn { clean(&input.reason) } else { None },
            notes,
            taxable_paise: taxable,
            cgst_paise: to_paise(tt.cgst),
            sgst_paise: to_paise(tt.sgst),
            utgst_paise: to_paise(tt.utgst),
            igst_paise: to_paise(tt.igst),
            cess_paise: to_paise(tt.cess),
            round_off_paise: total - base,
            total_paise: total,
            paid_paise: 0,
            journal_entry_id: None,
            cancel_entry_id: None,
            cancel_reason: None,
            row_version: 1,
            created_at: now.clone(),
            updated_at: now,
            recorded_at: None,
            cancelled_at: None,
            rule_pack_version: Some(RulePack::bundled().version.clone()),
            tax_override_reason: override_reason,
            tax_override_by: override_by,
            tax_override_at: override_at,
        };
        // Rates are checked on the supplier's invoice date when known.
        let rate_date = doc
            .supplier_invoice_date
            .as_deref()
            .unwrap_or(&doc.doc_date);
        let warnings = match NaiveDate::parse_from_str(rate_date, "%Y-%m-%d") {
            Ok(date) => records
                .iter()
                .enumerate()
                .filter_map(|(i, l)| {
                    let category = TaxCategory::parse(&l.tax_category)?;
                    Some(line_warnings(
                        RulePack::bundled(),
                        date,
                        i,
                        LineFacts {
                            hsn: l.hsn_sac.as_deref(),
                            category,
                            rate_bp: u32::try_from(l.gst_rate_bp).unwrap_or(0),
                        },
                    ))
                })
                .flatten()
                .collect(),
            Err(_) => Vec::new(),
        };
        let ok = e.is_empty();
        let reverse_charge_tax = if reverse_charge { tax_total } else { 0 };
        Ok((
            e,
            ok.then_some(Prepared {
                doc,
                lines: records,
                reverse_charge_tax,
                warnings,
            }),
        ))
    }

    async fn purchase_number_preview(
        &self,
        uow: &mut S::Uow,
        org: &OrganisationRecord,
        d: &PurchaseDocumentRecord,
    ) -> AppResult<Option<String>> {
        if d.doc_type != "debit_note" {
            return Ok(None);
        }
        let Ok(date) = NaiveDate::parse_from_str(&d.doc_date, "%Y-%m-%d") else {
            return Ok(None);
        };
        let fy = FinancialYear::containing(date, u32::try_from(org.fy_start_month).unwrap_or(4));
        let s = uow
            .default_series(
                &org.id,
                d.gst_registration_id.as_deref(),
                DocumentType::DebitNote.as_str(),
                &fy.code(),
            )
            .await?;
        Ok(s.map(|s| {
            SeriesFormat {
                prefix: s.prefix,
                suffix: s.suffix,
                padding: u8::try_from(s.padding).unwrap_or(4),
            }
            .format(u64::try_from(s.next_number).unwrap_or(1))
        }))
    }

    async fn load_purchase(
        &self,
        uow: &mut S::Uow,
        org_id: &str,
        id: &str,
    ) -> AppResult<PurchaseDocumentView> {
        let d = uow
            .get_purchase(org_id, id)
            .await?
            .ok_or(AppError::NotFound("Purchase"))?;
        let lines = uow.purchase_lines(&d.id).await?;
        let original = match &d.original_document_id {
            Some(oid) => uow.get_purchase(org_id, oid).await?.map(|o| pref(&o)),
            None => None,
        };
        let notes = uow.debit_notes_for(org_id, &d.id).await?;
        let debited_paise = notes
            .iter()
            .filter(|n| n.status == "recorded")
            .map(|n| n.total_paise)
            .sum();
        let attachments = uow
            .list_attachments(org_id, "purchase_document", &d.id)
            .await?;
        Ok(PurchaseDocumentView {
            display_status: purchase_status(&d, debited_paise, self.today()),
            document: d,
            lines,
            original,
            debit_notes: notes.iter().map(pref).collect(),
            debited_paise,
            attachments,
        })
    }

    // -----------------------------------------------------------------------
    // Use cases
    // -----------------------------------------------------------------------

    /// Accounts a purchase line can be booked to.
    pub async fn purchase_accounts(&self, ctx: &RequestContext) -> AppResult<Vec<AccountRecord>> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let Some(org) = self.find_org(ctx, &mut uow).await? else {
            return Ok(Vec::new());
        };
        Ok(uow
            .list_accounts(&org.id)
            .await?
            .into_iter()
            .filter(bookable)
            .collect())
    }

    pub async fn list_purchases(&self, ctx: &RequestContext) -> AppResult<Vec<PurchaseSummary>> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let Some(org) = self.find_org(ctx, &mut uow).await? else {
            return Ok(Vec::new());
        };
        let today = self.today();
        let docs = uow.list_purchases(&org.id).await?;
        let mut debited: HashMap<String, i64> = HashMap::new();
        for n in docs
            .iter()
            .filter(|n| n.doc_type == "debit_note" && n.status == "recorded")
        {
            if let Some(o) = &n.original_document_id {
                *debited.entry(o.clone()).or_default() += n.total_paise;
            }
        }
        let mut out = Vec::with_capacity(docs.len());
        for d in docs {
            let itc_pending_lines = if d.status == "recorded" {
                i64::try_from(
                    uow.purchase_lines(&d.id)
                        .await?
                        .iter()
                        .filter(|l| l.itc_status == "pending")
                        .count(),
                )
                .unwrap_or(0)
            } else {
                0
            };
            out.push(PurchaseSummary {
                display_status: purchase_status(
                    &d,
                    debited.get(&d.id).copied().unwrap_or(0),
                    today,
                ),
                document: d,
                itc_pending_lines,
            });
        }
        Ok(out)
    }

    pub async fn get_purchase(
        &self,
        ctx: &RequestContext,
        id: &str,
    ) -> AppResult<PurchaseDocumentView> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        self.load_purchase(&mut uow, &org.id, id).await
    }

    /// Live figures for the editor.
    pub async fn calculate_purchase(
        &self,
        ctx: &RequestContext,
        input: PurchaseDocumentInput,
    ) -> AppResult<PurchaseCalculation> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let (_, prepared) = self
            .prepare_purchase(&mut uow, &org, &input, None, None)
            .await?;
        if let Some(p) = prepared {
            let d = &p.doc;
            return Ok(PurchaseCalculation {
                supply_kind: Some(match d.supply_kind.as_str() {
                    "intra_state" => SupplyKind::IntraState,
                    "intra_union_territory" => SupplyKind::IntraUnionTerritory,
                    _ => SupplyKind::InterState,
                }),
                lines: p.lines.iter().map(|l| Some(line_calc(l))).collect(),
                totals: TotalsCalc {
                    taxable_paise: d.taxable_paise,
                    cgst_paise: d.cgst_paise,
                    sgst_paise: d.sgst_paise,
                    utgst_paise: d.utgst_paise,
                    igst_paise: d.igst_paise,
                    cess_paise: d.cess_paise,
                    round_off_paise: d.round_off_paise,
                    total_paise: d.total_paise,
                },
                reverse_charge_tax_paise: p.reverse_charge_tax,
                number_preview: self.purchase_number_preview(&mut uow, &org, d).await?,
                warnings: p.warnings.clone(),
                rule_pack_version: RulePack::bundled().version.clone(),
            });
        }
        // Incomplete: reuse the sales calculator for per-line figures, with
        // the supplier as the "other" state.
        let sales_like = crate::sales::SalesDocumentInput {
            doc_type: "tax_invoice".into(),
            party_id: clean(&input.party_id).unwrap_or_default(),
            doc_date: input.doc_date.clone(),
            prices_include_tax: input.prices_include_tax,
            lines: input
                .lines
                .iter()
                .map(|l| crate::sales::SalesLineInput {
                    quantity: l.quantity.clone(),
                    unit_price: l.unit_price.clone(),
                    discount: l.discount.clone(),
                    tax_category: l.tax_category.clone(),
                    gst_rate: l.gst_rate.clone(),
                    cess_rate: l.cess_rate.clone(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        drop(uow);
        let c = self.calculate_sales(ctx, sales_like).await?;
        Ok(PurchaseCalculation {
            supply_kind: c.supply_kind,
            lines: c.lines,
            totals: c.totals,
            reverse_charge_tax_paise: 0,
            number_preview: None,
            warnings: Vec::new(),
            rule_pack_version: c.rule_pack_version,
        })
    }

    async fn write_purchase_draft(
        &self,
        uow: &mut S::Uow,
        ctx: &RequestContext,
        org_id: &str,
        id: Option<&str>,
        expected_version: Option<i64>,
        mut p: Prepared,
    ) -> AppResult<PurchaseDocumentRecord> {
        let now = self.now();
        match id {
            Some(id) => {
                let before = uow
                    .get_purchase(org_id, id)
                    .await?
                    .ok_or(AppError::NotFound("Purchase"))?;
                if before.status != "draft" {
                    return Err(AppError::Conflict(
                        "This purchase is recorded and can't be edited.".into(),
                    ));
                }
                let version = expected_version.unwrap_or(before.row_version);
                p.doc.id = before.id.clone();
                p.doc.created_at.clone_from(&before.created_at);
                p.doc.updated_at.clone_from(&now);
                if !uow.update_purchase(&p.doc, version).await? {
                    return Err(AppError::stale());
                }
                p.doc.row_version = version + 1;
            }
            None => {
                p.doc.id = new_id().to_string();
                uow.insert_purchase(&p.doc).await?;
                let c = Change {
                    action: "purchase_document.create",
                    entity_id: Some(&p.doc.id),
                    before: None,
                    after: json(&p.doc),
                };
                audit(uow, ctx, org_id, c, &now).await?;
            }
        }
        for l in &mut p.lines {
            l.document_id.clone_from(&p.doc.id);
        }
        uow.replace_purchase_lines(&p.doc.id, &p.lines).await?;
        Ok(p.doc)
    }

    pub async fn save_purchase_draft(
        &self,
        ctx: &RequestContext,
        id: Option<&str>,
        input: PurchaseDocumentInput,
        expected_version: Option<i64>,
    ) -> AppResult<PurchaseDocumentView> {
        ctx.require(Permission::PrepareDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let (e, prepared) = self
            .prepare_purchase(&mut uow, &org, &input, id, Some(ctx))
            .await?;
        e.finish()?;
        let doc = self
            .write_purchase_draft(
                &mut uow,
                ctx,
                &org.id,
                id,
                expected_version,
                prepared.expect("validated"),
            )
            .await?;
        let view = self.load_purchase(&mut uow, &org.id, &doc.id).await?;
        uow.commit().await?;
        Ok(view)
    }

    /// Validate, (number a debit note), freeze and post — one transaction.
    pub async fn record_purchase(
        &self,
        ctx: &RequestContext,
        id: Option<&str>,
        input: PurchaseDocumentInput,
        expected_version: Option<i64>,
    ) -> AppResult<PurchaseDocumentView> {
        ctx.require(Permission::PostDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let (mut e, prepared) = self
            .prepare_purchase(&mut uow, &org, &input, id, Some(ctx))
            .await?;
        if let Some(p) = &prepared
            && let Some(lock) = self
                .gst_lock(
                    &mut uow,
                    &org.id,
                    p.doc.gst_registration_id.as_deref(),
                    &p.doc.doc_date,
                    "purchases",
                )
                .await?
        {
            e.add(
                "docDate",
                format!("{lock} is recorded as filed, so nothing can be recorded in that month. Date it in an open month; its ITC is claimed there."),
            );
        }
        let mut series = None;
        if let Some(p) = &prepared
            && p.doc.doc_type == "debit_note"
        {
            let date = NaiveDate::parse_from_str(&p.doc.doc_date, "%Y-%m-%d").unwrap_or_default();
            let fy =
                FinancialYear::containing(date, u32::try_from(org.fy_start_month).unwrap_or(4));
            match uow.default_series(&org.id, p.doc.gst_registration_id.as_deref(), "debit_note", &fy.code()).await? {
                None => e.add("docDate", format!("There's no debit note number series for FY {}. Add one in Settings → Numbering.", fy.code())),
                Some(s) => {
                    if let Some(last) = uow.latest_purchase_number_date(&org.id, &s.id).await?
                        && last > p.doc.doc_date
                    {
                        e.add("docDate", format!("Numbers run in date order: the last debit note in this series is dated {last}."));
                    }
                    series = Some((s, fy));
                }
            }
        }
        e.finish()?;
        let mut p = prepared.expect("validated");
        let lines = p.lines.clone();
        // Numbering fields are set before the draft write so the frozen row is final.
        let now = self.now();
        if let Some((s, fy)) = &series {
            let n = uow.reserve_next_number(&org.id, &s.id, &now).await?;
            p.doc.number = Some(
                SeriesFormat {
                    prefix: s.prefix.clone(),
                    suffix: s.suffix.clone(),
                    padding: u8::try_from(s.padding).unwrap_or(4),
                }
                .format(u64::try_from(n).unwrap_or(1)),
            );
            p.doc.series_id = Some(s.id.clone());
            p.doc.financial_year = Some(fy.code());
        }
        let mut doc = self
            .write_purchase_draft(&mut uow, ctx, &org.id, id, expected_version, p)
            .await?;
        let label = match doc.doc_type.as_str() {
            "debit_note" => format!("Debit note {}", doc.number.clone().unwrap_or_default()),
            "expense" => "Expense".into(),
            _ => format!(
                "Bill {}",
                doc.supplier_invoice_number.clone().unwrap_or_default()
            ),
        };
        let entry_id = post(
            &mut uow,
            ctx,
            Posting {
                org_id: &org.id,
                entry_date: &doc.doc_date,
                source_type: if doc.doc_type == "debit_note" {
                    "purchase_debit_note"
                } else {
                    "purchase"
                },
                source_id: Some(&doc.id),
                narration: Some(format!(
                    "{} — {}",
                    capital(&label),
                    doc.party_name
                        .clone()
                        .unwrap_or_else(|| "cash expense".into())
                )),
                reverses_entry_id: None,
                idempotency_key: Some(format!("purchase:record:{}", doc.id)),
                lines: purchase_post_lines(&doc, &lines),
            },
            &now,
        )
        .await?;
        let version = doc.row_version;
        doc.status = "recorded".into();
        doc.journal_entry_id = Some(entry_id);
        doc.recorded_at = Some(now.clone());
        doc.updated_at.clone_from(&now);
        if doc.payment_mode != "credit" {
            doc.paid_paise = doc.total_paise;
        }
        if !uow.update_purchase(&doc, version).await? {
            return Err(AppError::stale());
        }
        let c = Change {
            action: "purchase_document.record",
            entity_id: Some(&doc.id),
            before: None,
            after: json(&doc),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        let view = self.load_purchase(&mut uow, &org.id, &doc.id).await?;
        uow.commit().await?;
        Ok(view)
    }

    /// Decide (or change) a line's input tax credit. Posts an adjusting entry
    /// that moves the tax between ITC pending, input tax and cost.
    pub async fn review_itc(
        &self,
        ctx: &RequestContext,
        document_id: &str,
        line_id: &str,
        input: ItcReviewInput,
        expected_version: i64,
    ) -> AppResult<PurchaseDocumentView> {
        ctx.require(Permission::PostDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let mut doc = uow
            .get_purchase(&org.id, document_id)
            .await?
            .ok_or(AppError::NotFound("Purchase"))?;
        if doc.status != "recorded" {
            return Err(AppError::Conflict(
                "ITC is reviewed on recorded purchases.".into(),
            ));
        }
        if doc.row_version != expected_version {
            return Err(AppError::stale());
        }
        let lines = uow.purchase_lines(&doc.id).await?;
        let before = lines
            .iter()
            .find(|l| l.id == line_id)
            .cloned()
            .ok_or(AppError::NotFound("Line"))?;
        let tax = tax_of(&before);
        if tax == 0 {
            return Err(AppError::Invalid("this line carries no GST".into()));
        }
        // After the claim month's GSTR-3B is filed, only a line still pending
        // may be reviewed; its ITC moves to the first month not yet filed.
        let filings = self
            .filed_returns(&mut uow, &org.id, doc.gst_registration_id.as_deref())
            .await?;
        let doc_month = doc.doc_date.get(..7).unwrap_or_default().to_owned();
        let claim_month = before.itc_period.clone().unwrap_or(doc_month);
        let mut itc_period = before.itc_period.clone();
        if crate::filing::is_filed(&filings, "gstr3b", &claim_month) {
            let claim = crate::returns::Period::parse(&claim_month)?;
            if before.itc_status != "pending" {
                return Err(AppError::Conflict(format!(
                    "GSTR-3B for {} is recorded as filed with this line's ITC, so its review can't change. Correct it with a debit note in an open month.",
                    claim.label()
                )));
            }
            itc_period = Some(crate::filing::first_open_3b(&filings, claim.next()).code());
        }
        let registered = doc.gst_registration_id.is_some();
        let supplier_ok = doc.party_gstin.is_some()
            && matches!(doc.party_gst_type.as_deref(), Some("regular" | "sez"));
        let claimable = if !registered {
            Err("Your business isn't GST-registered, so input tax can't be claimed.")
        } else if !supplier_ok && !doc.reverse_charge {
            Err("Input tax can only be claimed on a registered supplier's tax invoice.")
        } else {
            Ok(())
        };
        let mut e = Errors::new("");
        let dec = decide_itc(
            &mut e,
            "",
            Some(&input.status),
            input.reason.clone(),
            &input.eligible,
            tax,
            claimable,
        );
        e.finish()?;
        let now = self.now();
        let mut after = before.clone();
        after.itc_status = dec.status.into();
        after.itc_reason = dec.reason;
        after.itc_eligible_paise = dec.eligible_paise;
        after.itc_common =
            input.common.unwrap_or(false) && matches!(dec.status, "eligible" | "partial");
        after.itc_reviewed_at = Some(now.clone());
        after.itc_period = itc_period;
        after.itc_reviewed_by = Some(ctx.actor.user_id.clone());
        let today = self.today().format("%Y-%m-%d").to_string();
        if (after.itc_status.as_str(), after.itc_eligible_paise)
            != (before.itc_status.as_str(), before.itc_eligible_paise)
        {
            let mut lines_out: Vec<PostLine> = signed(&doc, itc_allocation(&before))
                .iter()
                .map(PostLine::mirrored)
                .collect();
            lines_out.extend(signed(&doc, itc_allocation(&after)));
            post(
                &mut uow,
                ctx,
                Posting {
                    org_id: &org.id,
                    entry_date: &today,
                    source_type: "itc_review",
                    source_id: Some(&doc.id),
                    narration: Some(format!(
                        "ITC review — {}: {} → {}",
                        before.description, before.itc_status, after.itc_status
                    )),
                    reverses_entry_id: None,
                    idempotency_key: None,
                    lines: lines_out,
                },
                &now,
            )
            .await?;
        }
        uow.update_line_itc(&after).await?;
        doc.updated_at.clone_from(&now);
        if !uow.update_purchase(&doc, expected_version).await? {
            return Err(AppError::stale());
        }
        let c = Change {
            action: "purchase_line.itc_review",
            entity_id: Some(line_id),
            before: json(&before),
            after: json(&after),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        let view = self.load_purchase(&mut uow, &org.id, &doc.id).await?;
        uow.commit().await?;
        Ok(view)
    }

    pub async fn cancel_purchase(
        &self,
        ctx: &RequestContext,
        id: &str,
        reason: &str,
        expected_version: i64,
    ) -> AppResult<PurchaseDocumentView> {
        ctx.require(Permission::PostDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let before = uow
            .get_purchase(&org.id, id)
            .await?
            .ok_or(AppError::NotFound("Purchase"))?;
        let mut e = Errors::new("");
        let reason = reason.trim();
        if reason.is_empty() {
            e.add("reason", "Say why it's being cancelled.");
        }
        check_len(&mut e, "reason", &Some(reason.to_owned()), 300);
        e.finish()?;
        if before.status != "recorded" {
            return Err(AppError::Conflict(
                "Only recorded purchases can be cancelled.".into(),
            ));
        }
        if let Some(lock) = self
            .gst_lock(
                &mut uow,
                &org.id,
                before.gst_registration_id.as_deref(),
                &before.doc_date,
                "purchases",
            )
            .await?
        {
            return Err(AppError::Conflict(format!(
                "{lock} is recorded as filed with this purchase, so it can't be cancelled. Record a debit note in an open month instead."
            )));
        }
        if uow
            .debit_notes_for(&org.id, id)
            .await?
            .iter()
            .any(|d| d.status != "cancelled")
        {
            return Err(AppError::Conflict(
                "This bill has debit notes. Cancel or delete them first.".into(),
            ));
        }
        if before.payment_mode == "credit" && before.paid_paise > 0 {
            return Err(AppError::Conflict(
                "Payments are recorded against this bill.".into(),
            ));
        }
        if before.row_version != expected_version {
            return Err(AppError::stale());
        }
        let now = self.now();
        // Reverse the bill as it stands now, including any ITC reviews.
        let lines = uow.purchase_lines(&before.id).await?;
        let entry_id = post(
            &mut uow,
            ctx,
            Posting {
                org_id: &org.id,
                entry_date: &before.doc_date,
                source_type: if before.doc_type == "debit_note" {
                    "purchase_debit_note"
                } else {
                    "purchase"
                },
                source_id: Some(&before.id),
                narration: Some(format!("Cancellation — {reason}")),
                reverses_entry_id: before.journal_entry_id.as_deref(),
                idempotency_key: Some(format!("purchase:cancel:{}", before.id)),
                lines: purchase_post_lines(&before, &lines)
                    .iter()
                    .map(PostLine::mirrored)
                    .collect(),
            },
            &now,
        )
        .await?;
        let mut d = before.clone();
        d.status = "cancelled".into();
        d.cancel_reason = Some(reason.to_owned());
        d.cancel_entry_id = Some(entry_id);
        d.cancelled_at = Some(now.clone());
        d.updated_at.clone_from(&now);
        if !uow.update_purchase(&d, expected_version).await? {
            return Err(AppError::stale());
        }
        let c = Change {
            action: "purchase_document.cancel",
            entity_id: Some(id),
            before: json(&before),
            after: json(&d),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        let view = self.load_purchase(&mut uow, &org.id, id).await?;
        uow.commit().await?;
        Ok(view)
    }

    pub async fn delete_purchase_draft(
        &self,
        ctx: &RequestContext,
        id: &str,
        expected_version: i64,
    ) -> AppResult<()> {
        ctx.require(Permission::PrepareDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let before = uow
            .get_purchase(&org.id, id)
            .await?
            .ok_or(AppError::NotFound("Purchase"))?;
        if before.row_version != expected_version {
            return Err(AppError::stale());
        }
        if !uow.delete_purchase_draft(&org.id, id).await? {
            return Err(AppError::Conflict(
                "Only drafts can be deleted. Cancel a recorded purchase instead.".into(),
            ));
        }
        let now = self.now();
        let c = Change {
            action: "purchase_document.delete",
            entity_id: Some(id),
            before: json(&before),
            after: None,
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        uow.commit().await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Attachments (spec §5.3: original bills, with hash and metadata)
    // -----------------------------------------------------------------------

    /// Attach a file to a purchase. Content is stored once per SHA-256; the
    /// attachment row keeps who added it and when, and can't be changed.
    pub async fn add_attachment(
        &self,
        ctx: &RequestContext,
        document_id: &str,
        file_name: &str,
        bytes: &[u8],
    ) -> AppResult<AttachmentRecord> {
        ctx.require(Permission::PrepareDocuments)?;
        let file_name = file_name.trim();
        if bytes.is_empty() {
            return Err(AppError::Invalid("the file is empty".into()));
        }
        if bytes.len() > MAX_ATTACHMENT_BYTES {
            return Err(AppError::Invalid("attachments can be up to 5 MB".into()));
        }
        let content_type = content_type_for(file_name).ok_or_else(|| {
            AppError::Invalid(
                "attach a PDF, image (PNG, JPG, WEBP, HEIC), Excel or CSV file".into(),
            )
        })?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let doc = uow
            .get_purchase(&org.id, document_id)
            .await?
            .ok_or(AppError::NotFound("Purchase"))?;
        let sha256 = sha256_hex(bytes);
        let now = self.now();
        uow.put_blob(&sha256, bytes).await?;
        let a = AttachmentRecord {
            id: new_id().to_string(),
            organisation_id: org.id.clone(),
            entity_type: "purchase_document".into(),
            entity_id: doc.id.clone(),
            file_name: file_name.chars().take(200).collect(),
            content_type: content_type.into(),
            size_bytes: i64::try_from(bytes.len()).unwrap_or(i64::MAX),
            sha256,
            uploaded_at: now.clone(),
            uploaded_by: ctx.actor.user_id.clone(),
        };
        uow.insert_attachment(&a).await?;
        let c = Change {
            action: "attachment.create",
            entity_id: Some(&a.id),
            before: None,
            after: json(&a),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        uow.commit().await?;
        Ok(a)
    }

    pub async fn attachment_content(
        &self,
        ctx: &RequestContext,
        id: &str,
    ) -> AppResult<AttachmentContent> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let a = uow
            .get_attachment(&org.id, id)
            .await?
            .ok_or(AppError::NotFound("Attachment"))?;
        let bytes = uow
            .blob(&a.sha256)
            .await?
            .ok_or(AppError::NotFound("Attachment content"))?;
        if sha256_hex(&bytes) != a.sha256 {
            return Err(AppError::Conflict(
                "This attachment's content doesn't match its recorded hash.".into(),
            ));
        }
        Ok(AttachmentContent {
            file_name: a.file_name,
            content_type: a.content_type,
            bytes,
        })
    }

    /// Remove an attachment from a draft. Attachments of recorded purchases stay.
    pub async fn remove_attachment(&self, ctx: &RequestContext, id: &str) -> AppResult<()> {
        ctx.require(Permission::PrepareDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let a = uow
            .get_attachment(&org.id, id)
            .await?
            .ok_or(AppError::NotFound("Attachment"))?;
        let doc = uow.get_purchase(&org.id, &a.entity_id).await?;
        if doc.is_some_and(|d| d.status != "draft") {
            return Err(AppError::Conflict(
                "Attachments of a recorded purchase are kept for the audit trail.".into(),
            ));
        }
        uow.delete_attachment(&org.id, id).await?;
        let now = self.now();
        let c = Change {
            action: "attachment.delete",
            entity_id: Some(id),
            before: json(&a),
            after: None,
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        uow.commit().await?;
        Ok(())
    }
}

fn line_calc(l: &PurchaseLineRecord) -> LineCalc {
    LineCalc {
        amount_paise: l.amount_paise,
        taxable_paise: l.taxable_paise,
        cgst_paise: l.cgst_paise,
        sgst_paise: l.sgst_paise,
        utgst_paise: l.utgst_paise,
        igst_paise: l.igst_paise,
        cess_paise: l.cess_paise,
        total_paise: l.total_paise,
        computed_tax_paise: l.computed_tax_paise,
    }
}
