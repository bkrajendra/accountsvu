//! Phase 1.1 use cases: business profile, GST registrations, chart of
//! accounts and document number series. Every change writes an audit event
//! in the same transaction (spec §7.6).

use av_accounting::chart::DEFAULT_CHART;
use av_domain::business::{is_valid_email, is_valid_phone, is_valid_pincode};
use av_domain::{
    Constitution, DocumentType, FilingFrequency, FinancialYear, Gstin, Pan, RegistrationType,
    SeriesFormat, StateCode, new_id,
};
use av_persistence::records::{
    AccountRecord, AuditRecord, OrganisationRecord, RegistrationRecord, SeriesRecord,
};
use av_persistence::{
    AccountRepo, AuditRepo, LedgerQueries, OrganisationRepo, RegistrationRepo, SeriesRepo, Store,
    UnitOfWork,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::AppCore;
use crate::context::{OrgScope, Permission, RequestContext};
use crate::error::{AppError, AppResult, Errors};
const DEFAULT_PADDING: u8 = 4;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganisationInput {
    pub legal_name: String,
    #[serde(default)]
    pub trade_name: Option<String>,
    #[serde(default)]
    pub pan: Option<String>,
    pub constitution: String,
    #[serde(default)]
    pub address_line1: Option<String>,
    #[serde(default)]
    pub address_line2: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub pincode: Option<String>,
    pub state_code: u8,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default = "default_fy_start")]
    pub fy_start_month: u32,
    pub books_start_date: String,
    #[serde(default = "yes")]
    pub round_to_rupee: bool,
}

fn default_fy_start() -> u32 {
    4
}
fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrationInput {
    pub gstin: String,
    #[serde(default = "default_reg_type")]
    pub registration_type: String,
    pub filing_frequency: String,
    pub effective_from: String,
    #[serde(default)]
    pub signatory_name: Option<String>,
}

fn default_reg_type() -> String {
    "regular".into()
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupBusinessInput {
    pub organisation: OrganisationInput,
    #[serde(default)]
    pub registration: Option<RegistrationInput>,
    /// Base for the tax-invoice series, e.g. `INV` → `INV/26-27/0001`.
    #[serde(default)]
    pub invoice_prefix: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRegistrationInput {
    pub registration_type: String,
    pub filing_frequency: String,
    pub effective_from: String,
    #[serde(default)]
    pub effective_to: Option<String>,
    #[serde(default)]
    pub signatory_name: Option<String>,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesInput {
    #[serde(default)]
    pub gst_registration_id: Option<String>,
    pub document_type: String,
    pub financial_year: String,
    pub prefix: String,
    #[serde(default)]
    pub suffix: String,
    #[serde(default = "default_padding")]
    pub padding: u8,
    #[serde(default = "one")]
    pub next_number: i64,
    #[serde(default)]
    pub is_default: bool,
}

fn default_padding() -> u8 {
    DEFAULT_PADDING
}
fn one() -> i64 {
    1
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesUpdateInput {
    pub prefix: String,
    #[serde(default)]
    pub suffix: String,
    pub padding: u8,
    pub next_number: i64,
    pub is_default: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinancialYearInfo {
    pub code: String,
    pub label: String,
    pub start_date: String,
    pub end_date: String,
}

impl From<FinancialYear> for FinancialYearInfo {
    fn from(fy: FinancialYear) -> Self {
        FinancialYearInfo {
            code: fy.code(),
            label: fy.to_string(),
            start_date: fy.start_date().to_string(),
            end_date: fy.end_date().to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BusinessProfile {
    pub organisation: Option<OrganisationRecord>,
    pub registrations: Vec<RegistrationRecord>,
    pub current_financial_year: FinancialYearInfo,
    pub next_financial_year: FinancialYearInfo,
    pub account_count: usize,
    pub has_postings: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesView {
    #[serde(flatten)]
    pub series: SeriesRecord,
    pub gstin: Option<String>,
    /// The number the next document will receive.
    pub next_preview: String,
    /// Prefix/suffix/padding are locked once a number has been issued.
    pub locked: bool,
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

pub(crate) fn clean(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

pub(crate) fn parse_date(e: &mut Errors, field: &str, s: &str) -> Option<NaiveDate> {
    match NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d") {
        Ok(d) => Some(d),
        Err(_) => {
            e.add(field, "Enter a valid date.");
            None
        }
    }
}

pub(crate) fn check_len(e: &mut Errors, field: &str, v: &Option<String>, max: usize) {
    if v.as_ref().is_some_and(|s| s.chars().count() > max) {
        e.add(field, format!("Keep this under {max} characters."));
    }
}

/// Validate organisation input into a record (id/timestamps filled by caller).
fn validate_organisation(input: &OrganisationInput, prefix: &str) -> (Errors, OrganisationRecord) {
    let mut e = Errors::new(prefix);
    let legal_name = input.legal_name.trim().to_owned();
    if legal_name.is_empty() {
        e.add("legalName", "Enter the legal name of the business.");
    } else if legal_name.chars().count() > 200 {
        e.add("legalName", "Keep this under 200 characters.");
    }
    let trade_name = clean(&input.trade_name);
    check_len(&mut e, "tradeName", &trade_name, 200);

    let constitution = Constitution::parse(&input.constitution);
    if constitution.is_none() {
        e.add("constitution", "Choose the type of business.");
    }

    let pan = match clean(&input.pan) {
        None => None,
        Some(raw) => match Pan::parse(&raw) {
            Ok(p) => {
                if let Some(expected) = constitution.and_then(Constitution::expected_pan_types)
                    && !expected.contains(&p.holder_type())
                {
                    e.add(
                        "pan",
                        format!(
                            "The 4th letter of this PAN ({}) doesn't match the business type you chose. Check both.",
                            p.holder_type()
                        ),
                    );
                }
                Some(p.as_str().to_owned())
            }
            Err(err) => {
                e.add("pan", err.to_string());
                None
            }
        },
    };

    if StateCode::new(input.state_code).is_err() || input.state_code == 96 {
        e.add(
            "stateCode",
            "Choose the state of the principal place of business.",
        );
    }
    let pincode = clean(&input.pincode);
    if pincode.as_deref().is_some_and(|p| !is_valid_pincode(p)) {
        e.add("pincode", "PIN code must be 6 digits.");
    }
    let email = clean(&input.email);
    if email.as_deref().is_some_and(|m| !is_valid_email(m)) {
        e.add("email", "Enter a valid e-mail address.");
    }
    let phone = clean(&input.phone);
    if phone.as_deref().is_some_and(|p| !is_valid_phone(p)) {
        e.add("phone", "Enter a valid phone number.");
    }
    let address_line1 = clean(&input.address_line1);
    let address_line2 = clean(&input.address_line2);
    let city = clean(&input.city);
    check_len(&mut e, "addressLine1", &address_line1, 200);
    check_len(&mut e, "addressLine2", &address_line2, 200);
    check_len(&mut e, "city", &city, 100);

    if !(1..=12).contains(&input.fy_start_month) {
        e.add(
            "fyStartMonth",
            "Choose the month your financial year starts.",
        );
    }
    parse_date(&mut e, "booksStartDate", &input.books_start_date);

    let rec = OrganisationRecord {
        id: String::new(),
        legal_name,
        trade_name,
        pan,
        constitution: constitution.map(|c| c.as_str().to_owned()),
        address_line1,
        address_line2,
        city,
        pincode,
        state_code: i64::from(input.state_code),
        email,
        phone,
        fy_start_month: i64::from(input.fy_start_month),
        books_start_date: input.books_start_date.trim().to_owned(),
        round_to_rupee: input.round_to_rupee,
        row_version: 1,
        created_at: String::new(),
        updated_at: String::new(),
    };
    (e, rec)
}

fn parse_registration_type(e: &mut Errors, s: &str) -> Option<RegistrationType> {
    match RegistrationType::parse(s) {
        Some(RegistrationType::Composition) => {
            e.add("registrationType", "Composition taxpayers aren't supported yet. Only regular registrations can be added.");
            None
        }
        Some(t) => Some(t),
        None => {
            e.add("registrationType", "Choose a registration type.");
            None
        }
    }
}

fn parse_frequency(e: &mut Errors, s: &str) -> Option<FilingFrequency> {
    let f = FilingFrequency::parse(s);
    if f.is_none() {
        e.add("filingFrequency", "Choose monthly or quarterly (QRMP).");
    }
    f
}

/// Validate a new registration. `org_pan` is the business PAN, if known.
fn validate_registration(
    input: &RegistrationInput,
    org_pan: Option<&str>,
    prefix: &str,
) -> (Errors, Option<RegistrationRecord>) {
    let mut e = Errors::new(prefix);
    let gstin = match Gstin::parse(&input.gstin) {
        Ok(g) => {
            if let Some(pan) = org_pan
                && g.pan() != pan
            {
                e.add(
                    "gstin",
                    format!(
                        "This GSTIN belongs to PAN {}, but the business PAN is {pan}.",
                        g.pan()
                    ),
                );
            }
            Some(g)
        }
        Err(err) => {
            e.add("gstin", format!("{}.", capitalise(&err.to_string())));
            None
        }
    };
    let reg_type = parse_registration_type(&mut e, &input.registration_type);
    let freq = parse_frequency(&mut e, &input.filing_frequency);
    parse_date(&mut e, "effectiveFrom", &input.effective_from);
    let signatory_name = clean(&input.signatory_name);
    check_len(&mut e, "signatoryName", &signatory_name, 100);

    let rec = match (gstin, reg_type, freq) {
        (Some(g), Some(t), Some(f)) if e.is_empty() => Some(RegistrationRecord {
            id: new_id().to_string(),
            organisation_id: String::new(),
            gstin: g.as_str().to_owned(),
            state_code: i64::from(g.state().code()),
            registration_type: t.as_str().to_owned(),
            filing_frequency: f.as_str().to_owned(),
            signatory_name,
            effective_from: input.effective_from.trim().to_owned(),
            effective_to: None,
            status: "active".into(),
            row_version: 1,
            created_at: String::new(),
            updated_at: None,
        }),
        _ => None,
    };
    (e, rec)
}

pub(crate) fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

pub(crate) fn json<T: Serialize>(v: &T) -> Option<String> {
    serde_json::to_string(v).ok()
}

/// What changed, for the audit log. The entity type is the part of `action`
/// before the dot.
pub(crate) struct Change<'a> {
    pub(crate) action: &'a str,
    pub(crate) entity_id: Option<&'a str>,
    pub(crate) before: Option<String>,
    pub(crate) after: Option<String>,
}

/// Record an audit event in the same unit of work as the change itself.
pub(crate) async fn audit(
    uow: &mut (impl AuditRepo + Send),
    ctx: &RequestContext,
    org_id: &str,
    c: Change<'_>,
    now: &str,
) -> AppResult<()> {
    let Change {
        action,
        entity_id,
        before,
        after,
    } = c;
    uow.insert_audit(&AuditRecord {
        id: new_id().to_string(),
        organisation_id: Some(org_id.to_owned()),
        occurred_at: now.to_owned(),
        actor: ctx.actor.user_id.clone(),
        action: action.into(),
        entity_type: action.split('.').next().unwrap_or(action).into(),
        entity_id: entity_id.map(str::to_owned),
        reason: None,
        before_json: before,
        after_json: after,
    })
    .await?;
    Ok(())
}

/// Default series created for a new GSTIN (or an unregistered business).
fn default_series_plan(registered: bool, invoice_base: &str) -> Vec<(DocumentType, String)> {
    if registered {
        vec![
            (DocumentType::TaxInvoice, invoice_base.to_owned()),
            (
                DocumentType::BillOfSupply,
                DocumentType::BillOfSupply.default_prefix().to_owned(),
            ),
            (
                DocumentType::CreditNote,
                DocumentType::CreditNote.default_prefix().to_owned(),
            ),
            (
                DocumentType::DebitNote,
                DocumentType::DebitNote.default_prefix().to_owned(),
            ),
        ]
    } else {
        vec![(DocumentType::BillOfSupply, invoice_base.to_owned())]
    }
}

fn series_prefix(base: &str, fy: FinancialYear) -> String {
    format!("{base}/{}/", fy.short_code())
}

async fn create_default_series(
    uow: &mut (impl SeriesRepo + Send),
    org_id: &str,
    registration_id: Option<&str>,
    fy: FinancialYear,
    invoice_base: &str,
    now: &str,
) -> AppResult<Vec<SeriesRecord>> {
    let mut created = Vec::new();
    for (doc, base) in default_series_plan(registration_id.is_some(), invoice_base) {
        let fmt = SeriesFormat::new(
            &series_prefix(&base, fy),
            "",
            DEFAULT_PADDING,
            doc.is_gst_document(),
        )
        .map_err(|err| AppError::Invalid(err.to_string()))?;
        if uow
            .series_scope_taken(
                org_id,
                registration_id,
                &fy.code(),
                &fmt.prefix,
                &fmt.suffix,
                None,
            )
            .await?
        {
            continue; // never collide with a series the user already made
        }
        let s = SeriesRecord {
            id: new_id().to_string(),
            organisation_id: org_id.to_owned(),
            gst_registration_id: registration_id.map(str::to_owned),
            document_type: doc.as_str().to_owned(),
            financial_year: fy.code(),
            prefix: fmt.prefix,
            suffix: fmt.suffix,
            padding: i64::from(DEFAULT_PADDING),
            next_number: 1,
            is_default: true,
            row_version: 1,
            created_at: now.to_owned(),
            updated_at: now.to_owned(),
        };
        uow.clear_default_series(&s, now).await?;
        uow.insert_series(&s).await?;
        created.push(s);
    }
    Ok(created)
}

// ---------------------------------------------------------------------------
// Use cases
// ---------------------------------------------------------------------------

impl<S: Store> AppCore<S> {
    fn fy_start_month(org: Option<&OrganisationRecord>) -> u32 {
        org.and_then(|o| u32::try_from(o.fy_start_month).ok())
            .filter(|m| (1..=12).contains(m))
            .unwrap_or(4)
    }

    fn current_fy(&self, org: Option<&OrganisationRecord>) -> FinancialYear {
        FinancialYear::containing(self.today(), Self::fy_start_month(org))
    }

    pub async fn business_profile(&self, ctx: &RequestContext) -> AppResult<BusinessProfile> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let organisation = self.find_org(ctx, &mut uow).await?;
        let fy = self.current_fy(organisation.as_ref());
        let (registrations, account_count, has_postings) = match &organisation {
            Some(o) => (
                uow.list_registrations(&o.id).await?,
                uow.list_accounts(&o.id).await?.len(),
                uow.journal_entry_count(&o.id).await? > 0,
            ),
            None => (Vec::new(), 0, false),
        };
        Ok(BusinessProfile {
            registrations,
            account_count,
            has_postings,
            current_financial_year: fy.into(),
            next_financial_year: fy.next().into(),
            organisation,
        })
    }

    /// First-run onboarding: organisation, optional first GSTIN, default chart
    /// of accounts and number series — all in one transaction.
    pub async fn setup_business(
        &self,
        ctx: &RequestContext,
        input: SetupBusinessInput,
    ) -> AppResult<BusinessProfile> {
        ctx.require(Permission::ManageBusiness)?;
        let (mut errors, mut org) = validate_organisation(&input.organisation, "organisation.");

        let mut reg = None;
        if let Some(r) = &input.registration {
            let (re, rec) = validate_registration(r, org.pan.as_deref(), "registration.");
            errors.absorb(re);
            reg = rec;
        }

        let fy = FinancialYear::containing(self.today(), Self::fy_start_month(Some(&org)));
        let base = clean(&input.invoice_prefix)
            .unwrap_or_else(|| "INV".into())
            .to_ascii_uppercase();
        if SeriesFormat::new(&series_prefix(&base, fy), "", DEFAULT_PADDING, true).is_err()
            || base.contains('/')
        {
            let mut top = Errors::new("");
            top.add("invoicePrefix", "Use up to 5 letters or digits (e.g. INV).");
            errors.absorb(top);
        }
        errors.finish()?;

        let now = self.now();
        org.id = match &ctx.scope {
            OrgScope::Install => new_id().to_string(),
            OrgScope::Organisation(id) => id.clone(),
        };
        org.created_at.clone_from(&now);
        org.updated_at.clone_from(&now);
        if org.pan.is_none()
            && let Some(r) = &reg
        {
            org.pan = Some(r.gstin[2..12].to_owned()); // the PAN is embedded in the GSTIN
        }

        let mut tx = self.store.begin().await?;
        if self.find_org(ctx, &mut tx).await?.is_some() {
            return Err(AppError::AlreadySetUp);
        }
        tx.insert_organisation(&org).await?;
        let created = Change {
            action: "organisation.create",
            entity_id: Some(&org.id),
            before: None,
            after: json(&org),
        };
        audit(&mut tx, ctx, &org.id, created, &now).await?;

        for a in DEFAULT_CHART {
            tx.insert_account(&AccountRecord {
                id: new_id().to_string(),
                organisation_id: org.id.clone(),
                code: a.code.into(),
                name: a.name.into(),
                kind: a.kind.as_str().into(),
                system_role: a.role.map(str::to_owned),
                is_active: true,
                created_at: now.clone(),
            })
            .await?;
        }
        let seeded = Change {
            action: "chart_of_accounts.seed",
            entity_id: None,
            before: None,
            after: Some(format!("{{\"accounts\":{}}}", DEFAULT_CHART.len())),
        };
        audit(&mut tx, ctx, &org.id, seeded, &now).await?;
        crate::banking::seed_money_accounts(&mut tx, &org.id, &now).await?;

        let reg_id = if let Some(mut r) = reg {
            r.organisation_id.clone_from(&org.id);
            r.created_at.clone_from(&now);
            tx.insert_registration(&r).await?;
            let c = Change {
                action: "gst_registration.create",
                entity_id: Some(&r.id),
                before: None,
                after: json(&r),
            };
            audit(&mut tx, ctx, &org.id, c, &now).await?;
            Some(r.id)
        } else {
            None
        };

        for s in create_default_series(&mut tx, &org.id, reg_id.as_deref(), fy, &base, &now).await?
        {
            let c = Change {
                action: "invoice_series.create",
                entity_id: Some(&s.id),
                before: None,
                after: json(&s),
            };
            audit(&mut tx, ctx, &org.id, c, &now).await?;
        }
        tx.commit().await?;
        self.business_profile(ctx).await
    }

    pub async fn update_organisation(
        &self,
        ctx: &RequestContext,
        input: OrganisationInput,
        expected_version: i64,
    ) -> AppResult<BusinessProfile> {
        ctx.require(Permission::ManageBusiness)?;
        let (mut errors, mut org) = validate_organisation(&input, "");
        let mut tx = self.store.begin().await?;
        let before = self.require_org(ctx, &mut tx).await?;

        let registrations = tx.list_registrations(&before.id).await?;
        if let Some(pan) = &org.pan
            && registrations.iter().any(|r| &r.gstin[2..12] != pan)
        {
            errors.add("pan", "The PAN must match the PAN inside your GSTINs.");
        }
        if org.fy_start_month != before.fy_start_month
            && tx.journal_entry_count(&before.id).await? > 0
        {
            errors.add(
                "fyStartMonth",
                "The financial year start can't change after transactions are posted.",
            );
        }
        errors.finish()?;

        org.id.clone_from(&before.id);
        org.created_at.clone_from(&before.created_at);
        org.updated_at = self.now();
        if !tx.update_organisation(&org, expected_version).await? {
            return Err(AppError::stale());
        }
        org.row_version = expected_version + 1;
        let c = Change {
            action: "organisation.update",
            entity_id: Some(&org.id),
            before: json(&before),
            after: json(&org),
        };
        audit(&mut tx, ctx, &org.id, c, &org.updated_at).await?;
        tx.commit().await?;
        self.business_profile(ctx).await
    }

    pub async fn add_gst_registration(
        &self,
        ctx: &RequestContext,
        input: RegistrationInput,
    ) -> AppResult<BusinessProfile> {
        ctx.require(Permission::ManageBusiness)?;
        let mut tx = self.store.begin().await?;
        let org = self.require_org(ctx, &mut tx).await?;
        let (mut errors, rec) = validate_registration(&input, org.pan.as_deref(), "");
        if let Some(r) = &rec
            && tx.gstin_exists(&org.id, &r.gstin).await?
        {
            errors.add("gstin", "This GSTIN is already added.");
        }
        errors.finish()?;
        let mut r = rec.expect("validated");
        let now = self.now();
        r.organisation_id.clone_from(&org.id);
        r.created_at.clone_from(&now);
        tx.insert_registration(&r).await?;
        let c = Change {
            action: "gst_registration.create",
            entity_id: Some(&r.id),
            before: None,
            after: json(&r),
        };
        audit(&mut tx, ctx, &org.id, c, &now).await?;

        let fy = self.current_fy(Some(&org));
        for s in create_default_series(&mut tx, &org.id, Some(&r.id), fy, "INV", &now).await? {
            let c = Change {
                action: "invoice_series.create",
                entity_id: Some(&s.id),
                before: None,
                after: json(&s),
            };
            audit(&mut tx, ctx, &org.id, c, &now).await?;
        }
        tx.commit().await?;
        self.business_profile(ctx).await
    }

    pub async fn update_gst_registration(
        &self,
        ctx: &RequestContext,
        id: &str,
        input: UpdateRegistrationInput,
        expected_version: i64,
    ) -> AppResult<BusinessProfile> {
        ctx.require(Permission::ManageBusiness)?;
        let mut e = Errors::new("");
        let reg_type = parse_registration_type(&mut e, &input.registration_type);
        let freq = parse_frequency(&mut e, &input.filing_frequency);
        let from = parse_date(&mut e, "effectiveFrom", &input.effective_from);
        let effective_to = clean(&input.effective_to);
        if let Some(to) = &effective_to
            && let (Some(from), Some(to)) = (from, parse_date(&mut e, "effectiveTo", to))
            && to < from
        {
            e.add("effectiveTo", "The end date must be after the start date.");
        }
        if !matches!(input.status.as_str(), "active" | "cancelled" | "suspended") {
            e.add("status", "Choose a status.");
        }
        let signatory_name = clean(&input.signatory_name);
        check_len(&mut e, "signatoryName", &signatory_name, 100);
        e.finish()?;

        let mut tx = self.store.begin().await?;
        let org = self.require_org(ctx, &mut tx).await?;
        let before = tx
            .get_registration(&org.id, id)
            .await?
            .ok_or(AppError::NotFound("GST registration"))?;
        let mut r = before.clone();
        r.registration_type = reg_type.expect("validated").as_str().into();
        r.filing_frequency = freq.expect("validated").as_str().into();
        r.effective_from = input.effective_from.trim().into();
        r.effective_to = effective_to;
        r.signatory_name = signatory_name;
        r.status.clone_from(&input.status);
        let now = self.now();
        r.updated_at = Some(now.clone());
        if !tx.update_registration(&r, expected_version).await? {
            return Err(AppError::stale());
        }
        r.row_version = expected_version + 1;
        let c = Change {
            action: "gst_registration.update",
            entity_id: Some(id),
            before: json(&before),
            after: json(&r),
        };
        audit(&mut tx, ctx, &org.id, c, &now).await?;
        tx.commit().await?;
        self.business_profile(ctx).await
    }

    pub async fn list_accounts(&self, ctx: &RequestContext) -> AppResult<Vec<AccountRecord>> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let Some(org) = self.find_org(ctx, &mut uow).await? else {
            return Ok(Vec::new());
        };
        Ok(uow.list_accounts(&org.id).await?)
    }

    pub async fn list_series(&self, ctx: &RequestContext) -> AppResult<Vec<SeriesView>> {
        ctx.require(Permission::ViewBusiness)?;
        let mut uow = self.store.begin().await?;
        let Some(org) = self.find_org(ctx, &mut uow).await? else {
            return Ok(Vec::new());
        };
        let regs = uow.list_registrations(&org.id).await?;
        let series = uow.list_series(&org.id).await?;
        Ok(series.into_iter().map(|s| series_view(s, &regs)).collect())
    }

    pub async fn create_series(
        &self,
        ctx: &RequestContext,
        input: SeriesInput,
    ) -> AppResult<Vec<SeriesView>> {
        ctx.require(Permission::ManageBusiness)?;
        let mut e = Errors::new("");
        let doc = DocumentType::parse(&input.document_type);
        if doc.is_none() {
            e.add("documentType", "Choose a document type.");
        }
        let fmt = match SeriesFormat::new(
            &input.prefix,
            &input.suffix,
            input.padding,
            doc.is_none_or(|d| d.is_gst_document()),
        ) {
            Ok(f) => Some(f),
            Err(err) => {
                e.add("prefix", format!("{}.", capitalise(&err.to_string())));
                None
            }
        };
        if input.next_number < 1 {
            e.add("nextNumber", "Start from 1 or higher.");
        }

        let mut tx = self.store.begin().await?;
        let org = self.require_org(ctx, &mut tx).await?;
        let fy = self.current_fy(Some(&org));
        if input.financial_year != fy.code() && input.financial_year != fy.next().code() {
            e.add(
                "financialYear",
                format!("Choose {} or {}.", fy.code(), fy.next().code()),
            );
        }
        let reg_id = clean(&input.gst_registration_id);
        if let Some(id) = &reg_id
            && tx.get_registration(&org.id, id).await?.is_none()
        {
            e.add("gstRegistrationId", "Choose a GSTIN.");
        }
        if let Some(f) = &fmt
            && tx
                .series_scope_taken(
                    &org.id,
                    reg_id.as_deref(),
                    &input.financial_year,
                    &f.prefix,
                    &f.suffix,
                    None,
                )
                .await?
        {
            e.add(
                "prefix",
                "Another series for this GSTIN and year already uses this prefix.",
            );
        }
        if let (Some(f), Some(d)) = (&fmt, doc)
            && d.is_gst_document()
            && u64::try_from(input.next_number).is_ok_and(|n| !f.fits(n))
        {
            e.add(
                "nextNumber",
                "This number would make document numbers longer than 16 characters.",
            );
        }
        e.finish()?;

        let (f, d) = (fmt.expect("validated"), doc.expect("validated"));
        let now = self.now();
        let s = SeriesRecord {
            id: new_id().to_string(),
            organisation_id: org.id.clone(),
            gst_registration_id: reg_id,
            document_type: d.as_str().into(),
            financial_year: input.financial_year,
            prefix: f.prefix,
            suffix: f.suffix,
            padding: i64::from(f.padding),
            next_number: input.next_number,
            is_default: input.is_default,
            row_version: 1,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        if s.is_default {
            tx.clear_default_series(&s, &now).await?;
        }
        tx.insert_series(&s).await?;
        let c = Change {
            action: "invoice_series.create",
            entity_id: Some(&s.id),
            before: None,
            after: json(&s),
        };
        audit(&mut tx, ctx, &org.id, c, &now).await?;
        tx.commit().await?;
        self.list_series(ctx).await
    }

    pub async fn update_series(
        &self,
        ctx: &RequestContext,
        id: &str,
        input: SeriesUpdateInput,
        expected_version: i64,
    ) -> AppResult<Vec<SeriesView>> {
        ctx.require(Permission::ManageBusiness)?;
        let mut tx = self.store.begin().await?;
        let org = self.require_org(ctx, &mut tx).await?;
        let before = tx
            .get_series(&org.id, id)
            .await?
            .ok_or(AppError::NotFound("Number series"))?;
        let doc = DocumentType::parse(&before.document_type)
            .ok_or_else(|| AppError::Invalid("unknown document type".into()))?;

        let mut e = Errors::new("");
        let fmt = match SeriesFormat::new(
            &input.prefix,
            &input.suffix,
            input.padding,
            doc.is_gst_document(),
        ) {
            Ok(f) => Some(f),
            Err(err) => {
                e.add("prefix", format!("{}.", capitalise(&err.to_string())));
                None
            }
        };
        let issued = before.next_number > 1;
        if let Some(f) = &fmt {
            let changed = f.prefix != before.prefix
                || f.suffix != before.suffix
                || i64::from(f.padding) != before.padding;
            if issued && changed {
                e.add("prefix", "Numbers have already been issued from this series. Create a new series instead.");
            } else if tx
                .series_scope_taken(
                    &org.id,
                    before.gst_registration_id.as_deref(),
                    &before.financial_year,
                    &f.prefix,
                    &f.suffix,
                    Some(id),
                )
                .await?
            {
                e.add(
                    "prefix",
                    "Another series for this GSTIN and year already uses this prefix.",
                );
            }
            if doc.is_gst_document() && u64::try_from(input.next_number).is_ok_and(|n| !f.fits(n)) {
                e.add(
                    "nextNumber",
                    "This number would make document numbers longer than 16 characters.",
                );
            }
        }
        if input.next_number < before.next_number {
            e.add(
                "nextNumber",
                format!(
                    "The next number can't go below {} — that could repeat a number already used.",
                    before.next_number
                ),
            );
        }
        e.finish()?;

        let f = fmt.expect("validated");
        let now = self.now();
        let mut s = before.clone();
        s.prefix = f.prefix;
        s.suffix = f.suffix;
        s.padding = i64::from(f.padding);
        s.next_number = input.next_number;
        s.is_default = input.is_default;
        s.updated_at.clone_from(&now);
        if !tx.update_series(&s, expected_version).await? {
            return Err(AppError::stale());
        }
        s.row_version = expected_version + 1;
        if s.is_default {
            tx.clear_default_series(&s, &now).await?;
        }
        let c = Change {
            action: "invoice_series.update",
            entity_id: Some(id),
            before: json(&before),
            after: json(&s),
        };
        audit(&mut tx, ctx, &org.id, c, &now).await?;
        tx.commit().await?;
        self.list_series(ctx).await
    }

    pub async fn list_audit_events(
        &self,
        ctx: &RequestContext,
        limit: i64,
    ) -> AppResult<Vec<AuditRecord>> {
        ctx.require(Permission::ViewAudit)?;
        let mut uow = self.store.begin().await?;
        let Some(org) = self.find_org(ctx, &mut uow).await? else {
            return Ok(Vec::new());
        };
        Ok(uow.list_audit(&org.id, limit.clamp(1, 500)).await?)
    }
}

fn series_view(s: SeriesRecord, regs: &[RegistrationRecord]) -> SeriesView {
    let gstin = s
        .gst_registration_id
        .as_deref()
        .and_then(|id| regs.iter().find(|r| r.id == id))
        .map(|r| r.gstin.clone());
    let padding = u8::try_from(s.padding).unwrap_or(DEFAULT_PADDING);
    let next_preview = SeriesFormat {
        prefix: s.prefix.clone(),
        suffix: s.suffix.clone(),
        padding,
    }
    .format(u64::try_from(s.next_number).unwrap_or(1));
    SeriesView {
        locked: s.next_number > 1,
        gstin,
        next_preview,
        series: s,
    }
}
