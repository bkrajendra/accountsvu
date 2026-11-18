/**
 * In-memory stand-in for the Rust core, used only when the UI runs in a plain
 * browser (`pnpm dev`). It mimics command shapes and a few key validations so
 * screens can be built and reviewed without the desktop shell. The real rules
 * live in crates/application and are the only ones that matter.
 */
import type {
  Account,
  AuditEvent,
  BusinessProfile,
  FinancialYearInfo,
  ImportField,
  ImportKind,
  ImportMapping,
  ImportTable,
  Item,
  ItemInput,
  Organisation,
  Attachment,
  ItcStatus,
  LineCalc,
  Party,
  PaymentMode,
  PurchaseCalculation,
  PurchaseDocument,
  PurchaseDocumentInput,
  PurchaseDocumentView,
  PurchaseLine,
  PurchaseStatus,
  PartyInput,
  SalesCalculation,
  SalesDisplayStatus,
  SalesDocument,
  SalesDocumentInput,
  SalesDocumentView,
  SalesLine,
  SalesLineInput,
  SupplyKind,
  TotalsCalc,
  OrganisationInput,
  Registration,
  RegistrationInput,
  Series,
  SeriesInput,
  SeriesUpdateInput,
  SetupBusinessInput,
  UpdateRegistrationInput,
} from "./api";
import { gstinError, gstinParts, STATES } from "./gst";

type Fail = { code: string; message: string; fields: { field: string; message: string }[] };

let org: Organisation | null = null;
let regs: Registration[] = [];
let series: Series[] = [];
let accounts: Account[] = [];
let parties: Party[] = [];
let items: Item[] = [];
const audit: AuditEvent[] = [];

const now = () => new Date().toISOString();
const id = () => crypto.randomUUID();

function fy(offset = 0): FinancialYearInfo {
  const d = new Date();
  const start = (d.getMonth() >= 3 ? d.getFullYear() : d.getFullYear() - 1) + offset;
  const code = `${start}-${String((start + 1) % 100).padStart(2, "0")}`;
  return { code, label: `FY ${code}`, startDate: `${start}-04-01`, endDate: `${start + 1}-03-31` };
}

function shortFy(): string {
  const c = fy().code;
  return `${c.slice(2, 4)}-${c.slice(5)}`;
}

function profile(): BusinessProfile {
  return {
    organisation: org,
    registrations: regs,
    currentFinancialYear: fy(),
    nextFinancialYear: fy(1),
    accountCount: accounts.length,
    hasPostings: false,
  };
}

function fail(fields: [string, string][]): never {
  throw { code: "validation_failed", message: "Please correct the highlighted fields", fields: fields.map(([field, message]) => ({ field, message })) } satisfies Fail;
}

function log(action: string, entityId: string | null, after?: unknown, before?: unknown) {
  audit.unshift({
    id: id(),
    occurredAt: now(),
    actor: "owner",
    action,
    entityType: action.split(".")[0],
    entityId,
    reason: null,
    beforeJson: before ? JSON.stringify(before) : null,
    afterJson: after ? JSON.stringify(after) : null,
  });
}

function checkOrg(o: OrganisationInput, prefix: string): [string, string][] {
  const e: [string, string][] = [];
  if (!o.legalName?.trim()) e.push([`${prefix}legalName`, "Enter the legal name of the business."]);
  if (!o.constitution) e.push([`${prefix}constitution`, "Choose the type of business."]);
  if (o.pincode && !/^[1-9][0-9]{5}$/.test(o.pincode)) e.push([`${prefix}pincode`, "PIN code must be 6 digits."]);
  return e;
}

function toOrg(o: OrganisationInput, base?: Organisation): Organisation {
  const n = (v?: string) => (v?.trim() ? v.trim() : null);
  return {
    id: base?.id ?? id(),
    legalName: o.legalName.trim(),
    tradeName: n(o.tradeName),
    pan: n(o.pan)?.toUpperCase() ?? null,
    constitution: o.constitution,
    addressLine1: n(o.addressLine1),
    addressLine2: n(o.addressLine2),
    city: n(o.city),
    pincode: n(o.pincode),
    stateCode: o.stateCode,
    email: n(o.email),
    phone: n(o.phone),
    fyStartMonth: o.fyStartMonth,
    booksStartDate: o.booksStartDate,
    roundToRupee: o.roundToRupee,
    rowVersion: (base?.rowVersion ?? 0) + 1,
    createdAt: base?.createdAt ?? now(),
    updatedAt: now(),
  };
}

function addSeries(regId: string | null, gstin: string | null, docs: [string, string][]) {
  for (const [documentType, base] of docs) {
    const prefix = `${base}/${shortFy()}/`;
    const s: Series = {
      id: id(), organisationId: org!.id, gstRegistrationId: regId, documentType, financialYear: fy().code,
      prefix, suffix: "", padding: 4, nextNumber: 1, isDefault: true, rowVersion: 1, createdAt: now(),
      updatedAt: now(), gstin, nextPreview: `${prefix}0001`, locked: false,
    };
    series.push(s);
    log("invoice_series.create", s.id, s);
  }
}

function newReg(r: RegistrationInput): Registration {
  const parts = gstinParts(r.gstin)!;
  return {
    id: id(), organisationId: org!.id, gstin: r.gstin.trim().toUpperCase(), stateCode: parts.stateCode,
    registrationType: r.registrationType, filingFrequency: r.filingFrequency as Registration["filingFrequency"],
    signatoryName: r.signatoryName || null, effectiveFrom: r.effectiveFrom, effectiveTo: null, status: "active",
    rowVersion: 1, createdAt: now(), updatedAt: null,
  };
}

const SEED: [string, string, Account["kind"], string | null][] = [
  ["1000", "Cash in hand", "asset", "cash"], ["1010", "Bank account", "asset", "bank"],
  ["1100", "Accounts receivable", "asset", "receivables"], ["1203", "Input IGST", "asset", "input_igst"],
  ["2000", "Accounts payable", "liability", "payables"], ["2103", "Output IGST", "liability", "output_igst"],
  ["3000", "Owner's capital", "equity", "capital"], ["4000", "Sales", "income", "sales"],
  ["5000", "Purchases", "expense", "purchases"], ["6900", "Round-off", "expense", "round_off"],
  ["1400", "Fixed assets", "asset", "fixed_assets"], ["5100", "Freight inward", "expense", null],
  ["6000", "Rent", "expense", null], ["6040", "Office expenses", "expense", null],
];

function preview(series: Series): Series {
  return { ...series, nextPreview: `${series.prefix}${String(series.nextNumber).padStart(series.padding, "0")}${series.suffix}`, locked: series.nextNumber > 1 };
}

// ---------------------------------------------------------------------------
// Masters (preview only — the Rust core has the full rules)
// ---------------------------------------------------------------------------

const toPaise = (v?: string) => {
  const t = v?.replace(/[₹,\s]/g, "");
  if (!t) return null;
  const [r, f = ""] = t.split(".");
  return Number(r) * 100 + Number((f + "00").slice(0, 2));
};
const toBp = (v?: string) => (v ? (toPaise(v.replace("%", "")) ?? 0) : 0);
const blank = (v?: string) => (v?.trim() ? v.trim() : null);
const HAS_GSTIN = ["regular", "composition", "sez"];

function checkParty(input: PartyInput, exceptId?: string): [string, string][] {
  const e: [string, string][] = [];
  if (!input.name?.trim()) e.push(["name", "Enter a name."]);
  if (parties.some((p) => p.id !== exceptId && p.name.toLowerCase() === input.name.trim().toLowerCase()))
    e.push(["name", "A party with this name already exists."]);
  if (HAS_GSTIN.includes(input.gstType)) {
    const g = input.gstin ? gstinError(input.gstin) : "A GSTIN is required for a registered party.";
    if (g) e.push(["gstin", g]);
  } else if (input.gstType !== "overseas" && !input.stateCode) e.push(["stateCode", "Choose a state."]);
  if (!input.shippingSame && !input.shippingLine1?.trim()) e.push(["shippingLine1", "Enter the shipping address."]);
  return e;
}

function toParty(input: PartyInput, base?: Party): Party {
  const parts = HAS_GSTIN.includes(input.gstType) && input.gstin ? gstinParts(input.gstin) : null;
  const opening = toPaise(input.openingBalance) ?? 0;
  const signed = input.openingBalanceSide === "payable" ? -opening : opening;
  return {
    id: base?.id ?? id(), organisationId: org!.id, kind: input.kind, name: input.name.trim(), legalName: blank(input.legalName),
    gstType: input.gstType, gstin: parts ? input.gstin!.toUpperCase() : null, pan: parts?.pan ?? blank(input.pan),
    stateCode: parts?.stateCode ?? (input.gstType === "overseas" ? 96 : (input.stateCode ?? 0)),
    addressLine1: blank(input.addressLine1), addressLine2: blank(input.addressLine2), city: blank(input.city), pincode: blank(input.pincode),
    shippingSame: input.shippingSame, shippingLine1: blank(input.shippingLine1), shippingLine2: blank(input.shippingLine2),
    shippingCity: blank(input.shippingCity), shippingPincode: blank(input.shippingPincode), shippingStateCode: input.shippingStateCode ?? null,
    contactPerson: blank(input.contactPerson), email: blank(input.email), phone: blank(input.phone),
    paymentTermsDays: input.paymentTermsDays ?? null, creditLimitPaise: toPaise(input.creditLimit),
    openingBalancePaise: signed, openingEntryId: signed ? id() : null, notes: blank(input.notes), isActive: input.isActive,
    rowVersion: (base?.rowVersion ?? 0) + 1, createdAt: base?.createdAt ?? now(), updatedAt: now(), balancePaise: signed,
  };
}

function checkItem(input: ItemInput, exceptId?: string): [string, string][] {
  const e: [string, string][] = [];
  if (!input.name?.trim()) e.push(["name", "Enter a name."]);
  if (items.some((i) => i.id !== exceptId && i.name.toLowerCase() === input.name.trim().toLowerCase()))
    e.push(["name", "An item with this name already exists."]);
  if (input.taxCategory !== "non_gst" && !input.hsnSac?.trim())
    e.push(["hsnSac", input.kind === "goods" ? "Enter the HSN code." : "Enter the SAC code."]);
  else if (input.kind === "service" && input.hsnSac && !/^99\d{4}$/.test(input.hsnSac))
    e.push(["hsnSac", "A SAC code has 6 digits and starts with 99."]);
  else if (input.kind === "goods" && input.hsnSac && !/^\d{4}(\d{2}){0,2}$/.test(input.hsnSac))
    e.push(["hsnSac", "An HSN code has 4, 6 or 8 digits."]);
  if (input.taxCategory === "taxable" && !toBp(input.gstRate)) e.push(["gstRate", "Choose a GST rate."]);
  return e;
}

function toItem(input: ItemInput, base?: Item): Item {
  return {
    id: base?.id ?? id(), organisationId: org!.id, kind: input.kind, name: input.name.trim(), code: blank(input.code),
    description: blank(input.description), hsnSac: blank(input.hsnSac), unit: input.unit || (input.kind === "service" ? "OTH" : "NOS"),
    taxCategory: input.taxCategory, gstRateBp: input.taxCategory === "taxable" ? toBp(input.gstRate) : 0, cessRateBp: toBp(input.cessRate),
    salesPricePaise: toPaise(input.salesPrice), purchasePricePaise: toPaise(input.purchasePrice), priceIncludesTax: input.priceIncludesTax,
    trackStock: input.trackStock, isActive: input.isActive, rowVersion: (base?.rowVersion ?? 0) + 1, createdAt: base?.createdAt ?? now(), updatedAt: now(),
  };
}

const field = (key: string, label: string, required = false): ImportField => ({ key, label, required, hint: "" });

const IMPORT_FIELDS: Record<ImportKind, ImportField[]> = {
  parties: [
    field("name", "Name", true), field("kind", "Type"), field("gstType", "GST status"), field("gstin", "GSTIN"),
    field("stateCode", "State"), field("addressLine1", "Address line 1"), field("city", "City"), field("pincode", "PIN code"),
    field("email", "E-mail"), field("phone", "Phone"), field("openingBalance", "Opening balance"),
  ],
  items: [
    field("name", "Name", true), field("kind", "Type"), field("code", "Item code"), field("hsnSac", "HSN/SAC"),
    field("unit", "Unit (UQC)"), field("taxCategory", "Tax category"), field("gstRate", "GST rate %"), field("salesPrice", "Sales price"),
  ],
};

const norm = (h: string) => h.toLowerCase().replace(/[^a-z0-9]/g, "");

function parseCsv(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"' && text[i + 1] === '"') {
        cell += '"';
        i++;
      } else if (c === '"') quoted = false;
      else cell += c;
    } else if (c === '"') quoted = true;
    else if (c === ",") {
      row.push(cell);
      cell = "";
    } else if (c === "\n" || c === "\r") {
      if (c === "\r" && text[i + 1] === "\n") i++;
      row.push(cell);
      rows.push(row);
      row = [];
      cell = "";
    } else cell += c;
  }
  if (cell || row.length) {
    row.push(cell);
    rows.push(row);
  }
  return rows.filter((r) => r.some((c) => c.trim()));
}

function rowInput(kind: ImportKind, cells: string[], mapping: ImportMapping): PartyInput | ItemInput {
  const get = (k: string) => (mapping[k] != null ? cells[mapping[k]]?.trim() || undefined : undefined);
  if (kind === "parties") {
    const gstin = get("gstin");
    const state = get("stateCode");
    const stateCode = state ? Number(state) || STATES.find((s) => norm(s.name) === norm(state))?.code : undefined;
    const ob = get("openingBalance");
    const kind = (get("kind")?.toLowerCase() as PartyInput["kind"]) ?? "customer";
    return {
      kind,
      name: get("name") ?? "",
      gstType: (get("gstType")?.toLowerCase() as PartyInput["gstType"]) ?? (gstin ? "regular" : kind === "customer" ? "consumer" : "unregistered"),
      gstin, stateCode, addressLine1: get("addressLine1"), city: get("city"), pincode: get("pincode"),
      email: get("email"), phone: get("phone"),
      openingBalance: ob?.replace(/\s*(dr|cr)$/i, "").replace(/^-/, ""),
      openingBalanceSide: ob && /cr$|^-/i.test(ob) ? "payable" : "receivable",
      shippingSame: true, isActive: true,
    };
  }
  const hsn = get("hsnSac");
  return {
    kind: (get("kind")?.toLowerCase() as ItemInput["kind"]) ?? (hsn?.startsWith("99") ? "service" : "goods"),
    name: get("name") ?? "", code: get("code"), hsnSac: hsn, unit: get("unit")?.toUpperCase(),
    taxCategory: (get("taxCategory")?.toLowerCase() as ItemInput["taxCategory"]) ?? "taxable",
    gstRate: get("gstRate"), salesPrice: get("salesPrice"), priceIncludesTax: false, trackStock: false, isActive: true,
  };
}

function evaluateImport(kind: ImportKind, table: ImportTable, mapping: ImportMapping) {
  if (mapping.name == null) fail([["mapping.name", "Choose the column for Name."]]);
  const seen = new Set<string>();
  const labels: Record<string, string> = Object.fromEntries(IMPORT_FIELDS[kind].map((f) => [f.key, f.label]));
  return table.rows.map((cells, i) => {
    const input = rowInput(kind, cells, mapping);
    const errs = kind === "parties" ? checkParty(input as PartyInput) : checkItem(input as ItemInput);
    if (input.name && seen.has(input.name.toLowerCase())) errs.push(["name", "Appears more than once in this file."]);
    seen.add(input.name.toLowerCase());
    const issue = errs.length
      ? { row: i + 2, label: input.name || "(no name)", errors: errs.map(([f, m]) => ({ field: f, message: `${labels[f] ?? f}: ${m}` })) }
      : null;
    return { input, issue };
  });
}

function masters(command: string, args: Record<string, any>): unknown {
  switch (command) {
    case "list_parties":
      return parties;
    case "create_party": {
      const e = checkParty(args.input);
      if (e.length) fail(e);
      const p = toParty(args.input);
      parties = [...parties, p].sort((a, b) => a.name.localeCompare(b.name));
      log("party.create", p.id, p);
      return p;
    }
    case "update_party": {
      const cur = parties.find((p) => p.id === args.id)!;
      if (args.expectedVersion !== cur.rowVersion)
        throw { code: "conflict", message: "This record was changed elsewhere. Reload it and try again.", fields: [] } satisfies Fail;
      const e = checkParty(args.input, cur.id);
      if (e.length) fail(e);
      const p = toParty(args.input, cur);
      parties = parties.map((x) => (x.id === p.id ? p : x));
      log("party.update", p.id, p, cur);
      return p;
    }
    case "item_options":
      return {
        units: [["NOS", "Numbers"], ["PCS", "Pieces"], ["KGS", "Kilograms"], ["LTR", "Litres"], ["MTR", "Metres"], ["BOX", "Box"], ["SET", "Sets"], ["OTH", "Others"]].map(
          ([code, label]) => ({ code, label }),
        ),
        gstRates: [0, 10, 25, 100, 150, 300, 500, 600, 750, 1200, 1800, 2800, 4000].map((basisPoints) => ({
          basisPoints,
          legacy: [600, 1200, 2800].includes(basisPoints),
        })),
      };
    case "list_items":
      return items;
    case "create_item": {
      const e = checkItem(args.input);
      if (e.length) fail(e);
      const i = toItem(args.input);
      items = [...items, i].sort((a, b) => a.name.localeCompare(b.name));
      log("item.create", i.id, i);
      return i;
    }
    case "update_item": {
      const cur = items.find((i) => i.id === args.id)!;
      const e = checkItem(args.input, cur.id);
      if (e.length) fail(e);
      const i = toItem(args.input, cur);
      items = items.map((x) => (x.id === i.id ? i : x));
      log("item.update", i.id, i, cur);
      return i;
    }
    case "import_template": {
      const fields = IMPORT_FIELDS[args.kind as ImportKind];
      return { fields, fileName: `accountsvu-${args.kind}-template.csv`, csv: fields.map((f) => f.label).join(",") + "\n" };
    }
    case "parse_import": {
      const kind = args.kind as ImportKind;
      if (!/\.(csv|txt)$/i.test(args.fileName))
        throw { code: "invalid_input", message: "The browser preview reads CSV only; Excel files are read by the desktop app.", fields: [] } satisfies Fail;
      const grid = parseCsv(new TextDecoder().decode(new Uint8Array(args.bytes)).replace(/^﻿/, ""));
      if (!grid.length) throw { code: "invalid_input", message: "The file has no rows.", fields: [] } satisfies Fail;
      const [headers, ...rows] = grid;
      const mapping: ImportMapping = {};
      for (const f of IMPORT_FIELDS[kind]) {
        const col = headers.findIndex((h) => [norm(f.key), norm(f.label)].includes(norm(h)));
        if (col >= 0) mapping[f.key] = col;
      }
      return { table: { fileName: args.fileName, headers, rows: rows.map((r) => headers.map((_, i) => r[i] ?? "")) }, fields: IMPORT_FIELDS[kind], mapping };
    }
    case "preview_import": {
      const results = evaluateImport(args.kind, args.table, args.mapping);
      const issues = results.flatMap((r) => (r.issue ? [r.issue] : []));
      return { total: results.length, valid: results.length - issues.length, issues };
    }
    case "commit_import": {
      const results = evaluateImport(args.kind, args.table, args.mapping);
      for (const r of results.filter((x) => !x.issue)) {
        if (args.kind === "parties") {
          const p = toParty(r.input as PartyInput);
          parties.push(p);
          log("party.create", p.id, p);
        } else {
          const i = toItem(r.input as ItemInput);
          items.push(i);
          log("item.create", i.id, i);
        }
      }
      parties.sort((a, b) => a.name.localeCompare(b.name));
      items.sort((a, b) => a.name.localeCompare(b.name));
      const skipped = results.flatMap((r) => (r.issue ? [r.issue] : []));
      const created = results.length - skipped.length;
      log("import.run", null, { fileName: args.table.fileName, kind: args.kind, created, skipped: skipped.length });
      return { created, skipped };
    }
    default:
      return undefined;
  }
}

// ---------------------------------------------------------------------------
// Sales (preview only — the Rust GST engine is the real calculator)
// ---------------------------------------------------------------------------

let salesDocs: (SalesDocument & { lines: SalesLine[] })[] = [];

function supplyFor(input: SalesDocumentInput) {
  const reg = regs.find((r) => r.id === input.gstRegistrationId) ?? regs.find((r) => r.status === "active");
  const supplier = reg?.stateCode ?? org?.stateCode ?? 0;
  const original = salesDocs.find((x) => x.id === input.originalDocumentId);
  const party = parties.find((p) => p.id === (original?.partyId ?? input.partyId));
  const pos = original?.placeOfSupply ?? input.placeOfSupply ?? party?.stateCode ?? null;
  const ut = [4, 26, 31, 35, 38, 97];
  const kind: SupplyKind | null = pos == null ? null : pos !== supplier ? "inter_state" : ut.includes(pos) ? "intra_union_territory" : "intra_state";
  return { reg, supplier, pos, kind, party, original };
}

function calcLine(l: SalesLineInput, kind: SupplyKind, taxFree: boolean, inclusive: boolean): LineCalc | null {
  const qty = Number(l.quantity);
  const price = toPaise(l.unitPrice);
  if (!(qty > 0) || price == null) return null;
  const amount = Math.round(qty * price) - (toPaise(l.discount) ?? 0);
  const bp = l.taxCategory === "taxable" && !taxFree ? toBp(l.gstRate) : 0;
  const taxable = inclusive && bp ? Math.round((amount * 10000) / (10000 + bp)) : amount;
  const c: LineCalc = { amountPaise: amount, taxablePaise: taxable, cgstPaise: 0, sgstPaise: 0, utgstPaise: 0, igstPaise: 0, cessPaise: 0, totalPaise: 0 };
  if (kind === "inter_state") c.igstPaise = Math.round((taxable * bp) / 10000);
  else {
    const half = Math.round((taxable * bp) / 20000);
    c.cgstPaise = half;
    if (kind === "intra_state") c.sgstPaise = half;
    else c.utgstPaise = half;
  }
  c.totalPaise = taxable + c.cgstPaise + c.sgstPaise + c.utgstPaise + c.igstPaise;
  return c;
}

function calculate(input: SalesDocumentInput): SalesCalculation {
  const s = supplyFor(input);
  const taxFree = input.docType === "bill_of_supply" || input.zeroRated;
  const lines = input.lines.map((l) => calcLine(l, s.kind ?? "intra_state", taxFree, input.pricesIncludeTax));
  const t: TotalsCalc = { taxablePaise: 0, cgstPaise: 0, sgstPaise: 0, utgstPaise: 0, igstPaise: 0, cessPaise: 0, roundOffPaise: 0, totalPaise: 0 };
  for (const c of lines) {
    if (!c) continue;
    t.taxablePaise += c.taxablePaise;
    t.cgstPaise += c.cgstPaise;
    t.sgstPaise += c.sgstPaise;
    t.utgstPaise += c.utgstPaise;
    t.igstPaise += c.igstPaise;
  }
  const exact = t.taxablePaise + t.cgstPaise + t.sgstPaise + t.utgstPaise + t.igstPaise;
  t.totalPaise = org?.roundToRupee ? Math.round(exact / 100) * 100 : exact;
  t.roundOffPaise = t.totalPaise - exact;
  const sr = series.find((x) => x.isDefault && x.documentType === input.docType && x.gstRegistrationId === (s.reg?.id ?? null));
  return { supplyKind: s.kind, placeOfSupply: s.pos, supplierState: s.supplier, lines, totals: t, numberPreview: s.party && sr ? sr.nextPreview : null };
}

function checkSales(input: SalesDocumentInput): [string, string][] {
  const e: [string, string][] = [];
  const s = supplyFor(input);
  if (!s.party) e.push(["partyId", "Choose a customer."]);
  if (input.docType === "credit_note" && !input.reason?.trim()) e.push(["reason", "Say why the credit note is issued, e.g. goods returned."]);
  if (!input.lines.length) e.push(["lines", "Add at least one line."]);
  input.lines.forEach((l, i) => {
    if (!l.description.trim() && !l.itemId) e.push([`lines.${i}.description`, "Describe the line or pick an item."]);
    if (!(Number(l.quantity) > 0)) e.push([`lines.${i}.quantity`, "Enter a quantity above zero."]);
    if (toPaise(l.unitPrice) == null) e.push([`lines.${i}.unitPrice`, "Enter a price."]);
    if (input.docType === "bill_of_supply" && l.taxCategory === "taxable") e.push([`lines.${i}.gstRate`, "A bill of supply can't carry taxable items. Use a tax invoice."]);
  });
  if (input.docDate > new Date().toISOString().slice(0, 10)) e.push(["docDate", "Can't be a future date."]);
  return e;
}

function display(d: SalesDocument): SalesDisplayStatus {
  if (d.status !== "issued") return d.status;
  if (d.docType !== "credit_note" && d.dueDate && d.dueDate < new Date().toISOString().slice(0, 10)) return "overdue";
  return "issued";
}

function salesView(id: string): SalesDocumentView {
  const d = salesDocs.find((x) => x.id === id);
  if (!d) throw { code: "not_found", message: "Document not found", fields: [] } satisfies Fail;
  const ref = (x: SalesDocument) => ({ id: x.id, docType: x.docType, number: x.number, docDate: x.docDate, status: x.status, totalPaise: x.totalPaise });
  const notes = salesDocs.filter((x) => x.originalDocumentId === d.id);
  const o = salesDocs.find((x) => x.id === d.originalDocumentId);
  return {
    ...d,
    displayStatus: display(d),
    original: o ? ref(o) : null,
    creditNotes: notes.map(ref),
    creditedPaise: notes.filter((x) => x.status === "issued").reduce((a, x) => a + x.totalPaise, 0),
  };
}

function writeSales(id: string | null, input: SalesDocumentInput): SalesDocument & { lines: SalesLine[] } {
  const e = checkSales(input);
  if (e.length) fail(e);
  const s = supplyFor(input);
  const c = calculate(input);
  const existing = salesDocs.find((x) => x.id === id);
  if (existing && existing.status !== "draft") throw { code: "conflict", message: "This document has been issued and can't be edited.", fields: [] } satisfies Fail;
  const docId = existing?.id ?? crypto.randomUUID();
  const item = (iid?: string) => items.find((i) => i.id === iid);
  const doc = {
    id: docId, organisationId: org!.id, docType: input.docType, status: "draft" as const, gstRegistrationId: s.reg?.id ?? null, seriesId: null,
    financialYear: null, number: null, docDate: input.docDate, dueDate: input.dueDate ?? input.docDate, partyId: s.party!.id, partyName: s.party!.name,
    partyGstin: s.party!.gstin, partyGstType: s.party!.gstType, partyAddress: s.party!.addressLine1, placeOfSupply: s.pos ?? 0,
    supplierState: s.supplier, supplyKind: s.kind ?? "intra_state", zeroRated: input.zeroRated, reverseCharge: false,
    pricesIncludeTax: input.pricesIncludeTax, originalDocumentId: input.originalDocumentId ?? null, reason: input.reason ?? null,
    notes: input.notes ?? null, ...c.totals, paidPaise: 0, journalEntryId: null, cancelEntryId: null, cancelReason: null,
    rowVersion: (existing?.rowVersion ?? 0) + 1, createdAt: existing?.createdAt ?? now(), updatedAt: now(), issuedAt: null, cancelledAt: null,
    lines: input.lines.map((l, i) => ({
      id: crypto.randomUUID(), documentId: docId, lineNo: i + 1, itemId: l.itemId ?? null,
      description: l.description || item(l.itemId)?.name || "", hsnSac: l.hsnSac ?? item(l.itemId)?.hsnSac ?? null, unit: l.unit ?? null,
      quantity: l.quantity, unitPricePaise: toPaise(l.unitPrice) ?? 0, discountPaise: toPaise(l.discount) ?? 0, taxCategory: l.taxCategory,
      gstRateBp: toBp(l.gstRate), cessRateBp: 0, ...c.lines[i]!,
    })),
  };
  salesDocs = [doc, ...salesDocs.filter((x) => x.id !== docId)];
  if (!existing) log("sales_document.create", docId, doc);
  return doc;
}

function salesCommand(command: string, args: Record<string, any>): unknown {
  switch (command) {
    case "list_sales":
      return salesDocs.map((d) => ({ ...d, displayStatus: display(d) }));
    case "get_sales_document":
      return salesView(args.id);
    case "calculate_sales":
      return calculate(args.input);
    case "save_sales_draft":
      return salesView(writeSales(args.id, args.input).id);
    case "issue_sales_document": {
      const d = writeSales(args.id, args.input);
      const sr = series.find((x) => x.isDefault && x.documentType === d.docType && x.gstRegistrationId === d.gstRegistrationId);
      if (!sr) fail([["docDate", "There's no number series for this document. Add one in Settings → Numbering."]]);
      const number = sr!.nextPreview;
      series = series.map((x) => (x.id === sr!.id ? preview({ ...x, nextNumber: x.nextNumber + 1 }) : x));
      Object.assign(d, { status: "issued", number, seriesId: sr!.id, financialYear: sr!.financialYear, journalEntryId: crypto.randomUUID(), issuedAt: now() });
      const sign = d.docType === "credit_note" ? -1 : 1;
      parties = parties.map((p) => (p.id === d.partyId ? { ...p, balancePaise: p.balancePaise + sign * d.totalPaise } : p));
      log("sales_document.issue", d.id, d);
      return salesView(d.id);
    }
    case "cancel_sales_document": {
      const d = salesDocs.find((x) => x.id === args.id)!;
      if (!String(args.reason).trim()) fail([["reason", "Say why it's being cancelled."]]);
      if (salesDocs.some((x) => x.originalDocumentId === d.id && x.status !== "cancelled"))
        throw { code: "conflict", message: "This invoice has credit notes. Cancel or delete them first.", fields: [] } satisfies Fail;
      Object.assign(d, { status: "cancelled", cancelReason: args.reason, cancelEntryId: crypto.randomUUID(), cancelledAt: now(), rowVersion: d.rowVersion + 1 });
      const sign = d.docType === "credit_note" ? 1 : -1;
      parties = parties.map((p) => (p.id === d.partyId ? { ...p, balancePaise: p.balancePaise + sign * d.totalPaise } : p));
      log("sales_document.cancel", d.id, d);
      return salesView(d.id);
    }
    case "delete_sales_draft": {
      const d = salesDocs.find((x) => x.id === args.id);
      salesDocs = salesDocs.filter((x) => x.id !== args.id);
      log("sales_document.delete", args.id, null, d);
      return null;
    }
    default:
      return undefined;
  }
}

// ---------------------------------------------------------------------------
// Purchases (preview only)
// ---------------------------------------------------------------------------

let purchaseDocs: (PurchaseDocument & { lines: PurchaseLine[] })[] = [];
const attachmentsStore: (Attachment & { bytes: number[] })[] = [];
const BOOKABLE = (a: Account) => (a.kind === "expense" && a.systemRole !== "round_off") || (a.kind === "asset" && (a.systemRole === null || ["inventory", "fixed_assets"].includes(a.systemRole)));

function purchaseSupply(input: PurchaseDocumentInput) {
  const original = purchaseDocs.find((x) => x.id === input.originalDocumentId);
  const reg = regs.find((r) => r.status === "active");
  const ours = reg?.stateCode ?? org?.stateCode ?? 0;
  const party = parties.find((p) => p.id === (original?.partyId ?? input.partyId));
  const theirs = party?.stateCode ?? ours;
  const ut = [4, 26, 31, 35, 38, 97];
  const kind: SupplyKind = theirs !== ours ? "inter_state" : ut.includes(ours) ? "intra_union_territory" : "intra_state";
  return { reg, ours, theirs, kind, party, original };
}

function calculatePurchase(input: PurchaseDocumentInput): PurchaseCalculation {
  const s = purchaseSupply(input);
  const lines = input.lines.map((l) => calcLine({ ...l, description: l.description }, s.kind, false, input.pricesIncludeTax));
  const t: TotalsCalc = { taxablePaise: 0, cgstPaise: 0, sgstPaise: 0, utgstPaise: 0, igstPaise: 0, cessPaise: 0, roundOffPaise: 0, totalPaise: 0 };
  for (const c of lines) {
    if (!c) continue;
    t.taxablePaise += c.taxablePaise;
    t.cgstPaise += c.cgstPaise;
    t.sgstPaise += c.sgstPaise;
    t.utgstPaise += c.utgstPaise;
    t.igstPaise += c.igstPaise;
  }
  const tax = t.cgstPaise + t.sgstPaise + t.utgstPaise + t.igstPaise;
  const base = t.taxablePaise + (input.reverseCharge ? 0 : tax);
  t.totalPaise = org?.roundToRupee ? Math.round(base / 100) * 100 : base;
  t.roundOffPaise = t.totalPaise - base;
  const sr = series.find((x) => x.isDefault && x.documentType === "debit_note");
  return { supplyKind: s.kind, lines, totals: t, reverseChargeTaxPaise: input.reverseCharge ? tax : 0, numberPreview: input.docType === "debit_note" ? (sr?.nextPreview ?? null) : null };
}

function checkPurchase(input: PurchaseDocumentInput): [string, string][] {
  const e: [string, string][] = [];
  const s = purchaseSupply(input);
  const credit = (input.paymentMode ?? "credit") === "credit";
  if (!s.party && (input.docType === "bill" || credit)) e.push(["partyId", "Choose the supplier."]);
  if (input.docType === "bill" && !input.supplierInvoiceNumber?.trim()) e.push(["supplierInvoiceNumber", "Enter the supplier's invoice number."]);
  if (input.docType === "bill" && !input.supplierInvoiceDate) e.push(["supplierInvoiceDate", "Enter the supplier's invoice date."]);
  if (input.docType === "debit_note" && !input.reason?.trim()) e.push(["reason", "Say why, e.g. goods returned to the supplier."]);
  if (input.docType === "bill" && s.party && input.supplierInvoiceNumber &&
    purchaseDocs.some((x) => x.status === "recorded" && x.partyId === s.party!.id && x.supplierInvoiceNumber === input.supplierInvoiceNumber!.toUpperCase()))
    e.push(["supplierInvoiceNumber", `This invoice from ${s.party.name} is already recorded.`]);
  if (!input.lines.length) e.push(["lines", "Add at least one line."]);
  input.lines.forEach((l, i) => {
    if (!l.description.trim() && !l.itemId) e.push([`lines.${i}.description`, "Describe the line or pick an item."]);
    if (!accounts.some((a) => a.id === l.accountId && BOOKABLE(a))) e.push([`lines.${i}.accountId`, "Choose what this is for."]);
    if (!(Number(l.quantity) > 0)) e.push([`lines.${i}.quantity`, "Enter a quantity above zero."]);
    if (toPaise(l.unitPrice) == null) e.push([`lines.${i}.unitPrice`, "Enter a price."]);
    const taxed = l.taxCategory === "taxable";
    if (taxed && ["eligible", "partial"].includes(l.itcStatus ?? "") && !s.party?.gstin && !input.reverseCharge)
      e.push([`lines.${i}.itcStatus`, "Input tax can only be claimed on a registered supplier's tax invoice."]);
    if (taxed && ["partial", "ineligible"].includes(l.itcStatus ?? "") && !l.itcReason?.trim())
      e.push([`lines.${i}.itcReason`, "Say why, e.g. blocked credit under section 17(5)."]);
  });
  return e;
}

function purchaseStatus(d: PurchaseDocument): PurchaseStatus {
  if (d.status === "draft") return "draft";
  if (d.status === "cancelled") return "cancelled";
  if (d.docType === "debit_note") return "recorded";
  if (d.paidPaise >= d.totalPaise) return "paid";
  if (d.dueDate && d.dueDate < new Date().toISOString().slice(0, 10)) return "overdue";
  return "unpaid";
}

function purchaseView(id: string): PurchaseDocumentView {
  const d = purchaseDocs.find((x) => x.id === id);
  if (!d) throw { code: "not_found", message: "Purchase not found", fields: [] } satisfies Fail;
  const ref = (x: PurchaseDocument) => ({ id: x.id, docType: x.docType, number: x.number, supplierInvoiceNumber: x.supplierInvoiceNumber, docDate: x.docDate, status: x.status, totalPaise: x.totalPaise });
  const notes = purchaseDocs.filter((x) => x.originalDocumentId === d.id);
  const o = purchaseDocs.find((x) => x.id === d.originalDocumentId);
  return {
    ...d,
    displayStatus: purchaseStatus(d),
    original: o ? ref(o) : null,
    debitNotes: notes.map(ref),
    debitedPaise: notes.filter((x) => x.status === "recorded").reduce((a, x) => a + x.totalPaise, 0),
    attachments: attachmentsStore.filter((a) => a.entityId === d.id).map(({ bytes: _b, ...a }) => a),
  };
}

function writePurchase(id: string | null, input: PurchaseDocumentInput) {
  const e = checkPurchase(input);
  if (e.length) fail(e);
  const s = purchaseSupply(input);
  const c = calculatePurchase(input);
  const existing = purchaseDocs.find((x) => x.id === id);
  if (existing && existing.status !== "draft") throw { code: "conflict", message: "This purchase is recorded and can't be edited.", fields: [] } satisfies Fail;
  const docId = existing?.id ?? crypto.randomUUID();
  const mode = input.docType === "debit_note" ? "credit" : (input.paymentMode ?? "credit");
  const doc = {
    id: docId, organisationId: org!.id, docType: input.docType, status: "draft" as const, gstRegistrationId: s.reg?.id ?? null, seriesId: null, number: null as string | null,
    financialYear: null, supplierInvoiceNumber: input.supplierInvoiceNumber?.toUpperCase() ?? null, supplierInvoiceDate: input.supplierInvoiceDate ?? null,
    docDate: input.docDate,
    dueDate: mode === "credit"
      ? (input.dueDate ?? (() => { const b = new Date(input.supplierInvoiceDate ?? input.docDate); b.setDate(b.getDate() + (s.party?.paymentTermsDays ?? 0)); return b.toISOString().slice(0, 10); })())
      : null,
    partyId: s.party?.id ?? null, partyName: s.party?.name ?? null, partyGstin: s.party?.gstin ?? null, partyGstType: s.party?.gstType ?? null,
    placeOfSupply: s.ours, supplierState: s.theirs, supplyKind: s.kind, reverseCharge: input.reverseCharge, pricesIncludeTax: input.pricesIncludeTax,
    paymentMode: mode as PaymentMode, originalDocumentId: input.originalDocumentId ?? null, reason: input.reason ?? null, notes: input.notes ?? null,
    ...c.totals, paidPaise: 0, journalEntryId: null, cancelEntryId: null, cancelReason: null, rowVersion: (existing?.rowVersion ?? 0) + 1,
    createdAt: existing?.createdAt ?? now(), updatedAt: now(), recordedAt: null, cancelledAt: null,
    lines: input.lines.map((l, i) => {
      const lc = c.lines[i]!;
      const tax = lc.cgstPaise + lc.sgstPaise + lc.utgstPaise + lc.igstPaise;
      const itc = (tax ? (l.itcStatus ?? "pending") : "na") as ItcStatus;
      return {
        id: crypto.randomUUID(), documentId: docId, lineNo: i + 1, itemId: l.itemId ?? null, accountId: l.accountId, description: l.description || items.find((x) => x.id === l.itemId)?.name || "",
        hsnSac: l.hsnSac ?? null, unit: l.unit ?? null, quantity: l.quantity, unitPricePaise: toPaise(l.unitPrice) ?? 0, discountPaise: toPaise(l.discount) ?? 0,
        taxCategory: l.taxCategory, gstRateBp: toBp(l.gstRate), cessRateBp: 0, ...lc, itcStatus: itc, itcReason: l.itcReason ?? null,
        itcEligiblePaise: itc === "eligible" ? tax : itc === "partial" ? (toPaise(l.itcEligible) ?? 0) : 0, itcReviewedAt: null, itcReviewedBy: null,
      };
    }),
  };
  purchaseDocs = [doc, ...purchaseDocs.filter((x) => x.id !== docId)];
  if (!existing) log("purchase_document.create", docId, doc);
  return doc;
}

async function purchaseCommand(command: string, args: Record<string, any>): Promise<unknown> {
  switch (command) {
    case "purchase_accounts":
      return accounts.filter(BOOKABLE);
    case "list_purchases":
      return purchaseDocs.map((d) => ({ ...d, displayStatus: purchaseStatus(d), itcPendingLines: d.status === "recorded" ? d.lines.filter((l) => l.itcStatus === "pending").length : 0 }));
    case "get_purchase":
      return purchaseView(args.id);
    case "calculate_purchase":
      return calculatePurchase(args.input);
    case "save_purchase_draft":
      return purchaseView(writePurchase(args.id, args.input).id);
    case "record_purchase": {
      const d = writePurchase(args.id, args.input);
      if (d.docType === "debit_note") {
        const sr = series.find((x) => x.isDefault && x.documentType === "debit_note");
        if (!sr) fail([["docDate", "There's no debit note number series. Add one in Settings → Numbering."]]);
        d.number = sr!.nextPreview;
        series = series.map((x) => (x.id === sr!.id ? preview({ ...x, nextNumber: x.nextNumber + 1 }) : x));
      }
      Object.assign(d, { status: "recorded", journalEntryId: crypto.randomUUID(), recordedAt: now(), paidPaise: d.paymentMode === "credit" ? 0 : d.totalPaise });
      if (d.partyId && d.paymentMode === "credit") {
        const sign = d.docType === "debit_note" ? 1 : -1;
        parties = parties.map((p) => (p.id === d.partyId ? { ...p, balancePaise: p.balancePaise + sign * d.totalPaise } : p));
      }
      log("purchase_document.record", d.id, d);
      return purchaseView(d.id);
    }
    case "review_itc": {
      const d = purchaseDocs.find((x) => x.id === args.documentId)!;
      const l = d.lines.find((x) => x.id === args.lineId)!;
      const tax = l.cgstPaise + l.sgstPaise + l.utgstPaise + l.igstPaise;
      const st = args.input.status as ItcStatus;
      if (["partial", "ineligible"].includes(st) && !args.input.reason?.trim()) fail([["itcReason", "Say why, e.g. blocked credit under section 17(5)."]]);
      if (["eligible", "partial"].includes(st) && !d.partyGstin && !d.reverseCharge) fail([["itcStatus", "Input tax can only be claimed on a registered supplier's tax invoice."]]);
      const eligible = st === "eligible" ? tax : st === "partial" ? (toPaise(args.input.eligible) ?? 0) : 0;
      if (st === "partial" && !(eligible > 0 && eligible < tax)) fail([["itcEligible", "Enter an amount between ₹0 and the line's GST."]]);
      Object.assign(l, { itcStatus: st, itcReason: args.input.reason || null, itcEligiblePaise: eligible, itcReviewedAt: now(), itcReviewedBy: "owner" });
      d.rowVersion += 1;
      log("purchase_line.itc_review", l.id, l);
      return purchaseView(d.id);
    }
    case "cancel_purchase": {
      const d = purchaseDocs.find((x) => x.id === args.id)!;
      if (!String(args.reason).trim()) fail([["reason", "Say why it's being cancelled."]]);
      if (purchaseDocs.some((x) => x.originalDocumentId === d.id && x.status !== "cancelled"))
        throw { code: "conflict", message: "This bill has debit notes. Cancel or delete them first.", fields: [] } satisfies Fail;
      Object.assign(d, { status: "cancelled", cancelReason: args.reason, cancelEntryId: crypto.randomUUID(), cancelledAt: now(), rowVersion: d.rowVersion + 1 });
      if (d.partyId && d.paymentMode === "credit") {
        const sign = d.docType === "debit_note" ? -1 : 1;
        parties = parties.map((p) => (p.id === d.partyId ? { ...p, balancePaise: p.balancePaise + sign * d.totalPaise } : p));
      }
      log("purchase_document.cancel", d.id, d);
      return purchaseView(d.id);
    }
    case "delete_purchase_draft": {
      const d = purchaseDocs.find((x) => x.id === args.id);
      purchaseDocs = purchaseDocs.filter((x) => x.id !== args.id);
      log("purchase_document.delete", args.id, null, d);
      return null;
    }
    case "add_attachment": {
      const bytes = new Uint8Array(args.bytes);
      if (bytes.length > 5 * 1024 * 1024) throw { code: "invalid_input", message: "invalid input: attachments can be up to 5 MB", fields: [] } satisfies Fail;
      const ext = String(args.fileName).split(".").pop()?.toLowerCase() ?? "";
      const types: Record<string, string> = { pdf: "application/pdf", png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", webp: "image/webp" };
      if (!types[ext]) throw { code: "invalid_input", message: "invalid input: attach a PDF or image file", fields: [] } satisfies Fail;
      const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
      const sha256 = Array.from(digest, (b) => b.toString(16).padStart(2, "0")).join("");
      const a = { id: crypto.randomUUID(), entityType: "purchase_document", entityId: args.documentId, fileName: args.fileName, contentType: types[ext], sizeBytes: bytes.length, sha256, uploadedAt: now(), uploadedBy: "owner", bytes: args.bytes };
      attachmentsStore.push(a);
      log("attachment.create", a.id, { fileName: a.fileName, sha256 });
      return a;
    }
    case "attachment_content": {
      const a = attachmentsStore.find((x) => x.id === args.id)!;
      return { fileName: a.fileName, contentType: a.contentType, bytes: a.bytes };
    }
    case "remove_attachment": {
      const i = attachmentsStore.findIndex((x) => x.id === args.id);
      const d = purchaseDocs.find((x) => x.id === attachmentsStore[i]?.entityId);
      if (d && d.status !== "draft") throw { code: "conflict", message: "Attachments of a recorded purchase are kept for the audit trail.", fields: [] } satisfies Fail;
      attachmentsStore.splice(i, 1);
      return null;
    }
    default:
      return undefined;
  }
}

export async function previewBackend(command: string, args: Record<string, any> = {}): Promise<unknown> {
  await new Promise((r) => setTimeout(r, 120)); // feel like real IPC
  switch (command) {
    case "app_health":
      return { appVersion: "0.1.0", schemaVersion: 5, database: "(browser preview — in memory)", organisationConfigured: !!org };
    case "dashboard_summary":
      return {
        salesThisMonth: "0", purchasesThisMonth: "0", cashAndBank: "0", customerDues: "0", estimatedGstPayable: "0",
        registrations: regs.map((r) => ({ gstin: r.gstin, filingFrequency: r.filingFrequency })),
        organisationConfigured: !!org,
      };
    case "business_profile":
      return profile();
    case "setup_business": {
      const input = args.input as SetupBusinessInput;
      if (org) throw { code: "already_set_up", message: "The business is already set up", fields: [] } satisfies Fail;
      const e = checkOrg(input.organisation, "organisation.");
      if (input.registration) {
        const g = gstinError(input.registration.gstin);
        if (g) e.push(["registration.gstin", g]);
      }
      if (e.length) fail(e);
      org = toOrg(input.organisation);
      if (!org.pan && input.registration) org.pan = gstinParts(input.registration.gstin)!.pan;
      log("organisation.create", org.id, org);
      accounts = SEED.map(([code, name, kind, systemRole]) => ({ id: id(), code, name, kind, systemRole, isActive: true }));
      log("chart_of_accounts.seed", null, { accounts: accounts.length });
      const base = (input.invoicePrefix || "INV").toUpperCase();
      if (input.registration) {
        const r = newReg(input.registration);
        regs.push(r);
        log("gst_registration.create", r.id, r);
        addSeries(r.id, r.gstin, [["tax_invoice", base], ["bill_of_supply", "BOS"], ["credit_note", "CN"], ["debit_note", "DN"]]);
      } else {
        addSeries(null, null, [["bill_of_supply", base]]);
      }
      return profile();
    }
    case "update_organisation": {
      if (!org) throw { code: "not_set_up", message: "Set up your business first", fields: [] } satisfies Fail;
      if (args.expectedVersion !== org.rowVersion) throw { code: "conflict", message: "This record was changed elsewhere. Reload it and try again.", fields: [] } satisfies Fail;
      const e = checkOrg(args.input, "");
      if (e.length) fail(e);
      const before = org;
      org = toOrg(args.input, org);
      log("organisation.update", org.id, org, before);
      return profile();
    }
    case "add_gst_registration": {
      const input = args.input as RegistrationInput;
      const g = gstinError(input.gstin);
      if (g) fail([["gstin", g]]);
      if (regs.some((r) => r.gstin === input.gstin.toUpperCase())) fail([["gstin", "This GSTIN is already added."]]);
      if (org?.pan && gstinParts(input.gstin)!.pan !== org.pan) fail([["gstin", `This GSTIN belongs to a different PAN than ${org.pan}.`]]);
      const r = newReg(input);
      regs.push(r);
      log("gst_registration.create", r.id, r);
      addSeries(r.id, r.gstin, [["tax_invoice", "INV"], ["bill_of_supply", "BOS"], ["credit_note", "CN"], ["debit_note", "DN"]]);
      return profile();
    }
    case "update_gst_registration": {
      const input = args.input as UpdateRegistrationInput;
      regs = regs.map((r) =>
        r.id === args.id
          ? { ...r, ...input, filingFrequency: input.filingFrequency as Registration["filingFrequency"], status: input.status as Registration["status"], signatoryName: input.signatoryName || null, effectiveTo: input.effectiveTo || null, rowVersion: r.rowVersion + 1, updatedAt: now() }
          : r,
      );
      log("gst_registration.update", args.id);
      return profile();
    }
    case "list_accounts":
      return accounts;
    case "list_series":
      return series;
    case "create_series": {
      const input = args.input as SeriesInput;
      const prefix = input.prefix.trim().toUpperCase();
      if (series.some((s) => s.gstRegistrationId === (input.gstRegistrationId ?? null) && s.financialYear === input.financialYear && s.prefix === prefix && s.suffix === input.suffix.toUpperCase()))
        fail([["prefix", "Another series for this GSTIN and year already uses this prefix."]]);
      if (input.isDefault) series = series.map((s) => (s.documentType === input.documentType && s.gstRegistrationId === (input.gstRegistrationId ?? null) && s.financialYear === input.financialYear ? { ...s, isDefault: false } : s));
      const s = preview({
        id: id(), organisationId: org!.id, gstRegistrationId: input.gstRegistrationId ?? null, documentType: input.documentType,
        financialYear: input.financialYear, prefix, suffix: input.suffix.toUpperCase(), padding: input.padding, nextNumber: input.nextNumber,
        isDefault: input.isDefault, rowVersion: 1, createdAt: now(), updatedAt: now(),
        gstin: regs.find((r) => r.id === input.gstRegistrationId)?.gstin ?? null, nextPreview: "", locked: false,
      });
      series.push(s);
      log("invoice_series.create", s.id, s);
      return series;
    }
    case "update_series": {
      const input = args.input as SeriesUpdateInput;
      const cur = series.find((s) => s.id === args.id)!;
      if (input.nextNumber < cur.nextNumber) fail([["nextNumber", `The next number can't go below ${cur.nextNumber}.`]]);
      series = series.map((s) => (s.id === args.id ? preview({ ...s, ...input, prefix: input.prefix.toUpperCase(), rowVersion: s.rowVersion + 1 }) : s));
      log("invoice_series.update", args.id);
      return series;
    }
    case "list_audit_events":
      return audit.slice(0, args.limit ?? 100);
    default: {
      const r = masters(command, args) ?? salesCommand(command, args) ?? (await purchaseCommand(command, args));
      if (r !== undefined) return r;
      throw { code: "not_found", message: `Unknown command ${command}`, fields: [] } satisfies Fail;
    }
  }
}
