//! Excel workpapers (Phase 2.3): the GSTR-3B workpaper as a workbook a
//! practitioner can review and keep with the return.
//!
//! Amounts go in as rupees through `Decimal` (no float arithmetic here);
//! every sheet is plain values, no formulas, so it reads the same anywhere.

use av_gst_engine::offset::{HeadPayment, Heads};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use rust_xlsxwriter::{Format, FormatAlign, Workbook, Worksheet, XlsxError};

use crate::gstr3b::{Gstr3bView, Row};

fn rupees(paise: i64) -> f64 {
    Decimal::new(paise, 2).to_f64().unwrap_or_default()
}

struct Sheet<'a> {
    ws: &'a mut Worksheet,
    row: u32,
    money: Format,
    bold: Format,
    head: Format,
    title: Format,
}

impl<'a> Sheet<'a> {
    fn new(ws: &'a mut Worksheet) -> Self {
        Sheet {
            ws,
            row: 0,
            money: Format::new().set_num_format("#,##0.00"),
            bold: Format::new().set_bold(),
            head: Format::new()
                .set_bold()
                .set_background_color("#EEF4FF")
                .set_align(FormatAlign::Center),
            title: Format::new().set_bold().set_font_size(14),
        }
    }

    fn title(&mut self, text: &str) -> Result<(), XlsxError> {
        self.ws
            .write_string_with_format(self.row, 0, text, &self.title)?;
        self.row += 1;
        Ok(())
    }

    fn note(&mut self, text: &str) -> Result<(), XlsxError> {
        self.ws.write_string(self.row, 0, text)?;
        self.row += 1;
        Ok(())
    }

    fn section(&mut self, text: &str) -> Result<(), XlsxError> {
        self.row += 1;
        self.ws
            .write_string_with_format(self.row, 0, text, &self.bold)?;
        self.row += 1;
        Ok(())
    }

    fn header(&mut self, cols: &[&str]) -> Result<(), XlsxError> {
        for (i, c) in cols.iter().enumerate() {
            self.ws
                .write_string_with_format(self.row, col(i), *c, &self.head)?;
        }
        self.row += 1;
        Ok(())
    }

    /// A label followed by money cells.
    fn line(&mut self, label: &str, amounts: &[i64]) -> Result<(), XlsxError> {
        self.ws.write_string(self.row, 0, label)?;
        for (i, a) in amounts.iter().enumerate() {
            self.ws
                .write_number_with_format(self.row, col(i + 1), rupees(*a), &self.money)?;
        }
        self.row += 1;
        Ok(())
    }

    /// Text cells, then money cells.
    fn mixed(&mut self, texts: &[&str], amounts: &[i64]) -> Result<(), XlsxError> {
        for (i, t) in texts.iter().enumerate() {
            self.ws.write_string(self.row, col(i), *t)?;
        }
        for (i, a) in amounts.iter().enumerate() {
            self.ws.write_number_with_format(
                self.row,
                col(texts.len() + i),
                rupees(*a),
                &self.money,
            )?;
        }
        self.row += 1;
        Ok(())
    }
}

fn col(i: usize) -> u16 {
    u16::try_from(i).unwrap_or(u16::MAX)
}

fn heads(h: Heads) -> [i64; 4] {
    [h.igst, h.cgst, h.sgst, h.cess]
}

fn tax_row(s: &mut Sheet<'_>, r: &Row, with_taxable: bool) -> Result<(), XlsxError> {
    let t = heads(r.tax);
    if with_taxable {
        s.line(r.label, &[r.taxable_paise, t[0], t[1], t[2], t[3]])
    } else {
        s.line(r.label, &t)
    }
}

fn payment(s: &mut Sheet<'_>, label: &str, p: &HeadPayment) -> Result<(), XlsxError> {
    s.line(
        label,
        &[
            p.liability,
            p.by_credit.igst,
            p.by_credit.cgst,
            p.by_credit.sgst,
            p.by_credit.cess,
            p.cash,
        ],
    )
}

pub fn gstr3b_workbook(v: &Gstr3bView) -> Result<Vec<u8>, XlsxError> {
    let mut wb = Workbook::new();

    {
        let ws = wb.add_worksheet().set_name("GSTR-3B")?;
        ws.set_column_width(0, 70)?;
        for c in 1..=6 {
            ws.set_column_width(c, 16)?;
        }
        let mut s = Sheet::new(ws);
        s.title(&format!("GSTR-3B workpaper — {}", v.label))?;
        s.note(&format!(
            "GSTIN {} · {} · due {} · prepared by AccountsVU",
            v.gstin, v.state_name, v.due_date
        ))?;

        s.section("3.1 Outward supplies and inward supplies liable to reverse charge")?;
        s.header(&[
            "Nature of supplies",
            "Taxable value",
            "IGST",
            "CGST",
            "SGST/UTGST",
            "Cess",
        ])?;
        for r in &v.outward {
            tax_row(&mut s, r, true)?;
        }

        s.section("3.2 Inter-state supplies to unregistered persons and composition taxpayers")?;
        s.header(&[
            "Place of supply",
            "Unregistered: taxable",
            "Unregistered: IGST",
            "Composition: taxable",
            "Composition: IGST",
        ])?;
        for r in &v.inter_state {
            s.line(
                &format!("{:02}-{}", r.place_of_supply, r.place_name),
                &[
                    r.unregistered_taxable_paise,
                    r.unregistered_igst_paise,
                    r.composition_taxable_paise,
                    r.composition_igst_paise,
                ],
            )?;
        }
        if v.inter_state.is_empty() {
            s.note("None")?;
        }

        s.section("4 Eligible ITC")?;
        s.header(&["Details", "IGST", "CGST", "SGST/UTGST", "Cess"])?;
        s.note("(A) ITC available (whether in full or part)")?;
        for r in &v.itc_available {
            tax_row(&mut s, r, false)?;
        }
        s.note("(B) ITC reversed")?;
        for r in &v.itc_reversed {
            tax_row(&mut s, r, false)?;
        }
        s.line("(C) Net ITC available (A) − (B)", &heads(v.itc_net))?;
        s.note("(D) Other details")?;
        for r in &v.itc_other {
            tax_row(&mut s, r, false)?;
        }

        s.section("5 Values of exempt, nil-rated and non-GST inward supplies")?;
        s.header(&["Nature of supplies", "Inter-state", "Intra-state"])?;
        for r in &v.inward_exempt {
            s.line(r.label, &[r.inter_state_paise, r.intra_state_paise])?;
        }

        s.section("6.1 Payment of tax (proposal, rule 88A order)")?;
        s.header(&[
            "Tax head",
            "Tax payable",
            "Paid by IGST credit",
            "Paid by CGST credit",
            "Paid by SGST credit",
            "Paid by cess credit",
            "Paid in cash",
        ])?;
        payment(&mut s, "IGST", &v.offset.igst)?;
        payment(&mut s, "CGST", &v.offset.cgst)?;
        payment(&mut s, "SGST/UTGST", &v.offset.sgst)?;
        payment(&mut s, "Cess", &v.offset.cess)?;
        let rc = heads(v.offset.reverse_charge);
        s.line(
            "Reverse charge (cash only): IGST, CGST, SGST/UTGST, cess",
            &rc,
        )?;
        s.line("Total cash payable", &[v.offset.cash_total])?;
        s.line(
            "Credit brought forward (AccountsVU estimate)",
            &heads(v.credit_brought_forward),
        )?;
        s.line("Credit carried forward", &heads(v.offset.carry_forward))?;

        s.section("Rule 42 common credit (monthly, provisional)")?;
        s.line(
            "Common credit C2 (IGST, CGST, SGST/UTGST, cess)",
            &heads(v.rule42.common_credit),
        )?;
        s.line("Exempt turnover E", &[v.rule42.exempt_turnover_paise])?;
        s.line("Total turnover F", &[v.rule42.total_turnover_paise])?;
        s.line(
            "Reversal D1 = C2 × E ÷ F (in 4B(1))",
            &heads(v.rule42.reversal),
        )?;

        if !v.exceptions.is_empty() {
            s.section("Points to check")?;
            for e in &v.exceptions {
                s.note(&format!("[{}] {}", e.severity, e.message))?;
            }
        }
    }

    {
        let ws = wb.add_worksheet().set_name("ITC lines")?;
        for (c, w) in [
            12, 18, 12, 28, 18, 34, 8, 12, 30, 8, 14, 14, 14, 14, 14, 14, 14, 14,
        ]
        .iter()
        .enumerate()
        {
            ws.set_column_width(col(c), *w)?;
        }
        let mut s = Sheet::new(ws);
        s.header(&[
            "Date",
            "Supplier invoice",
            "Type",
            "Supplier",
            "GSTIN",
            "Description",
            "Table",
            "ITC status",
            "Reason",
            "Common",
            "IGST",
            "CGST",
            "SGST/UTGST",
            "Cess",
            "Eligible IGST",
            "Eligible CGST",
            "Eligible SGST/UTGST",
            "Eligible cess",
        ])?;
        for l in &v.itc_lines {
            let t = heads(l.tax);
            let e = heads(l.eligible);
            s.mixed(
                &[
                    &l.doc_date,
                    l.supplier_invoice.as_deref().unwrap_or(""),
                    &l.doc_type,
                    &l.party_name,
                    l.gstin.as_deref().unwrap_or(""),
                    &l.description,
                    l.table,
                    &l.status,
                    l.reason.as_deref().unwrap_or(""),
                    if l.common { "Yes" } else { "" },
                ],
                &[t[0], t[1], t[2], t[3], e[0], e[1], e[2], e[3]],
            )?;
        }
    }

    {
        let ws = wb.add_worksheet().set_name("GSTR-1 cross-check")?;
        ws.set_column_width(0, 46)?;
        for c in 1..=3 {
            ws.set_column_width(c, 16)?;
        }
        let mut s = Sheet::new(ws);
        s.header(&["Figure", "GSTR-1", "GSTR-3B", "Difference"])?;
        for c in &v.cross_check {
            s.line(
                c.label,
                &[c.gstr1_paise, c.gstr3b_paise, c.difference_paise],
            )?;
        }
    }

    wb.save_to_buffer()
}
