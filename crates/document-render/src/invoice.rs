//! Invoice data (what the application hands over) and the print view (what
//! the Typst template reads). The view holds only display strings, so the
//! template never formats money and data can't inject Typst markup.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::format::{amount_in_words, rate, rupees};

/// A business or customer as printed.
#[derive(Clone, Debug, Default)]
pub struct PartyBlock {
    pub name: String,
    /// Shown under the name when it differs (legal name under a trade name).
    pub subtitle: Option<String>,
    pub address_lines: Vec<String>,
    pub gstin: Option<String>,
    /// Extra lines such as "PAN ABCDE1234F" or "Phone …".
    pub extra: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogoFormat {
    Png,
    Jpeg,
    Svg,
}

#[derive(Clone, Debug)]
pub struct Logo {
    pub bytes: Vec<u8>,
    pub format: LogoFormat,
}

#[derive(Clone, Debug, Default)]
pub struct InvoiceLine {
    pub no: i64,
    pub description: String,
    pub hsn_sac: Option<String>,
    /// Quantity as entered ("2", "1.5").
    pub quantity: String,
    pub unit: Option<String>,
    pub unit_price_paise: i64,
    pub discount_paise: i64,
    /// "taxable", "exempt", "nil_rated", "non_gst".
    pub tax_category: String,
    pub gst_rate_bp: i64,
    pub taxable_paise: i64,
    pub cgst_paise: i64,
    pub sgst_paise: i64,
    pub utgst_paise: i64,
    pub igst_paise: i64,
    pub cess_paise: i64,
}

/// Everything needed to print a sales document.
#[derive(Clone, Debug, Default)]
pub struct InvoiceData {
    /// "Tax invoice", "Bill of supply", "Credit note".
    pub title: String,
    /// Document number; `None` for a draft.
    pub number: Option<String>,
    pub cancelled: bool,
    /// Pre-formatted dates ("19 Nov 2026").
    pub date: String,
    pub due_date: Option<String>,
    pub seller: PartyBlock,
    pub buyer: PartyBlock,
    /// "Maharashtra (27)".
    pub place_of_supply: String,
    /// "Within the state · CGST + SGST".
    pub supply_note: String,
    pub zero_rated: bool,
    pub reverse_charge: bool,
    /// For a credit note: "INV/0001 dated 1 Nov 2026".
    pub against: Option<String>,
    pub reason: Option<String>,
    pub notes: Option<String>,
    pub lines: Vec<InvoiceLine>,
    pub taxable_paise: i64,
    pub cgst_paise: i64,
    pub sgst_paise: i64,
    pub utgst_paise: i64,
    pub igst_paise: i64,
    pub cess_paise: i64,
    pub round_off_paise: i64,
    pub total_paise: i64,
    /// Authorised signatory printed under "For <business>".
    pub signatory: Option<String>,
    pub logo: Option<Logo>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ViewLine {
    no: String,
    description: String,
    discount: Option<String>,
    hsn: String,
    qty: String,
    rate: String,
    gst: String,
    taxable: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ViewTaxRow {
    rate: String,
    taxable: String,
    cgst: String,
    sgst: String,
    igst: String,
    cess: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ViewParty {
    name: String,
    subtitle: Option<String>,
    lines: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct View {
    title: String,
    number: String,
    watermark: Option<String>,
    date: String,
    due: Option<String>,
    seller: ViewParty,
    buyer: ViewParty,
    place: String,
    supply: String,
    flags: Vec<String>,
    against: Option<String>,
    reason: Option<String>,
    notes: Option<String>,
    lines: Vec<ViewLine>,
    /// Tax breakup by rate; empty when nothing is taxed.
    tax_rows: Vec<ViewTaxRow>,
    /// The state-tax column heading: "SGST", "UTGST" or "SGST/UTGST".
    state_tax: String,
    show_cgst: bool,
    show_igst: bool,
    show_cess: bool,
    totals: Vec<(String, String)>,
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

fn gst_label(l: &InvoiceLine) -> String {
    match l.tax_category.as_str() {
        "taxable" => rate(l.gst_rate_bp),
        "nil_rated" => "Nil".into(),
        "exempt" => "Exempt".into(),
        "non_gst" => "Non-GST".into(),
        other => other.replace('_', "-"),
    }
}

impl InvoiceData {
    /// The file extension the logo is exposed under to the template.
    pub(crate) fn logo_path(&self) -> Option<&'static str> {
        self.logo.as_ref().map(|l| match l.format {
            LogoFormat::Png => "/logo.png",
            LogoFormat::Jpeg => "/logo.jpg",
            LogoFormat::Svg => "/logo.svg",
        })
    }

    pub(crate) fn view(&self) -> View {
        // Rate-wise tax breakup (Rule 46: rate and amount of each tax).
        let mut by_rate: BTreeMap<i64, [i64; 6]> = BTreeMap::new();
        for l in self.lines.iter().filter(|l| l.tax_category == "taxable") {
            let e = by_rate.entry(l.gst_rate_bp).or_default();
            e[0] += l.taxable_paise;
            e[1] += l.cgst_paise;
            e[2] += l.sgst_paise;
            e[3] += l.utgst_paise;
            e[4] += l.igst_paise;
            e[5] += l.cess_paise;
        }
        let any_tax = self.cgst_paise
            + self.sgst_paise
            + self.utgst_paise
            + self.igst_paise
            + self.cess_paise
            != 0;
        let tax_rows = if any_tax {
            by_rate
                .iter()
                .map(|(bp, t)| ViewTaxRow {
                    rate: rate(*bp),
                    taxable: rupees(t[0]),
                    cgst: rupees(t[1]),
                    sgst: rupees(t[2] + t[3]),
                    igst: rupees(t[4]),
                    cess: rupees(t[5]),
                })
                .collect()
        } else {
            Vec::new()
        };
        let state_tax = match (self.sgst_paise != 0, self.utgst_paise != 0) {
            (false, true) => "UTGST",
            (true, true) => "SGST/UTGST",
            _ => "SGST",
        }
        .to_string();

        let mut totals = vec![("Taxable value".to_string(), rupees(self.taxable_paise))];
        for (label, paise) in [
            ("CGST", self.cgst_paise),
            ("SGST", self.sgst_paise),
            ("UTGST", self.utgst_paise),
            ("IGST", self.igst_paise),
            ("Cess", self.cess_paise),
            ("Round-off", self.round_off_paise),
        ] {
            if paise != 0 {
                totals.push((label.to_string(), rupees(paise)));
            }
        }

        let mut flags = Vec::new();
        if self.zero_rated {
            flags.push("Zero-rated supply under LUT/bond".to_string());
        }
        flags.push(format!(
            "Tax payable on reverse charge: {}",
            if self.reverse_charge { "Yes" } else { "No" }
        ));

        let watermark = if self.cancelled {
            Some("CANCELLED".to_string())
        } else if self.number.is_none() {
            Some("DRAFT".to_string())
        } else {
            None
        };
        let number = self.number.clone().unwrap_or_else(|| "Draft".into());

        View {
            title: self.title.clone(),
            footer: format!("{} {number} · {}", self.title, self.seller.name),
            number,
            watermark,
            date: self.date.clone(),
            due: self.due_date.clone(),
            seller: party(&self.seller),
            buyer: party(&self.buyer),
            place: self.place_of_supply.clone(),
            supply: self.supply_note.clone(),
            flags,
            against: self.against.clone(),
            reason: self.reason.clone(),
            notes: self.notes.clone().filter(|n| !n.trim().is_empty()),
            lines: self
                .lines
                .iter()
                .map(|l| ViewLine {
                    no: l.no.to_string(),
                    description: l.description.clone(),
                    discount: (l.discount_paise > 0)
                        .then(|| format!("Less discount {}", rupees(l.discount_paise))),
                    hsn: l.hsn_sac.clone().unwrap_or_else(|| "—".into()),
                    qty: match &l.unit {
                        Some(u) if !u.is_empty() => format!("{} {u}", l.quantity),
                        _ => l.quantity.clone(),
                    },
                    rate: rupees(l.unit_price_paise),
                    gst: gst_label(l),
                    taxable: rupees(l.taxable_paise),
                })
                .collect(),
            tax_rows,
            state_tax,
            show_cgst: self.cgst_paise + self.sgst_paise + self.utgst_paise != 0,
            show_igst: self.igst_paise != 0,
            show_cess: self.cess_paise != 0,
            totals,
            total: rupees(self.total_paise),
            words: amount_in_words(self.total_paise),
            sign_for: format!("For {}", self.seller.name),
            signatory: self.signatory.clone(),
            logo: self.logo_path().map(str::to_string),
        }
    }
}
