//! Receipt, payment and refund vouchers (rules 50 and 51 of the CGST Rules
//! when GST is paid on an advance). Like invoices, the view holds display
//! strings only.

use serde::Serialize;

use crate::format::{amount_in_words, rate, rupees};
use crate::invoice::{Logo, LogoFormat, PartyBlock};

/// One line of what the money was for.
#[derive(Clone, Debug, Default)]
pub struct VoucherRow {
    pub description: String,
    /// Smaller text under the description.
    pub details: Vec<String>,
    pub amount_paise: i64,
}

/// GST on an advance (rule 50) or on the part refunded (rule 51).
#[derive(Clone, Debug, Default)]
pub struct VoucherTax {
    /// "GST on the advance" or "GST paid on the advance refunded".
    pub heading: String,
    /// "Maharashtra (27)".
    pub place_of_supply: String,
    /// "Within the state · CGST + SGST".
    pub supply_note: String,
    pub rate_bp: i64,
    pub taxable_paise: i64,
    pub cgst_paise: i64,
    pub sgst_paise: i64,
    pub utgst_paise: i64,
    pub igst_paise: i64,
    pub cess_paise: i64,
}

/// Everything needed to print a receipt, payment or refund voucher.
#[derive(Clone, Debug, Default)]
pub struct VoucherData {
    /// "Receipt voucher", "Payment voucher", "Refund voucher".
    pub title: String,
    pub number: String,
    pub cancelled: bool,
    pub date: String,
    pub issuer: PartyBlock,
    /// "Received from" or "Paid to".
    pub counterparty_label: String,
    pub counterparty: PartyBlock,
    /// Label/value pairs: mode, reference, account, receipt voucher refunded.
    pub details: Vec<(String, String)>,
    pub rows: Vec<VoucherRow>,
    pub tax: Option<VoucherTax>,
    pub narration: Option<String>,
    pub amount_paise: i64,
    /// A line under the title such as "Under rule 50 of the CGST Rules".
    pub rule_note: Option<String>,
    pub signatory: Option<String>,
    pub logo: Option<Logo>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ViewParty {
    name: String,
    subtitle: Option<String>,
    lines: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ViewRow {
    description: String,
    details: Vec<String>,
    amount: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ViewTax {
    heading: String,
    place: String,
    supply: String,
    /// Label/value pairs: rate, taxable value, each tax charged, total tax.
    rows: Vec<(String, String)>,
    total: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct View {
    title: String,
    number: String,
    watermark: Option<String>,
    date: String,
    rule_note: Option<String>,
    issuer: ViewParty,
    counterparty_label: String,
    counterparty: ViewParty,
    details: Vec<(String, String)>,
    rows: Vec<ViewRow>,
    tax: Option<ViewTax>,
    narration: Option<String>,
    total: String,
    words: String,
    sign_for: String,
    signatory: Option<String>,
    logo: Option<String>,
    footer: String,
}

fn party(p: &PartyBlock) -> ViewParty {
    let mut lines = p.address_lines.clone();
    if let Some(g) = &p.gstin {
        lines.push(format!("GSTIN {g}"));
    }
    lines.extend(p.extra.iter().cloned());
    ViewParty {
        name: p.name.clone(),
        subtitle: p.subtitle.clone(),
        lines,
    }
}

fn tax_view(t: &VoucherTax) -> ViewTax {
    let state = if t.utgst_paise != 0 { "UTGST" } else { "SGST" };
    let half = t.rate_bp / 2;
    let mut rows = vec![
        ("Rate".to_string(), rate(t.rate_bp)),
        ("Taxable value".to_string(), rupees(t.taxable_paise)),
    ];
    if t.igst_paise != 0 {
        rows.push((format!("IGST @ {}", rate(t.rate_bp)), rupees(t.igst_paise)));
    }
    if t.cgst_paise != 0 {
        rows.push((format!("CGST @ {}", rate(half)), rupees(t.cgst_paise)));
    }
    if t.sgst_paise + t.utgst_paise != 0 {
        rows.push((
            format!("{state} @ {}", rate(t.rate_bp - half)),
            rupees(t.sgst_paise + t.utgst_paise),
        ));
    }
    if t.cess_paise != 0 {
        rows.push(("Cess".to_string(), rupees(t.cess_paise)));
    }
    rows.push((
        "Tax payable on reverse charge".to_string(),
        "No".to_string(),
    ));
    ViewTax {
        heading: t.heading.clone(),
        place: t.place_of_supply.clone(),
        supply: t.supply_note.clone(),
        rows,
        total: rupees(t.cgst_paise + t.sgst_paise + t.utgst_paise + t.igst_paise + t.cess_paise),
    }
}

impl VoucherData {
    pub(crate) fn logo_path(&self) -> Option<&'static str> {
        self.logo.as_ref().map(|l| match l.format {
            LogoFormat::Png => "/logo.png",
            LogoFormat::Jpeg => "/logo.jpg",
            LogoFormat::Svg => "/logo.svg",
        })
    }

    pub(crate) fn view(&self) -> View {
        View {
            title: self.title.clone(),
            number: self.number.clone(),
            watermark: self.cancelled.then(|| "CANCELLED".to_string()),
            date: self.date.clone(),
            rule_note: self.rule_note.clone(),
            issuer: party(&self.issuer),
            counterparty_label: self.counterparty_label.clone(),
            counterparty: party(&self.counterparty),
            details: self.details.clone(),
            rows: self
                .rows
                .iter()
                .map(|r| ViewRow {
                    description: r.description.clone(),
                    details: r.details.clone(),
                    amount: rupees(r.amount_paise),
                })
                .collect(),
            tax: self.tax.as_ref().map(tax_view),
            narration: self.narration.clone().filter(|n| !n.trim().is_empty()),
            total: rupees(self.amount_paise),
            words: amount_in_words(self.amount_paise),
            sign_for: format!("For {}", self.issuer.name),
            signatory: self.signatory.clone(),
            logo: self.logo_path().map(str::to_string),
            footer: format!("{} {} · {}", self.title, self.number, self.issuer.name),
        }
    }
}
