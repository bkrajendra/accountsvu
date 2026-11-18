//! Phase 1.2 use cases: parties (customers/suppliers) and items/services.
//! Validation is pure (no storage) so the CSV/Excel import reuses it row by row.

use av_domain::business::{is_valid_email, is_valid_phone, is_valid_pincode};
use av_domain::masters::{UQC, is_valid_uqc, validate_hsn_sac};
use av_domain::{
    Gstin, ItemKind, Money, Pan, PartyGstType, PartyKind, StateCode, TaxCategory, new_id,
};
use av_gst_engine::TaxRate;
use av_persistence::records::{ItemRecord, PartyRecord};
use av_persistence::{ItemRepo, JournalRepo, PartyRepo, Store, UnitOfWork};
use serde::{Deserialize, Serialize};

use crate::AppCore;
use crate::context::{Permission, RequestContext};
use crate::error::{AppError, AppResult, Errors};
use crate::posting::{PostLine, Posting, post};
use crate::setup::{Change, audit, capitalise, check_len, clean, json};

const OTHER_COUNTRY: u8 = 96;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartyInput {
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub legal_name: Option<String>,
    pub gst_type: String,
    #[serde(default)]
    pub gstin: Option<String>,
    #[serde(default)]
    pub pan: Option<String>,
    /// Place of supply. Taken from the GSTIN when there is one.
    #[serde(default)]
    pub state_code: Option<u8>,
    #[serde(default)]
    pub address_line1: Option<String>,
    #[serde(default)]
    pub address_line2: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub pincode: Option<String>,
    #[serde(default = "yes")]
    pub shipping_same: bool,
    #[serde(default)]
    pub shipping_line1: Option<String>,
    #[serde(default)]
    pub shipping_line2: Option<String>,
    #[serde(default)]
    pub shipping_city: Option<String>,
    #[serde(default)]
    pub shipping_pincode: Option<String>,
    #[serde(default)]
    pub shipping_state_code: Option<u8>,
    #[serde(default)]
    pub contact_person: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub payment_terms_days: Option<i64>,
    /// Decimal rupees as a string.
    #[serde(default)]
    pub credit_limit: Option<String>,
    /// Decimal rupees as a string, always positive; the side says who owes whom.
    #[serde(default)]
    pub opening_balance: Option<String>,
    /// `receivable` (they owe us) or `payable` (we owe them).
    #[serde(default)]
    pub opening_balance_side: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default = "yes")]
    pub is_active: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemInput {
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub hsn_sac: Option<String>,
    /// GST UQC. Defaults to NOS for goods and OTH for services.
    #[serde(default)]
    pub unit: Option<String>,
    pub tax_category: String,
    /// Percent as a decimal string, e.g. "18" or "0.25".
    #[serde(default)]
    pub gst_rate: Option<String>,
    #[serde(default)]
    pub cess_rate: Option<String>,
    #[serde(default)]
    pub sales_price: Option<String>,
    #[serde(default)]
    pub purchase_price: Option<String>,
    #[serde(default)]
    pub price_includes_tax: bool,
    #[serde(default)]
    pub track_stock: bool,
    #[serde(default = "yes")]
    pub is_active: bool,
}

/// A party with its current ledger balance (paise; positive = they owe us).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartyView {
    #[serde(flatten)]
    pub party: PartyRecord,
    pub balance_paise: i64,
}

/// Choices for the item form, from the domain and rule data.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemOptions {
    pub units: Vec<UnitOption>,
    pub gst_rates: Vec<RateOption>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitOption {
    pub code: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RateOption {
    pub basis_points: u32,
    pub legacy: bool,
}

// ---------------------------------------------------------------------------
// Parsing helpers (shared with import)
// ---------------------------------------------------------------------------

/// Rupee amount → paise. Accepts "1,250.50" and "₹ 1250". `None` when blank.
pub(crate) fn parse_amount(
    e: &mut Errors,
    field: &str,
    s: &Option<String>,
    allow_negative: bool,
) -> Option<i64> {
    let raw = clean(s)?;
    let t = raw.replace(['₹', ' '], "");
    match Money::parse(&t).ok().and_then(|m| m.to_paise().ok()) {
        Some(p) if p < 0 && !allow_negative => {
            e.add(field, "Enter a positive amount.");
            None
        }
        Some(p) => Some(p),
        None => {
            e.add(field, "Enter an amount like 1250 or 1250.50.");
            None
        }
    }
}

/// Percentage → basis points ("18" or "18%" → 1800, "0.25" → 25).
pub(crate) fn parse_rate(e: &mut Errors, field: &str, s: &Option<String>) -> Option<u32> {
    let raw = clean(s)?;
    let t = raw.trim_end_matches('%').trim().to_owned();
    // Two decimal places of a percentage are exactly basis points.
    match Money::parse(&t)
        .ok()
        .and_then(|m| m.to_paise().ok())
        .and_then(|bp| u32::try_from(bp).ok())
    {
        Some(bp) => Some(bp),
        None => {
            e.add(field, "Enter a rate like 18 or 0.25.");
            None
        }
    }
}

fn parse_state(e: &mut Errors, field: &str, code: Option<u8>, required: bool) -> Option<StateCode> {
    match code {
        Some(c) => match StateCode::new(c) {
            Ok(s) => Some(s),
            Err(_) => {
                e.add(field, "Choose a state.");
                None
            }
        },
        None => {
            if required {
                e.add(field, "Choose a state.");
            }
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Party validation
// ---------------------------------------------------------------------------

/// Validate a party form. Returns a record with empty id/organisation fields.
pub(crate) fn validate_party(input: &PartyInput, prefix: &str) -> (Errors, Option<PartyRecord>) {
    let mut e = Errors::new(prefix);

    let kind = PartyKind::parse(input.kind.trim());
    if kind.is_none() {
        e.add("kind", "Choose customer, supplier or both.");
    }
    let name = input.name.trim().to_owned();
    if name.is_empty() {
        e.add("name", "Enter a name.");
    }
    check_len(&mut e, "name", &Some(name.clone()), 100);
    let legal_name = clean(&input.legal_name);
    check_len(&mut e, "legalName", &legal_name, 200);

    let gst_type = PartyGstType::parse(input.gst_type.trim());
    if gst_type.is_none() {
        e.add("gstType", "Choose the party's GST status.");
    }
    let gstin_raw = clean(&input.gstin);
    let mut gstin = None;
    match (gst_type, &gstin_raw) {
        (Some(t), None) if t.requires_gstin() => {
            e.add("gstin", "A GSTIN is required for a registered party.")
        }
        (Some(t), Some(_)) if t.forbids_gstin() => {
            e.add(
                "gstin",
                "Remove the GSTIN, or change the GST status to Registered.",
            );
        }
        (_, Some(raw)) => match Gstin::parse(raw) {
            Ok(g) => gstin = Some(g),
            Err(err) => e.add("gstin", format!("{}.", capitalise(&err.to_string()))),
        },
        _ => {}
    }

    // PAN: from the GSTIN when registered; otherwise optional.
    let pan = match &gstin {
        Some(g) => Some(g.pan().to_owned()),
        None => match clean(&input.pan) {
            Some(p) => match Pan::parse(&p) {
                Ok(p) => Some(p.as_str().to_owned()),
                Err(_) => {
                    e.add("pan", "Enter a valid 10-character PAN.");
                    None
                }
            },
            None => None,
        },
    };

    // Place of supply.
    let state = if let Some(g) = &gstin {
        Some(g.state())
    } else if gst_type == Some(PartyGstType::Overseas) {
        StateCode::new(OTHER_COUNTRY).ok()
    } else {
        let s = parse_state(&mut e, "stateCode", input.state_code, gst_type.is_some());
        if s.is_some_and(|s| s.code() == OTHER_COUNTRY) {
            e.add(
                "stateCode",
                "Use GST status Overseas for a party outside India.",
            );
        }
        s
    };

    let pincode = clean(&input.pincode);
    if pincode.as_deref().is_some_and(|p| !is_valid_pincode(p))
        && gst_type != Some(PartyGstType::Overseas)
    {
        e.add("pincode", "PIN code must be 6 digits.");
    }
    let (address_line1, address_line2, city) = (
        clean(&input.address_line1),
        clean(&input.address_line2),
        clean(&input.city),
    );
    check_len(&mut e, "addressLine1", &address_line1, 200);
    check_len(&mut e, "addressLine2", &address_line2, 200);
    check_len(&mut e, "city", &city, 80);

    let shipping_same = input.shipping_same;
    let (
        mut shipping_line1,
        mut shipping_line2,
        mut shipping_city,
        mut shipping_pincode,
        mut shipping_state,
    ) = (None, None, None, None, None);
    if !shipping_same {
        shipping_line1 = clean(&input.shipping_line1);
        shipping_line2 = clean(&input.shipping_line2);
        shipping_city = clean(&input.shipping_city);
        shipping_pincode = clean(&input.shipping_pincode);
        if shipping_line1.is_none() {
            e.add("shippingLine1", "Enter the shipping address.");
        }
        check_len(&mut e, "shippingLine1", &shipping_line1, 200);
        check_len(&mut e, "shippingLine2", &shipping_line2, 200);
        if shipping_pincode
            .as_deref()
            .is_some_and(|p| !is_valid_pincode(p))
        {
            e.add("shippingPincode", "PIN code must be 6 digits.");
        }
        shipping_state = parse_state(&mut e, "shippingStateCode", input.shipping_state_code, true);
    }

    let contact_person = clean(&input.contact_person);
    check_len(&mut e, "contactPerson", &contact_person, 100);
    let email = clean(&input.email);
    if email.as_deref().is_some_and(|m| !is_valid_email(m)) {
        e.add("email", "Enter a valid e-mail address.");
    }
    let phone = clean(&input.phone);
    if phone.as_deref().is_some_and(|p| !is_valid_phone(p)) {
        e.add("phone", "Enter a valid phone number.");
    }
    if input
        .payment_terms_days
        .is_some_and(|d| !(0..=365).contains(&d))
    {
        e.add("paymentTermsDays", "Use 0 to 365 days.");
    }
    let credit_limit_paise = parse_amount(&mut e, "creditLimit", &input.credit_limit, false);

    let opening =
        parse_amount(&mut e, "openingBalance", &input.opening_balance, false).unwrap_or(0);
    let side = clean(&input.opening_balance_side);
    let payable = match side.as_deref() {
        Some("payable") => true,
        Some("receivable") => false,
        None => kind == Some(PartyKind::Supplier),
        Some(_) => {
            e.add("openingBalanceSide", "Choose who owes whom.");
            false
        }
    };
    let opening_balance_paise = if payable { -opening } else { opening };

    let notes = clean(&input.notes);
    check_len(&mut e, "notes", &notes, 1000);

    let rec = match (kind, gst_type, state) {
        (Some(k), Some(t), Some(s)) if e.is_empty() => Some(PartyRecord {
            id: String::new(),
            organisation_id: String::new(),
            kind: k.as_str().into(),
            name,
            legal_name,
            gst_type: t.as_str().into(),
            gstin: gstin.map(|g| g.as_str().to_owned()),
            pan,
            state_code: i64::from(s.code()),
            address_line1,
            address_line2,
            city,
            pincode,
            shipping_same,
            shipping_line1,
            shipping_line2,
            shipping_city,
            shipping_pincode,
            shipping_state_code: shipping_state.map(|s| i64::from(s.code())),
            contact_person,
            email,
            phone,
            payment_terms_days: input.payment_terms_days,
            credit_limit_paise,
            opening_balance_paise,
            opening_entry_id: None,
            notes,
            is_active: input.is_active,
            row_version: 1,
            created_at: String::new(),
            updated_at: String::new(),
        }),
        _ => None,
    };
    (e, rec)
}

// ---------------------------------------------------------------------------
// Item validation
// ---------------------------------------------------------------------------

pub(crate) fn validate_item(input: &ItemInput, prefix: &str) -> (Errors, Option<ItemRecord>) {
    let mut e = Errors::new(prefix);
    let kind = ItemKind::parse(input.kind.trim());
    if kind.is_none() {
        e.add("kind", "Choose goods or service.");
    }
    let name = input.name.trim().to_owned();
    if name.is_empty() {
        e.add("name", "Enter a name.");
    }
    check_len(&mut e, "name", &Some(name.clone()), 120);
    let code = clean(&input.code);
    check_len(&mut e, "code", &code, 40);
    let description = clean(&input.description);
    check_len(&mut e, "description", &description, 500);

    let category = TaxCategory::parse(input.tax_category.trim());
    if category.is_none() {
        e.add("taxCategory", "Choose how GST applies.");
    }

    let hsn_sac = clean(&input.hsn_sac).map(|s| s.replace(' ', ""));
    match (&hsn_sac, kind) {
        (Some(code), Some(k)) => {
            if let Err(err) = validate_hsn_sac(code, k) {
                e.add("hsnSac", format!("{}.", capitalise(&err.to_string())));
            }
        }
        (None, Some(k)) if category != Some(TaxCategory::NonGst) => e.add(
            "hsnSac",
            if k == ItemKind::Goods {
                "Enter the HSN code."
            } else {
                "Enter the SAC code."
            },
        ),
        _ => {}
    }

    let unit = clean(&input.unit)
        .map(|u| u.to_ascii_uppercase())
        .unwrap_or_else(|| {
            if kind == Some(ItemKind::Service) {
                "OTH".into()
            } else {
                "NOS".into()
            }
        });
    if !is_valid_uqc(&unit) {
        e.add("unit", "Choose a GST unit (UQC).");
    }

    let gst_rate_bp = parse_rate(&mut e, "gstRate", &input.gst_rate);
    let gst_rate_bp = match category {
        Some(TaxCategory::Taxable) => match gst_rate_bp {
            Some(bp) if bp > 0 && TaxRate(bp).is_known() => bp,
            Some(_) => {
                e.add("gstRate", "Choose a GST rate.");
                0
            }
            None => {
                e.add("gstRate", "Choose a GST rate.");
                0
            }
        },
        // Exempt, nil-rated and non-GST supplies carry no tax.
        Some(_) => {
            if gst_rate_bp.is_some_and(|bp| bp > 0) {
                e.add(
                    "gstRate",
                    "Exempt, nil-rated and non-GST items have no GST rate.",
                );
            }
            0
        }
        None => 0,
    };
    let cess_rate_bp = parse_rate(&mut e, "cessRate", &input.cess_rate).unwrap_or(0);
    if cess_rate_bp > 50_000 {
        e.add("cessRate", "Cess can't exceed 500%.");
    }

    let sales_price_paise = parse_amount(&mut e, "salesPrice", &input.sales_price, false);
    let purchase_price_paise = parse_amount(&mut e, "purchasePrice", &input.purchase_price, false);
    if input.track_stock && kind == Some(ItemKind::Service) {
        e.add("trackStock", "Services don't carry stock.");
    }

    let rec = match (kind, category) {
        (Some(k), Some(c)) if e.is_empty() => Some(ItemRecord {
            id: String::new(),
            organisation_id: String::new(),
            kind: k.as_str().into(),
            name,
            code,
            description,
            hsn_sac,
            unit,
            tax_category: c.as_str().into(),
            gst_rate_bp: i64::from(gst_rate_bp),
            cess_rate_bp: i64::from(cess_rate_bp),
            sales_price_paise,
            purchase_price_paise,
            price_includes_tax: input.price_includes_tax,
            track_stock: input.track_stock,
            is_active: input.is_active,
            row_version: 1,
            created_at: String::new(),
            updated_at: String::new(),
        }),
        _ => None,
    };
    (e, rec)
}

// ---------------------------------------------------------------------------
// Opening balances
// ---------------------------------------------------------------------------

/// Ledger lines for a party's opening balance, as at the books start date.
/// Customers sit in Accounts Receivable, suppliers in Accounts Payable; a
/// party that is both goes by the sign. The other side is the Opening Balance
/// equity account.
fn opening_lines(p: &PartyRecord) -> Vec<PostLine> {
    let amount = p.opening_balance_paise;
    let control = match PartyKind::parse(&p.kind) {
        Some(PartyKind::Customer) => "receivables",
        Some(PartyKind::Supplier) => "payables",
        _ if amount >= 0 => "receivables",
        _ => "payables",
    };
    let party = Some(p.id.as_str());
    if amount > 0 {
        vec![
            PostLine::debit(control, amount, party),
            PostLine::credit("opening_balance", amount, None),
        ]
    } else {
        vec![
            PostLine::debit("opening_balance", -amount, None),
            PostLine::credit(control, -amount, party),
        ]
    }
}

/// Bring the posted opening entry in line with `after`: reverse the old entry
/// (if any) when the amount or ledger changed, then post the new one.
async fn sync_opening_entry(
    uow: &mut (impl JournalRepo + Send),
    ctx: &RequestContext,
    books_start: &str,
    before: Option<&PartyRecord>,
    after: &mut PartyRecord,
    now: &str,
) -> AppResult<()> {
    let unchanged = before.is_some_and(|b| {
        b.opening_balance_paise == after.opening_balance_paise && b.kind == after.kind
    });
    if unchanged {
        return Ok(());
    }
    if let Some(b) = before
        && let Some(old_id) = &b.opening_entry_id
    {
        post(
            uow,
            ctx,
            Posting {
                org_id: &after.organisation_id,
                entry_date: books_start,
                source_type: "party_opening_balance",
                source_id: Some(&after.id),
                narration: Some(format!("Reversal of opening balance — {}", b.name)),
                reverses_entry_id: Some(old_id),
                idempotency_key: None,
                lines: opening_lines(b).iter().map(PostLine::mirrored).collect(),
            },
            now,
        )
        .await?;
    }
    after.opening_entry_id = None;
    if after.opening_balance_paise != 0 {
        let id = post(
            uow,
            ctx,
            Posting {
                org_id: &after.organisation_id,
                entry_date: books_start,
                source_type: "party_opening_balance",
                source_id: Some(&after.id),
                narration: Some(format!("Opening balance — {}", after.name)),
                reverses_entry_id: None,
                idempotency_key: None,
                lines: opening_lines(after),
            },
            now,
        )
        .await?;
        after.opening_entry_id = Some(id);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Use cases
// ---------------------------------------------------------------------------

/// Uniqueness checks against stored parties.
pub(crate) async fn check_party_unique(
    uow: &mut (impl PartyRepo + Send),
    e: &mut Errors,
    org_id: &str,
    p: &PartyRecord,
    except_id: Option<&str>,
) -> AppResult<()> {
    if uow.party_name_taken(org_id, &p.name, except_id).await? {
        e.add("name", "A party with this name already exists.");
    }
    if let Some(g) = &p.gstin
        && uow.party_gstin_taken(org_id, g, except_id).await?
    {
        e.add(
            "gstin",
            "A party with this GSTIN already exists. Set its type to “Both” to buy and sell.",
        );
    }
    Ok(())
}

pub(crate) async fn check_item_unique(
    uow: &mut (impl ItemRepo + Send),
    e: &mut Errors,
    org_id: &str,
    i: &ItemRecord,
    except_id: Option<&str>,
) -> AppResult<()> {
    if uow.item_name_taken(org_id, &i.name, except_id).await? {
        e.add("name", "An item with this name already exists.");
    }
    if let Some(c) = &i.code
        && uow.item_code_taken(org_id, c, except_id).await?
    {
        e.add("code", "Another item already uses this code.");
    }
    Ok(())
}

/// Insert a validated party (and its opening entry) inside an open unit of work.
pub(crate) async fn insert_party<U: UnitOfWork>(
    uow: &mut U,
    ctx: &RequestContext,
    org_id: &str,
    books_start: &str,
    mut p: PartyRecord,
    now: &str,
) -> AppResult<PartyRecord> {
    p.id = new_id().to_string();
    p.organisation_id = org_id.to_owned();
    p.created_at = now.to_owned();
    p.updated_at = now.to_owned();
    // The party row must exist before journal lines can reference it, and the
    // entry must exist before the party can point at it.
    uow.insert_party(&p).await?;
    if p.opening_balance_paise != 0 {
        let mut no_opening = p.clone();
        no_opening.opening_balance_paise = 0;
        sync_opening_entry(uow, ctx, books_start, Some(&no_opening), &mut p, now).await?;
        if !uow.update_party(&p, 1).await? {
            return Err(AppError::stale());
        }
        p.row_version = 2;
    }
    let c = Change {
        action: "party.create",
        entity_id: Some(&p.id),
        before: None,
        after: json(&p),
    };
    audit(uow, ctx, org_id, c, now).await?;
    Ok(p)
}

pub(crate) async fn insert_item<U: UnitOfWork>(
    uow: &mut U,
    ctx: &RequestContext,
    org_id: &str,
    mut i: ItemRecord,
    now: &str,
) -> AppResult<ItemRecord> {
    i.id = new_id().to_string();
    i.organisation_id = org_id.to_owned();
    i.created_at = now.to_owned();
    i.updated_at = now.to_owned();
    uow.insert_item(&i).await?;
    let c = Change {
        action: "item.create",
        entity_id: Some(&i.id),
        before: None,
        after: json(&i),
    };
    audit(uow, ctx, org_id, c, now).await?;
    Ok(i)
}

impl<S: Store> AppCore<S> {
    pub async fn list_parties(&self, ctx: &RequestContext) -> AppResult<Vec<PartyView>> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let Some(org) = self.find_org(ctx, &mut uow).await? else {
            return Ok(Vec::new());
        };
        let balances: std::collections::HashMap<String, i64> =
            uow.party_balances(&org.id).await?.into_iter().collect();
        Ok(uow
            .list_parties(&org.id)
            .await?
            .into_iter()
            .map(|p| PartyView {
                balance_paise: balances.get(&p.id).copied().unwrap_or(0),
                party: p,
            })
            .collect())
    }

    pub async fn create_party(
        &self,
        ctx: &RequestContext,
        input: PartyInput,
    ) -> AppResult<PartyView> {
        ctx.require(Permission::ManageMasters)?;
        let (mut e, rec) = validate_party(&input, "");
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        if let Some(p) = &rec {
            check_party_unique(&mut uow, &mut e, &org.id, p, None).await?;
        }
        e.finish()?;
        let now = self.now();
        let p = insert_party(
            &mut uow,
            ctx,
            &org.id,
            &org.books_start_date,
            rec.expect("validated"),
            &now,
        )
        .await?;
        uow.commit().await?;
        let balance_paise = p.opening_balance_paise;
        Ok(PartyView {
            party: p,
            balance_paise,
        })
    }

    pub async fn update_party(
        &self,
        ctx: &RequestContext,
        id: &str,
        input: PartyInput,
        expected_version: i64,
    ) -> AppResult<PartyView> {
        ctx.require(Permission::ManageMasters)?;
        let (mut e, rec) = validate_party(&input, "");
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let before = uow
            .get_party(&org.id, id)
            .await?
            .ok_or(AppError::NotFound("Party"))?;
        if let Some(p) = &rec {
            check_party_unique(&mut uow, &mut e, &org.id, p, Some(id)).await?;
        }
        e.finish()?;
        if before.row_version != expected_version {
            return Err(AppError::stale());
        }

        let now = self.now();
        let mut p = rec.expect("validated");
        p.id.clone_from(&before.id);
        p.organisation_id.clone_from(&org.id);
        p.created_at.clone_from(&before.created_at);
        p.updated_at.clone_from(&now);
        p.opening_entry_id.clone_from(&before.opening_entry_id);
        sync_opening_entry(
            &mut uow,
            ctx,
            &org.books_start_date,
            Some(&before),
            &mut p,
            &now,
        )
        .await?;
        if !uow.update_party(&p, expected_version).await? {
            return Err(AppError::stale());
        }
        p.row_version = expected_version + 1;
        let c = Change {
            action: "party.update",
            entity_id: Some(id),
            before: json(&before),
            after: json(&p),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        let balance_paise = uow
            .party_balances(&org.id)
            .await?
            .into_iter()
            .find(|(pid, _)| pid == id)
            .map_or(0, |(_, b)| b);
        uow.commit().await?;
        Ok(PartyView {
            party: p,
            balance_paise,
        })
    }

    pub fn item_options(&self) -> ItemOptions {
        ItemOptions {
            units: UQC
                .iter()
                .map(|(code, label)| UnitOption { code, label })
                .collect(),
            gst_rates: av_gst_engine::KNOWN_RATES
                .iter()
                .map(|(bp, legacy)| RateOption {
                    basis_points: *bp,
                    legacy: *legacy,
                })
                .collect(),
        }
    }

    pub async fn list_items(&self, ctx: &RequestContext) -> AppResult<Vec<ItemRecord>> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let Some(org) = self.find_org(ctx, &mut uow).await? else {
            return Ok(Vec::new());
        };
        Ok(uow.list_items(&org.id).await?)
    }

    pub async fn create_item(
        &self,
        ctx: &RequestContext,
        input: ItemInput,
    ) -> AppResult<ItemRecord> {
        ctx.require(Permission::ManageMasters)?;
        let (mut e, rec) = validate_item(&input, "");
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        if let Some(i) = &rec {
            check_item_unique(&mut uow, &mut e, &org.id, i, None).await?;
        }
        e.finish()?;
        let now = self.now();
        let i = insert_item(&mut uow, ctx, &org.id, rec.expect("validated"), &now).await?;
        uow.commit().await?;
        Ok(i)
    }

    pub async fn update_item(
        &self,
        ctx: &RequestContext,
        id: &str,
        input: ItemInput,
        expected_version: i64,
    ) -> AppResult<ItemRecord> {
        ctx.require(Permission::ManageMasters)?;
        let (mut e, rec) = validate_item(&input, "");
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let before = uow
            .get_item(&org.id, id)
            .await?
            .ok_or(AppError::NotFound("Item"))?;
        if let Some(i) = &rec {
            check_item_unique(&mut uow, &mut e, &org.id, i, Some(id)).await?;
        }
        e.finish()?;
        let now = self.now();
        let mut i = rec.expect("validated");
        i.id.clone_from(&before.id);
        i.organisation_id.clone_from(&org.id);
        i.created_at.clone_from(&before.created_at);
        i.updated_at.clone_from(&now);
        if !uow.update_item(&i, expected_version).await? {
            return Err(AppError::stale());
        }
        i.row_version = expected_version + 1;
        let c = Change {
            action: "item.update",
            entity_id: Some(id),
            before: json(&before),
            after: json(&i),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        uow.commit().await?;
        Ok(i)
    }
}
