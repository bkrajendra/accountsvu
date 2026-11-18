//! CSV/Excel import of masters with a preview and field-mapping step
//! (spec §5.1, D15).
//!
//! Flow: the UI sends the file → [`parse_import_file`] returns its rows and a
//! suggested column mapping → the user adjusts the mapping →
//! [`AppCore::preview_import`] validates every row with the same rules as the
//! forms → [`AppCore::commit_import`] creates the valid rows in one
//! transaction and reports the rows it skipped. Nothing is stored until commit.

use std::collections::{HashMap, HashSet};
use std::io::Cursor;

use av_domain::StateCode;
use av_persistence::records::{ItemRecord, PartyRecord};
use av_persistence::{Store, UnitOfWork};
use calamine::{Data, Reader, open_workbook_auto_from_rs};
use serde::{Deserialize, Serialize};

use crate::AppCore;
use crate::context::{Permission, RequestContext};
use crate::error::{AppError, AppResult, Errors, FieldError};
use crate::masters::{
    ItemInput, PartyInput, check_item_unique, check_party_unique, insert_item, insert_party,
    validate_item, validate_party,
};
use crate::setup::{Change, audit};

const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;
const MAX_ROWS: usize = 5_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    Parties,
    Items,
}

impl ImportKind {
    fn noun(self) -> &'static str {
        match self {
            ImportKind::Parties => "parties",
            ImportKind::Items => "items",
        }
    }
}

/// A target field the user maps a column to.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportField {
    pub key: &'static str,
    pub label: &'static str,
    pub required: bool,
    pub hint: &'static str,
    #[serde(skip)]
    aliases: &'static [&'static str],
    #[serde(skip)]
    example: &'static str,
}

const fn f(
    key: &'static str,
    label: &'static str,
    required: bool,
    hint: &'static str,
    aliases: &'static [&'static str],
    example: &'static str,
) -> ImportField {
    ImportField {
        key,
        label,
        required,
        hint,
        aliases,
        example,
    }
}

const PARTY_FIELDS: &[ImportField] = &[
    f(
        "name",
        "Name",
        true,
        "",
        &[
            "partyname",
            "customername",
            "suppliername",
            "ledgername",
            "displayname",
        ],
        "Ravi Stores",
    ),
    f(
        "kind",
        "Type",
        false,
        "customer, supplier or both (default customer)",
        &["partytype", "type", "category"],
        "customer",
    ),
    f(
        "gstType",
        "GST status",
        false,
        "regular, composition, unregistered, consumer, overseas, sez (guessed from GSTIN)",
        &["gststatus", "registrationtype", "gsttype"],
        "regular",
    ),
    f(
        "gstin",
        "GSTIN",
        false,
        "",
        &["gstno", "gstnumber", "gstinuin"],
        "27AAPFU0939F1ZV",
    ),
    f(
        "legalName",
        "Legal name",
        false,
        "",
        &["legalname", "registeredname"],
        "",
    ),
    f("pan", "PAN", false, "", &["panno", "pannumber"], ""),
    f(
        "stateCode",
        "State",
        false,
        "state name or 2-digit code (from GSTIN when present)",
        &["state", "statename", "placeofsupply", "pos"],
        "Maharashtra",
    ),
    f(
        "addressLine1",
        "Address line 1",
        false,
        "",
        &["address", "address1", "billingaddress", "street"],
        "12 MG Road",
    ),
    f(
        "addressLine2",
        "Address line 2",
        false,
        "",
        &["address2", "area", "locality"],
        "",
    ),
    f("city", "City", false, "", &["town", "district"], "Pune"),
    f(
        "pincode",
        "PIN code",
        false,
        "",
        &["pin", "postcode", "zip", "zipcode", "postalcode"],
        "411001",
    ),
    f(
        "contactPerson",
        "Contact person",
        false,
        "",
        &["contact", "contactname"],
        "",
    ),
    f(
        "email",
        "E-mail",
        false,
        "",
        &["emailid", "mail", "emailaddress"],
        "accounts@ravistores.in",
    ),
    f(
        "phone",
        "Phone",
        false,
        "",
        &["mobile", "mobileno", "phoneno", "contactno", "telephone"],
        "9820012345",
    ),
    f(
        "paymentTermsDays",
        "Payment terms (days)",
        false,
        "",
        &["creditdays", "paymentterms", "terms"],
        "30",
    ),
    f("creditLimit", "Credit limit", false, "", &["limit"], ""),
    f(
        "openingBalance",
        "Opening balance",
        false,
        "positive or Dr = they owe you; negative or Cr = you owe them",
        &["opening", "openingbal", "balance"],
        "12500 Dr",
    ),
    f(
        "notes",
        "Notes",
        false,
        "",
        &["remarks", "comment", "comments"],
        "",
    ),
];

const ITEM_FIELDS: &[ImportField] = &[
    f(
        "name",
        "Name",
        true,
        "",
        &["itemname", "productname", "servicename", "description1"],
        "Laptop 14-inch",
    ),
    f(
        "kind",
        "Type",
        false,
        "goods or service (guessed from HSN/SAC)",
        &["itemtype", "type", "goodsservice"],
        "goods",
    ),
    f(
        "code",
        "Item code",
        false,
        "",
        &["sku", "itemcode", "productcode", "partno", "partnumber"],
        "LAP-14",
    ),
    f("description", "Description", false, "", &["details"], ""),
    f(
        "hsnSac",
        "HSN/SAC",
        false,
        "required unless non-GST",
        &["hsn", "sac", "hsncode", "saccode", "hsnsaccode"],
        "8471",
    ),
    f(
        "unit",
        "Unit (UQC)",
        false,
        "e.g. NOS, KGS, PCS (default NOS / OTH)",
        &["uqc", "uom", "unitofmeasure", "units"],
        "NOS",
    ),
    f(
        "taxCategory",
        "Tax category",
        false,
        "taxable, exempt, nil_rated, non_gst (default taxable)",
        &["category", "taxability", "gstcategory"],
        "taxable",
    ),
    f(
        "gstRate",
        "GST rate %",
        false,
        "e.g. 18",
        &["gst", "rate", "taxrate", "gstpercent", "igstrate"],
        "18",
    ),
    f(
        "cessRate",
        "Cess rate %",
        false,
        "",
        &["cess", "cesspercent"],
        "",
    ),
    f(
        "salesPrice",
        "Sales price",
        false,
        "",
        &["price", "rate1", "sellingprice", "mrp", "saleprice"],
        "52000",
    ),
    f(
        "purchasePrice",
        "Purchase price",
        false,
        "",
        &["cost", "costprice", "buyingprice"],
        "45000",
    ),
    f(
        "priceIncludesTax",
        "Price includes GST",
        false,
        "yes or no",
        &["inclusive", "taxinclusive", "inclusiveoftax"],
        "no",
    ),
    f(
        "trackStock",
        "Track stock",
        false,
        "yes or no (goods only)",
        &["stock", "inventory", "stocktracked"],
        "yes",
    ),
];

fn fields_for(kind: ImportKind) -> &'static [ImportField] {
    match kind {
        ImportKind::Parties => PARTY_FIELDS,
        ImportKind::Items => ITEM_FIELDS,
    }
}

/// The rows of an uploaded file. Sent back unchanged with preview/commit, so
/// the server keeps no import state.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportTable {
    pub file_name: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Field key → zero-based column index.
pub type ImportMapping = HashMap<String, usize>;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedImport {
    pub table: ImportTable,
    pub fields: Vec<ImportField>,
    pub mapping: ImportMapping,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportTemplate {
    pub fields: Vec<ImportField>,
    pub file_name: String,
    pub csv: String,
}

/// A row that will not be imported, numbered as in the spreadsheet (the
/// header is row 1).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowIssue {
    pub row: usize,
    pub label: String,
    pub errors: Vec<FieldError>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub total: usize,
    pub valid: usize,
    pub issues: Vec<RowIssue>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub created: usize,
    pub skipped: Vec<RowIssue>,
}

// ---------------------------------------------------------------------------
// File parsing
// ---------------------------------------------------------------------------

/// Read a CSV or Excel (.xlsx/.xls/.ods) file into rows of text, and suggest
/// which column feeds which field.
pub fn parse_import_file(
    kind: ImportKind,
    file_name: &str,
    bytes: &[u8],
) -> AppResult<ParsedImport> {
    let mut grid = read_grid(file_name, bytes)?;
    // The first non-blank row is the header.
    let headers: Vec<String> = grid
        .remove(0)
        .into_iter()
        .map(|h| h.trim().to_owned())
        .collect();
    if grid.len() > MAX_ROWS {
        return Err(AppError::Invalid(format!(
            "the file has more than {MAX_ROWS} rows; split it into smaller files"
        )));
    }
    let width = headers.len();
    let rows = grid
        .into_iter()
        .map(|mut r| {
            r.resize(width, String::new());
            r.into_iter().map(|c| c.trim().to_owned()).collect()
        })
        .collect();
    let mapping = suggest_mapping(kind, &headers);
    Ok(ParsedImport {
        table: ImportTable {
            file_name: file_name.to_owned(),
            headers,
            rows,
        },
        fields: fields_for(kind).to_vec(),
        mapping,
    })
}

/// Read a CSV or Excel (.xlsx/.xls/.ods) file into rows of trimmed text,
/// dropping fully blank rows.
pub(crate) fn read_grid(file_name: &str, bytes: &[u8]) -> AppResult<Vec<Vec<String>>> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(AppError::Invalid("the file is larger than 5 MB".into()));
    }
    let lower = file_name.to_ascii_lowercase();
    let mut grid = if lower.ends_with(".csv") || lower.ends_with(".txt") {
        read_csv(bytes)?
    } else if [".xlsx", ".xlsm", ".xls", ".ods"]
        .iter()
        .any(|e| lower.ends_with(e))
    {
        read_spreadsheet(bytes)?
    } else {
        return Err(AppError::Invalid(
            "choose a .csv, .xls or .xlsx file".into(),
        ));
    };
    grid.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
    if grid.is_empty() {
        return Err(AppError::Invalid("the file has no rows".into()));
    }
    for row in &mut grid {
        for c in row.iter_mut() {
            *c = c.trim().to_owned();
        }
    }
    Ok(grid)
}

fn read_csv(bytes: &[u8]) -> AppResult<Vec<Vec<String>>> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    // Excel's plain "CSV" on Windows is ANSI, not UTF-8.
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t.to_owned(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    };
    let first = text.lines().next().unwrap_or_default();
    let delimiter = b",;\t"
        .iter()
        .copied()
        .max_by_key(|d| first.bytes().filter(|b| b == d).count())
        .unwrap_or(b',');
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(text.as_bytes());
    rdr.records()
        .map(|r| {
            r.map(|rec| rec.iter().map(str::to_owned).collect())
                .map_err(|e| AppError::Invalid(format!("the CSV file could not be read: {e}")))
        })
        .collect()
}

fn read_spreadsheet(bytes: &[u8]) -> AppResult<Vec<Vec<String>>> {
    let mut wb = open_workbook_auto_from_rs(Cursor::new(bytes.to_vec()))
        .map_err(|e| AppError::Invalid(format!("the spreadsheet could not be read: {e}")))?;
    let range = wb
        .worksheet_range_at(0)
        .ok_or_else(|| AppError::Invalid("the spreadsheet has no sheets".into()))?
        .map_err(|e| AppError::Invalid(format!("the first sheet could not be read: {e}")))?;
    Ok(range
        .rows()
        .map(|r| r.iter().map(cell_text).collect())
        .collect())
}

fn cell_text(c: &Data) -> String {
    match c {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        // Whole numbers (PIN codes, phone numbers) must not gain a ".0".
        Data::Float(v) if v.fract() == 0.0 && v.abs() < 1e15 => format!("{v:.0}"),
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => {
            if *b {
                "yes".into()
            } else {
                "no".into()
            }
        }
        other => other.to_string(),
    }
}

fn normalise(h: &str) -> String {
    h.chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

fn suggest_mapping(kind: ImportKind, headers: &[String]) -> ImportMapping {
    let norm: Vec<String> = headers.iter().map(|h| normalise(h)).collect();
    let mut used = HashSet::new();
    let mut mapping = ImportMapping::new();
    for field in fields_for(kind) {
        let candidates: Vec<String> = std::iter::once(normalise(field.key))
            .chain(std::iter::once(normalise(field.label)))
            .chain(field.aliases.iter().map(|a| normalise(a)))
            .collect();
        if let Some(col) = norm
            .iter()
            .enumerate()
            .position(|(i, h)| !used.contains(&i) && candidates.contains(h))
        {
            used.insert(col);
            mapping.insert(field.key.to_owned(), col);
        }
    }
    mapping
}

/// A blank template with one example row.
pub fn import_template(kind: ImportKind) -> ImportTemplate {
    let fields = fields_for(kind);
    let mut w = csv::Writer::from_writer(Vec::new());
    // Writing to a Vec cannot fail.
    let _ = w.write_record(fields.iter().map(|f| f.label));
    let _ = w.write_record(fields.iter().map(|f| f.example));
    let csv = String::from_utf8(w.into_inner().unwrap_or_default()).unwrap_or_default();
    ImportTemplate {
        fields: fields.to_vec(),
        file_name: format!("accountsvu-{}-template.csv", kind.noun()),
        csv,
    }
}

// ---------------------------------------------------------------------------
// Row → form input
// ---------------------------------------------------------------------------

struct Row<'a> {
    cells: &'a [String],
    mapping: &'a ImportMapping,
}

impl Row<'_> {
    fn get(&self, key: &str) -> Option<String> {
        self.mapping
            .get(key)
            .and_then(|&i| self.cells.get(i))
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    }
    /// Enum-like values: "Nil Rated" → "nil_rated".
    fn token(&self, key: &str) -> Option<String> {
        self.get(key)
            .map(|s| s.to_lowercase().replace(['-', ' '], "_"))
    }
    fn flag(&self, key: &str) -> bool {
        self.token(key)
            .is_some_and(|v| matches!(v.as_str(), "yes" | "y" | "true" | "1"))
    }
}

fn state_from_text(s: &str) -> Option<u8> {
    if let Ok(n) = s.parse::<u8>() {
        return Some(n);
    }
    let n = normalise(s);
    StateCode::all()
        .find(|st| normalise(st.name()) == n)
        .map(|st| st.code())
}

fn party_input(r: &Row<'_>, e: &mut Errors) -> PartyInput {
    let gstin = r.get("gstin");
    let kind = r.token("kind").unwrap_or_else(|| "customer".into());
    let gst_type = r.token("gstType").unwrap_or_else(|| {
        if gstin.is_some() {
            "regular".into()
        } else if kind == "customer" {
            "consumer".into()
        } else {
            "unregistered".into()
        }
    });
    let state_code = r.get("stateCode").and_then(|s| {
        let code = state_from_text(&s);
        if code.is_none() {
            e.add("stateCode", format!("Unknown state “{s}”."));
        }
        code
    });
    // "12500 Dr" / "12500 Cr" / "-12500".
    let (opening_balance, opening_balance_side) = match r.get("openingBalance") {
        Some(v) => {
            let lower = v.to_ascii_lowercase();
            let (num, side) = if let Some(n) = lower.strip_suffix("cr") {
                (n.trim().to_owned(), Some("payable"))
            } else if let Some(n) = lower.strip_suffix("dr") {
                (n.trim().to_owned(), Some("receivable"))
            } else if let Some(n) = lower.strip_prefix('-') {
                (n.trim().to_owned(), Some("payable"))
            } else {
                (lower.trim().to_owned(), None)
            };
            (Some(num), side.map(str::to_owned))
        }
        None => (None, None),
    };
    let payment_terms_days = r
        .get("paymentTermsDays")
        .and_then(|v| match v.parse::<i64>() {
            Ok(d) => Some(d),
            Err(_) => {
                e.add("paymentTermsDays", "Enter a whole number of days.");
                None
            }
        });
    PartyInput {
        kind,
        name: r.get("name").unwrap_or_default(),
        legal_name: r.get("legalName"),
        gst_type,
        gstin,
        pan: r.get("pan"),
        state_code,
        address_line1: r.get("addressLine1"),
        address_line2: r.get("addressLine2"),
        city: r.get("city"),
        pincode: r.get("pincode"),
        shipping_same: true,
        contact_person: r.get("contactPerson"),
        email: r.get("email"),
        phone: r.get("phone"),
        payment_terms_days,
        credit_limit: r.get("creditLimit"),
        opening_balance,
        opening_balance_side,
        notes: r.get("notes"),
        is_active: true,
        ..Default::default()
    }
}

fn item_input(r: &Row<'_>) -> ItemInput {
    let hsn = r.get("hsnSac");
    let kind = r.token("kind").unwrap_or_else(|| {
        if hsn.as_deref().is_some_and(|h| h.starts_with("99")) {
            "service".into()
        } else {
            "goods".into()
        }
    });
    ItemInput {
        kind,
        name: r.get("name").unwrap_or_default(),
        code: r.get("code"),
        description: r.get("description"),
        hsn_sac: hsn,
        unit: r.get("unit"),
        tax_category: r.token("taxCategory").unwrap_or_else(|| "taxable".into()),
        gst_rate: r.get("gstRate"),
        cess_rate: r.get("cessRate"),
        sales_price: r.get("salesPrice"),
        purchase_price: r.get("purchasePrice"),
        price_includes_tax: r.flag("priceIncludesTax"),
        track_stock: r.flag("trackStock"),
        is_active: true,
    }
}

fn check_mapping(kind: ImportKind, table: &ImportTable, mapping: &ImportMapping) -> AppResult<()> {
    let mut e = Errors::new("mapping.");
    for field in fields_for(kind) {
        match mapping.get(field.key) {
            None if field.required => {
                e.add(field.key, format!("Choose the column for {}.", field.label))
            }
            Some(&col) if col >= table.headers.len() => {
                e.add(field.key, "That column isn't in the file.")
            }
            _ => {}
        }
    }
    e.finish()
}

/// Turn field errors into "Label: message" against the import's own field names.
fn labelled(kind: ImportKind, errors: Vec<FieldError>) -> Vec<FieldError> {
    let fields = fields_for(kind);
    errors
        .into_iter()
        .map(|fe| {
            let label = fields
                .iter()
                .find(|f| f.key == fe.field)
                .map_or(fe.field.as_str(), |f| f.label);
            FieldError {
                message: format!("{label}: {}", fe.message),
                field: fe.field,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Use cases
// ---------------------------------------------------------------------------

enum Outcome<T> {
    Valid(T),
    Invalid(RowIssue),
}

/// A row after the file-level checks, waiting for the database checks.
struct Staged<T> {
    row: usize,
    label: String,
    errors: Errors,
    record: Option<T>,
}

/// Validate every row with the form rules and catch duplicates within the
/// file. Pure: no storage access.
fn stage_rows<T>(
    kind: ImportKind,
    table: &ImportTable,
    mapping: &ImportMapping,
    mut build: impl FnMut(&Row<'_>, &mut Errors) -> Option<T>,
    name_of: impl Fn(&T) -> (String, Option<String>),
) -> Vec<Staged<T>> {
    let mut seen_names = HashSet::new();
    let mut seen_keys = HashSet::new();
    let key_field = if kind == ImportKind::Parties {
        "gstin"
    } else {
        "code"
    };
    table
        .rows
        .iter()
        .enumerate()
        .map(|(i, cells)| {
            let row = Row { cells, mapping };
            let label = row.get("name").unwrap_or_else(|| "(no name)".into());
            let mut errors = Errors::new("");
            let record = build(&row, &mut errors);
            if let Some(r) = &record {
                let (name, key) = name_of(r);
                if !seen_names.insert(name.to_lowercase()) {
                    errors.add("name", "Appears more than once in this file.");
                }
                if let Some(k) = key
                    && !seen_keys.insert(k.to_lowercase())
                {
                    errors.add(key_field, "Appears more than once in this file.");
                }
            }
            // Spreadsheet row numbers: the header is row 1.
            Staged {
                row: i + 2,
                label,
                errors,
                record,
            }
        })
        .collect()
}

fn outcome<T>(kind: ImportKind, s: Staged<T>) -> Option<Outcome<T>> {
    match (s.record, s.errors.finish()) {
        (Some(r), Ok(())) => Some(Outcome::Valid(r)),
        (_, Err(err)) => Some(Outcome::Invalid(RowIssue {
            row: s.row,
            label: s.label,
            errors: labelled(kind, err.field_errors().to_vec()),
        })),
        (None, Ok(())) => None,
    }
}

impl<S: Store> AppCore<S> {
    /// Validate every row, including duplicates within the file and against
    /// existing records.
    async fn evaluate(
        &self,
        kind: ImportKind,
        uow: &mut S::Uow,
        org_id: &str,
        table: &ImportTable,
        mapping: &ImportMapping,
    ) -> AppResult<(Vec<Outcome<PartyRecord>>, Vec<Outcome<ItemRecord>>)> {
        match kind {
            ImportKind::Parties => {
                let mut staged = stage_rows(
                    kind,
                    table,
                    mapping,
                    |row, e| {
                        let input = party_input(row, e);
                        let (pe, rec) = validate_party(&input, "");
                        e.absorb(pe);
                        rec
                    },
                    |p| (p.name.clone(), p.gstin.clone()),
                );
                for s in &mut staged {
                    if let Some(p) = &s.record {
                        check_party_unique(uow, &mut s.errors, org_id, p, None).await?;
                    }
                }
                Ok((
                    staged
                        .into_iter()
                        .filter_map(|s| outcome(kind, s))
                        .collect(),
                    Vec::new(),
                ))
            }
            ImportKind::Items => {
                let mut staged = stage_rows(
                    kind,
                    table,
                    mapping,
                    |row, e| {
                        let (ie, rec) = validate_item(&item_input(row), "");
                        e.absorb(ie);
                        rec
                    },
                    |i| (i.name.clone(), i.code.clone()),
                );
                for s in &mut staged {
                    if let Some(i) = &s.record {
                        check_item_unique(uow, &mut s.errors, org_id, i, None).await?;
                    }
                }
                Ok((
                    Vec::new(),
                    staged
                        .into_iter()
                        .filter_map(|s| outcome(kind, s))
                        .collect(),
                ))
            }
        }
    }

    pub async fn preview_import(
        &self,
        ctx: &RequestContext,
        kind: ImportKind,
        table: &ImportTable,
        mapping: &ImportMapping,
    ) -> AppResult<ImportPreview> {
        ctx.require(Permission::ManageMasters)?;
        check_mapping(kind, table, mapping)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let (parties, items) = self
            .evaluate(kind, &mut uow, &org.id, table, mapping)
            .await?;
        let mut issues = Vec::new();
        let mut valid = 0;
        for o in parties
            .into_iter()
            .map(|o| o.map_valid())
            .chain(items.into_iter().map(|o| o.map_valid()))
        {
            match o {
                Outcome::Valid(()) => valid += 1,
                Outcome::Invalid(issue) => issues.push(issue),
            }
        }
        Ok(ImportPreview {
            total: table.rows.len(),
            valid,
            issues,
        })
    }

    /// Create every valid row in one transaction; invalid rows are skipped and
    /// reported. Re-validates, so a stale preview can't import bad data.
    pub async fn commit_import(
        &self,
        ctx: &RequestContext,
        kind: ImportKind,
        table: &ImportTable,
        mapping: &ImportMapping,
    ) -> AppResult<ImportResult> {
        ctx.require(Permission::ManageMasters)?;
        check_mapping(kind, table, mapping)?;
        let mut uow = self.store.begin().await?;
        let org = self.require_org(ctx, &mut uow).await?;
        let (parties, items) = self
            .evaluate(kind, &mut uow, &org.id, table, mapping)
            .await?;
        let now = self.now();
        let mut created = 0;
        let mut skipped = Vec::new();
        for o in parties {
            match o {
                Outcome::Valid(p) => {
                    insert_party(&mut uow, ctx, &org.id, &org.books_start_date, p, &now).await?;
                    created += 1;
                }
                Outcome::Invalid(issue) => skipped.push(issue),
            }
        }
        for o in items {
            match o {
                Outcome::Valid(i) => {
                    insert_item(&mut uow, ctx, &org.id, i, &now).await?;
                    created += 1;
                }
                Outcome::Invalid(issue) => skipped.push(issue),
            }
        }
        let summary = serde_json::json!({
            "fileName": table.file_name,
            "kind": kind.noun(),
            "created": created,
            "skipped": skipped.len(),
        });
        let c = Change {
            action: "import.run",
            entity_id: None,
            before: None,
            after: Some(summary.to_string()),
        };
        audit(&mut uow, ctx, &org.id, c, &now).await?;
        uow.commit().await?;
        Ok(ImportResult { created, skipped })
    }
}

impl<T> Outcome<T> {
    fn map_valid(self) -> Outcome<()> {
        match self {
            Outcome::Valid(_) => Outcome::Valid(()),
            Outcome::Invalid(i) => Outcome::Invalid(i),
        }
    }
}
