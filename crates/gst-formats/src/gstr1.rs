//! GSTR-1 in the JSON layout of the GST portal's offline tool / "Prepare
//! offline → Upload" (tables 4, 5, 6, 7, 8, 9B, 12 and 13).
//!
//! The input is a neutral [`Gstr1Return`]; [`to_json`] lays it out. Empty
//! tables are left out, as the offline tool does.

use chrono::NaiveDate;
use serde::Serialize;

use crate::{Amount, Quantity, Rate};

/// Format version of this adapter. Bump it whenever the output changes, and
/// update the golden files in the same commit.
pub const ADAPTER_VERSION: &str = "gstr1-json/1";

/// Value of the `version` field the portal expects in the file header.
pub const PORTAL_SCHEMA: &str = "GST3.2.2";

// ---------------------------------------------------------------------------
// Input model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct Gstr1Return {
    pub gstin: String,
    /// Return period as calendar month.
    pub year: i32,
    pub month: u32,
    /// Table 4A/4B/6B/6C: invoices to registered persons, including SEZ and deemed exports.
    pub b2b: Vec<Invoice>,
    /// Table 5: inter-state invoices to unregistered persons above the B2CL limit.
    pub b2cl: Vec<Invoice>,
    /// Table 7: other supplies to unregistered persons, net of their credit notes.
    pub b2cs: Vec<B2csRow>,
    /// Table 6A: exports.
    pub exp: Vec<Invoice>,
    /// Table 9B: credit/debit notes to registered persons.
    pub cdnr: Vec<Note>,
    /// Table 9B: credit/debit notes to unregistered persons (B2CL and exports).
    pub cdnur: Vec<Note>,
    /// Table 8: nil-rated, exempt and non-GST supplies.
    pub nil: Vec<NilRow>,
    /// Table 12: HSN summary, split into B2B and B2C.
    pub hsn_b2b: Vec<HsnRow>,
    pub hsn_b2c: Vec<HsnRow>,
    /// Table 13: documents issued.
    pub documents: Vec<DocumentSeries>,
}

/// Tax at one rate on a document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RateItem {
    pub rate_bp: u32,
    pub taxable_paise: i64,
    pub igst_paise: i64,
    pub cgst_paise: i64,
    /// SGST or UTGST.
    pub sgst_paise: i64,
    pub cess_paise: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvoiceType {
    /// Regular B2B.
    Regular,
    SezWithPayment,
    SezWithoutPayment,
    DeemedExport,
}

impl InvoiceType {
    fn code(self) -> &'static str {
        match self {
            InvoiceType::Regular => "R",
            InvoiceType::SezWithPayment => "SEWP",
            InvoiceType::SezWithoutPayment => "SEWOP",
            InvoiceType::DeemedExport => "DE",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Invoice {
    /// Recipient GSTIN (B2B only).
    pub ctin: Option<String>,
    pub number: String,
    pub date: NaiveDate,
    /// Invoice value including tax and round-off.
    pub value_paise: i64,
    /// Place of supply state code (96 for exports).
    pub pos: u8,
    pub reverse_charge: bool,
    /// CGST + SGST/UTGST rather than IGST.
    pub intra_state: bool,
    pub invoice_type: InvoiceType,
    /// Exports: tax paid (with payment) or under bond/LUT.
    pub export_with_payment: bool,
    pub items: Vec<RateItem>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteType {
    Credit,
    Debit,
}

/// Where the original supply of an unregistered note was reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnregisteredType {
    B2cl,
    ExportWithPayment,
    ExportWithoutPayment,
}

#[derive(Clone, Debug)]
pub struct Note {
    pub ctin: Option<String>,
    pub note_type: NoteType,
    pub number: String,
    pub date: NaiveDate,
    pub value_paise: i64,
    pub pos: u8,
    pub reverse_charge: bool,
    pub intra_state: bool,
    pub invoice_type: InvoiceType,
    /// CDNUR only.
    pub unregistered_type: Option<UnregisteredType>,
    pub items: Vec<RateItem>,
}

#[derive(Clone, Copy, Debug)]
pub struct B2csRow {
    pub pos: u8,
    pub intra_state: bool,
    pub item: RateItem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NilSupply {
    InterRegistered,
    IntraRegistered,
    InterUnregistered,
    IntraUnregistered,
}

impl NilSupply {
    fn code(self) -> &'static str {
        match self {
            NilSupply::InterRegistered => "INTRB2B",
            NilSupply::IntraRegistered => "INTRAB2B",
            NilSupply::InterUnregistered => "INTRB2C",
            NilSupply::IntraUnregistered => "INTRAB2C",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NilRow {
    pub supply: NilSupply,
    pub nil_paise: i64,
    pub exempt_paise: i64,
    pub non_gst_paise: i64,
}

#[derive(Clone, Debug)]
pub struct HsnRow {
    pub hsn: String,
    pub description: String,
    /// Unit quantity code; `NA` for services.
    pub uqc: String,
    /// Decimal text.
    pub quantity: String,
    pub item: RateItem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DocumentNature {
    OutwardInvoices,
    DebitNotes,
    CreditNotes,
}

impl DocumentNature {
    fn code(self) -> (u8, &'static str) {
        match self {
            DocumentNature::OutwardInvoices => (1, "Invoices for outward supply"),
            DocumentNature::DebitNotes => (4, "Debit Note"),
            DocumentNature::CreditNotes => (5, "Credit Note"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DocumentSeries {
    pub nature: DocumentNature,
    pub from: String,
    pub to: String,
    pub total: u32,
    pub cancelled: u32,
}

// ---------------------------------------------------------------------------
// Portal layout
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct File<'a> {
    gstin: &'a str,
    fp: String,
    version: &'static str,
    hash: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    b2b: Vec<PartyInvoices>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    b2cl: Vec<PosInvoices>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    b2cs: Vec<B2cs>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    exp: Vec<ExportGroup>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    cdnr: Vec<PartyNotes>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    cdnur: Vec<Cdnur>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nil: Option<Nil>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hsn: Option<Hsn>,
    #[serde(skip_serializing_if = "Option::is_none")]
    doc_issue: Option<DocIssue>,
}

#[derive(Serialize)]
struct ItemDetail {
    txval: Amount,
    rt: Rate,
    #[serde(skip_serializing_if = "Option::is_none")]
    iamt: Option<Amount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    camt: Option<Amount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    samt: Option<Amount>,
    csamt: Amount,
}

#[derive(Serialize)]
struct NumberedItem {
    num: usize,
    itm_det: ItemDetail,
}

/// Tax heads the portal expects for a supply: IGST when inter-state,
/// CGST + SGST when intra-state (B2CL and exports are always inter-state).
#[derive(Clone, Copy)]
enum Heads {
    Intra,
    Inter,
}

fn heads(intra_state: bool) -> Heads {
    if intra_state {
        Heads::Intra
    } else {
        Heads::Inter
    }
}

fn detail(i: &RateItem, heads: Heads) -> ItemDetail {
    let (iamt, camt, samt) = match heads {
        Heads::Inter => (Some(Amount(i.igst_paise)), None, None),
        Heads::Intra => (None, Some(Amount(i.cgst_paise)), Some(Amount(i.sgst_paise))),
    };
    ItemDetail {
        txval: Amount(i.taxable_paise),
        rt: Rate(i.rate_bp),
        iamt,
        camt,
        samt,
        csamt: Amount(i.cess_paise),
    }
}

fn numbered(items: &[RateItem], heads: Heads) -> Vec<NumberedItem> {
    items
        .iter()
        .enumerate()
        .map(|(n, i)| NumberedItem {
            num: n + 1,
            itm_det: detail(i, heads),
        })
        .collect()
}

fn date(d: NaiveDate) -> String {
    d.format("%d-%m-%Y").to_string()
}

fn pos(code: u8) -> String {
    format!("{code:02}")
}

fn yn(b: bool) -> &'static str {
    if b { "Y" } else { "N" }
}

#[derive(Serialize)]
struct B2bInvoice {
    inum: String,
    idt: String,
    val: Amount,
    pos: String,
    rchrg: &'static str,
    inv_typ: &'static str,
    itms: Vec<NumberedItem>,
}

#[derive(Serialize)]
struct PartyInvoices {
    ctin: String,
    inv: Vec<B2bInvoice>,
}

#[derive(Serialize)]
struct B2clInvoice {
    inum: String,
    idt: String,
    val: Amount,
    itms: Vec<NumberedItem>,
}

#[derive(Serialize)]
struct PosInvoices {
    pos: String,
    inv: Vec<B2clInvoice>,
}

#[derive(Serialize)]
struct B2cs {
    sply_ty: &'static str,
    rt: Rate,
    typ: &'static str,
    pos: String,
    txval: Amount,
    #[serde(skip_serializing_if = "Option::is_none")]
    iamt: Option<Amount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    camt: Option<Amount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    samt: Option<Amount>,
    csamt: Amount,
}

#[derive(Serialize)]
struct ExportItem {
    txval: Amount,
    rt: Rate,
    iamt: Amount,
    csamt: Amount,
}

#[derive(Serialize)]
struct ExportInvoice {
    inum: String,
    idt: String,
    val: Amount,
    itms: Vec<ExportItem>,
}

#[derive(Serialize)]
struct ExportGroup {
    exp_typ: &'static str,
    inv: Vec<ExportInvoice>,
}

#[derive(Serialize)]
struct RegisteredNote {
    ntty: &'static str,
    nt_num: String,
    nt_dt: String,
    val: Amount,
    pos: String,
    rchrg: &'static str,
    inv_typ: &'static str,
    itms: Vec<NumberedItem>,
}

#[derive(Serialize)]
struct PartyNotes {
    ctin: String,
    nt: Vec<RegisteredNote>,
}

#[derive(Serialize)]
struct Cdnur {
    typ: &'static str,
    ntty: &'static str,
    nt_num: String,
    nt_dt: String,
    val: Amount,
    #[serde(skip_serializing_if = "Option::is_none")]
    pos: Option<String>,
    itms: Vec<NumberedItem>,
}

#[derive(Serialize)]
struct NilEntry {
    sply_ty: &'static str,
    nil_amt: Amount,
    expt_amt: Amount,
    ngsup_amt: Amount,
}

#[derive(Serialize)]
struct Nil {
    inv: Vec<NilEntry>,
}

#[derive(Serialize)]
struct HsnEntry {
    num: usize,
    hsn_sc: String,
    desc: String,
    uqc: String,
    qty: Quantity,
    rt: Rate,
    txval: Amount,
    iamt: Amount,
    camt: Amount,
    samt: Amount,
    csamt: Amount,
}

#[derive(Serialize)]
struct Hsn {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    hsn_b2b: Vec<HsnEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    hsn_b2c: Vec<HsnEntry>,
}

#[derive(Serialize)]
struct DocRange {
    num: usize,
    from: String,
    to: String,
    totnum: u32,
    cancel: u32,
    net_issue: u32,
}

#[derive(Serialize)]
struct DocGroup {
    doc_num: u8,
    doc_typ: &'static str,
    docs: Vec<DocRange>,
}

#[derive(Serialize)]
struct DocIssue {
    doc_det: Vec<DocGroup>,
}

fn note_code(t: NoteType) -> &'static str {
    match t {
        NoteType::Credit => "C",
        NoteType::Debit => "D",
    }
}

/// Group by key, keeping first-seen order of keys and documents.
fn group<K: PartialEq + Clone, T, U>(
    items: &[T],
    key: impl Fn(&T) -> K,
    map: impl Fn(&T) -> U,
) -> Vec<(K, Vec<U>)> {
    let mut out: Vec<(K, Vec<U>)> = Vec::new();
    for i in items {
        let k = key(i);
        match out.iter_mut().find(|(g, _)| *g == k) {
            Some((_, v)) => v.push(map(i)),
            None => out.push((k, vec![map(i)])),
        }
    }
    out
}

fn hsn_entries(rows: &[HsnRow]) -> Vec<HsnEntry> {
    rows.iter()
        .enumerate()
        .map(|(n, r)| HsnEntry {
            num: n + 1,
            hsn_sc: r.hsn.clone(),
            desc: r.description.clone(),
            uqc: r.uqc.clone(),
            qty: Quantity(r.quantity.clone()),
            rt: Rate(r.item.rate_bp),
            txval: Amount(r.item.taxable_paise),
            iamt: Amount(r.item.igst_paise),
            camt: Amount(r.item.cgst_paise),
            samt: Amount(r.item.sgst_paise),
            csamt: Amount(r.item.cess_paise),
        })
        .collect()
}

/// The return as portal JSON, pretty-printed with a trailing newline.
pub fn to_json(r: &Gstr1Return) -> String {
    let b2b = group(
        &r.b2b,
        |i| i.ctin.clone().unwrap_or_default(),
        |i| B2bInvoice {
            inum: i.number.clone(),
            idt: date(i.date),
            val: Amount(i.value_paise),
            pos: pos(i.pos),
            rchrg: yn(i.reverse_charge),
            inv_typ: i.invoice_type.code(),
            itms: numbered(&i.items, heads(i.intra_state)),
        },
    );
    let b2cl = group(
        &r.b2cl,
        |i| i.pos,
        |i| B2clInvoice {
            inum: i.number.clone(),
            idt: date(i.date),
            val: Amount(i.value_paise),
            itms: numbered(&i.items, Heads::Inter),
        },
    );
    let exp = group(
        &r.exp,
        |i| i.export_with_payment,
        |i| ExportInvoice {
            inum: i.number.clone(),
            idt: date(i.date),
            val: Amount(i.value_paise),
            itms: i
                .items
                .iter()
                .map(|t| ExportItem {
                    txval: Amount(t.taxable_paise),
                    rt: Rate(t.rate_bp),
                    iamt: Amount(t.igst_paise),
                    csamt: Amount(t.cess_paise),
                })
                .collect(),
        },
    );
    let cdnr = group(
        &r.cdnr,
        |n| n.ctin.clone().unwrap_or_default(),
        |n| RegisteredNote {
            ntty: note_code(n.note_type),
            nt_num: n.number.clone(),
            nt_dt: date(n.date),
            val: Amount(n.value_paise),
            pos: pos(n.pos),
            rchrg: yn(n.reverse_charge),
            inv_typ: n.invoice_type.code(),
            itms: numbered(&n.items, heads(n.intra_state)),
        },
    );
    let docs = group(
        &{
            let mut d = r.documents.clone();
            d.sort_by_key(|s| s.nature);
            d
        },
        |s| s.nature,
        |s| (s.from.clone(), s.to.clone(), s.total, s.cancelled),
    );

    let file = File {
        gstin: &r.gstin,
        fp: format!("{:02}{}", r.month, r.year),
        version: PORTAL_SCHEMA,
        hash: "hash",
        b2b: b2b
            .into_iter()
            .map(|(ctin, inv)| PartyInvoices { ctin, inv })
            .collect(),
        b2cl: b2cl
            .into_iter()
            .map(|(p, inv)| PosInvoices { pos: pos(p), inv })
            .collect(),
        b2cs: r
            .b2cs
            .iter()
            .map(|row| {
                let (iamt, camt, samt) = if row.intra_state {
                    (
                        None,
                        Some(Amount(row.item.cgst_paise)),
                        Some(Amount(row.item.sgst_paise)),
                    )
                } else {
                    (Some(Amount(row.item.igst_paise)), None, None)
                };
                B2cs {
                    sply_ty: if row.intra_state { "INTRA" } else { "INTER" },
                    rt: Rate(row.item.rate_bp),
                    typ: "OE",
                    pos: pos(row.pos),
                    txval: Amount(row.item.taxable_paise),
                    iamt,
                    camt,
                    samt,
                    csamt: Amount(row.item.cess_paise),
                }
            })
            .collect(),
        exp: exp
            .into_iter()
            .map(|(paid, inv)| ExportGroup {
                exp_typ: if paid { "WPAY" } else { "WOPAY" },
                inv,
            })
            .collect(),
        cdnr: cdnr
            .into_iter()
            .map(|(ctin, nt)| PartyNotes { ctin, nt })
            .collect(),
        cdnur: r
            .cdnur
            .iter()
            .map(|n| {
                let typ = n.unregistered_type.unwrap_or(UnregisteredType::B2cl);
                let export = typ != UnregisteredType::B2cl;
                Cdnur {
                    typ: match typ {
                        UnregisteredType::B2cl => "B2CL",
                        UnregisteredType::ExportWithPayment => "EXPWP",
                        UnregisteredType::ExportWithoutPayment => "EXPWOP",
                    },
                    ntty: note_code(n.note_type),
                    nt_num: n.number.clone(),
                    nt_dt: date(n.date),
                    val: Amount(n.value_paise),
                    pos: (!export).then(|| pos(n.pos)),
                    itms: numbered(&n.items, Heads::Inter),
                }
            })
            .collect(),
        nil: (!r.nil.is_empty()).then(|| {
            let mut rows = r.nil.clone();
            rows.sort_by_key(|n| n.supply);
            Nil {
                inv: rows
                    .iter()
                    .map(|n| NilEntry {
                        sply_ty: n.supply.code(),
                        nil_amt: Amount(n.nil_paise),
                        expt_amt: Amount(n.exempt_paise),
                        ngsup_amt: Amount(n.non_gst_paise),
                    })
                    .collect(),
            }
        }),
        hsn: (!r.hsn_b2b.is_empty() || !r.hsn_b2c.is_empty()).then(|| Hsn {
            hsn_b2b: hsn_entries(&r.hsn_b2b),
            hsn_b2c: hsn_entries(&r.hsn_b2c),
        }),
        doc_issue: (!docs.is_empty()).then(|| DocIssue {
            doc_det: docs
                .into_iter()
                .map(|(nature, ranges)| {
                    let (doc_num, doc_typ) = nature.code();
                    DocGroup {
                        doc_num,
                        doc_typ,
                        docs: ranges
                            .into_iter()
                            .enumerate()
                            .map(|(n, (from, to, total, cancelled))| DocRange {
                                num: n + 1,
                                from,
                                to,
                                totnum: total,
                                cancel: cancelled,
                                net_issue: total.saturating_sub(cancelled),
                            })
                            .collect(),
                    }
                })
                .collect(),
        }),
    };
    let mut s = serde_json::to_string_pretty(&file).expect("GSTR-1 JSON is always serialisable");
    s.push('\n');
    s
}
