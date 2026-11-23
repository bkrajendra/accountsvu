//! Golden-file test for the GSTR-1 JSON adapter. Run with
//! `UPDATE_GOLDEN=1 cargo test -p av-gst-formats` to rewrite the file after a
//! deliberate format change (and bump `ADAPTER_VERSION`).

use av_gst_formats::gstr1::*;
use chrono::NaiveDate;

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn item(rate_bp: u32, taxable: i64, igst: i64, cgst: i64, sgst: i64) -> RateItem {
    RateItem {
        rate_bp,
        taxable_paise: taxable,
        igst_paise: igst,
        cgst_paise: cgst,
        sgst_paise: sgst,
        cess_paise: 0,
    }
}

fn sample() -> Gstr1Return {
    Gstr1Return {
        gstin: "27AAPFU0939F1ZV".into(),
        year: 2026,
        month: 10,
        b2b: vec![
            Invoice {
                ctin: Some("29AABCU9603R1ZJ".into()),
                number: "INV/26-27/0001".into(),
                date: d(2026, 10, 3),
                value_paise: 11_800_000,
                pos: 29,
                reverse_charge: false,
                intra_state: false,
                invoice_type: InvoiceType::Regular,
                export_with_payment: false,
                items: vec![item(1800, 10_000_000, 1_800_000, 0, 0)],
            },
            Invoice {
                ctin: Some("27AABCT3518Q1ZW".into()),
                number: "INV/26-27/0002".into(),
                date: d(2026, 10, 9),
                value_paise: 1_590_000,
                pos: 27,
                reverse_charge: false,
                intra_state: true,
                invoice_type: InvoiceType::Regular,
                export_with_payment: false,
                items: vec![
                    item(500, 1_000_000, 0, 25_000, 25_000),
                    item(1800, 300_000, 0, 27_000, 27_000),
                ],
            },
            Invoice {
                ctin: Some("29AABCU9603R1ZJ".into()),
                number: "INV/26-27/0004".into(),
                date: d(2026, 10, 20),
                value_paise: 500_000,
                pos: 29,
                reverse_charge: false,
                intra_state: false,
                invoice_type: InvoiceType::SezWithoutPayment,
                export_with_payment: false,
                items: vec![item(1800, 500_000, 0, 0, 0)],
            },
        ],
        b2cl: vec![Invoice {
            ctin: None,
            number: "INV/26-27/0003".into(),
            date: d(2026, 10, 12),
            value_paise: 17_700_000,
            pos: 24,
            reverse_charge: false,
            intra_state: false,
            invoice_type: InvoiceType::Regular,
            export_with_payment: false,
            items: vec![item(1800, 15_000_000, 2_700_000, 0, 0)],
        }],
        b2cs: vec![
            B2csRow {
                pos: 27,
                intra_state: true,
                item: item(500, 200_050, 0, 5_001, 5_001),
            },
            B2csRow {
                pos: 7,
                intra_state: false,
                item: item(1800, 100_000, 18_000, 0, 0),
            },
        ],
        exp: vec![Invoice {
            ctin: None,
            number: "EXP/26-27/0001".into(),
            date: d(2026, 10, 15),
            value_paise: 5_000_000,
            pos: 96,
            reverse_charge: false,
            intra_state: false,
            invoice_type: InvoiceType::Regular,
            export_with_payment: false,
            items: vec![item(1800, 5_000_000, 0, 0, 0)],
        }],
        cdnr: vec![Note {
            ctin: Some("29AABCU9603R1ZJ".into()),
            note_type: NoteType::Credit,
            number: "CN/26-27/0001".into(),
            date: d(2026, 10, 25),
            value_paise: 1_180_000,
            pos: 29,
            reverse_charge: false,
            intra_state: false,
            invoice_type: InvoiceType::Regular,
            unregistered_type: None,
            items: vec![item(1800, 1_000_000, 180_000, 0, 0)],
        }],
        cdnur: vec![Note {
            ctin: None,
            note_type: NoteType::Credit,
            number: "CN/26-27/0002".into(),
            date: d(2026, 10, 28),
            value_paise: 1_180_000,
            pos: 24,
            reverse_charge: false,
            intra_state: false,
            invoice_type: InvoiceType::Regular,
            unregistered_type: Some(UnregisteredType::B2cl),
            items: vec![item(1800, 1_000_000, 180_000, 0, 0)],
        }],
        nil: vec![
            NilRow {
                supply: NilSupply::IntraUnregistered,
                nil_paise: 0,
                exempt_paise: 45_000,
                non_gst_paise: 0,
            },
            NilRow {
                supply: NilSupply::InterRegistered,
                nil_paise: 120_000,
                exempt_paise: 0,
                non_gst_paise: 0,
            },
        ],
        hsn_b2b: vec![HsnRow {
            hsn: "8415".into(),
            description: "Air conditioners".into(),
            uqc: "NOS".into(),
            quantity: "4".into(),
            item: item(1800, 9_000_000, 1_620_000, 0, 0),
        }],
        hsn_b2c: vec![HsnRow {
            hsn: "998314".into(),
            description: "IT design and development".into(),
            uqc: "NA".into(),
            quantity: "0".into(),
            item: item(1800, 100_000, 18_000, 0, 0),
        }],
        documents: vec![
            DocumentSeries {
                nature: DocumentNature::CreditNotes,
                from: "CN/26-27/0001".into(),
                to: "CN/26-27/0002".into(),
                total: 2,
                cancelled: 0,
            },
            DocumentSeries {
                nature: DocumentNature::OutwardInvoices,
                from: "INV/26-27/0001".into(),
                to: "INV/26-27/0005".into(),
                total: 5,
                cancelled: 1,
            },
        ],
        // ₹11,800 advance at 18% intra-state; ₹5,900 of an earlier one adjusted.
        at: vec![AdvanceRow {
            pos: 27,
            intra_state: true,
            items: vec![item(1800, 1_000_000, 0, 90_000, 90_000)],
        }],
        txpd: vec![AdvanceRow {
            pos: 29,
            intra_state: false,
            items: vec![item(1800, 500_000, 90_000, 0, 0)],
        }],
    }
}

fn check(name: &str, actual: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing golden file {name}; run with UPDATE_GOLDEN=1"))
        .replace("\r\n", "\n");
    assert_eq!(actual, expected, "GSTR-1 JSON differs from {name}");
}

#[test]
fn full_return_matches_golden_file() {
    let json = to_json(&sample());
    // Valid JSON with exact decimal numbers.
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["fp"], "102026");
    assert_eq!(
        v["b2b"].as_array().unwrap().len(),
        2,
        "grouped by recipient GSTIN"
    );
    assert!(json.contains("\"txval\": 2000.50"));
    assert_eq!(v["at"][0]["sply_ty"], "INTRA");
    assert!(json.contains("\"ad_amt\": 10000"));
    check("gstr1_full.json", &json);
}

#[test]
fn empty_return_has_only_the_header() {
    let r = Gstr1Return {
        gstin: "27AAPFU0939F1ZV".into(),
        year: 2027,
        month: 3,
        ..Default::default()
    };
    check("gstr1_empty.json", &to_json(&r));
}
