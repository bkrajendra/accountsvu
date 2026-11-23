//! Printed documents (spec D16). Pure and deterministic: the same data always
//! yields the same PDF bytes, which the golden-file tests rely on.

pub mod format;
mod invoice;
mod voucher;
mod world;

pub use invoice::{InvoiceData, InvoiceLine, Logo, LogoFormat, PartyBlock};
pub use voucher::{VoucherData, VoucherRow, VoucherTax};

use typst::foundations::Smart;
use typst_layout::PagedDocument;
use typst_pdf::PdfOptions;

const INVOICE_TEMPLATE: &str = include_str!("../templates/invoice.typ");
const VOUCHER_TEMPLATE: &str = include_str!("../templates/voucher.typ");

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("The document could not be laid out: {0}")]
    Layout(String),
    #[error("The PDF could not be written: {0}")]
    Export(String),
}

fn messages(diags: &[typst::diag::SourceDiagnostic]) -> String {
    diags
        .iter()
        .map(|d| d.message.to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

/// The print view handed to the template, as pretty JSON. Golden tests diff
/// this so layout data changes are reviewable as text.
pub fn invoice_view_json(data: &InvoiceData) -> String {
    serde_json::to_string_pretty(&data.view()).expect("view serialises")
}

fn compile_invoice(data: &InvoiceData) -> Result<PagedDocument, RenderError> {
    let view = serde_json::to_vec(&data.view()).map_err(|e| RenderError::Layout(e.to_string()))?;
    let mut files = vec![("/data.json", view)];
    if let (Some(path), Some(logo)) = (data.logo_path(), &data.logo) {
        files.push((path, logo.bytes.clone()));
    }
    let world = world::DocWorld::new(INVOICE_TEMPLATE, files);
    typst::compile(&world)
        .output
        .map_err(|e| RenderError::Layout(messages(&e)))
}

/// Renders a sales document to PDF bytes.
pub fn render_invoice(data: &InvoiceData) -> Result<Vec<u8>, RenderError> {
    let doc = compile_invoice(data)?;
    let options = PdfOptions {
        ident: Smart::Custom(format!(
            "accountsvu:{}:{}",
            data.title,
            data.number.as_deref().unwrap_or("draft")
        )),
        creator: Smart::Custom(Some("AccountsVU".into())),
        timestamp: None,
        ..PdfOptions::default()
    };
    typst_pdf::pdf(&doc, &options).map_err(|e| RenderError::Export(messages(&e)))
}

/// Number of pages a rendered document has.
pub fn invoice_page_count(data: &InvoiceData) -> Result<usize, RenderError> {
    Ok(compile_invoice(data)?.pages().len())
}

/// The voucher print view as pretty JSON (golden tests).
pub fn voucher_view_json(data: &VoucherData) -> String {
    serde_json::to_string_pretty(&data.view()).expect("view serialises")
}

/// Renders a receipt, payment or refund voucher to PDF bytes.
pub fn render_voucher(data: &VoucherData) -> Result<Vec<u8>, RenderError> {
    let view = serde_json::to_vec(&data.view()).map_err(|e| RenderError::Layout(e.to_string()))?;
    let mut files = vec![("/data.json", view)];
    if let (Some(path), Some(logo)) = (data.logo_path(), &data.logo) {
        files.push((path, logo.bytes.clone()));
    }
    let world = world::DocWorld::new(VOUCHER_TEMPLATE, files);
    let doc: PagedDocument = typst::compile(&world)
        .output
        .map_err(|e| RenderError::Layout(messages(&e)))?;
    let options = PdfOptions {
        ident: Smart::Custom(format!("accountsvu:{}:{}", data.title, data.number)),
        creator: Smart::Custom(Some("AccountsVU".into())),
        timestamp: None,
        ..PdfOptions::default()
    };
    typst_pdf::pdf(&doc, &options).map_err(|e| RenderError::Export(messages(&e)))
}
