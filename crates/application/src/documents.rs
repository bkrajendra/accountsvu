//! Printed documents (Phase 1.7): maps stored sales documents, the business
//! profile and the logo onto the print model and renders a PDF with Typst.

use av_document_render::{InvoiceData, InvoiceLine, Logo, LogoFormat, PartyBlock, render_invoice};
use av_domain::StateCode;
use av_persistence::Store;
use chrono::NaiveDate;
use serde::Serialize;

use crate::AppCore;
use crate::context::RequestContext;
use crate::error::{AppError, AppResult};

/// A rendered file ready to save.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedFile {
    pub file_name: String,
    #[serde(skip)]
    pub bytes: Vec<u8>,
    pub size_bytes: usize,
}

/// "2026-11-19" → "19 Nov 2026" (unchanged if it isn't an ISO date).
pub(crate) fn print_date(iso: &str) -> String {
    NaiveDate::parse_from_str(iso, "%Y-%m-%d")
        .map(|d| d.format("%-d %b %Y").to_string())
        .unwrap_or_else(|_| iso.to_string())
}

fn place_label(code: i64) -> String {
    if code == 96 {
        return "Outside India (96)".into();
    }
    let name = u8::try_from(code)
        .ok()
        .and_then(|c| StateCode::new(c).ok())
        .map(|s| s.name())
        .unwrap_or("Unknown state");
    format!("{name} ({code:02})")
}

fn supply_label(kind: &str) -> &'static str {
    match kind {
        "intra_state" => "Within the state · CGST + SGST",
        "intra_union_territory" => "Within the UT · CGST + UTGST",
        _ => "Between states · IGST",
    }
}

fn doc_title(doc_type: &str) -> &'static str {
    match doc_type {
        "bill_of_supply" => "Bill of supply",
        "credit_note" => "Credit note",
        _ => "Tax invoice",
    }
}

/// Keeps a file name to letters, digits, '-' and '_' ("INV/25-26/0001" → "INV-25-26-0001").
fn safe_file_stem(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let t = out.trim_matches('-');
    if t.is_empty() {
        "document".into()
    } else {
        t.to_string()
    }
}

fn logo_format(content_type: &str) -> Option<LogoFormat> {
    match content_type {
        "image/png" => Some(LogoFormat::Png),
        "image/jpeg" => Some(LogoFormat::Jpeg),
        "image/svg+xml" => Some(LogoFormat::Svg),
        _ => None,
    }
}

impl<S: Store> AppCore<S> {
    /// The sales document (invoice, bill of supply, credit note) as a PDF.
    /// Drafts print with a DRAFT watermark, cancelled documents with CANCELLED.
    pub async fn sales_document_pdf(
        &self,
        ctx: &RequestContext,
        id: &str,
    ) -> AppResult<RenderedFile> {
        let doc = self.get_sales_document(ctx, id).await?;
        let profile = self.business_profile(ctx).await?;
        let org = profile.organisation.ok_or(AppError::NotSetUp)?;
        let reg = profile
            .registrations
            .iter()
            .find(|r| Some(&r.id) == doc.document.gst_registration_id.as_ref());
        let logo = self.logo(ctx).await?.and_then(|l| {
            logo_format(&l.content_type).map(|format| Logo {
                bytes: l.bytes,
                format,
            })
        });

        let d = &doc.document;
        let mut seller_address = Vec::new();
        for l in [&org.address_line1, &org.address_line2]
            .into_iter()
            .flatten()
        {
            if !l.trim().is_empty() {
                seller_address.push(l.clone());
            }
        }
        let state = u8::try_from(org.state_code)
            .ok()
            .and_then(|c| StateCode::new(c).ok())
            .map(|s| s.name().to_string());
        let city_line = [org.city.clone(), org.pincode.clone(), state]
            .into_iter()
            .flatten()
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>();
        if !city_line.is_empty() {
            // "Pune 411038, Maharashtra"
            let (place, rest) = city_line.split_at(city_line.len().min(2));
            let mut line = place.join(" ");
            if !rest.is_empty() {
                line = format!("{line}, {}", rest.join(", "));
            }
            seller_address.push(line);
        }
        let contact = [org.email.clone(), org.phone.clone()]
            .into_iter()
            .flatten()
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>();
        let mut seller_extra = Vec::new();
        if let (None, Some(pan)) = (reg, &org.pan) {
            seller_extra.push(format!("PAN {pan}"));
        }
        if !contact.is_empty() {
            seller_extra.push(contact.join(" · "));
        }
        let display_name = org
            .trade_name
            .clone()
            .unwrap_or_else(|| org.legal_name.clone());

        let data = InvoiceData {
            title: doc_title(&d.doc_type).into(),
            number: d.number.clone(),
            cancelled: d.status == "cancelled",
            date: print_date(&d.doc_date),
            due_date: (d.doc_type != "credit_note")
                .then(|| d.due_date.as_deref().map(print_date))
                .flatten(),
            seller: PartyBlock {
                subtitle: org.trade_name.as_ref().map(|_| org.legal_name.clone()),
                name: display_name,
                address_lines: seller_address,
                gstin: reg.map(|r| r.gstin.clone()),
                extra: seller_extra,
            },
            buyer: PartyBlock {
                name: d.party_name.clone(),
                subtitle: None,
                address_lines: d
                    .party_address
                    .iter()
                    .flat_map(|a| a.lines())
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect(),
                gstin: d.party_gstin.clone(),
                extra: Vec::new(),
            },
            place_of_supply: place_label(d.place_of_supply),
            supply_note: supply_label(&d.supply_kind).into(),
            zero_rated: d.zero_rated,
            reverse_charge: d.reverse_charge,
            against: doc.original.as_ref().map(|o| {
                format!(
                    "{} dated {}",
                    o.number.clone().unwrap_or_else(|| "draft".into()),
                    print_date(&o.doc_date)
                )
            }),
            reason: d.reason.clone(),
            notes: d.notes.clone(),
            lines: doc
                .lines
                .iter()
                .map(|l| InvoiceLine {
                    no: l.line_no,
                    description: l.description.clone(),
                    hsn_sac: l.hsn_sac.clone(),
                    quantity: l.quantity.clone(),
                    unit: l.unit.clone(),
                    unit_price_paise: l.unit_price_paise,
                    discount_paise: l.discount_paise,
                    tax_category: l.tax_category.clone(),
                    gst_rate_bp: l.gst_rate_bp,
                    taxable_paise: l.taxable_paise,
                    cgst_paise: l.cgst_paise,
                    sgst_paise: l.sgst_paise,
                    utgst_paise: l.utgst_paise,
                    igst_paise: l.igst_paise,
                    cess_paise: l.cess_paise,
                })
                .collect(),
            taxable_paise: d.taxable_paise,
            cgst_paise: d.cgst_paise,
            sgst_paise: d.sgst_paise,
            utgst_paise: d.utgst_paise,
            igst_paise: d.igst_paise,
            cess_paise: d.cess_paise,
            round_off_paise: d.round_off_paise,
            total_paise: d.total_paise,
            signatory: reg.and_then(|r| r.signatory_name.clone()),
            logo,
        };
        let bytes = render_invoice(&data).map_err(|e| AppError::Invalid(e.to_string()))?;
        let stem = match &d.number {
            Some(n) => safe_file_stem(n),
            None => format!("{}-draft", safe_file_stem(doc_title(&d.doc_type))),
        };
        Ok(RenderedFile {
            file_name: format!("{stem}.pdf"),
            size_bytes: bytes.len(),
            bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_and_labels() {
        assert_eq!(safe_file_stem("INV/25-26/0001"), "INV-25-26-0001");
        assert_eq!(safe_file_stem("//"), "document");
        assert_eq!(print_date("2026-11-05"), "5 Nov 2026");
        assert_eq!(place_label(27), "Maharashtra (27)");
        assert_eq!(place_label(7), "Delhi (07)");
        assert_eq!(place_label(96), "Outside India (96)");
    }
}
