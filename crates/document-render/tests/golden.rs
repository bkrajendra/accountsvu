//! Golden-file tests for printed documents.
//!
//! Each case checks two goldens: the print view (pretty JSON, reviewable as
//! text) and the SHA-256 of the PDF bytes (catches any layout change). Run
//! with `UPDATE_GOLDEN=1` to rewrite them after an intended change, then look
//! at the PDFs written to `target/golden/` before committing.

use std::path::PathBuf;

use av_document_render::{
    InvoiceData, InvoiceLine, Logo, LogoFormat, PartyBlock, invoice_page_count, invoice_view_json,
    render_invoice,
};
use sha2::{Digest, Sha256};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn out_dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/golden");
    std::fs::create_dir_all(&d).expect("golden output dir");
    d
}

fn check(name: &str, data: &InvoiceData) {
    let view = invoice_view_json(data);
    let pdf = render_invoice(data).expect("renders");
    assert!(pdf.starts_with(b"%PDF-"), "{name}: not a PDF");
    assert_eq!(
        pdf,
        render_invoice(data).expect("renders"),
        "{name}: output is not deterministic"
    );
    let hash = format!("{:x}\n", Sha256::digest(&pdf));
    std::fs::write(out_dir().join(format!("{name}.pdf")), &pdf).expect("write pdf");

    let view_path = dir().join(format!("{name}.json"));
    let hash_path = dir().join(format!("{name}.pdf.sha256"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&view_path, &view).expect("write view golden");
        std::fs::write(&hash_path, &hash).expect("write hash golden");
        return;
    }
    let want_view = std::fs::read_to_string(&view_path)
        .unwrap_or_else(|_| panic!("{name}: missing golden; run with UPDATE_GOLDEN=1"));
    assert_eq!(
        view.replace("\r\n", "\n"),
        want_view.replace("\r\n", "\n"),
        "{name}: print view changed"
    );
    let want_hash = std::fs::read_to_string(&hash_path).expect("hash golden");
    assert_eq!(
        hash.trim(),
        want_hash.trim(),
        "{name}: PDF changed (see target/golden/{name}.pdf)"
    );
}

fn seller() -> PartyBlock {
    PartyBlock {
        name: "Khope Traders".into(),
        subtitle: Some("Khope Traders Private Limited".into()),
        address_lines: vec![
            "12 MG Road, Kothrud".into(),
            "Pune 411038, Maharashtra".into(),
        ],
        gstin: Some("27AAPFU0939F1ZV".into()),
        extra: vec!["accounts@khope.example · 98200 00000".into()],
    }
}

#[allow(clippy::too_many_arguments)]
fn line(
    no: i64,
    desc: &str,
    hsn: &str,
    qty: &str,
    unit: &str,
    price: i64,
    bp: i64,
    intra: bool,
) -> InvoiceLine {
    let q: i64 = qty.parse().unwrap_or(1);
    let taxable = price * q;
    let tax = taxable * bp / 10_000;
    InvoiceLine {
        no,
        description: desc.into(),
        hsn_sac: Some(hsn.into()),
        quantity: qty.into(),
        unit: Some(unit.into()),
        unit_price_paise: price,
        discount_paise: 0,
        tax_category: "taxable".into(),
        gst_rate_bp: bp,
        taxable_paise: taxable,
        cgst_paise: if intra { tax / 2 } else { 0 },
        sgst_paise: if intra { tax - tax / 2 } else { 0 },
        igst_paise: if intra { 0 } else { tax },
        ..InvoiceLine::default()
    }
}

fn totals(d: &mut InvoiceData) {
    d.taxable_paise = d.lines.iter().map(|l| l.taxable_paise).sum();
    d.cgst_paise = d.lines.iter().map(|l| l.cgst_paise).sum();
    d.sgst_paise = d.lines.iter().map(|l| l.sgst_paise).sum();
    d.igst_paise = d.lines.iter().map(|l| l.igst_paise).sum();
    let exact = d.taxable_paise + d.cgst_paise + d.sgst_paise + d.igst_paise;
    let rounded = (exact + 50) / 100 * 100;
    d.round_off_paise = rounded - exact;
    d.total_paise = rounded;
}

fn intra_invoice() -> InvoiceData {
    let mut lines = vec![
        line(
            1,
            "Steel rack, 5 shelves",
            "94032090",
            "2",
            "NOS",
            450_000,
            1800,
            true,
        ),
        line(
            2,
            "Installation and setup",
            "998719",
            "1",
            "",
            150_050,
            1800,
            true,
        ),
        line(
            3,
            "Packing material",
            "48191010",
            "10",
            "BOX",
            4_525,
            1200,
            true,
        ),
    ];
    lines[0].discount_paise = 50_000;
    lines[0].taxable_paise -= 50_000;
    lines[0].cgst_paise = lines[0].taxable_paise * 900 / 10_000;
    lines[0].sgst_paise = lines[0].cgst_paise;
    lines[1].unit = None;
    let mut d = InvoiceData {
        title: "Tax invoice".into(),
        number: Some("INV/25-26/0042".into()),
        date: "19 Nov 2026".into(),
        due_date: Some("19 Dec 2026".into()),
        seller: seller(),
        buyer: PartyBlock {
            name: "Sahyadri Stores".into(),
            address_lines: vec!["Shop 4, Station Road".into(), "Nashik 422001".into()],
            gstin: Some("27ABCDE1234F1Z5".into()),
            ..PartyBlock::default()
        },
        place_of_supply: "Maharashtra (27)".into(),
        supply_note: "Within the state · CGST + SGST".into(),
        notes: Some(
            "Bank: HDFC Bank, A/c 50100012345678, IFSC HDFC0001234.\nThank you for your business."
                .into(),
        ),
        lines,
        signatory: Some("Rajendra Khope".into()),
        logo: Some(Logo {
            bytes: include_bytes!("fixtures/logo.png").to_vec(),
            format: LogoFormat::Png,
        }),
        ..InvoiceData::default()
    };
    totals(&mut d);
    d
}

#[test]
fn intra_state_tax_invoice_with_logo() {
    check("tax_invoice_intra_state", &intra_invoice());
}

#[test]
fn inter_state_credit_note_with_svg_logo() {
    let mut d = InvoiceData {
        title: "Credit note".into(),
        number: Some("CN/25-26/0003".into()),
        date: "20 Nov 2026".into(),
        seller: seller(),
        buyer: PartyBlock {
            name: "Deccan Hardware LLP".into(),
            address_lines: vec!["Plot 7, Peenya Industrial Area, Bengaluru 560058".into()],
            gstin: Some("29AAACD1234E1Z2".into()),
            ..PartyBlock::default()
        },
        place_of_supply: "Karnataka (29)".into(),
        supply_note: "Between states · IGST".into(),
        against: Some("INV/25-26/0040 dated 2 Nov 2026".into()),
        reason: Some("Two racks returned damaged in transit".into()),
        lines: vec![line(1, "Steel rack, 5 shelves", "94032090", "2", "NOS", 450_000, 1800, false)],
        logo: Some(Logo {
            bytes: br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 40"><rect width="40" height="40" rx="9" fill="#0A84FF"/><path d="M12 30 L20 10 L28 30 Z" fill="#fff"/><rect x="48" y="14" width="64" height="12" rx="3" fill="#1d1d1f"/></svg>"##.to_vec(),
            format: LogoFormat::Svg,
        }),
        ..InvoiceData::default()
    };
    totals(&mut d);
    check("credit_note_inter_state", &d);
}

#[test]
fn long_draft_breaks_across_pages_without_logo() {
    let mut d = intra_invoice();
    d.number = None;
    d.logo = None;
    d.notes = None;
    d.lines = (1..=60)
        .map(|i| {
            line(
                i,
                &format!("Spare part assortment, lot {i}"),
                "84879000",
                "3",
                "NOS",
                12_345,
                if i % 3 == 0 { 500 } else { 1800 },
                true,
            )
        })
        .collect();
    totals(&mut d);
    assert!(invoice_page_count(&d).expect("lays out") >= 2);
    check("draft_multi_page", &d);
}

#[test]
fn cancelled_bill_of_supply_has_no_tax_table() {
    let mut d = intra_invoice();
    d.title = "Bill of supply".into();
    d.cancelled = true;
    d.logo = None;
    for l in &mut d.lines {
        l.tax_category = "exempt".into();
        l.gst_rate_bp = 0;
        l.cgst_paise = 0;
        l.sgst_paise = 0;
    }
    totals(&mut d);
    let view = invoice_view_json(&d);
    assert!(view.contains("\"tax_rows\": []"));
    assert!(view.contains("CANCELLED"));
    check("bill_of_supply_cancelled", &d);
}

#[test]
fn text_from_data_is_never_markup() {
    let mut d = intra_invoice();
    d.buyer.name = "#set page(paper: \"a0\") *bold* $x^2$ <label> @ref #panic()".into();
    d.notes = Some("]) #include \"/etc/passwd\"".into());
    assert_eq!(invoice_page_count(&d).expect("plain text renders"), 1);
}
