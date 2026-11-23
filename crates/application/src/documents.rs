//! Printed documents (Phase 1.7): maps stored sales documents, the business
//! profile and the logo onto the print model and renders a PDF with Typst.

use av_document_render::{
    InvoiceData, InvoiceLine, Logo, LogoFormat, PartyBlock, VoucherData, VoucherRow, VoucherTax,
    render_invoice, render_voucher,
};
use av_domain::StateCode;
use av_persistence::records::{OrganisationRecord, PartyRecord, RegistrationRecord};
use av_persistence::{BankingRepo, PartyRepo, RegistrationRepo, Store, UnitOfWork};
use chrono::NaiveDate;
use serde::Serialize;

use crate::AppCore;
use crate::advances::{Parts, supply_kind};
use crate::context::{Permission, RequestContext};
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

/// The business as printed at the head of a document.
fn issuer_block(org: &OrganisationRecord, reg: Option<&RegistrationRecord>) -> PartyBlock {
    let mut address = Vec::new();
    for l in [&org.address_line1, &org.address_line2]
        .into_iter()
        .flatten()
    {
        if !l.trim().is_empty() {
            address.push(l.clone());
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
        address.push(line);
    }
    let contact = [org.email.clone(), org.phone.clone()]
        .into_iter()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>();
    let mut extra = Vec::new();
    if let (None, Some(pan)) = (reg, &org.pan) {
        extra.push(format!("PAN {pan}"));
    }
    if !contact.is_empty() {
        extra.push(contact.join(" · "));
    }
    PartyBlock {
        subtitle: org.trade_name.as_ref().map(|_| org.legal_name.clone()),
        name: org
            .trade_name
            .clone()
            .unwrap_or_else(|| org.legal_name.clone()),
        address_lines: address,
        gstin: reg.map(|r| r.gstin.clone()),
        extra,
    }
}

/// A party's postal address as printed lines.
fn party_lines(p: &PartyRecord) -> Vec<String> {
    let mut out: Vec<String> = [&p.address_line1, &p.address_line2]
        .into_iter()
        .flatten()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let city = [p.city.clone(), p.pincode.clone()]
        .into_iter()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let state = u8::try_from(p.state_code)
        .ok()
        .and_then(|c| StateCode::new(c).ok())
        .map(|s| s.name().to_string());
    let line = [Some(city).filter(|c| !c.is_empty()), state]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
    if !line.is_empty() {
        out.push(line);
    }
    out
}

fn mode_label(mode: &str) -> String {
    match mode {
        "cash" => "Cash",
        "upi" => "UPI",
        "neft" => "NEFT",
        "rtgs" => "RTGS",
        "imps" => "IMPS",
        "cheque" => "Cheque",
        "card" => "Card",
        "other" => "Other",
        m => return m.to_string(),
    }
    .to_string()
}

fn voucher_tax(
    heading: &str,
    reg: Option<&RegistrationRecord>,
    place_of_supply: i64,
    rate_bp: i64,
    p: Parts,
) -> VoucherTax {
    let kind = reg.map_or("inter_state", |r| {
        crate::sales::kind_str(supply_kind(r, place_of_supply))
    });
    VoucherTax {
        heading: heading.into(),
        place_of_supply: place_label(place_of_supply),
        supply_note: supply_label(kind).into(),
        rate_bp,
        taxable_paise: p.taxable,
        cgst_paise: p.cgst,
        sgst_paise: p.sgst,
        utgst_paise: p.utgst,
        igst_paise: p.igst,
        cess_paise: p.cess,
    }
}

fn rupees(paise: i64) -> String {
    av_document_render::format::rupees(paise)
}

impl<S: Store> AppCore<S> {
    async fn print_logo(&self, ctx: &RequestContext) -> AppResult<Option<Logo>> {
        Ok(self.logo(ctx).await?.and_then(|l| {
            logo_format(&l.content_type).map(|format| Logo {
                bytes: l.bytes,
                format,
            })
        }))
    }

    /// A receipt, payment or refund as a PDF. A receipt with GST paid on
    /// its advance prints as a receipt voucher (rule 50); a refund of such
    /// an advance as a refund voucher with the tax given back (rule 51).
    pub async fn voucher_pdf(&self, ctx: &RequestContext, id: &str) -> AppResult<RenderedFile> {
        ctx.require(Permission::ViewBusiness)?;
        let v = self.get_voucher(ctx, id).await?;
        let logo = self.print_logo(ctx).await?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let rec = &v.voucher;
        let receipt = rec.voucher_type == "receipt";
        let refund = rec.purpose == "refund";
        let regs: Vec<RegistrationRecord> = uow.list_registrations(&org.id).await?;
        let reg_of = |id: &str| regs.iter().find(|r| r.id == id);

        let mut details = Vec::new();
        let mut tax = None;
        let mut tax_reg = None;
        let mut rule_note = None;
        // Rule 50: GST paid on this advance.
        let state = self.advance_state(&mut uow, &org.id, rec).await?;
        if let Some(t) = &state.tax {
            tax_reg = reg_of(&t.gst_registration_id);
            tax = Some(voucher_tax(
                "GST on the advance",
                tax_reg,
                t.place_of_supply,
                t.rate_bp,
                Parts::of_tax(t),
            ));
            rule_note = Some("Under rule 50 of the CGST Rules".to_string());
        }
        // Rule 51: the advance this refund gives back and the tax on it.
        let mut refund_of = None;
        if refund
            && let Some(orig_id) = &rec.refund_of_id
            && let Some(orig) = uow.get_voucher(&org.id, orig_id).await?
        {
            details.push((
                if orig.voucher_type == "receipt" {
                    "Receipt voucher"
                } else {
                    "Payment voucher"
                }
                .to_string(),
                format!("{} dated {}", orig.number, print_date(&orig.voucher_date)),
            ));
            let st = self.advance_state(&mut uow, &org.id, &orig).await?;
            let parts = st
                .adjustments
                .iter()
                .filter(|a| a.refund_voucher_id.as_deref() == Some(rec.id.as_str()))
                .fold(Parts::default(), |acc, a| acc.plus(Parts::of_adjustment(a)));
            if let Some(t) = &st.tax
                && parts.gross != 0
            {
                tax_reg = reg_of(&t.gst_registration_id);
                tax = Some(voucher_tax(
                    "GST paid on the advance refunded",
                    tax_reg,
                    t.place_of_supply,
                    t.rate_bp,
                    parts,
                ));
                rule_note = Some("Under rule 51 of the CGST Rules".to_string());
            }
            refund_of = Some(orig);
        }
        details.push(("Mode".to_string(), mode_label(&rec.mode)));
        if let Some(r) = rec.reference.as_ref().filter(|r| !r.trim().is_empty()) {
            details.push(("Reference".to_string(), r.clone()));
        }
        details.push((
            if receipt { "Into" } else { "Out of" }.to_string(),
            v.money_account_name.clone(),
        ));
        let party = match &rec.party_id {
            Some(p) => uow.get_party(&org.id, p).await?,
            None => None,
        };
        uow.commit().await?;

        let counter_name = v.counter_account_name.clone().unwrap_or_default();
        let counterparty = match &party {
            Some(p) => PartyBlock {
                name: p.name.clone(),
                subtitle: p.legal_name.clone().filter(|l| l != &p.name),
                address_lines: party_lines(p),
                gstin: p.gstin.clone(),
                extra: Vec::new(),
            },
            None => PartyBlock {
                name: counter_name.clone(),
                ..PartyBlock::default()
            },
        };

        // What the money was for.
        let doc_word = if receipt { "invoice" } else { "bill" };
        let mut rows = Vec::new();
        if party.is_none() {
            rows.push(VoucherRow {
                description: format!("Booked to {counter_name}"),
                details: Vec::new(),
                amount_paise: rec.amount_paise,
            });
        } else if refund {
            rows.push(VoucherRow {
                description: match &refund_of {
                    Some(o) => format!("Refund of advance {}", o.number),
                    None if receipt => "Refund received".to_string(),
                    None => "Refund paid".to_string(),
                },
                details: Vec::new(),
                amount_paise: rec.amount_paise,
            });
        } else {
            let mut at_time = 0;
            let mut later = Vec::new();
            for a in &v.allocations {
                let number = a.number.clone().unwrap_or_else(|| doc_word.to_string());
                match &a.allocated_on {
                    None => {
                        at_time += a.amount_paise;
                        let dated = a
                            .doc_date
                            .as_deref()
                            .map(|d| format!(" dated {}", print_date(d)))
                            .unwrap_or_default();
                        rows.push(VoucherRow {
                            description: format!("Against {doc_word} {number}{dated}"),
                            details: Vec::new(),
                            amount_paise: a.amount_paise,
                        });
                    }
                    Some(on) => later.push(format!(
                        "Adjusted against {number} on {}: {}",
                        print_date(on),
                        rupees(a.amount_paise)
                    )),
                }
            }
            for r in &v.refunds {
                later.push(format!(
                    "Refunded by {} on {}: {}",
                    r.number,
                    print_date(&r.voucher_date),
                    rupees(r.amount_paise)
                ));
            }
            let advance = rec.amount_paise - at_time;
            if advance > 0 {
                rows.push(VoucherRow {
                    description: "Advance".to_string(),
                    details: later,
                    amount_paise: advance,
                });
            }
        }

        let issuing_reg = tax_reg.or_else(|| regs.iter().find(|r| r.status == "active"));
        let data = VoucherData {
            title: if refund && !receipt {
                "Refund voucher"
            } else if receipt {
                "Receipt voucher"
            } else {
                "Payment voucher"
            }
            .into(),
            number: rec.number.clone(),
            cancelled: rec.status == "cancelled",
            date: print_date(&rec.voucher_date),
            issuer: issuer_block(&org, issuing_reg),
            counterparty_label: if receipt { "Received from" } else { "Paid to" }.into(),
            counterparty,
            details,
            rows,
            tax,
            narration: rec.narration.clone(),
            amount_paise: rec.amount_paise,
            rule_note,
            signatory: issuing_reg.and_then(|r| r.signatory_name.clone()),
            logo,
        };
        let bytes = render_voucher(&data).map_err(|e| AppError::Invalid(e.to_string()))?;
        Ok(RenderedFile {
            file_name: format!("{}.pdf", safe_file_stem(&rec.number)),
            size_bytes: bytes.len(),
            bytes,
        })
    }

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
        let logo = self.print_logo(ctx).await?;

        let d = &doc.document;
        let data = InvoiceData {
            title: doc_title(&d.doc_type).into(),
            number: d.number.clone(),
            cancelled: d.status == "cancelled",
            date: print_date(&d.doc_date),
            due_date: (d.doc_type != "credit_note")
                .then(|| d.due_date.as_deref().map(print_date))
                .flatten(),
            seller: issuer_block(&org, reg),
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
