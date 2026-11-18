use super::*;
use crate::masters::{ItemInput, PartyInput};

const GSTIN_MH: &str = "27AAPFU0939F1ZV"; // PAN AAPFU0939F (firm)

fn ctx() -> RequestContext {
    RequestContext::local_owner()
}

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 17).unwrap()
}

async fn core() -> AppCore {
    AppCore::in_memory_on(today()).await.unwrap()
}

fn org_input() -> OrganisationInput {
    OrganisationInput {
        legal_name: "Umang Traders".into(),
        trade_name: Some("Umang".into()),
        pan: None,
        constitution: "partnership".into(),
        address_line1: Some("12 MG Road".into()),
        address_line2: None,
        city: Some("Pune".into()),
        pincode: Some("411001".into()),
        state_code: 27,
        email: Some("accounts@umang.in".into()),
        phone: Some("+91 98200 12345".into()),
        fy_start_month: 4,
        books_start_date: "2026-04-01".into(),
        round_to_rupee: true,
    }
}

fn reg_input(gstin: &str) -> RegistrationInput {
    RegistrationInput {
        gstin: gstin.into(),
        registration_type: "regular".into(),
        filing_frequency: "monthly".into(),
        effective_from: "2020-07-01".into(),
        signatory_name: Some("A. Partner".into()),
    }
}

fn fields(err: &AppError) -> Vec<&str> {
    err.field_errors()
        .iter()
        .map(|f| f.field.as_str())
        .collect()
}

#[tokio::test]
async fn fresh_install_health_and_dashboard() {
    let core = core().await;
    let h = core.health().await.unwrap();
    assert!(!h.organisation_configured);
    assert_eq!(h.schema_version, 6);
    let d = core
        .dashboard_summary(&ctx(), "2026-11-01", "2026-11-30")
        .await
        .unwrap();
    assert_eq!(d.sales_this_month, Money::ZERO);
    assert!(d.registrations.is_empty());
    let p = core.business_profile(&ctx()).await.unwrap();
    assert!(p.organisation.is_none());
    assert_eq!(p.current_financial_year.code, "2026-27");
}

#[tokio::test]
async fn rejects_bad_dates() {
    let core = core().await;
    let err = core
        .dashboard_summary(&ctx(), "2026/11/01", "2026-11-30")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
}

#[tokio::test]
async fn setup_creates_everything_in_one_go() {
    let core = core().await;
    let p = core
        .setup_business(
            &ctx(),
            SetupBusinessInput {
                organisation: org_input(),
                registration: Some(reg_input(GSTIN_MH)),
                invoice_prefix: Some("um".into()),
            },
        )
        .await
        .unwrap();

    let org = p.organisation.unwrap();
    assert_eq!(
        org.pan.as_deref(),
        Some("AAPFU0939F"),
        "PAN derived from GSTIN"
    );
    assert_eq!(p.registrations.len(), 1);
    assert_eq!(p.registrations[0].state_code, 27);
    assert_eq!(p.account_count, av_accounting::chart::DEFAULT_CHART.len());

    let series = core.list_series(&ctx()).await.unwrap();
    let inv = series
        .iter()
        .find(|s| s.series.document_type == "tax_invoice")
        .unwrap();
    assert_eq!(inv.next_preview, "UM/26-27/0001");
    assert!(inv.series.is_default);
    assert_eq!(inv.gstin.as_deref(), Some(GSTIN_MH));
    assert_eq!(series.len(), 4);

    let audit = core.list_audit_events(&ctx(), 50).await.unwrap();
    let actions: Vec<_> = audit.iter().map(|a| a.action.as_str()).collect();
    assert!(actions.contains(&"organisation.create"));
    assert!(actions.contains(&"gst_registration.create"));
    assert!(actions.contains(&"chart_of_accounts.seed"));
    assert_eq!(
        actions
            .iter()
            .filter(|a| **a == "invoice_series.create")
            .count(),
        4
    );

    // Second setup is refused.
    let again = core
        .setup_business(
            &ctx(),
            SetupBusinessInput {
                organisation: org_input(),
                registration: None,
                invoice_prefix: None,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(again.code(), "already_set_up");
}

#[tokio::test]
async fn setup_reports_field_errors_and_writes_nothing() {
    let core = core().await;
    let mut org = org_input();
    org.legal_name = "  ".into();
    org.pan = Some("ABCPE1234F".into()); // individual PAN for a partnership
    org.pincode = Some("12".into());
    let err = core
        .setup_business(
            &ctx(),
            SetupBusinessInput {
                organisation: org,
                registration: Some(reg_input("27AAPFU0939F1ZA")),
                invoice_prefix: Some("INVOICE-LONG".into()),
            },
        )
        .await
        .unwrap_err();
    let f = fields(&err);
    for expected in [
        "organisation.legalName",
        "organisation.pan",
        "organisation.pincode",
        "registration.gstin",
        "invoicePrefix",
    ] {
        assert!(f.contains(&expected), "missing {expected} in {f:?}");
    }
    assert!(
        core.business_profile(&ctx())
            .await
            .unwrap()
            .organisation
            .is_none()
    );
}

#[tokio::test]
async fn gstin_pan_must_match_business_pan() {
    let core = core().await;
    let mut org = org_input();
    org.pan = Some("AAPFX1234F".into());
    let err = core
        .setup_business(
            &ctx(),
            SetupBusinessInput {
                organisation: org,
                registration: Some(reg_input(GSTIN_MH)),
                invoice_prefix: None,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["registration.gstin"]);
}

#[tokio::test]
async fn composition_is_not_supported_yet() {
    let core = core().await;
    let mut r = reg_input(GSTIN_MH);
    r.registration_type = "composition".into();
    let err = core
        .setup_business(
            &ctx(),
            SetupBusinessInput {
                organisation: org_input(),
                registration: Some(r),
                invoice_prefix: None,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["registration.registrationType"]);
}

#[tokio::test]
async fn unregistered_business_gets_bill_of_supply_series() {
    let core = core().await;
    core.setup_business(
        &ctx(),
        SetupBusinessInput {
            organisation: org_input(),
            registration: None,
            invoice_prefix: None,
        },
    )
    .await
    .unwrap();
    let series = core.list_series(&ctx()).await.unwrap();
    assert_eq!(series.len(), 1);
    assert_eq!(series[0].series.document_type, "bill_of_supply");
    assert_eq!(series[0].next_preview, "INV/26-27/0001");
    assert!(series[0].gstin.is_none());
}

#[tokio::test]
async fn organisation_update_uses_optimistic_concurrency() {
    let core = core().await;
    let p = core
        .setup_business(
            &ctx(),
            SetupBusinessInput {
                organisation: org_input(),
                registration: Some(reg_input(GSTIN_MH)),
                invoice_prefix: None,
            },
        )
        .await
        .unwrap();
    let v = p.organisation.unwrap().row_version;

    let mut edit = org_input();
    edit.trade_name = Some("Umang & Co".into());
    let p = core
        .update_organisation(&ctx(), edit.clone(), v)
        .await
        .unwrap();
    let org = p.organisation.unwrap();
    assert_eq!(org.trade_name.as_deref(), Some("Umang & Co"));
    assert_eq!(
        org.pan.as_deref(),
        None,
        "PAN cleared because input omitted it"
    );

    let stale = core
        .update_organisation(&ctx(), edit.clone(), v)
        .await
        .unwrap_err();
    assert_eq!(stale.code(), "conflict");

    edit.pan = Some("AAPFZ9999F".into());
    let err = core
        .update_organisation(&ctx(), edit, org.row_version)
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["pan"]);

    let audit = core.list_audit_events(&ctx(), 10).await.unwrap();
    assert_eq!(
        audit
            .iter()
            .filter(|a| a.action == "organisation.update")
            .count(),
        1
    );
    assert!(
        audit
            .iter()
            .find(|a| a.action == "organisation.update")
            .unwrap()
            .before_json
            .is_some()
    );
}

#[tokio::test]
async fn add_and_update_registrations() {
    let core = core().await;
    core.setup_business(
        &ctx(),
        SetupBusinessInput {
            organisation: org_input(),
            registration: Some(reg_input(GSTIN_MH)),
            invoice_prefix: None,
        },
    )
    .await
    .unwrap();

    let dup = core
        .add_gst_registration(&ctx(), reg_input(GSTIN_MH))
        .await
        .unwrap_err();
    assert_eq!(fields(&dup), vec!["gstin"]);

    // Same PAN, Karnataka.
    let ka = "29AAPFU0939F1Z".to_owned();
    let ka = format!("{ka}{}", check_char(&ka));
    let p = core
        .add_gst_registration(&ctx(), reg_input(&ka))
        .await
        .unwrap();
    assert_eq!(p.registrations.len(), 2);
    assert_eq!(
        core.list_series(&ctx()).await.unwrap().len(),
        8,
        "each GSTIN gets its own series"
    );

    let reg = p.registrations.iter().find(|r| r.gstin == ka).unwrap();
    let p = core
        .update_gst_registration(
            &ctx(),
            &reg.id,
            UpdateRegistrationInput {
                registration_type: "regular".into(),
                filing_frequency: "quarterly".into(),
                effective_from: "2021-01-01".into(),
                effective_to: Some("2020-01-01".into()),
                signatory_name: None,
                status: "active".into(),
            },
            reg.row_version,
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&p), vec!["effectiveTo"]);

    let p = core
        .update_gst_registration(
            &ctx(),
            &reg.id,
            UpdateRegistrationInput {
                registration_type: "regular".into(),
                filing_frequency: "quarterly".into(),
                effective_from: "2021-01-01".into(),
                effective_to: None,
                signatory_name: None,
                status: "active".into(),
            },
            reg.row_version,
        )
        .await
        .unwrap();
    let reg = p.registrations.iter().find(|r| r.gstin == ka).unwrap();
    assert_eq!(reg.filing_frequency, "quarterly");
}

#[tokio::test]
async fn series_rules() {
    let core = core().await;
    let p = core
        .setup_business(
            &ctx(),
            SetupBusinessInput {
                organisation: org_input(),
                registration: Some(reg_input(GSTIN_MH)),
                invoice_prefix: None,
            },
        )
        .await
        .unwrap();
    let reg_id = p.registrations[0].id.clone();

    let base = SeriesInput {
        gst_registration_id: Some(reg_id.clone()),
        document_type: "tax_invoice".into(),
        financial_year: "2026-27".into(),
        prefix: "INV/26-27/".into(),
        suffix: String::new(),
        padding: 4,
        next_number: 1,
        is_default: false,
    };
    let err = core.create_series(&ctx(), base.clone()).await.unwrap_err();
    assert_eq!(fields(&err), vec!["prefix"], "duplicate scope");

    let err = core
        .create_series(
            &ctx(),
            SeriesInput {
                prefix: "SHOWROOM/26-27/".into(),
                financial_year: "2024-25".into(),
                ..base.clone()
            },
        )
        .await
        .unwrap_err();
    let f = fields(&err);
    assert!(
        f.contains(&"prefix") && f.contains(&"financialYear"),
        "{f:?}"
    );

    let all = core
        .create_series(
            &ctx(),
            SeriesInput {
                prefix: "SR/26-27/".into(),
                is_default: true,
                ..base.clone()
            },
        )
        .await
        .unwrap();
    let defaults: Vec<_> = all
        .iter()
        .filter(|s| s.series.document_type == "tax_invoice" && s.series.is_default)
        .collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].series.prefix, "SR/26-27/");

    // Next-year series is allowed.
    core.create_series(
        &ctx(),
        SeriesInput {
            financial_year: "2027-28".into(),
            prefix: "INV/27-28/".into(),
            ..base.clone()
        },
    )
    .await
    .unwrap();

    // Next number can't go backwards; prefix locks after use.
    let sr = defaults[0].series.clone();
    let all = core
        .update_series(
            &ctx(),
            &sr.id,
            SeriesUpdateInput {
                prefix: sr.prefix.clone(),
                suffix: String::new(),
                padding: 4,
                next_number: 50,
                is_default: true,
            },
            sr.row_version,
        )
        .await
        .unwrap();
    let sr = all.iter().find(|s| s.series.id == sr.id).unwrap();
    assert!(sr.locked);
    assert_eq!(sr.next_preview, "SR/26-27/0050");
    let err = core
        .update_series(
            &ctx(),
            &sr.series.id,
            SeriesUpdateInput {
                prefix: "X/".into(),
                suffix: String::new(),
                padding: 4,
                next_number: 10,
                is_default: true,
            },
            sr.series.row_version,
        )
        .await
        .unwrap_err();
    let f = fields(&err);
    assert!(f.contains(&"prefix") && f.contains(&"nextNumber"), "{f:?}");
}

fn user(id: &str, role: Role) -> Actor {
    Actor {
        user_id: id.into(),
        display_name: id.into(),
        role,
    }
}

/// Commercial mode: two tenants in one database never see each other's data,
/// even when they guess ids.
#[tokio::test]
async fn tenants_are_isolated() {
    let core = core().await;
    let a = RequestContext::for_organisation(user("alice", Role::Owner), "org-a");
    let b = RequestContext::for_organisation(user("bob", Role::Owner), "org-b");

    let pa = core
        .setup_business(
            &a,
            SetupBusinessInput {
                organisation: org_input(),
                registration: Some(reg_input(GSTIN_MH)),
                invoice_prefix: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(pa.organisation.as_ref().unwrap().id, "org-a");

    // B is not set up yet and sees nothing of A.
    let pb = core.business_profile(&b).await.unwrap();
    assert!(pb.organisation.is_none() && pb.registrations.is_empty());
    assert!(core.list_series(&b).await.unwrap().is_empty());
    assert!(core.list_audit_events(&b, 50).await.unwrap().is_empty());

    // The same GSTIN may exist in another tenant (e.g. a firm that switched accountants).
    core.setup_business(
        &b,
        SetupBusinessInput {
            organisation: org_input(),
            registration: Some(reg_input(GSTIN_MH)),
            invoice_prefix: None,
        },
    )
    .await
    .unwrap();

    // B can't edit A's records by id.
    let a_reg = &pa.registrations[0];
    let upd = UpdateRegistrationInput {
        registration_type: "regular".into(),
        filing_frequency: "quarterly".into(),
        effective_from: "2020-07-01".into(),
        effective_to: None,
        signatory_name: None,
        status: "active".into(),
    };
    let err = core
        .update_gst_registration(&b, &a_reg.id, upd, a_reg.row_version)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "not_found");
    let a_series = core.list_series(&a).await.unwrap()[0].series.clone();
    let err = core
        .update_series(
            &b,
            &a_series.id,
            SeriesUpdateInput {
                prefix: "X/".into(),
                suffix: String::new(),
                padding: 4,
                next_number: 9,
                is_default: true,
            },
            a_series.row_version,
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "not_found");

    // Audit entries carry the acting user and stay in their tenant.
    let audit_a = core.list_audit_events(&a, 100).await.unwrap();
    assert!(
        audit_a
            .iter()
            .all(|e| e.actor == "alice" && e.organisation_id.as_deref() == Some("org-a"))
    );
}

#[tokio::test]
async fn roles_are_enforced() {
    let core = core().await;
    let owner = RequestContext::for_organisation(user("o", Role::Owner), "org");
    core.setup_business(
        &owner,
        SetupBusinessInput {
            organisation: org_input(),
            registration: None,
            invoice_prefix: None,
        },
    )
    .await
    .unwrap();

    let operator = RequestContext::for_organisation(user("op", Role::Operator), "org");
    let auditor = RequestContext::for_organisation(user("au", Role::Auditor), "org");

    // Everyone can read setup; only the owner can change it.
    assert!(
        core.business_profile(&operator)
            .await
            .unwrap()
            .organisation
            .is_some()
    );
    let err = core
        .add_gst_registration(&operator, reg_input(GSTIN_MH))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "forbidden");
    let err = core
        .add_gst_registration(&auditor, reg_input(GSTIN_MH))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "forbidden");

    // The audit log is for owners, accountants and auditors, not operators.
    assert!(
        !core
            .list_audit_events(&auditor, 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        core.list_audit_events(&operator, 10)
            .await
            .unwrap_err()
            .code(),
        "forbidden"
    );
}

/// Mirrors the GSTIN check-digit algorithm so tests can build valid GSTINs.
fn check_char(first14: &str) -> char {
    const CS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let sum: u32 = first14
        .bytes()
        .enumerate()
        .map(|(i, c)| {
            let v = CS.iter().position(|x| *x == c).unwrap() as u32;
            let p = v * if i % 2 == 0 { 1 } else { 2 };
            p / 36 + p % 36
        })
        .sum();
    CS[((36 - sum % 36) % 36) as usize] as char
}

// ---------------------------------------------------------------------------
// Phase 1.2 masters
// ---------------------------------------------------------------------------

async fn set_up(core: &AppCore) {
    core.setup_business(
        &ctx(),
        SetupBusinessInput {
            organisation: org_input(),
            registration: Some(reg_input(GSTIN_MH)),
            invoice_prefix: None,
        },
    )
    .await
    .unwrap();
}

fn customer(name: &str) -> PartyInput {
    PartyInput {
        kind: "customer".into(),
        name: name.into(),
        gst_type: "consumer".into(),
        state_code: Some(27),
        shipping_same: true,
        is_active: true,
        ..Default::default()
    }
}

fn goods(name: &str) -> ItemInput {
    ItemInput {
        kind: "goods".into(),
        name: name.into(),
        hsn_sac: Some("8471".into()),
        tax_category: "taxable".into(),
        gst_rate: Some("18".into()),
        is_active: true,
        ..Default::default()
    }
}

#[tokio::test]
async fn registered_party_takes_state_and_pan_from_gstin() {
    let core = core().await;
    set_up(&core).await;
    let ka = "29AAPFU0939F1Z".to_owned();
    let ka = format!("{ka}{}", check_char(&ka));
    let p = core
        .create_party(
            &ctx(),
            PartyInput {
                gst_type: "regular".into(),
                gstin: Some(ka.to_lowercase()),
                state_code: Some(27),
                ..customer("Acme Retail")
            },
        )
        .await
        .unwrap();
    assert_eq!(
        p.party.state_code, 29,
        "place of supply follows the GSTIN, not the form"
    );
    assert_eq!(p.party.pan.as_deref(), Some("AAPFU0939F"));
    assert_eq!(p.party.gstin.as_deref(), Some(ka.as_str()));

    // Names are unique ignoring case; GSTINs are unique.
    let err = core
        .create_party(&ctx(), customer("ACME retail"))
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["name"]);
    let err = core
        .create_party(
            &ctx(),
            PartyInput {
                gst_type: "regular".into(),
                gstin: Some(ka.clone()),
                ..customer("Other")
            },
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["gstin"]);
}

#[tokio::test]
async fn party_gst_status_rules() {
    let core = core().await;
    set_up(&core).await;
    let err = core
        .create_party(
            &ctx(),
            PartyInput {
                gst_type: "regular".into(),
                ..customer("A")
            },
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["gstin"]);
    let err = core
        .create_party(
            &ctx(),
            PartyInput {
                gstin: Some(GSTIN_MH.into()),
                ..customer("B")
            },
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["gstin"], "consumers have no GSTIN");
    let err = core
        .create_party(
            &ctx(),
            PartyInput {
                state_code: None,
                ..customer("C")
            },
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["stateCode"]);
    let p = core
        .create_party(
            &ctx(),
            PartyInput {
                gst_type: "overseas".into(),
                state_code: None,
                pincode: Some("SW1A".into()),
                ..customer("D")
            },
        )
        .await
        .unwrap();
    assert_eq!(p.party.state_code, 96);
    let err = core
        .create_party(
            &ctx(),
            PartyInput {
                shipping_same: false,
                ..customer("E")
            },
        )
        .await
        .unwrap_err();
    let f = fields(&err);
    assert!(
        f.contains(&"shippingLine1") && f.contains(&"shippingStateCode"),
        "{f:?}"
    );
}

#[tokio::test]
async fn opening_balances_post_to_the_ledger_and_reverse_on_change() {
    let core = core().await;
    set_up(&core).await;
    let p = core
        .create_party(
            &ctx(),
            PartyInput {
                opening_balance: Some("12,500.50".into()),
                ..customer("Ravi Stores")
            },
        )
        .await
        .unwrap();
    assert_eq!(p.party.opening_balance_paise, 1_250_050);
    assert_eq!(p.balance_paise, 1_250_050);
    assert!(p.party.opening_entry_id.is_some());
    let d = core
        .dashboard_summary(&ctx(), "2026-11-01", "2026-11-30")
        .await
        .unwrap();
    assert_eq!(d.customer_dues, Money::from_paise(1_250_050));

    // Change it: the old entry is reversed and a new one posted.
    let p = core
        .update_party(
            &ctx(),
            &p.party.id,
            PartyInput {
                opening_balance: Some("500".into()),
                opening_balance_side: Some("payable".into()),
                ..customer("Ravi Stores")
            },
            p.party.row_version,
        )
        .await
        .unwrap();
    assert_eq!(p.balance_paise, -50_000, "an advance from the customer");
    assert!(core.business_profile(&ctx()).await.unwrap().has_postings);
    assert_eq!(
        core.list_parties(&ctx()).await.unwrap()[0].balance_paise,
        -50_000
    );

    // Clearing it reverses again and leaves no balance.
    let p = core
        .update_party(
            &ctx(),
            &p.party.id,
            customer("Ravi Stores"),
            p.party.row_version,
        )
        .await
        .unwrap();
    assert_eq!(p.balance_paise, 0);
    assert!(p.party.opening_entry_id.is_none());

    // Stale edits are refused.
    let err = core
        .update_party(&ctx(), &p.party.id, customer("Ravi Stores"), 1)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "conflict");
}

#[tokio::test]
async fn supplier_opening_balance_defaults_to_payable() {
    let core = core().await;
    set_up(&core).await;
    let p = core
        .create_party(
            &ctx(),
            PartyInput {
                kind: "supplier".into(),
                opening_balance: Some("2000".into()),
                ..customer("Steel Co")
            },
        )
        .await
        .unwrap();
    assert_eq!(p.balance_paise, -200_000);
}

#[tokio::test]
async fn item_rules() {
    let core = core().await;
    set_up(&core).await;
    let i = core
        .create_item(
            &ctx(),
            ItemInput {
                code: Some("LAP-01".into()),
                ..goods("Laptop")
            },
        )
        .await
        .unwrap();
    assert_eq!(i.gst_rate_bp, 1800);
    assert_eq!(i.unit, "NOS");

    let svc = core
        .create_item(
            &ctx(),
            ItemInput {
                kind: "service".into(),
                hsn_sac: Some("998314".into()),
                ..goods("IT support")
            },
        )
        .await
        .unwrap();
    assert_eq!(svc.unit, "OTH");

    let err = core
        .create_item(
            &ctx(),
            ItemInput {
                code: Some("lap-01".into()),
                ..goods("Laptop 2")
            },
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["code"]);
    let err = core
        .create_item(
            &ctx(),
            ItemInput {
                gst_rate: Some("17".into()),
                ..goods("Odd")
            },
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["gstRate"]);
    let err = core
        .create_item(
            &ctx(),
            ItemInput {
                tax_category: "exempt".into(),
                ..goods("Milk")
            },
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["gstRate"], "exempt items carry no rate");
    let err = core
        .create_item(
            &ctx(),
            ItemInput {
                hsn_sac: Some("998314".into()),
                ..goods("Wrong")
            },
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["hsnSac"]);
    let err = core
        .create_item(
            &ctx(),
            ItemInput {
                kind: "service".into(),
                hsn_sac: Some("998314".into()),
                track_stock: true,
                ..goods("Svc")
            },
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["trackStock"]);

    let i = core
        .update_item(
            &ctx(),
            &i.id,
            ItemInput {
                gst_rate: Some("5".into()),
                sales_price: Some("45000".into()),
                ..goods("Laptop")
            },
            i.row_version,
        )
        .await
        .unwrap();
    assert_eq!((i.gst_rate_bp, i.sales_price_paise), (500, Some(4_500_000)));
    assert_eq!(core.list_items(&ctx()).await.unwrap().len(), 2);
}

#[tokio::test]
async fn operators_cannot_change_masters() {
    let core = core().await;
    let owner = RequestContext::for_organisation(user("o", Role::Owner), "org");
    core.setup_business(
        &owner,
        SetupBusinessInput {
            organisation: org_input(),
            registration: None,
            invoice_prefix: None,
        },
    )
    .await
    .unwrap();
    let operator = RequestContext::for_organisation(user("op", Role::Operator), "org");
    assert_eq!(
        core.create_party(&operator, customer("X"))
            .await
            .unwrap_err()
            .code(),
        "forbidden"
    );
    let accountant = RequestContext::for_organisation(user("ac", Role::Accountant), "org");
    core.create_party(&accountant, customer("X")).await.unwrap();
}

// ---------------------------------------------------------------------------
// Phase 1.2 import
// ---------------------------------------------------------------------------

use crate::import::{ImportKind, import_template, parse_import_file};

#[tokio::test]
async fn csv_party_import_previews_then_creates_valid_rows() {
    let core = core().await;
    set_up(&core).await;
    core.create_party(&ctx(), customer("Existing Co"))
        .await
        .unwrap();

    // Semicolon-separated with a BOM, odd header names, Dr/Cr balances.
    let csv = "\u{feff}Party Name;GST No;State;Mobile;Opening;Type\n\
               Ravi Stores;27AAPFU0939F1ZV;Maharashtra;9820012345;12500 Dr;customer\n\
               Steel Co;;Karnataka;;2000 Cr;supplier\n\
               ;;Goa;;;\n\
               Existing Co;;27;;;\n\
               Steel Co;;Kerala;;;supplier\n\
               Bad State;;Atlantis;;;\n";
    let parsed = parse_import_file(ImportKind::Parties, "parties.csv", csv.as_bytes()).unwrap();
    assert_eq!(parsed.table.headers.len(), 6);
    assert_eq!(parsed.table.rows.len(), 6);
    for (field, col) in [
        ("name", 0),
        ("gstin", 1),
        ("stateCode", 2),
        ("phone", 3),
        ("openingBalance", 4),
        ("kind", 5),
    ] {
        assert_eq!(parsed.mapping.get(field), Some(&col), "{field}");
    }

    let preview = core
        .preview_import(&ctx(), ImportKind::Parties, &parsed.table, &parsed.mapping)
        .await
        .unwrap();
    assert_eq!((preview.total, preview.valid), (6, 2));
    let rows: Vec<usize> = preview.issues.iter().map(|i| i.row).collect();
    assert_eq!(rows, vec![4, 5, 6, 7], "sheet row numbers, header is row 1");
    assert!(
        preview.issues[1].errors[0].message.starts_with("Name:"),
        "{:?}",
        preview.issues[1].errors
    );
    assert!(preview.issues[3].errors[0].message.contains("Atlantis"));
    assert!(
        core.list_parties(&ctx()).await.unwrap().len() == 1,
        "preview writes nothing"
    );

    let result = core
        .commit_import(&ctx(), ImportKind::Parties, &parsed.table, &parsed.mapping)
        .await
        .unwrap();
    assert_eq!((result.created, result.skipped.len()), (2, 4));
    let parties = core.list_parties(&ctx()).await.unwrap();
    let ravi = parties
        .iter()
        .find(|p| p.party.name == "Ravi Stores")
        .unwrap();
    assert_eq!(
        (
            ravi.party.gst_type.as_str(),
            ravi.party.state_code,
            ravi.balance_paise
        ),
        ("regular", 27, 1_250_000)
    );
    let steel = parties.iter().find(|p| p.party.name == "Steel Co").unwrap();
    assert_eq!(
        (steel.party.gst_type.as_str(), steel.balance_paise),
        ("unregistered", -200_000)
    );

    let audit = core.list_audit_events(&ctx(), 50).await.unwrap();
    assert!(audit.iter().any(|a| a.action == "import.run"));
}

#[tokio::test]
async fn item_import_guesses_service_and_requires_name_column() {
    let core = core().await;
    set_up(&core).await;
    let csv = "Item Name,HSN,GST %,Price,Inclusive\nLaptop,8471,18,\"52,000\",no\nAMC,998713,18%,5000,yes\nRice,1006,0,40,\n";
    let parsed = parse_import_file(ImportKind::Items, "items.CSV", csv.as_bytes()).unwrap();
    let preview = core
        .preview_import(&ctx(), ImportKind::Items, &parsed.table, &parsed.mapping)
        .await
        .unwrap();
    assert_eq!(preview.valid, 2, "{:?}", preview.issues);
    assert!(
        preview.issues[0].errors[0]
            .message
            .starts_with("GST rate %:"),
        "taxable rice needs a rate"
    );
    core.commit_import(&ctx(), ImportKind::Items, &parsed.table, &parsed.mapping)
        .await
        .unwrap();
    let items = core.list_items(&ctx()).await.unwrap();
    let amc = items.iter().find(|i| i.name == "AMC").unwrap();
    assert_eq!(
        (amc.kind.as_str(), amc.unit.as_str(), amc.price_includes_tax),
        ("service", "OTH", true)
    );
    assert_eq!(
        items
            .iter()
            .find(|i| i.name == "Laptop")
            .unwrap()
            .sales_price_paise,
        Some(5_200_000)
    );

    let mut mapping = parsed.mapping.clone();
    mapping.remove("name");
    let err = core
        .preview_import(&ctx(), ImportKind::Items, &parsed.table, &mapping)
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["mapping.name"]);
}

#[test]
fn templates_and_bad_files() {
    let t = import_template(ImportKind::Parties);
    assert!(t.csv.starts_with("Name,Type,GST status,GSTIN"));
    let parsed = parse_import_file(ImportKind::Parties, "t.csv", t.csv.as_bytes()).unwrap();
    assert_eq!(
        parsed.mapping.len(),
        parsed.fields.len(),
        "the template maps itself completely"
    );
    assert!(parse_import_file(ImportKind::Items, "x.pdf", b"%PDF").is_err());
    assert!(parse_import_file(ImportKind::Items, "x.csv", b"\n\n").is_err());
}

#[test]
fn excel_numbers_keep_their_digits() {
    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet();
    for (c, h) in ["Name", "PIN", "Mobile", "GST Rate"].iter().enumerate() {
        ws.write_string(0, c as u16, *h).unwrap();
    }
    ws.write_string(1, 0, "Asha Traders").unwrap();
    ws.write_number(1, 1, 411001.0).unwrap();
    ws.write_number(1, 2, 9820012345.0).unwrap();
    ws.write_number(1, 3, 0.25).unwrap();
    let bytes = wb.save_to_buffer().unwrap();
    let parsed = parse_import_file(ImportKind::Parties, "book.xlsx", &bytes).unwrap();
    assert_eq!(
        parsed.table.rows[0],
        vec!["Asha Traders", "411001", "9820012345", "0.25"]
    );
    assert_eq!(parsed.mapping.get("pincode"), Some(&1));
}

// ---------------------------------------------------------------------------
// Phase 1.3 sales
// ---------------------------------------------------------------------------

use crate::sales::{DisplayStatus, SalesDocumentInput, SalesLineInput};

struct Shop {
    core: AppCore,
    customer: String,
    karnataka: String,
    laptop: String,
}

async fn shop() -> Shop {
    let core = core().await;
    set_up(&core).await;
    let ravi = core
        .create_party(
            &ctx(),
            PartyInput {
                payment_terms_days: Some(15),
                ..customer("Ravi Stores")
            },
        )
        .await
        .unwrap()
        .party
        .id;
    let karnataka = core
        .create_party(
            &ctx(),
            PartyInput {
                state_code: Some(29),
                ..customer("Bengaluru Mart")
            },
        )
        .await
        .unwrap()
        .party
        .id;
    let laptop = core.create_item(&ctx(), goods("Laptop")).await.unwrap().id;
    Shop {
        core,
        customer: ravi,
        karnataka,
        laptop,
    }
}

fn line(item: Option<&str>, qty: &str, price: &str, rate: &str) -> SalesLineInput {
    SalesLineInput {
        item_id: item.map(str::to_owned),
        description: if item.is_some() {
            String::new()
        } else {
            "Installation".into()
        },
        hsn_sac: if item.is_some() {
            None
        } else {
            Some("998713".into())
        },
        quantity: qty.into(),
        unit_price: price.into(),
        tax_category: "taxable".into(),
        gst_rate: Some(rate.into()),
        ..Default::default()
    }
}

fn invoice(party: &str, date: &str, lines: Vec<SalesLineInput>) -> SalesDocumentInput {
    SalesDocumentInput {
        doc_type: "tax_invoice".into(),
        party_id: party.into(),
        doc_date: date.into(),
        lines,
        ..Default::default()
    }
}

#[tokio::test]
async fn issuing_an_invoice_numbers_it_and_posts_the_ledger() {
    let s = shop().await;
    let input = invoice(
        &s.customer,
        "2026-11-10",
        vec![
            line(Some(&s.laptop), "2", "1000", "18"),
            line(None, "1", "499.50", "18"),
        ],
    );

    // Live calculation before anything is saved.
    let calc = s.core.calculate_sales(&ctx(), input.clone()).await.unwrap();
    assert_eq!(calc.number_preview.as_deref(), Some("INV/26-27/0001"));
    assert_eq!(calc.totals.taxable_paise, 249_950);
    assert_eq!(
        (
            calc.totals.cgst_paise,
            calc.totals.sgst_paise,
            calc.totals.igst_paise
        ),
        (22_496, 22_496, 0)
    );
    // 2499.50 + 449.92 = 2949.42 → 2949.00
    assert_eq!(
        (calc.totals.total_paise, calc.totals.round_off_paise),
        (294_900, -42)
    );

    let v = s
        .core
        .issue_sales_document(&ctx(), None, input, None)
        .await
        .unwrap();
    assert_eq!(v.document.number.as_deref(), Some("INV/26-27/0001"));
    assert_eq!(v.document.status, "issued");
    assert_eq!(
        v.document.due_date.as_deref(),
        Some("2026-11-25"),
        "15-day terms"
    );
    assert_eq!(v.lines[0].description, "Laptop");
    assert_eq!(
        v.lines[0].hsn_sac.as_deref(),
        Some("8471"),
        "HSN snapshot from the item"
    );
    assert_eq!(v.display_status, DisplayStatus::Issued);

    let parties = s.core.list_parties(&ctx()).await.unwrap();
    assert_eq!(
        parties
            .iter()
            .find(|p| p.party.id == s.customer)
            .unwrap()
            .balance_paise,
        294_900
    );
    let d = s
        .core
        .dashboard_summary(&ctx(), "2026-11-01", "2026-11-30")
        .await
        .unwrap();
    assert_eq!(d.sales_this_month, Money::from_paise(249_950));
    assert_eq!(d.estimated_gst_payable, Money::from_paise(44_992));

    // The next one takes the next number.
    let v2 = s
        .core
        .issue_sales_document(
            &ctx(),
            None,
            invoice(
                &s.karnataka,
                "2026-11-12",
                vec![line(Some(&s.laptop), "1", "1000", "18")],
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(v2.document.number.as_deref(), Some("INV/26-27/0002"));
    assert_eq!(
        (v2.document.igst_paise, v2.document.cgst_paise),
        (18_000, 0),
        "inter-state: IGST"
    );
    assert_eq!(v2.document.supply_kind, "inter_state");

    // Numbers run in date order within a series.
    let err = s
        .core
        .issue_sales_document(
            &ctx(),
            None,
            invoice(&s.customer, "2026-11-11", vec![line(None, "1", "10", "18")]),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["docDate"]);
}

#[tokio::test]
async fn drafts_are_editable_and_issued_documents_are_not() {
    let s = shop().await;
    let input = invoice(
        &s.customer,
        "2026-11-10",
        vec![line(Some(&s.laptop), "1", "1000", "18")],
    );
    let draft = s
        .core
        .save_sales_draft(&ctx(), None, input.clone(), None)
        .await
        .unwrap();
    assert_eq!(
        (
            draft.document.status.as_str(),
            draft.document.number.clone()
        ),
        ("draft", None)
    );
    assert!(
        !s.core.business_profile(&ctx()).await.unwrap().has_postings,
        "drafts post nothing"
    );

    let mut edited = input.clone();
    edited.lines[0].quantity = "3".into();
    let draft = s
        .core
        .save_sales_draft(
            &ctx(),
            Some(&draft.document.id),
            edited.clone(),
            Some(draft.document.row_version),
        )
        .await
        .unwrap();
    assert_eq!(draft.document.taxable_paise, 300_000);
    let stale = s
        .core
        .save_sales_draft(&ctx(), Some(&draft.document.id), edited.clone(), Some(1))
        .await
        .unwrap_err();
    assert_eq!(stale.code(), "conflict");

    let issued = s
        .core
        .issue_sales_document(
            &ctx(),
            Some(&draft.document.id),
            edited.clone(),
            Some(draft.document.row_version),
        )
        .await
        .unwrap();
    assert_eq!(issued.document.number.as_deref(), Some("INV/26-27/0001"));
    // Issuing twice (a double click) can't post or number twice.
    let again = s
        .core
        .issue_sales_document(
            &ctx(),
            Some(&draft.document.id),
            edited.clone(),
            Some(issued.document.row_version),
        )
        .await
        .unwrap_err();
    assert_eq!(again.code(), "conflict");
    let err = s
        .core
        .save_sales_draft(&ctx(), Some(&draft.document.id), edited, None)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "conflict");
    let err = s
        .core
        .delete_sales_draft(&ctx(), &draft.document.id, issued.document.row_version)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "conflict");

    // A draft can be deleted.
    let other = s
        .core
        .save_sales_draft(&ctx(), None, input, None)
        .await
        .unwrap();
    s.core
        .delete_sales_draft(&ctx(), &other.document.id, other.document.row_version)
        .await
        .unwrap();
    assert_eq!(s.core.list_sales(&ctx()).await.unwrap().len(), 1);
}

#[tokio::test]
async fn sales_validation_points_at_fields() {
    let s = shop().await;
    let mut bad = invoice(
        &s.customer,
        "2026-12-01",
        vec![line(None, "0", "", "17"), line(None, "1.2345", "10", "18")],
    );
    bad.lines[1].description = String::new();
    bad.lines[1].hsn_sac = None;
    let err = s
        .core
        .issue_sales_document(&ctx(), None, bad, None)
        .await
        .unwrap_err();
    let f = fields(&err);
    for expected in [
        "docDate",
        "lines.0.quantity",
        "lines.0.unitPrice",
        "lines.0.gstRate",
        "lines.1.quantity",
        "lines.1.description",
        "lines.1.hsnSac",
    ] {
        assert!(f.contains(&expected), "missing {expected} in {f:?}");
    }
    let err = s
        .core
        .issue_sales_document(&ctx(), None, invoice("", "2026-11-10", vec![]), None)
        .await
        .unwrap_err();
    let f = fields(&err);
    assert!(f.contains(&"partyId") && f.contains(&"lines"), "{f:?}");

    let mut zero = invoice(
        &s.customer,
        "2026-11-10",
        vec![line(Some(&s.laptop), "1", "100", "18")],
    );
    zero.zero_rated = true;
    assert_eq!(
        fields(
            &s.core
                .issue_sales_document(&ctx(), None, zero, None)
                .await
                .unwrap_err()
        ),
        vec!["zeroRated"]
    );

    let mut bos = invoice(
        &s.customer,
        "2026-11-10",
        vec![line(Some(&s.laptop), "1", "100", "18")],
    );
    bos.doc_type = "bill_of_supply".into();
    assert_eq!(
        fields(
            &s.core
                .issue_sales_document(&ctx(), None, bos, None)
                .await
                .unwrap_err()
        ),
        vec!["lines.0.gstRate"]
    );

    // Incomplete input still gives live figures for the lines that make sense.
    let calc = s
        .core
        .calculate_sales(
            &ctx(),
            invoice(
                "",
                "2026-11-10",
                vec![line(None, "2", "50", "18"), line(None, "x", "1", "18")],
            ),
        )
        .await
        .unwrap();
    assert_eq!(calc.lines[0].as_ref().unwrap().total_paise, 11_800);
    assert!(calc.lines[1].is_none());
}

#[tokio::test]
async fn credit_notes_and_cancellation() {
    let s = shop().await;
    let inv = s
        .core
        .issue_sales_document(
            &ctx(),
            None,
            invoice(
                &s.customer,
                "2026-11-10",
                vec![line(Some(&s.laptop), "2", "1000", "18")],
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(inv.document.total_paise, 236_000);

    let cn = |qty: &str| SalesDocumentInput {
        doc_type: "credit_note".into(),
        doc_date: "2026-11-15".into(),
        original_document_id: Some(inv.document.id.clone()),
        reason: Some("One laptop returned".into()),
        lines: vec![line(Some(&s.laptop), qty, "1000", "18")],
        ..Default::default()
    };
    let note = s
        .core
        .issue_sales_document(&ctx(), None, cn("1"), None)
        .await
        .unwrap();
    assert_eq!(note.document.number.as_deref(), Some("CN/26-27/0001"));
    assert_eq!(
        note.document.party_id, s.customer,
        "party comes from the invoice"
    );
    assert_eq!(note.original.as_ref().unwrap().id, inv.document.id);

    assert_eq!(ravi_balance(&s.core).await, 118_000);

    // Can't credit more than is left on the invoice.
    let err = s
        .core
        .issue_sales_document(&ctx(), None, cn("1.5"), None)
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["lines"]);
    let mut no_reason = cn("1");
    no_reason.reason = None;
    assert!(
        fields(
            &s.core
                .issue_sales_document(&ctx(), None, no_reason, None)
                .await
                .unwrap_err()
        )
        .contains(&"reason")
    );

    let inv_view = s
        .core
        .get_sales_document(&ctx(), &inv.document.id)
        .await
        .unwrap();
    assert_eq!(
        (inv_view.credit_notes.len(), inv_view.credited_paise),
        (1, 118_000)
    );

    // An invoice with live credit notes can't be cancelled.
    let err = s
        .core
        .cancel_sales_document(
            &ctx(),
            &inv.document.id,
            "Wrong customer",
            inv.document.row_version,
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "conflict");
    let err = s
        .core
        .cancel_sales_document(&ctx(), &note.document.id, " ", note.document.row_version)
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["reason"]);
    let note = s
        .core
        .cancel_sales_document(
            &ctx(),
            &note.document.id,
            "Raised in error",
            note.document.row_version,
        )
        .await
        .unwrap();
    assert_eq!(note.display_status, DisplayStatus::Cancelled);
    let inv = s
        .core
        .cancel_sales_document(
            &ctx(),
            &inv.document.id,
            "Wrong customer",
            inv.document.row_version,
        )
        .await
        .unwrap();
    assert_eq!(
        inv.document.cancel_reason.as_deref(),
        Some("Wrong customer")
    );
    assert_eq!(ravi_balance(&s.core).await, 0, "every posting reversed");
    let d = s
        .core
        .dashboard_summary(&ctx(), "2026-11-01", "2026-11-30")
        .await
        .unwrap();
    assert_eq!(d.sales_this_month, Money::ZERO);

    // The cancelled number is never reused.
    let next = s
        .core
        .issue_sales_document(
            &ctx(),
            None,
            invoice(&s.customer, "2026-11-16", vec![line(None, "1", "10", "18")]),
            None,
        )
        .await
        .unwrap();
    assert_eq!(next.document.number.as_deref(), Some("INV/26-27/0002"));
}

#[tokio::test]
async fn overdue_invoices_and_unregistered_businesses() {
    let s = shop().await;
    let mut input = invoice(
        &s.customer,
        "2026-10-01",
        vec![line(None, "1", "100", "18")],
    );
    input.due_date = Some("2026-10-31".into());
    let v = s
        .core
        .issue_sales_document(&ctx(), None, input, None)
        .await
        .unwrap();
    assert_eq!(v.display_status, DisplayStatus::Overdue);

    let core = core().await;
    core.setup_business(
        &ctx(),
        SetupBusinessInput {
            organisation: org_input(),
            registration: None,
            invoice_prefix: None,
        },
    )
    .await
    .unwrap();
    let p = core
        .create_party(&ctx(), customer("Walk-in"))
        .await
        .unwrap()
        .party
        .id;
    let err = core
        .issue_sales_document(
            &ctx(),
            None,
            invoice(&p, "2026-11-10", vec![line(None, "1", "100", "0")]),
            None,
        )
        .await
        .unwrap_err();
    assert!(fields(&err).contains(&"docType"));
    let mut bos = invoice(
        &p,
        "2026-11-10",
        vec![SalesLineInput {
            tax_category: "exempt".into(),
            gst_rate: None,
            hsn_sac: None,
            ..line(None, "1", "100", "0")
        }],
    );
    bos.doc_type = "bill_of_supply".into();
    let v = core
        .issue_sales_document(&ctx(), None, bos, None)
        .await
        .unwrap();
    assert_eq!(
        (v.document.number.as_deref(), v.document.total_paise),
        (Some("INV/26-27/0001"), 10_000)
    );
}

async fn ravi_balance(core: &AppCore) -> i64 {
    core.list_parties(&ctx())
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.party.name == "Ravi Stores")
        .unwrap()
        .balance_paise
}

#[tokio::test]
async fn operators_prepare_sales_but_cannot_issue() {
    let core = core().await;
    let owner = RequestContext::for_organisation(user("o", Role::Owner), "org");
    core.setup_business(
        &owner,
        SetupBusinessInput {
            organisation: org_input(),
            registration: Some(reg_input(GSTIN_MH)),
            invoice_prefix: None,
        },
    )
    .await
    .unwrap();
    let party = core
        .create_party(&owner, customer("Ravi Stores"))
        .await
        .unwrap()
        .party
        .id;
    let operator = RequestContext::for_organisation(user("op", Role::Operator), "org");
    let input = invoice(&party, "2026-11-10", vec![line(None, "1", "100", "18")]);
    let draft = core
        .save_sales_draft(&operator, None, input.clone(), None)
        .await
        .unwrap();
    let err = core
        .issue_sales_document(&operator, Some(&draft.document.id), input.clone(), None)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "forbidden");
    core.issue_sales_document(&owner, Some(&draft.document.id), input, None)
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// Phase 1.4 purchases
// ---------------------------------------------------------------------------

use crate::purchases::{
    ItcReviewInput, PurchaseDocumentInput, PurchaseLineInput, PurchaseStatus, sha256_hex,
};

struct Books {
    core: AppCore,
    supplier: String,
    unregistered: String,
    customer: String,
    purchases_acc: String,
    rent_acc: String,
}

async fn books() -> Books {
    let core = core().await;
    set_up(&core).await;
    // A Maharashtra supplier with a GSTIN (same PAN as the test GSTIN is fine here).
    let supplier = core
        .create_party(
            &ctx(),
            PartyInput {
                kind: "supplier".into(),
                gst_type: "regular".into(),
                gstin: Some(GSTIN_MH.into()),
                payment_terms_days: Some(30),
                ..customer("Steel Co")
            },
        )
        .await
        .unwrap()
        .party
        .id;
    let unregistered = core
        .create_party(
            &ctx(),
            PartyInput {
                kind: "supplier".into(),
                gst_type: "unregistered".into(),
                ..customer("Transporter")
            },
        )
        .await
        .unwrap()
        .party
        .id;
    let customer_id = core
        .create_party(&ctx(), customer("Ravi Stores"))
        .await
        .unwrap()
        .party
        .id;
    let accounts = core.purchase_accounts(&ctx()).await.unwrap();
    let purchases_acc = accounts
        .iter()
        .find(|a| a.system_role.as_deref() == Some("purchases"))
        .unwrap()
        .id
        .clone();
    let rent_acc = accounts
        .iter()
        .find(|a| a.name == "Rent")
        .unwrap()
        .id
        .clone();
    assert!(
        accounts
            .iter()
            .all(|a| a.system_role.as_deref() != Some("cash")),
        "cash can't be a purchase account"
    );
    Books {
        core,
        supplier,
        unregistered,
        customer: customer_id,
        purchases_acc,
        rent_acc,
    }
}

fn pline(account: &str, price: &str, rate: &str, itc: Option<&str>) -> PurchaseLineInput {
    PurchaseLineInput {
        account_id: account.into(),
        description: "Steel sheets".into(),
        hsn_sac: Some("7208".into()),
        quantity: "1".into(),
        unit_price: price.into(),
        tax_category: "taxable".into(),
        gst_rate: Some(rate.into()),
        itc_status: itc.map(str::to_owned),
        ..Default::default()
    }
}

fn bill(party: &str, number: &str, lines: Vec<PurchaseLineInput>) -> PurchaseDocumentInput {
    PurchaseDocumentInput {
        doc_type: "bill".into(),
        party_id: Some(party.into()),
        supplier_invoice_number: Some(number.into()),
        supplier_invoice_date: Some("2026-11-05".into()),
        doc_date: "2026-11-06".into(),
        lines,
        ..Default::default()
    }
}

async fn gst_payable(core: &AppCore) -> Money {
    core.dashboard_summary(&ctx(), "2026-11-01", "2026-11-30")
        .await
        .unwrap()
        .estimated_gst_payable
}

async fn balance_of(core: &AppCore, id: &str) -> i64 {
    core.list_parties(&ctx())
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.party.id == id)
        .unwrap()
        .balance_paise
}

#[tokio::test]
async fn supplier_bill_posts_payable_and_tracks_itc() {
    let b = books().await;
    // A sale first, so output GST exists: ₹1,00,000 @ 18% intra = ₹18,000.
    b.core
        .issue_sales_document(
            &ctx(),
            None,
            invoice(
                &b.customer,
                "2026-11-02",
                vec![line(None, "1", "100000", "18")],
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(gst_payable(&b.core).await, Money::from_rupees(18_000));

    let v = b
        .core
        .record_purchase(
            &ctx(),
            None,
            bill(
                &b.supplier,
                "sc/101",
                vec![pline(&b.purchases_acc, "50000", "18", None)],
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        v.document.supplier_invoice_number.as_deref(),
        Some("SC/101")
    );
    assert_eq!(
        (v.document.total_paise, v.document.cgst_paise),
        (5_900_000, 450_000)
    );
    assert_eq!(v.lines[0].itc_status, "pending", "ITC is never assumed");
    assert_eq!(v.display_status, PurchaseStatus::Unpaid);
    assert_eq!(
        v.document.due_date.as_deref(),
        Some("2026-12-05"),
        "30 days from the supplier's invoice date"
    );
    assert_eq!(
        balance_of(&b.core, &b.supplier).await,
        -5_900_000,
        "we owe the supplier"
    );
    assert_eq!(
        gst_payable(&b.core).await,
        Money::from_rupees(18_000),
        "pending ITC doesn't reduce the estimate"
    );
    let list = b.core.list_purchases(&ctx()).await.unwrap();
    assert_eq!(list[0].itc_pending_lines, 1);

    // The same supplier invoice can't be recorded twice.
    let err = b
        .core
        .record_purchase(
            &ctx(),
            None,
            bill(
                &b.supplier,
                "SC/101",
                vec![pline(&b.purchases_acc, "1", "18", None)],
            ),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["supplierInvoiceNumber"]);

    // Review: eligible → full credit; partial → part credit, rest to cost.
    let line = v.lines[0].id.clone();
    let v = b
        .core
        .review_itc(
            &ctx(),
            &v.document.id,
            &line,
            ItcReviewInput {
                status: "eligible".into(),
                ..Default::default()
            },
            v.document.row_version,
        )
        .await
        .unwrap();
    assert_eq!(gst_payable(&b.core).await, Money::from_rupees(9_000));
    let err = b
        .core
        .review_itc(
            &ctx(),
            &v.document.id,
            &line,
            ItcReviewInput {
                status: "partial".into(),
                eligible: Some("9000".into()),
                reason: Some("Common credit, rule 42".into()),
            },
            v.document.row_version,
        )
        .await
        .unwrap_err();
    assert_eq!(
        fields(&err),
        vec!["itcEligible"],
        "partial must be less than the full tax"
    );
    let v = b
        .core
        .review_itc(
            &ctx(),
            &v.document.id,
            &line,
            ItcReviewInput {
                status: "partial".into(),
                eligible: Some("3000".into()),
                reason: Some("Common credit, rule 42".into()),
            },
            v.document.row_version,
        )
        .await
        .unwrap();
    assert_eq!(gst_payable(&b.core).await, Money::from_rupees(15_000));
    assert_eq!(v.lines[0].itc_eligible_paise, 300_000);
    let err = b
        .core
        .review_itc(
            &ctx(),
            &v.document.id,
            &line,
            ItcReviewInput {
                status: "ineligible".into(),
                ..Default::default()
            },
            v.document.row_version,
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["itcReason"]);

    // Cancelling reverses everything, including the ITC reviews.
    let v = b
        .core
        .cancel_purchase(
            &ctx(),
            &v.document.id,
            "Entered twice",
            v.document.row_version,
        )
        .await
        .unwrap();
    assert_eq!(v.display_status, PurchaseStatus::Cancelled);
    assert_eq!(balance_of(&b.core, &b.supplier).await, 0);
    assert_eq!(gst_payable(&b.core).await, Money::from_rupees(18_000));
    // …and the invoice number can be recorded again once cancelled.
    b.core
        .record_purchase(
            &ctx(),
            None,
            bill(
                &b.supplier,
                "SC/101",
                vec![pline(&b.purchases_acc, "100", "18", None)],
            ),
            None,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn expenses_cash_purchases_and_itc_rules() {
    let b = books().await;
    let mut exp = PurchaseDocumentInput {
        doc_type: "expense".into(),
        doc_date: "2026-11-10".into(),
        payment_mode: Some("cash".into()),
        lines: vec![PurchaseLineInput {
            description: "Office tea".into(),
            hsn_sac: None,
            ..pline(&b.rent_acc, "500", "5", Some("eligible"))
        }],
        ..Default::default()
    };
    // No supplier GSTIN: ITC can't be claimed.
    let err = b
        .core
        .record_purchase(&ctx(), None, exp.clone(), None)
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["lines.0.itcStatus"]);
    exp.lines[0].itc_status = Some("ineligible".into());
    exp.lines[0].itc_reason = Some("No tax invoice".into());
    let v = b
        .core
        .record_purchase(&ctx(), None, exp.clone(), None)
        .await
        .unwrap();
    assert_eq!(
        (
            v.display_status,
            v.document.paid_paise,
            v.document.total_paise
        ),
        (PurchaseStatus::Paid, 52_500, 52_500)
    );
    let d = b
        .core
        .dashboard_summary(&ctx(), "2026-11-01", "2026-11-30")
        .await
        .unwrap();
    assert_eq!(
        d.cash_and_bank,
        Money::from_paise(-52_500),
        "paid from cash"
    );

    // Credit purchases need a supplier; customer-only parties can't supply.
    exp.payment_mode = Some("credit".into());
    assert!(
        fields(
            &b.core
                .record_purchase(&ctx(), None, exp.clone(), None)
                .await
                .unwrap_err()
        )
        .contains(&"partyId")
    );
    exp.party_id = Some(b.customer.clone());
    assert!(
        fields(
            &b.core
                .record_purchase(&ctx(), None, exp, None)
                .await
                .unwrap_err()
        )
        .contains(&"partyId")
    );

    // Bad account, missing invoice details.
    let mut bad = bill(
        &b.supplier,
        "",
        vec![PurchaseLineInput {
            account_id: "nope".into(),
            ..pline(&b.rent_acc, "10", "18", None)
        }],
    );
    bad.supplier_invoice_date = None;
    let err = b
        .core
        .record_purchase(&ctx(), None, bad, None)
        .await
        .unwrap_err();
    let f = fields(&err);
    for expected in [
        "supplierInvoiceNumber",
        "supplierInvoiceDate",
        "lines.0.accountId",
    ] {
        assert!(f.contains(&expected), "missing {expected} in {f:?}");
    }
}

#[tokio::test]
async fn reverse_charge_and_debit_notes() {
    let b = books().await;
    // Freight from an unregistered transporter under reverse charge: ₹10,000 @ 5%.
    let mut rcm = bill(
        &b.unregistered,
        "LR-77",
        vec![PurchaseLineInput {
            hsn_sac: Some("996511".into()),
            ..pline(&b.rent_acc, "10000", "5", Some("eligible"))
        }],
    );
    rcm.reverse_charge = true;
    let calc = b
        .core
        .calculate_purchase(&ctx(), rcm.clone())
        .await
        .unwrap();
    assert_eq!(
        (calc.totals.total_paise, calc.reverse_charge_tax_paise),
        (1_000_000, 50_000)
    );
    let v = b
        .core
        .record_purchase(&ctx(), None, rcm, None)
        .await
        .unwrap();
    assert_eq!(
        v.document.total_paise, 1_000_000,
        "the transporter is paid without GST"
    );
    assert_eq!(balance_of(&b.core, &b.unregistered).await, -1_000_000);

    // Debit note for goods returned to a regular supplier.
    let bill_v = b
        .core
        .record_purchase(
            &ctx(),
            None,
            bill(
                &b.supplier,
                "SC/200",
                vec![pline(&b.purchases_acc, "20000", "18", Some("eligible"))],
            ),
            None,
        )
        .await
        .unwrap();
    let dn = |price: &str| PurchaseDocumentInput {
        doc_type: "debit_note".into(),
        doc_date: "2026-11-12".into(),
        original_document_id: Some(bill_v.document.id.clone()),
        reason: Some("Damaged sheets returned".into()),
        lines: vec![pline(&b.purchases_acc, price, "18", Some("eligible"))],
        ..Default::default()
    };
    let err = b
        .core
        .record_purchase(&ctx(), None, dn("30000"), None)
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["lines"], "can't exceed the bill");
    let note = b
        .core
        .record_purchase(&ctx(), None, dn("5000"), None)
        .await
        .unwrap();
    assert_eq!(note.document.number.as_deref(), Some("DN/26-27/0001"));
    assert_eq!(note.document.party_id.as_deref(), Some(b.supplier.as_str()));
    assert_eq!(balance_of(&b.core, &b.supplier).await, -2_360_000 + 590_000);
    let bill_view = b
        .core
        .get_purchase(&ctx(), &bill_v.document.id)
        .await
        .unwrap();
    assert_eq!(
        (bill_view.debit_notes.len(), bill_view.debited_paise),
        (1, 590_000)
    );
    let err = b
        .core
        .cancel_purchase(
            &ctx(),
            &bill_v.document.id,
            "x",
            bill_v.document.row_version,
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "conflict");
}

#[tokio::test]
async fn attachments_are_hashed_deduplicated_and_kept() {
    let b = books().await;
    let draft = b
        .core
        .save_purchase_draft(
            &ctx(),
            None,
            bill(
                &b.supplier,
                "SC/300",
                vec![pline(&b.purchases_acc, "100", "18", None)],
            ),
            None,
        )
        .await
        .unwrap();
    let id = draft.document.id.clone();
    let a = b
        .core
        .add_attachment(&ctx(), &id, "bill.pdf", b"hello")
        .await
        .unwrap();
    assert_eq!(
        a.sha256,
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
    assert_eq!(a.sha256, sha256_hex(b"hello"));
    assert_eq!(
        (a.content_type.as_str(), a.size_bytes),
        ("application/pdf", 5)
    );
    let same = b
        .core
        .add_attachment(&ctx(), &id, "copy.PDF", b"hello")
        .await
        .unwrap();
    assert_eq!(same.sha256, a.sha256, "identical content stored once");
    assert!(
        b.core
            .add_attachment(&ctx(), &id, "virus.exe", b"MZ")
            .await
            .is_err()
    );
    let c = b.core.attachment_content(&ctx(), &a.id).await.unwrap();
    assert_eq!(
        (c.file_name.as_str(), c.bytes.as_slice()),
        ("bill.pdf", b"hello".as_slice())
    );

    b.core.remove_attachment(&ctx(), &same.id).await.unwrap();
    let v = b
        .core
        .record_purchase(
            &ctx(),
            Some(&id),
            bill(
                &b.supplier,
                "SC/300",
                vec![pline(&b.purchases_acc, "100", "18", None)],
            ),
            Some(draft.document.row_version),
        )
        .await
        .unwrap();
    assert_eq!(v.attachments.len(), 1);
    assert_eq!(
        b.core
            .remove_attachment(&ctx(), &a.id)
            .await
            .unwrap_err()
            .code(),
        "conflict",
        "kept once recorded"
    );
}

#[tokio::test]
async fn round_off_is_not_a_purchase_account() {
    let b = books().await;
    let accounts = b.core.purchase_accounts(&ctx()).await.unwrap();
    assert!(
        accounts
            .iter()
            .all(|a| a.system_role.as_deref() != Some("round_off"))
    );
    assert!(
        accounts
            .iter()
            .any(|a| a.system_role.as_deref() == Some("fixed_assets"))
    );
}

// ---------------------------------------------------------------------------
// Phase 1.5: banking
// ---------------------------------------------------------------------------

use crate::banking::{
    AllocationInput, MoneyAccountInput, OpeningBalanceInput, StatementImportInput, VoucherInput,
};

async fn money_account(core: &AppCore, role: &str) -> String {
    core.list_money_accounts(&ctx())
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.system_role.as_deref() == Some(role))
        .unwrap()
        .account
        .id
}

fn receipt(party: &str, money: &str, date: &str, amount: &str) -> VoucherInput {
    VoucherInput {
        voucher_type: "receipt".into(),
        voucher_date: date.into(),
        money_account_id: money.into(),
        party_id: Some(party.into()),
        amount: amount.into(),
        mode: Some("neft".into()),
        ..Default::default()
    }
}

fn alloc(doc: &str, amount: &str) -> AllocationInput {
    AllocationInput {
        document_id: doc.into(),
        amount: amount.into(),
    }
}

#[tokio::test]
async fn money_accounts_and_opening_balances() {
    let core = core().await;
    set_up(&core).await;
    let accounts = core.list_money_accounts(&ctx()).await.unwrap();
    assert_eq!(accounts.len(), 2, "setup creates cash and bank");
    assert_eq!(accounts[0].account.kind, "cash");

    let bad = core
        .create_money_account(
            &ctx(),
            &MoneyAccountInput {
                kind: "bank".into(),
                name: "Cash in hand".into(),
                ifsc: Some("HDFC123".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&bad), vec!["ifsc", "name"]);

    let hdfc = core
        .create_money_account(
            &ctx(),
            &MoneyAccountInput {
                kind: "bank".into(),
                name: "HDFC Current".into(),
                bank_name: Some("HDFC Bank".into()),
                account_number: Some("5010 0012 3456".into()),
                ifsc: Some("hdfc0001234".into()),
                opening_balance: Some("50,000".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(hdfc.ledger_code, "1011");
    assert_eq!(hdfc.balance_paise, 5_000_000);
    assert_eq!(hdfc.account.ifsc.as_deref(), Some("HDFC0001234"));
    assert_eq!(hdfc.account.account_number.as_deref(), Some("501000123456"));

    // Changing the opening balance reverses the old entry and posts the new.
    let hdfc = core
        .update_money_account(
            &ctx(),
            &hdfc.account.id,
            &MoneyAccountInput {
                kind: "bank".into(),
                name: "HDFC CA".into(),
                opening_balance: Some("40000".into()),
                ..Default::default()
            },
            hdfc.account.row_version,
        )
        .await
        .unwrap();
    assert_eq!(hdfc.balance_paise, 4_000_000);
    assert_eq!(hdfc.opening_balance_paise, 4_000_000);
    let d = core
        .dashboard_summary(&ctx(), "2026-11-01", "2026-11-30")
        .await
        .unwrap();
    assert_eq!(d.cash_and_bank, Money::from_rupees(40_000));

    // A balance blocks closing the account.
    let err = core
        .update_money_account(
            &ctx(),
            &hdfc.account.id,
            &MoneyAccountInput {
                kind: "bank".into(),
                name: "HDFC CA".into(),
                opening_balance: Some("40000".into()),
                is_active: Some(false),
                ..Default::default()
            },
            hdfc.account.row_version,
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["isActive"]);

    // Capital: a credit opening balance; receivables are not eligible.
    let openings = core.list_opening_balances(&ctx()).await.unwrap();
    assert!(
        openings
            .iter()
            .all(|o| o.system_role.as_deref() != Some("receivables"))
    );
    let capital = openings
        .iter()
        .find(|o| o.system_role.as_deref() == Some("capital"))
        .unwrap();
    let set = core
        .set_opening_balance(
            &ctx(),
            &OpeningBalanceInput {
                account_id: capital.account_id.clone(),
                amount: Some("40000".into()),
                side: Some("credit".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(set.amount_paise, -4_000_000);
}

#[tokio::test]
async fn receipts_settle_invoices_and_cancelling_releases_them() {
    let s = shop().await;
    let core = &s.core;
    let bank = money_account(core, "bank").await;
    let mut ids = Vec::new();
    for date in ["2026-11-02", "2026-11-03"] {
        let v = core
            .issue_sales_document(
                &ctx(),
                None,
                invoice(&s.customer, date, vec![line(None, "1", "1000", "18")]),
                None,
            )
            .await
            .unwrap();
        ids.push(v.document.id);
    }
    let open = core
        .open_documents(&ctx(), &s.customer, "receipt")
        .await
        .unwrap();
    assert_eq!(open.len(), 2);
    assert_eq!(open[0].outstanding_paise, 118_000);
    assert_eq!(ravi_balance(core).await, 236_000);

    // Over-allocation is refused field by field.
    let err = core
        .create_voucher(
            &ctx(),
            &VoucherInput {
                allocations: vec![alloc(&ids[0], "1180"), alloc(&ids[1], "1181")],
                ..receipt(&s.customer, &bank, "2026-11-10", "2000")
            },
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["allocations.1.amount"]);
    let err = core
        .create_voucher(
            &ctx(),
            &VoucherInput {
                allocations: vec![alloc(&ids[0], "1180"), alloc(&ids[1], "1000")],
                ..receipt(&s.customer, &bank, "2026-11-18", "2000")
            },
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["voucherDate", "allocations"]);

    let v = core
        .create_voucher(
            &ctx(),
            &VoucherInput {
                allocations: vec![alloc(&ids[0], "1180"), alloc(&ids[1], "820")],
                ..receipt(&s.customer, &bank, "2026-11-10", "2000")
            },
        )
        .await
        .unwrap();
    assert_eq!(v.voucher.number, "RCT/2026-27/0001");
    assert_eq!(v.allocated_paise, 200_000);
    assert_eq!(v.on_account_paise, 0);
    assert_eq!(ravi_balance(core).await, 36_000);
    let a = core.get_sales_document(&ctx(), &ids[0]).await.unwrap();
    let b = core.get_sales_document(&ctx(), &ids[1]).await.unwrap();
    assert_eq!(a.display_status, DisplayStatus::Paid);
    assert_eq!(b.display_status, DisplayStatus::PartPaid);
    assert_eq!(
        core.document_settlements(&ctx(), "sales", &ids[1])
            .await
            .unwrap()[0]
            .amount_paise,
        82_000
    );
    // Paid invoices can't be cancelled.
    assert!(
        core.cancel_sales_document(&ctx(), &ids[0], "x", a.document.row_version)
            .await
            .is_err()
    );
    let open = core
        .open_documents(&ctx(), &s.customer, "receipt")
        .await
        .unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].outstanding_paise, 36_000);

    // An advance: nothing allocated.
    let adv = core
        .create_voucher(&ctx(), &receipt(&s.customer, &bank, "2026-11-11", "500"))
        .await
        .unwrap();
    assert_eq!(adv.voucher.number, "RCT/2026-27/0002");
    assert_eq!(adv.on_account_paise, 50_000);
    assert_eq!(ravi_balance(core).await, -14_000);

    // Cancelling reverses the posting and releases the invoices.
    let c = core
        .cancel_voucher(
            &ctx(),
            &v.voucher.id,
            "wrong customer",
            v.voucher.row_version,
        )
        .await
        .unwrap();
    assert_eq!(c.voucher.status, "cancelled");
    assert_eq!(ravi_balance(core).await, 186_000);
    let a = core.get_sales_document(&ctx(), &ids[0]).await.unwrap();
    assert_eq!(a.display_status, DisplayStatus::Issued);
    assert_eq!(a.document.paid_paise, 0);
    assert!(
        core.cancel_voucher(&ctx(), &v.voucher.id, "again", c.voucher.row_version)
            .await
            .is_err()
    );
    let book = core.money_book(&ctx(), &bank, None, None).await.unwrap();
    assert_eq!(book.closing_paise, 50_000);
    assert_eq!(book.lines.len(), 3);
    assert_eq!(book.debit_total_paise, 250_000);
    assert_eq!(book.credit_total_paise, 200_000);
}

#[tokio::test]
async fn payments_settle_bills_and_book_other_accounts() {
    let b = books().await;
    let core = &b.core;
    let cash = money_account(core, "cash").await;
    let bank = money_account(core, "bank").await;
    let bill = core
        .record_purchase(
            &ctx(),
            None,
            bill(
                &b.supplier,
                "sc/9",
                vec![pline(&b.purchases_acc, "1000", "18", None)],
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(balance_of(core, &b.supplier).await, -118_000);
    let p = core
        .create_voucher(
            &ctx(),
            &VoucherInput {
                voucher_type: "payment".into(),
                voucher_date: "2026-11-12".into(),
                money_account_id: bank.clone(),
                party_id: Some(b.supplier.clone()),
                amount: "1180".into(),
                mode: Some("upi".into()),
                reference: Some("UTR123".into()),
                allocations: vec![alloc(&bill.document.id, "1180")],
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(p.voucher.number, "PMT/2026-27/0001");
    assert_eq!(balance_of(core, &b.supplier).await, 0);
    let after = core.get_purchase(&ctx(), &bill.document.id).await.unwrap();
    assert_eq!(after.display_status, PurchaseStatus::Paid);

    // Bank charges: no party, booked to an expense account. Cash forces mode.
    let charges = core
        .voucher_accounts(&ctx())
        .await
        .unwrap()
        .into_iter()
        .find(|a| a.system_role.as_deref() == Some("bank_charges"))
        .unwrap();
    let v = core
        .create_voucher(
            &ctx(),
            &VoucherInput {
                voucher_type: "payment".into(),
                voucher_date: "2026-11-12".into(),
                money_account_id: cash.clone(),
                counter_account_id: Some(charges.id.clone()),
                amount: "50".into(),
                mode: Some("neft".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(v.voucher.mode, "cash");
    assert_eq!(v.on_account_paise, 0);
    // A party and an account at once is refused.
    let err = core
        .create_voucher(
            &ctx(),
            &VoucherInput {
                voucher_type: "payment".into(),
                voucher_date: "2026-11-12".into(),
                money_account_id: cash.clone(),
                party_id: Some(b.supplier.clone()),
                counter_account_id: Some(charges.id.clone()),
                amount: "50".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&err), vec!["partyId"]);

    // Transfer: cash deposited into the bank is a receipt into the bank from
    // the cash ledger.
    let accounts = core.voucher_accounts(&ctx()).await.unwrap();
    let cash_ledger = accounts
        .iter()
        .find(|a| a.system_role.as_deref() == Some("cash"))
        .unwrap();
    core.create_voucher(
        &ctx(),
        &VoucherInput {
            voucher_type: "receipt".into(),
            voucher_date: "2026-11-13".into(),
            money_account_id: bank.clone(),
            counter_account_id: Some(cash_ledger.id.clone()),
            amount: "200".into(),
            mode: Some("cash".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let cash_book = core
        .money_book(&ctx(), &cash, Some("2026-11-13"), None)
        .await
        .unwrap();
    assert_eq!(cash_book.opening_paise, -5_000);
    assert_eq!(cash_book.closing_paise, -25_000);
    assert_eq!(cash_book.lines.len(), 1);
}

const STATEMENT: &str = "\
Account statement,,,,,
,,,,,
Date,Narration,Chq./Ref.No.,Withdrawal Amt.,Deposit Amt.,Closing Balance
10/11/26,NEFT-RAVI STORES,UTR777,,\"1,180.00\",\"1,180.00\"
12/11/26,SMS CHARGES,,118.00,,\"1,062.00\"
13/11/26,UPI-SOMEONE,,,10.00,\"1,072.00\"
";

#[tokio::test]
async fn statement_import_reconciles_against_the_books() {
    let s = shop().await;
    let core = &s.core;
    let bank = money_account(core, "bank").await;
    let r = core
        .create_voucher(&ctx(), &receipt(&s.customer, &bank, "2026-11-09", "1180"))
        .await
        .unwrap();

    let file = core
        .read_statement_file(&ctx(), &bank, "hdfc.csv", STATEMENT.as_bytes())
        .await
        .unwrap();
    assert!(!file.mapping_saved);
    assert_eq!(file.mapping.header_row, 1);
    let input = StatementImportInput {
        money_account_id: bank.clone(),
        file_name: file.file_name.clone(),
        rows: file.rows.clone(),
        mapping: file.mapping.clone(),
    };
    let preview = core.preview_statement(&ctx(), &input).await.unwrap();
    assert_eq!(preview.new_lines, 3);
    assert_eq!(preview.deposits_paise, 119_000);
    assert_eq!(preview.withdrawals_paise, 11_800);
    let first = core.import_statement(&ctx(), &input).await.unwrap();
    assert_eq!((first.imported, first.duplicates), (3, 0));
    // Re-importing an overlapping statement adds nothing; the mapping is remembered.
    let again = core
        .read_statement_file(&ctx(), &bank, "hdfc.csv", STATEMENT.as_bytes())
        .await
        .unwrap();
    assert!(again.mapping_saved);
    let second = core.import_statement(&ctx(), &input).await.unwrap();
    assert_eq!((second.imported, second.duplicates), (0, 3));

    let rec = core.reconciliation(&ctx(), &bank).await.unwrap();
    assert_eq!(rec.account.unmatched_lines, 3);
    assert_eq!(rec.account.statement_balance_paise, Some(107_200));
    let neft = rec
        .lines
        .iter()
        .find(|l| l.line.reference.as_deref() == Some("UTR777"))
        .unwrap();
    assert_eq!(neft.suggestions.len(), 1);
    assert_eq!(neft.suggestions[0].entry_id, r.voucher.journal_entry_id);
    assert_eq!(neft.suggestions[0].day_gap, -1);
    core.match_statement_line(&ctx(), &neft.line.id, &r.voucher.journal_entry_id)
        .await
        .unwrap();
    // A reconciled voucher cannot be cancelled until it is unmatched.
    let err = core
        .cancel_voucher(&ctx(), &r.voucher.id, "x", r.voucher.row_version)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "conflict");

    // Charges: create the payment straight from the line.
    let charges_line = rec
        .lines
        .iter()
        .find(|l| l.line.withdrawal_paise == 11_800)
        .unwrap();
    let charges = core
        .voucher_accounts(&ctx())
        .await
        .unwrap()
        .into_iter()
        .find(|a| a.system_role.as_deref() == Some("bank_charges"))
        .unwrap();
    let wrong = core
        .create_voucher(
            &ctx(),
            &VoucherInput {
                voucher_type: "receipt".into(),
                voucher_date: charges_line.line.line_date.clone(),
                money_account_id: bank.clone(),
                counter_account_id: Some(charges.id.clone()),
                amount: "118".into(),
                statement_line_id: Some(charges_line.line.id.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(fields(&wrong), vec!["amount"]);
    let pay = core
        .create_voucher(
            &ctx(),
            &VoucherInput {
                voucher_type: "payment".into(),
                voucher_date: charges_line.line.line_date.clone(),
                money_account_id: bank.clone(),
                counter_account_id: Some(charges.id.clone()),
                amount: "118".into(),
                statement_line_id: Some(charges_line.line.id.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(pay.statement_line_id.is_some());

    let upi = rec
        .lines
        .iter()
        .find(|l| l.line.deposit_paise == 1_000)
        .unwrap();
    assert!(upi.suggestions.is_empty());
    core.ignore_statement_line(&ctx(), &upi.line.id)
        .await
        .unwrap();
    let rec = core.reconciliation(&ctx(), &bank).await.unwrap();
    assert_eq!(rec.account.unmatched_lines, 0);
    assert!(rec.unmatched_entries.is_empty());

    // Unmatch, then undo the import: only unreconciled lines go.
    core.unmatch_statement_line(&ctx(), &upi.line.id)
        .await
        .unwrap();
    let removed = core
        .undo_statement_import(&ctx(), &bank, &first.batch)
        .await
        .unwrap();
    assert_eq!(removed, 1);
    let rec = core.reconciliation(&ctx(), &bank).await.unwrap();
    assert_eq!(rec.lines.len(), 2);
    assert!(rec.lines.iter().all(|l| l.line.status == "matched"));
}

#[tokio::test]
async fn operators_cannot_post_receipts() {
    let s = shop().await;
    let bank = money_account(&s.core, "bank").await;
    let op = RequestContext {
        actor: user("op", Role::Operator),
        ..ctx()
    };
    let err = s
        .core
        .create_voucher(&op, &receipt(&s.customer, &bank, "2026-11-10", "100"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "forbidden");
}
