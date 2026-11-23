//! Phase 1.3: sales documents — tax invoices, bills of supply and credit notes.
//!
//! Lifecycle: draft → issued → (cancelled). Drafts are freely editable.
//! Issuing validates everything, takes the next number from the series,
//! freezes a tax snapshot and posts a balanced journal entry — all in one
//! transaction. Issued documents are never edited: corrections are a credit
//! note or a cancellation, which posts a reversal (spec §5.2, §5.5).

use std::collections::HashMap;
use std::str::FromStr;

use av_domain::masters::validate_hsn_sac;
use av_domain::{
    DocumentType, FinancialYear, Gstin, ItemKind, Money, PartyGstType, PartyKind, SeriesFormat,
    StateCode, TaxCategory, new_id,
};
use av_gst_engine::checks::{
    Gstr1Placement, Gstr1Section, LineFacts, OutwardDoc, OutwardFacts, Recipient, TaxWarning,
    gstr1_placement, line_warnings, place_of_supply_warning,
};
use av_gst_engine::invoice::{LineInput, Supply, compute_document, line_amount};
use av_gst_engine::{RulePack, SupplyKind, TaxRate, document_totals};
use av_persistence::records::{
    OrganisationRecord, PartyRecord, RegistrationRecord, SalesDocumentRecord, SalesLineRecord,
};
use av_persistence::{
    ItemRepo, PartyRepo, RegistrationRepo, SalesRepo, SeriesRepo, Store, UnitOfWork,
};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::AppCore;
use crate::context::{Permission, RequestContext};
use crate::error::{AppError, AppResult, Errors};
use crate::masters::{parse_amount, parse_rate};
use crate::posting::{PostLine, Posting, post};
use crate::setup::{Change, audit, check_len, clean, json};
use crate::tax_override::{self, TaxOverrideInput};

const OTHER_COUNTRY: i64 = 96;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesLineInput {
    #[serde(default)]
    pub item_id: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub hsn_sac: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    /// Exact decimal, up to 3 places.
    pub quantity: String,
    /// Rupees per unit as a decimal string.
    pub unit_price: String,
    /// Flat discount for the line, in rupees.
    #[serde(default)]
    pub discount: Option<String>,
    pub tax_category: String,
    /// Percent, e.g. "18".
    #[serde(default)]
    pub gst_rate: Option<String>,
    #[serde(default)]
    pub cess_rate: Option<String>,
    /// Tax entered by hand instead of calculated (owner or accountant).
    #[serde(default)]
    pub tax_override: Option<TaxOverrideInput>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesDocumentInput {
    /// `tax_invoice`, `bill_of_supply` or `credit_note`.
    pub doc_type: String,
    #[serde(default)]
    pub party_id: String,
    pub doc_date: String,
    #[serde(default)]
    pub due_date: Option<String>,
    /// Which GSTIN issues it; defaults to the first active registration.
    #[serde(default)]
    pub gst_registration_id: Option<String>,
    /// Defaults to the party's state.
    #[serde(default)]
    pub place_of_supply: Option<u8>,
    #[serde(default)]
    pub zero_rated: bool,
    #[serde(default)]
    pub prices_include_tax: bool,
    /// Credit notes: the invoice being credited.
    #[serde(default)]
    pub original_document_id: Option<String>,
    /// Credit notes: why.
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub lines: Vec<SalesLineInput>,
    /// Why any line's tax was entered by hand.
    #[serde(default)]
    pub tax_override_reason: Option<String>,
}

/// Status as the user sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayStatus {
    Draft,
    Issued,
    Overdue,
    PartPaid,
    Paid,
    Cancelled,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesSummary {
    #[serde(flatten)]
    pub document: SalesDocumentRecord,
    pub display_status: DisplayStatus,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentRef {
    pub id: String,
    pub doc_type: String,
    pub number: Option<String>,
    pub doc_date: String,
    pub status: String,
    pub total_paise: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesDocumentView {
    #[serde(flatten)]
    pub document: SalesDocumentRecord,
    pub display_status: DisplayStatus,
    pub lines: Vec<SalesLineRecord>,
    /// For a credit note: the invoice it credits.
    pub original: Option<DocumentRef>,
    /// For an invoice: credit notes raised against it.
    pub credit_notes: Vec<DocumentRef>,
    /// Issued credit notes total, paise.
    pub credited_paise: i64,
    /// Where the document is reported in GSTR-1.
    pub gstr1: Gstr1Info,
}

/// GSTR-1 placement with its label, for display.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gstr1Info {
    pub section: Option<Gstr1Section>,
    pub label: Option<&'static str>,
    /// Exempt, nil-rated or non-GST lines reported in table 8.
    pub nil_table: bool,
}

impl From<Gstr1Placement> for Gstr1Info {
    fn from(p: Gstr1Placement) -> Self {
        Gstr1Info {
            section: p.section,
            label: p.section.map(Gstr1Section::label),
            nil_table: p.nil_table,
        }
    }
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineCalc {
    pub amount_paise: i64,
    pub taxable_paise: i64,
    pub cgst_paise: i64,
    pub sgst_paise: i64,
    pub utgst_paise: i64,
    pub igst_paise: i64,
    pub cess_paise: i64,
    pub total_paise: i64,
    /// Set when the tax was entered by hand: the tax as calculated.
    pub computed_tax_paise: Option<i64>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TotalsCalc {
    pub taxable_paise: i64,
    pub cgst_paise: i64,
    pub sgst_paise: i64,
    pub utgst_paise: i64,
    pub igst_paise: i64,
    pub cess_paise: i64,
    pub round_off_paise: i64,
    pub total_paise: i64,
}

/// Live figures for the editor. Lines that can't be computed yet are `None`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesCalculation {
    pub supply_kind: Option<SupplyKind>,
    pub place_of_supply: Option<i64>,
    pub supplier_state: Option<i64>,
    pub lines: Vec<Option<LineCalc>>,
    pub totals: TotalsCalc,
    /// The number the document would get if issued now.
    pub number_preview: Option<String>,
    /// Points to review before issuing; never blocking.
    pub warnings: Vec<TaxWarning>,
    pub gstr1: Option<Gstr1Info>,
    pub rule_pack_version: String,
}

// ---------------------------------------------------------------------------
// Preparation (shared by calculate, save and issue)
// ---------------------------------------------------------------------------

struct Prepared {
    doc: SalesDocumentRecord,
    lines: Vec<SalesLineRecord>,
    warnings: Vec<TaxWarning>,
    gstr1: Gstr1Info,
}

pub(crate) fn to_paise(m: Money) -> i64 {
    m.to_paise().unwrap_or(0)
}

pub(crate) fn parse_quantity(e: &mut Errors, field: &str, s: &str) -> Option<Decimal> {
    let t = s.trim().replace(',', "");
    match Decimal::from_str(&t) {
        Ok(q) if q.normalize().scale() > 3 => {
            e.add(field, "Use up to 3 decimal places.");
            None
        }
        Ok(q) if q <= Decimal::ZERO => {
            e.add(field, "Enter a quantity above zero.");
            None
        }
        Ok(q) => Some(q.normalize()),
        Err(_) => {
            e.add(field, "Enter a number.");
            None
        }
    }
}

pub(crate) fn kind_of(rec: &SalesDocumentRecord) -> SupplyKind {
    match rec.supply_kind.as_str() {
        "intra_state" => SupplyKind::IntraState,
        "intra_union_territory" => SupplyKind::IntraUnionTerritory,
        _ => SupplyKind::InterState,
    }
}

pub(crate) fn recipient_of(d: &SalesDocumentRecord) -> Recipient {
    if d.place_of_supply == OTHER_COUNTRY {
        return Recipient::Overseas;
    }
    match PartyGstType::parse(&d.party_gst_type) {
        Some(PartyGstType::Overseas) => Recipient::Overseas,
        Some(PartyGstType::Sez) => Recipient::Sez,
        Some(PartyGstType::Regular | PartyGstType::Composition) => Recipient::Registered,
        _ => Recipient::Unregistered,
    }
}

pub(crate) fn ymd(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap_or_default()
}

/// GSTR-1 placement of a sales document. Without lines (an original
/// invoice looked up for a credit note) the document is treated as taxable.
pub(crate) fn gstr1_of(
    d: &SalesDocumentRecord,
    lines: &[SalesLineRecord],
    original: Option<&SalesDocumentRecord>,
) -> Gstr1Info {
    let doc = match d.doc_type.as_str() {
        "credit_note" => OutwardDoc::CreditNote,
        "bill_of_supply" => OutwardDoc::BillOfSupply,
        _ => OutwardDoc::Invoice,
    };
    let (has_taxable, has_other) = if lines.is_empty() {
        (d.doc_type != "bill_of_supply", false)
    } else {
        (
            lines.iter().any(|l| l.tax_category == "taxable"),
            lines.iter().any(|l| l.tax_category != "taxable"),
        )
    };
    let pack = RulePack::bundled();
    let original_section = original.and_then(|o| {
        let f = OutwardFacts {
            doc: OutwardDoc::Invoice,
            date: ymd(&o.doc_date),
            recipient: recipient_of(o),
            kind: kind_of(o),
            zero_rated: o.zero_rated,
            total_paise: o.total_paise,
            has_taxable_lines: o.doc_type != "bill_of_supply",
            has_other_lines: false,
            original_section: None,
        };
        gstr1_placement(pack, &f).section
    });
    let facts = OutwardFacts {
        doc,
        date: ymd(&d.doc_date),
        recipient: recipient_of(d),
        kind: kind_of(d),
        zero_rated: d.zero_rated,
        total_paise: d.total_paise,
        has_taxable_lines: has_taxable,
        has_other_lines: has_other,
        original_section,
    };
    gstr1_placement(pack, &facts).into()
}

pub(crate) fn kind_str(k: SupplyKind) -> &'static str {
    match k {
        SupplyKind::IntraState => "intra_state",
        SupplyKind::IntraUnionTerritory => "intra_union_territory",
        SupplyKind::InterState => "inter_state",
    }
}

fn address_of(p: &PartyRecord) -> Option<String> {
    let parts: Vec<&str> = [&p.address_line1, &p.address_line2, &p.city, &p.pincode]
        .into_iter()
        .filter_map(|v| v.as_deref())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

fn doc_type_label(t: DocumentType) -> &'static str {
    match t {
        DocumentType::TaxInvoice => "tax invoice",
        DocumentType::BillOfSupply => "bill of supply",
        DocumentType::CreditNote => "credit note",
        DocumentType::DebitNote => "debit note",
        _ => "document",
    }
}

/// `credited_paise`: issued credit notes against the document, which reduce
/// what is still to be received just as receipts do.
pub(crate) fn display_status(
    d: &SalesDocumentRecord,
    credited_paise: i64,
    today: NaiveDate,
) -> DisplayStatus {
    let settled = d.paid_paise + credited_paise >= d.total_paise;
    match d.status.as_str() {
        "draft" => DisplayStatus::Draft,
        "cancelled" => DisplayStatus::Cancelled,
        _ if d.doc_type == "credit_note" => DisplayStatus::Issued,
        _ if settled && d.paid_paise > 0 => DisplayStatus::Paid,
        _ if settled => DisplayStatus::Issued,
        _ if d.paid_paise > 0 => DisplayStatus::PartPaid,
        _ if d
            .due_date
            .as_deref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .is_some_and(|due| due < today) =>
        {
            DisplayStatus::Overdue
        }
        _ => DisplayStatus::Issued,
    }
}

impl<S: Store> AppCore<S> {
    /// Validate a document and compute its tax. Returns field errors and,
    /// when valid, the records. `ctx` is `None` for live figures, which
    /// ignore problems with tax entered by hand.
    async fn prepare(
        &self,
        uow: &mut S::Uow,
        org: &OrganisationRecord,
        input: &SalesDocumentInput,
        self_id: Option<&str>,
        ctx: Option<&RequestContext>,
    ) -> AppResult<(Errors, Option<Prepared>)> {
        let mut e = Errors::new("");
        let today = self.today();
        let regs: Vec<RegistrationRecord> = uow
            .list_registrations(&org.id)
            .await?
            .into_iter()
            .filter(|r| r.status == "active")
            .collect();
        let registered = !regs.is_empty();

        // Document type.
        let doc_type = match DocumentType::parse(input.doc_type.trim()) {
            Some(
                t @ (DocumentType::TaxInvoice
                | DocumentType::BillOfSupply
                | DocumentType::CreditNote),
            ) => Some(t),
            _ => {
                e.add(
                    "docType",
                    "Choose tax invoice, bill of supply or credit note.",
                );
                None
            }
        };
        if !registered && doc_type.is_some_and(|t| t != DocumentType::BillOfSupply) {
            e.add(
                "docType",
                "A business without a GSTIN issues bills of supply only.",
            );
        }

        // Credit note: the original invoice decides most of the header.
        let mut original: Option<SalesDocumentRecord> = None;
        if doc_type == Some(DocumentType::CreditNote) {
            match clean(&input.original_document_id) {
                None => e.add(
                    "originalDocumentId",
                    "Choose the invoice this credit note is for.",
                ),
                Some(id) => match uow.get_sales(&org.id, &id).await? {
                    Some(o) if o.status == "issued" && o.doc_type != "credit_note" => {
                        original = Some(o)
                    }
                    Some(_) => e.add(
                        "originalDocumentId",
                        "Credit notes can only be raised against an issued invoice.",
                    ),
                    None => e.add("originalDocumentId", "That invoice wasn't found."),
                },
            }
            let reason = clean(&input.reason);
            if reason.is_none() {
                e.add(
                    "reason",
                    "Say why the credit note is issued, e.g. goods returned.",
                );
            }
            check_len(&mut e, "reason", &reason, 300);
        }

        // Issuing GSTIN.
        let registration: Option<RegistrationRecord> = if let Some(o) = &original {
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

        // Party.
        let party = if let Some(o) = &original {
            uow.get_party(&org.id, &o.party_id).await?
        } else if input.party_id.trim().is_empty() {
            e.add("partyId", "Choose a customer.");
            None
        } else {
            let p = uow.get_party(&org.id, input.party_id.trim()).await?;
            match &p {
                None => e.add("partyId", "That customer wasn't found."),
                Some(p) if !p.is_active => e.add("partyId", "This party is inactive."),
                Some(p) if PartyKind::parse(&p.kind) == Some(PartyKind::Supplier) => {
                    e.add("partyId", "This party is set up as a supplier only. Change it to Both to sell to them.");
                }
                _ => {}
            }
            p
        };

        // Dates.
        let doc_date = match NaiveDate::parse_from_str(input.doc_date.trim(), "%Y-%m-%d") {
            Ok(d) => Some(d),
            Err(_) => {
                e.add("docDate", "Enter a valid date.");
                None
            }
        };
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
            if let Some(o) = &original
                && NaiveDate::parse_from_str(&o.doc_date, "%Y-%m-%d").is_ok_and(|od| d < od)
            {
                e.add(
                    "docDate",
                    "A credit note can't be dated before its invoice.",
                );
            }
        }
        let due_date = match clean(&input.due_date) {
            Some(s) => match NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                Ok(due) if doc_date.is_some_and(|d| due < d) => {
                    e.add("dueDate", "The due date can't be before the document date.");
                    None
                }
                Ok(due) => Some(due),
                Err(_) => {
                    e.add("dueDate", "Enter a valid date.");
                    None
                }
            },
            None => doc_date.map(|d| {
                let days = party
                    .as_ref()
                    .and_then(|p| p.payment_terms_days)
                    .unwrap_or(0);
                d + chrono::Days::new(u64::try_from(days).unwrap_or(0))
            }),
        };

        // Place of supply and supply kind.
        let supplier_state = registration
            .as_ref()
            .map_or(org.state_code, |r| r.state_code);
        let place_of_supply = if let Some(o) = &original {
            Some(o.place_of_supply)
        } else {
            match input.place_of_supply {
                Some(c) => match StateCode::new(c) {
                    Ok(s) => Some(i64::from(s.code())),
                    Err(_) => {
                        e.add("placeOfSupply", "Choose a state.");
                        None
                    }
                },
                None => party.as_ref().map(|p| p.state_code),
            }
        };
        let supply_kind = place_of_supply.and_then(|pos| {
            let (a, b) = (u8::try_from(supplier_state).ok()?, u8::try_from(pos).ok()?);
            Some(SupplyKind::determine(
                StateCode::new(a).ok()?,
                StateCode::new(b).ok()?,
            ))
        });
        let zero_rated = original.as_ref().map_or(input.zero_rated, |o| o.zero_rated);
        let prices_include_tax = original
            .as_ref()
            .map_or(input.prices_include_tax, |o| o.prices_include_tax);
        let export_or_sez = place_of_supply == Some(OTHER_COUNTRY)
            || party
                .as_ref()
                .is_some_and(|p| PartyGstType::parse(&p.gst_type) == Some(PartyGstType::Sez));
        if zero_rated && !export_or_sez {
            e.add(
                "zeroRated",
                "Only exports and supplies to SEZ can be zero-rated.",
            );
        }

        // Lines.
        if input.lines.is_empty() {
            e.add("lines", "Add at least one line.");
        }
        let tax_free_document = doc_type == Some(DocumentType::BillOfSupply)
            || (doc_type == Some(DocumentType::CreditNote)
                && original
                    .as_ref()
                    .is_some_and(|o| o.doc_type == "bill_of_supply"));
        let mut engine_lines = Vec::new();
        let mut line_records = Vec::new();
        let mut line_facts: Vec<(usize, Option<String>, TaxCategory, u32)> = Vec::new();
        for (i, l) in input.lines.iter().enumerate() {
            let f = |name: &str| format!("lines.{i}.{name}");
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
            let category = TaxCategory::parse(l.tax_category.trim());
            if category.is_none() {
                le.add(&f("taxCategory"), "Choose how GST applies.");
            }
            let rate = match parse_rate(&mut le, &f("gstRate"), &l.gst_rate) {
                Some(bp) if category == Some(TaxCategory::Taxable) && !TaxRate(bp).is_known() => {
                    le.add(&f("gstRate"), "Choose a GST rate.");
                    0
                }
                Some(bp) if category == Some(TaxCategory::Taxable) => bp,
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
            if doc_type == Some(DocumentType::BillOfSupply) && registered && rate > 0 {
                le.add(
                    &f("gstRate"),
                    "A bill of supply can't carry taxable items. Use a tax invoice.",
                );
            }
            let cess = parse_rate(&mut le, &f("cessRate"), &l.cess_rate).unwrap_or(0);
            let hsn = clean(&l.hsn_sac).or_else(|| item.as_ref().and_then(|i| i.hsn_sac.clone()));
            let kind = item
                .as_ref()
                .and_then(|i| ItemKind::parse(&i.kind))
                .or_else(|| {
                    hsn.as_deref().map(|h| {
                        if h.starts_with("99") {
                            ItemKind::Service
                        } else {
                            ItemKind::Goods
                        }
                    })
                });
            match (&hsn, kind) {
                (Some(h), Some(k)) => {
                    if let Err(err) = validate_hsn_sac(h, k) {
                        le.add(&f("hsnSac"), format!("{err}."));
                    }
                }
                (None, _) if registered && category.is_some_and(|c| c != TaxCategory::NonGst) => {
                    le.add(&f("hsnSac"), "Enter the HSN/SAC code.");
                }
                _ => {}
            }
            if let Some(c) = category {
                line_facts.push((i, hsn.clone(), c, rate));
            }
            let quantity = parse_quantity(&mut le, &f("quantity"), &l.quantity);
            let price = parse_amount(&mut le, &f("unitPrice"), &Some(l.unit_price.clone()), false);
            if clean(&Some(l.unit_price.clone())).is_none() {
                le.add(&f("unitPrice"), "Enter a price.");
            }
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
                    line_records.push(SalesLineRecord {
                        id: new_id().to_string(),
                        document_id: String::new(),
                        line_no: i64::try_from(i + 1).unwrap_or(0),
                        item_id: item.as_ref().map(|i| i.id.clone()),
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
                        computed_tax_paise: None,
                    });
                }
            }
            e.absorb(le);
        }
        let notes = clean(&input.notes);
        check_len(&mut e, "notes", &notes, 1000);

        let (Some(t), Some(p), Some(d), Some(pos), Some(kind)) =
            (doc_type, &party, doc_date, place_of_supply, supply_kind)
        else {
            return Ok((e, None));
        };
        if !e.is_empty() {
            return Ok((e, None));
        }
        let supply = Supply {
            kind,
            zero_rated,
            prices_include_tax,
            tax_free_document,
        };
        let computed =
            compute_document(&engine_lines, supply, org.round_to_rupee).map_err(|errs| {
                AppError::Invalid(format!("line {} could not be computed", errs[0].0 + 1))
            })?;
        // Every line is valid here, so records line up with the input.
        let mut oe = Errors::new("");
        let mut taxes = Vec::with_capacity(line_records.len());
        for (k, (rec, c)) in line_records.iter_mut().zip(&computed.lines).enumerate() {
            let entered = input.lines.get(k).and_then(|l| l.tax_override.as_ref());
            let (tax, calculated) = tax_override::apply(&mut oe, k, entered, c.tax, kind);
            rec.amount_paise = to_paise(c.amount);
            rec.taxable_paise = to_paise(tax.taxable_value);
            rec.cgst_paise = to_paise(tax.cgst);
            rec.sgst_paise = to_paise(tax.sgst);
            rec.utgst_paise = to_paise(tax.utgst);
            rec.igst_paise = to_paise(tax.igst);
            rec.cess_paise = to_paise(tax.cess);
            rec.total_paise = to_paise(tax.line_total());
            rec.computed_tax_paise = calculated;
            taxes.push(tax);
        }
        let tt = document_totals(&taxes, org.round_to_rupee);
        let changed = line_records.iter().any(|l| l.computed_tax_paise.is_some());
        let (override_reason, override_by, override_at) = match ctx {
            Some(ctx) => {
                e.absorb(oe);
                tax_override::stamp(
                    &mut e,
                    ctx,
                    changed,
                    &input.tax_override_reason,
                    &self.now(),
                )
            }
            None => (None, None, None),
        };
        let total_paise = to_paise(tt.grand_total);
        if total_paise <= 0 {
            e.add("lines", "The total must be more than zero.");
        }
        if let Some(o) = &original {
            let already: i64 = uow
                .credit_notes_for(&org.id, &o.id)
                .await?
                .iter()
                .filter(|c| c.status == "issued" && Some(c.id.as_str()) != self_id)
                .map(|c| c.total_paise)
                .sum();
            let left = o.total_paise - already;
            if total_paise > left {
                e.add(
                    "lines",
                    format!(
                        "Credit notes can't exceed the invoice. {} is left to credit.",
                        Money::from_paise(left)
                    ),
                );
            }
        }
        let now = self.now();
        let doc = SalesDocumentRecord {
            id: String::new(),
            organisation_id: org.id.clone(),
            doc_type: t.as_str().into(),
            status: "draft".into(),
            gst_registration_id: registration.as_ref().map(|r| r.id.clone()),
            series_id: None,
            financial_year: None,
            number: None,
            doc_date: d.format("%Y-%m-%d").to_string(),
            due_date: if t == DocumentType::CreditNote {
                None
            } else {
                due_date.map(|d| d.format("%Y-%m-%d").to_string())
            },
            party_id: p.id.clone(),
            party_name: p.name.clone(),
            party_gstin: p.gstin.clone(),
            party_gst_type: p.gst_type.clone(),
            party_address: address_of(p),
            place_of_supply: pos,
            supplier_state,
            supply_kind: kind_str(kind).into(),
            zero_rated,
            reverse_charge: false,
            prices_include_tax,
            original_document_id: original.as_ref().map(|o| o.id.clone()),
            reason: if t == DocumentType::CreditNote {
                clean(&input.reason)
            } else {
                None
            },
            notes,
            taxable_paise: to_paise(tt.taxable_value),
            cgst_paise: to_paise(tt.cgst),
            sgst_paise: to_paise(tt.sgst),
            utgst_paise: to_paise(tt.utgst),
            igst_paise: to_paise(tt.igst),
            cess_paise: to_paise(tt.cess),
            round_off_paise: to_paise(tt.round_off),
            total_paise,
            paid_paise: 0,
            journal_entry_id: None,
            cancel_entry_id: None,
            cancel_reason: None,
            row_version: 1,
            created_at: now.clone(),
            updated_at: now,
            issued_at: None,
            cancelled_at: None,
            rule_pack_version: Some(RulePack::bundled().version.clone()),
            tax_override_reason: override_reason,
            tax_override_by: override_by,
            tax_override_at: override_at,
        };
        let warnings = review(&doc, p, original.as_ref(), &line_facts);
        let gstr1 = gstr1_of(&doc, &line_records, original.as_ref());
        let ok = e.is_empty();
        Ok((
            e,
            ok.then_some(Prepared {
                doc,
                lines: line_records,
                warnings,
                gstr1,
            }),
        ))
    }

    /// Live tax figures for the editor; never fails on incomplete input.
    pub async fn calculate_sales(
        &self,
        ctx: &RequestContext,
        input: SalesDocumentInput,
    ) -> AppResult<SalesCalculation> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let (_, prepared) = self.prepare(&mut uow, &org, &input, None, None).await?;
        if let Some(p) = prepared {
            let number_preview = self.number_preview(&mut uow, &org, &p.doc).await?;
            let d = &p.doc;
            return Ok(SalesCalculation {
                supply_kind: Some(kind_of(d)),
                place_of_supply: Some(d.place_of_supply),
                supplier_state: Some(d.supplier_state),
                lines: p
                    .lines
                    .iter()
                    .map(|l| {
                        Some(LineCalc {
                            amount_paise: l.amount_paise,
                            taxable_paise: l.taxable_paise,
                            cgst_paise: l.cgst_paise,
                            sgst_paise: l.sgst_paise,
                            utgst_paise: l.utgst_paise,
                            igst_paise: l.igst_paise,
                            cess_paise: l.cess_paise,
                            total_paise: l.total_paise,
                            computed_tax_paise: l.computed_tax_paise,
                        })
                    })
                    .collect(),
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
                number_preview,
                warnings: p.warnings.clone(),
                gstr1: Some(p.gstr1.clone()),
                rule_pack_version: RulePack::bundled().version.clone(),
            });
        }
        // Incomplete document: work out what we can, line by line.
        self.partial_calculation(&mut uow, &org, &input).await
    }

    async fn partial_calculation(
        &self,
        uow: &mut S::Uow,
        org: &OrganisationRecord,
        input: &SalesDocumentInput,
    ) -> AppResult<SalesCalculation> {
        let regs = uow.list_registrations(&org.id).await?;
        let reg = clean(&input.gst_registration_id)
            .and_then(|id| regs.iter().find(|r| r.id == id))
            .or_else(|| regs.iter().find(|r| r.status == "active"));
        let supplier_state = reg.map_or(org.state_code, |r| r.state_code);
        let party = if input.party_id.trim().is_empty() {
            None
        } else {
            uow.get_party(&org.id, input.party_id.trim()).await?
        };
        let pos = input
            .place_of_supply
            .map(i64::from)
            .or(party.as_ref().map(|p| p.state_code));
        let kind = pos.and_then(|pos| {
            Some(SupplyKind::determine(
                StateCode::new(u8::try_from(supplier_state).ok()?).ok()?,
                StateCode::new(u8::try_from(pos).ok()?).ok()?,
            ))
        });
        let tax_free = input.doc_type == "bill_of_supply";
        let mut lines = Vec::new();
        let mut totals = TotalsCalc::default();
        for l in &input.lines {
            let mut scratch = Errors::new("");
            let calc = (|| {
                let q = parse_quantity(&mut scratch, "q", &l.quantity)?;
                let p = parse_amount(&mut scratch, "p", &Some(l.unit_price.clone()), false)?;
                let d = parse_amount(&mut scratch, "d", &l.discount, false).unwrap_or(0);
                let c = TaxCategory::parse(l.tax_category.trim())?;
                let r = parse_rate(&mut scratch, "r", &l.gst_rate).unwrap_or(0);
                let cess = parse_rate(&mut scratch, "c", &l.cess_rate).unwrap_or(0);
                let li = LineInput {
                    quantity: q,
                    unit_price: Money::from_paise(p),
                    discount: Money::from_paise(d),
                    category: c,
                    rate: TaxRate(r),
                    cess: TaxRate(cess),
                };
                let supply = Supply {
                    kind: kind.unwrap_or(SupplyKind::IntraState),
                    zero_rated: input.zero_rated,
                    prices_include_tax: input.prices_include_tax,
                    tax_free_document: tax_free,
                };
                let doc = compute_document(std::slice::from_ref(&li), supply, false).ok()?;
                let t = doc.lines[0].tax;
                Some(LineCalc {
                    amount_paise: to_paise(doc.lines[0].amount),
                    taxable_paise: to_paise(t.taxable_value),
                    cgst_paise: to_paise(t.cgst),
                    sgst_paise: to_paise(t.sgst),
                    utgst_paise: to_paise(t.utgst),
                    igst_paise: to_paise(t.igst),
                    cess_paise: to_paise(t.cess),
                    total_paise: to_paise(t.line_total()),
                    computed_tax_paise: None,
                })
            })();
            if let Some(c) = &calc {
                totals.taxable_paise += c.taxable_paise;
                totals.cgst_paise += c.cgst_paise;
                totals.sgst_paise += c.sgst_paise;
                totals.utgst_paise += c.utgst_paise;
                totals.igst_paise += c.igst_paise;
                totals.cess_paise += c.cess_paise;
                totals.total_paise += c.total_paise;
            }
            lines.push(calc);
        }
        Ok(SalesCalculation {
            supply_kind: kind,
            place_of_supply: pos,
            supplier_state: Some(supplier_state),
            lines,
            totals,
            number_preview: None,
            warnings: Vec::new(),
            gstr1: None,
            rule_pack_version: RulePack::bundled().version.clone(),
        })
    }

    async fn number_preview(
        &self,
        uow: &mut S::Uow,
        org: &OrganisationRecord,
        d: &SalesDocumentRecord,
    ) -> AppResult<Option<String>> {
        let Ok(date) = NaiveDate::parse_from_str(&d.doc_date, "%Y-%m-%d") else {
            return Ok(None);
        };
        let fy = FinancialYear::containing(date, u32::try_from(org.fy_start_month).unwrap_or(4));
        let series = uow
            .default_series(
                &org.id,
                d.gst_registration_id.as_deref(),
                &d.doc_type,
                &fy.code(),
            )
            .await?;
        Ok(series.map(|s| {
            SeriesFormat {
                prefix: s.prefix,
                suffix: s.suffix,
                padding: u8::try_from(s.padding).unwrap_or(4),
            }
            .format(u64::try_from(s.next_number).unwrap_or(1))
        }))
    }

    fn view(
        &self,
        d: SalesDocumentRecord,
        lines: Vec<SalesLineRecord>,
        related: Related,
    ) -> SalesDocumentView {
        let gstr1 = gstr1_of(&d, &lines, related.original_record.as_ref());
        let credited_paise = related
            .credit_notes
            .iter()
            .filter(|c| c.status == "issued")
            .map(|c| c.total_paise)
            .sum();
        SalesDocumentView {
            display_status: display_status(&d, credited_paise, self.today()),
            document: d,
            lines,
            original: related.original,
            credit_notes: related.credit_notes,
            credited_paise,
            gstr1,
        }
    }

    async fn load_view(
        &self,
        uow: &mut S::Uow,
        org_id: &str,
        id: &str,
    ) -> AppResult<SalesDocumentView> {
        let d = uow
            .get_sales(org_id, id)
            .await?
            .ok_or(AppError::NotFound("Document"))?;
        let lines = uow.sales_lines(&d.id).await?;
        let original_record = match &d.original_document_id {
            Some(oid) => uow.get_sales(org_id, oid).await?,
            None => None,
        };
        let original = original_record.as_ref().map(doc_ref);
        let credit_notes = uow
            .credit_notes_for(org_id, &d.id)
            .await?
            .iter()
            .map(doc_ref)
            .collect();
        Ok(self.view(
            d,
            lines,
            Related {
                original,
                original_record,
                credit_notes,
            },
        ))
    }

    // -----------------------------------------------------------------------
    // Use cases
    // -----------------------------------------------------------------------

    pub async fn list_sales(&self, ctx: &RequestContext) -> AppResult<Vec<SalesSummary>> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let Some(org) = self.find_org(ctx, &mut uow).await? else {
            return Ok(Vec::new());
        };
        let today = self.today();
        let docs = uow.list_sales(&org.id).await?;
        let mut credited: HashMap<String, i64> = HashMap::new();
        for c in docs
            .iter()
            .filter(|c| c.doc_type == "credit_note" && c.status == "issued")
        {
            if let Some(o) = &c.original_document_id {
                *credited.entry(o.clone()).or_default() += c.total_paise;
            }
        }
        Ok(docs
            .into_iter()
            .map(|d| SalesSummary {
                display_status: display_status(
                    &d,
                    credited.get(&d.id).copied().unwrap_or(0),
                    today,
                ),
                document: d,
            })
            .collect())
    }

    pub async fn get_sales_document(
        &self,
        ctx: &RequestContext,
        id: &str,
    ) -> AppResult<SalesDocumentView> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        self.load_view(&mut uow, &org.id, id).await
    }

    /// Write (insert or update) a draft inside `uow`; returns its id.
    async fn write_draft(
        &self,
        uow: &mut S::Uow,
        ctx: &RequestContext,
        org_id: &str,
        id: Option<&str>,
        expected_version: Option<i64>,
        mut p: Prepared,
    ) -> AppResult<SalesDocumentRecord> {
        let now = self.now();
        match id {
            Some(id) => {
                let before = uow
                    .get_sales(org_id, id)
                    .await?
                    .ok_or(AppError::NotFound("Document"))?;
                if before.status != "draft" {
                    return Err(AppError::Conflict(
                        "This document has been issued and can't be edited.".into(),
                    ));
                }
                let version = expected_version.unwrap_or(before.row_version);
                p.doc.id = before.id.clone();
                p.doc.created_at.clone_from(&before.created_at);
                p.doc.updated_at.clone_from(&now);
                if !uow.update_sales(&p.doc, version).await? {
                    return Err(AppError::stale());
                }
                p.doc.row_version = version + 1;
            }
            None => {
                p.doc.id = new_id().to_string();
                uow.insert_sales(&p.doc).await?;
                let c = Change {
                    action: "sales_document.create",
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
        uow.replace_sales_lines(&p.doc.id, &p.lines).await?;
        Ok(p.doc)
    }

    /// Save a draft (new when `id` is `None`). Drafts get no number and post nothing.
    pub async fn save_sales_draft(
        &self,
        ctx: &RequestContext,
        id: Option<&str>,
        input: SalesDocumentInput,
        expected_version: Option<i64>,
    ) -> AppResult<SalesDocumentView> {
        ctx.require(Permission::PrepareDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let (e, prepared) = self.prepare(&mut uow, &org, &input, id, Some(ctx)).await?;
        e.finish()?;
        let doc = self
            .write_draft(
                &mut uow,
                ctx,
                &org.id,
                id,
                expected_version,
                prepared.expect("validated"),
            )
            .await?;
        let view = self.load_view(&mut uow, &org.id, &doc.id).await?;
        uow.commit().await?;
        Ok(view)
    }

    /// Validate, number, freeze and post — one transaction. `id` is the draft
    /// being issued, or `None` to issue straight from the editor.
    pub async fn issue_sales_document(
        &self,
        ctx: &RequestContext,
        id: Option<&str>,
        input: SalesDocumentInput,
        expected_version: Option<i64>,
    ) -> AppResult<SalesDocumentView> {
        ctx.require(Permission::PostDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let (mut e, prepared) = self.prepare(&mut uow, &org, &input, id, Some(ctx)).await?;
        if let Some(p) = &prepared
            && let Some(lock) = self
                .gst_lock(
                    &mut uow,
                    &org.id,
                    p.doc.gst_registration_id.as_deref(),
                    &p.doc.doc_date,
                    "sales",
                )
                .await?
        {
            e.add(
                "docDate",
                format!("{lock} is recorded as filed, so nothing can be issued in that month. Date it in an open month."),
            );
        }

        // Numbering: the default series for this GSTIN, type and FY.
        let mut series = None;
        if let Some(p) = &prepared {
            let date = NaiveDate::parse_from_str(&p.doc.doc_date, "%Y-%m-%d").unwrap_or_default();
            let fy =
                FinancialYear::containing(date, u32::try_from(org.fy_start_month).unwrap_or(4));
            let t = DocumentType::parse(&p.doc.doc_type).unwrap_or(DocumentType::TaxInvoice);
            match uow
                .default_series(
                    &org.id,
                    p.doc.gst_registration_id.as_deref(),
                    &p.doc.doc_type,
                    &fy.code(),
                )
                .await?
            {
                None => e.add(
                    "docDate",
                    format!(
                        "There's no {} number series for FY {}. Add one in Settings → Numbering.",
                        doc_type_label(t),
                        fy.code()
                    ),
                ),
                Some(s) => {
                    if let Some(last) = uow.latest_issued_date(&org.id, &s.id).await?
                        && last > p.doc.doc_date
                    {
                        let last = NaiveDate::parse_from_str(&last, "%Y-%m-%d")
                            .map_or(last.clone(), |d| d.format("%d %b %Y").to_string());
                        e.add("docDate", format!("Numbers run in date order: the last {} in this series is dated {last}.", doc_type_label(t)));
                    }
                    series = Some((s, fy));
                }
            }
        }
        e.finish()?;
        let (s, fy) = series.expect("validated");

        let mut doc = self
            .write_draft(
                &mut uow,
                ctx,
                &org.id,
                id,
                expected_version,
                prepared.expect("validated"),
            )
            .await?;
        let now = self.now();
        let n = uow.reserve_next_number(&org.id, &s.id, &now).await?;
        let number = SeriesFormat {
            prefix: s.prefix.clone(),
            suffix: s.suffix.clone(),
            padding: u8::try_from(s.padding).unwrap_or(4),
        }
        .format(u64::try_from(n).unwrap_or(1));
        let entry_id = post(
            &mut uow,
            ctx,
            Posting {
                org_id: &org.id,
                entry_date: &doc.doc_date,
                source_type: if doc.doc_type == "credit_note" {
                    "sales_credit_note"
                } else {
                    "sales_invoice"
                },
                source_id: Some(&doc.id),
                narration: Some(format!(
                    "{} {} — {}",
                    capital(doc_type_label(
                        DocumentType::parse(&doc.doc_type).unwrap_or(DocumentType::TaxInvoice)
                    )),
                    number,
                    doc.party_name
                )),
                reverses_entry_id: None,
                idempotency_key: Some(format!("sales:issue:{}", doc.id)),
                lines: sales_post_lines(&doc),
            },
            &now,
        )
        .await?;

        let version = doc.row_version;
        doc.status = "issued".into();
        doc.series_id = Some(s.id.clone());
        doc.financial_year = Some(fy.code());
        doc.number = Some(number);
        doc.journal_entry_id = Some(entry_id);
        doc.issued_at = Some(now.clone());
        doc.updated_at.clone_from(&now);
        if !uow.update_sales(&doc, version).await? {
            return Err(AppError::stale());
        }
        let c = Change {
            action: "sales_document.issue",
            entity_id: Some(&doc.id),
            before: None,
            after: json(&doc),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        let view = self.load_view(&mut uow, &org.id, &doc.id).await?;
        uow.commit().await?;
        Ok(view)
    }

    /// Cancel an issued document: the number stays used, and a reversal entry
    /// cancels its posting.
    pub async fn cancel_sales_document(
        &self,
        ctx: &RequestContext,
        id: &str,
        reason: &str,
        expected_version: i64,
    ) -> AppResult<SalesDocumentView> {
        ctx.require(Permission::PostDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let before = uow
            .get_sales(&org.id, id)
            .await?
            .ok_or(AppError::NotFound("Document"))?;
        let mut e = Errors::new("");
        let reason = reason.trim();
        if reason.is_empty() {
            e.add("reason", "Say why it's being cancelled.");
        }
        check_len(&mut e, "reason", &Some(reason.to_owned()), 300);
        e.finish()?;
        if before.status != "issued" {
            return Err(AppError::Conflict(
                "Only issued documents can be cancelled.".into(),
            ));
        }
        if let Some(lock) = self
            .gst_lock(
                &mut uow,
                &org.id,
                before.gst_registration_id.as_deref(),
                &before.doc_date,
                "sales",
            )
            .await?
        {
            return Err(AppError::Conflict(format!(
                "{lock} is recorded as filed with this document, so it can't be cancelled. Issue a credit note in an open month instead."
            )));
        }
        if uow
            .credit_notes_for(&org.id, id)
            .await?
            .iter()
            .any(|c| c.status != "cancelled")
        {
            return Err(AppError::Conflict(
                "This invoice has credit notes. Cancel or delete them first.".into(),
            ));
        }
        if before.paid_paise > 0 {
            return Err(AppError::Conflict(
                "Payments are recorded against this invoice.".into(),
            ));
        }
        if before.row_version != expected_version {
            return Err(AppError::stale());
        }
        let now = self.now();
        let entry_id = post(
            &mut uow,
            ctx,
            Posting {
                org_id: &org.id,
                entry_date: &before.doc_date,
                source_type: if before.doc_type == "credit_note" {
                    "sales_credit_note"
                } else {
                    "sales_invoice"
                },
                source_id: Some(&before.id),
                narration: Some(format!(
                    "Cancellation of {} — {reason}",
                    before.number.clone().unwrap_or_default()
                )),
                reverses_entry_id: before.journal_entry_id.as_deref(),
                idempotency_key: Some(format!("sales:cancel:{}", before.id)),
                lines: sales_post_lines(&before)
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
        if !uow.update_sales(&d, expected_version).await? {
            return Err(AppError::stale());
        }
        let c = Change {
            action: "sales_document.cancel",
            entity_id: Some(id),
            before: json(&before),
            after: json(&d),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        let view = self.load_view(&mut uow, &org.id, id).await?;
        uow.commit().await?;
        Ok(view)
    }

    pub async fn delete_sales_draft(
        &self,
        ctx: &RequestContext,
        id: &str,
        expected_version: i64,
    ) -> AppResult<()> {
        ctx.require(Permission::PrepareDocuments)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let before = uow
            .get_sales(&org.id, id)
            .await?
            .ok_or(AppError::NotFound("Document"))?;
        if before.row_version != expected_version {
            return Err(AppError::stale());
        }
        if !uow.delete_sales_draft(&org.id, id).await? {
            return Err(AppError::Conflict(
                "Only drafts can be deleted. Cancel an issued document instead.".into(),
            ));
        }
        let now = self.now();
        let c = Change {
            action: "sales_document.delete",
            entity_id: Some(id),
            before: json(&before),
            after: None,
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        uow.commit().await?;
        Ok(())
    }
}

struct Related {
    original: Option<DocumentRef>,
    original_record: Option<SalesDocumentRecord>,
    credit_notes: Vec<DocumentRef>,
}

/// Rule-pack review of a prepared document (warnings only). A credit note's
/// rates are checked on its invoice's date, since it adjusts that supply.
fn review(
    doc: &SalesDocumentRecord,
    party: &PartyRecord,
    original: Option<&SalesDocumentRecord>,
    lines: &[(usize, Option<String>, TaxCategory, u32)],
) -> Vec<TaxWarning> {
    let pack = RulePack::bundled();
    let rate_date = ymd(original.map_or(doc.doc_date.as_str(), |o| o.doc_date.as_str()));
    let mut out = Vec::new();
    if recipient_of(doc) != Recipient::Overseas {
        let pos = u8::try_from(doc.place_of_supply)
            .ok()
            .and_then(|c| StateCode::new(c).ok());
        let home = party
            .gstin
            .as_deref()
            .and_then(|g| Gstin::parse(g).ok())
            .map(|g| g.state());
        if let Some(w) = pos.and_then(|pos| place_of_supply_warning(pos, home)) {
            out.push(w);
        }
    }
    for (i, hsn, category, rate) in lines {
        out.extend(line_warnings(
            pack,
            rate_date,
            *i,
            LineFacts {
                hsn: hsn.as_deref(),
                category: *category,
                rate_bp: *rate,
            },
        ));
    }
    out
}

fn doc_ref(d: &SalesDocumentRecord) -> DocumentRef {
    DocumentRef {
        id: d.id.clone(),
        doc_type: d.doc_type.clone(),
        number: d.number.clone(),
        doc_date: d.doc_date.clone(),
        status: d.status.clone(),
        total_paise: d.total_paise,
    }
}

pub(crate) fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// The ledger effect of an invoice: Dr Receivable (party) = Cr Sales + output
/// taxes ± round-off. A credit note is the mirror image.
fn sales_post_lines(d: &SalesDocumentRecord) -> Vec<PostLine> {
    let party = Some(d.party_id.as_str());
    let mut lines = vec![
        PostLine::debit("receivables", d.total_paise, party),
        PostLine::credit("sales", d.taxable_paise, None),
        PostLine::credit("output_cgst", d.cgst_paise, None),
        PostLine::credit("output_sgst", d.sgst_paise, None),
        PostLine::credit("output_utgst", d.utgst_paise, None),
        PostLine::credit("output_igst", d.igst_paise, None),
        PostLine::credit("output_cess", d.cess_paise, None),
    ];
    match d.round_off_paise {
        r if r > 0 => lines.push(PostLine::credit("round_off", r, None)),
        r if r < 0 => lines.push(PostLine::debit("round_off", -r, None)),
        _ => {}
    }
    if d.doc_type == "credit_note" {
        lines.iter().map(PostLine::mirrored).collect()
    } else {
        lines
    }
}
