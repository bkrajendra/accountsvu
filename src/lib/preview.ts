/**
 * In-memory stand-in for the Rust core, used only when the UI runs in a plain
 * browser (`pnpm dev`). It mimics command shapes and a few key validations so
 * screens can be built and reviewed without the desktop shell. The real rules
 * live in crates/application and are the only ones that matter.
 */
import type {
  FilingFile,
  FilingInput,
  FilingView,
  FilingReturnType,
  Gstr1,
  Gstr2b,
  Gstr2bDecisionInput,
  Gstr2bImportResult,
  ReconAmounts,
  ReconBookSide,
  ReconDecision,
  ReconDifference,
  ReconPortalSide,
  ReconRow,
  ReconStatus,
  Gstr3b,
  Gstr3bRow,
  HeadPayment,
  Heads,
  OffsetProposal,
  ReturnCentre,
  ReturnPeriod,
  TaxTotals as ReturnTotals,
  Gstr1Info,
  Gstr1Section,
  TaxWarning,
  Account,
  BackupLog,
  LockStatus,
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
  Ageing,
  AgeingParty,
  Attachment,
  BalanceSheet,
  BookLine,
  BooksControl,
  BooksLock,
  FyClose,
  JournalRegister,
  Ledger,
  LogoContent,
  ProfitAndLoss,
  ReportLine,
  ReportSection,
  TrialBalance,
  TrialBalanceRow,
  YearStatus,
  MatchCandidate,
  MoneyAccount,
  MoneyAccountInput,
  MoneyAccountKind,
  MoneyBook,
  OpenDocument,
  OpeningBalanceInput,
  ParsedStatementLine,
  Reconciliation,
  StatementFile,
  StatementImportInput,
  StatementImportResult,
  StatementLine,
  StatementLineStatus,
  StatementMapping,
  StatementPreview,
  VoucherInput,
  VoucherType,
  VoucherView,
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
import { gstinError, gstinParts, stateName, STATES } from "./gst";

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
  ["6000", "Rent", "expense", null], ["6040", "Office expenses", "expense", null], ["6060", "Bank charges", "expense", "bank_charges"], ["3100", "Drawings", "equity", "drawings"],
  ["3200", "Retained earnings", "equity", "retained_earnings"], ["3900", "Opening balance adjustment", "equity", "opening_balance"],
  ["4900", "Other income", "income", "other_income"],
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

// HSN/SAC directory sample (Phase 2.5.1). The desktop app syncs the full GSTN
// master (about 22,600 codes); the preview "syncs" this extract.
const DIRECTORY_SAMPLE: [string, string][] = [
  ["02", "Meat and edible meat offal"],
  ["0201", "Meat of bovine animals, fresh and chilled"],
  ["020110", "Carcasses and half-carcasses"],
  ["02011000", "Carcasses and half-carcasses"],
  ["0202", "Meat of bovine animals, frozen"],
  ["020210", "Carcases and half-carcases"],
  ["02021000", "Carcases and half-carcases"],
  ["020230", "Boneless"],
  ["02023000", "Boneless"],
  ["0207", "Meat and edible offal, of the poultry of heading 0105, fresh, chilled or frozen"],
  ["020713", "Of fowls of the species Gallus domesticus: cuts and offal, fresh or chilled"],
  ["02071300", "Cuts and offal, fresh or chilled"],
  ["04", "Dairy produce; birds' eggs; natural honey; edible products of animal origin, not elsewhere specified"],
  ["0405", "Butter and other fats and oils derived from milk; dairy spreads"],
  ["040510", "Butter"],
  ["04051000", "Butter"],
  ["040590", "Other"],
  ["04059020", "Ghee"],
  ["04059090", "Other"],
  ["34", "Soap, organic surface-active agents, washing preparations, lubricating preparations"],
  ["3401", "Soap; organic surface-active products and preparations for use as soap, in the form of bars, cakes"],
  ["340111", "For toilet use (including medicated products)"],
  ["34011110", "Medicated toilet soap"],
  ["34011190", "Other"],
  ["84", "Nuclear reactors, boilers, machinery and mechanical appliances; parts thereof"],
  ["8415", "Air conditioning machines, comprising a motor-driven fan and elements for changing the temperature and humidity"],
  ["841510", "Of a kind designed to be fixed to a window, wall, ceiling or floor, self-contained or split-system"],
  ["84151010", "Window or wall types, self-contained"],
  ["84151090", "Other"],
  ["8418", "Refrigerators, freezers and other refrigerating or freezing equipment, electric or other"],
  ["841821", "Household type refrigerators: compression-type"],
  ["84182100", "Compression-type"],
  ["8471", "Automatic data processing machines and units thereof; magnetic or optical readers"],
  ["847130", "Portable automatic data processing machines, weighing not more than 10 kg"],
  ["84713010", "Personal computer"],
  ["84713090", "Other"],
  ["847160", "Input or output units, whether or not containing storage units in the same housing"],
  ["84716060", "Mouse"],
  ["84716040", "Printer"],
  ["85", "Electrical machinery and equipment and parts thereof; sound recorders and reproducers, television"],
  ["8517", "Telephone sets, including smartphones and other telephones for cellular networks or for other wireless networks"],
  ["851713", "Smartphones"],
  ["85171300", "Smartphones"],
  ["99", "All Services"],
  ["9954", "Construction services"],
  ["995411", "Construction services of single dwelling or multi dwelling or multi-storied residential buildings"],
  ["9983", "Other professional, technical and business services"],
  ["998311", "Management consulting and management services including financial, strategic, human resources, marketing"],
  ["998313", "Information technology (IT) consulting and support services"],
  ["998314", "Information technology (IT) design and development services"],
  ["9987", "Maintenance, repair and installation (except construction) services"],
  ["998713", "Maintenance and repair services of computers and peripheral equipment"],
  ["998719", "Maintenance and repair services of other goods n.e.c."],
];
type DirectoryState = { syncedAt: string; channel: "gstn" | "file"; origin: string; diff: Record<string, number> };
let directory: DirectoryState | null = null;
const GSTN_MASTER = "https://tutorial.gst.gov.in/downloads/HSN_SAC.xlsx";
const directoryMap = new Map(DIRECTORY_SAMPLE);

function directoryStatus() {
  const n = directory ? DIRECTORY_SAMPLE.length : 0;
  const sac = directory ? DIRECTORY_SAMPLE.filter(([c]) => c.startsWith("99")).length : 0;
  return {
    ready: !!directory, hsnCodes: n - sac, sacCodes: sac, delisted: 0,
    syncedAt: directory?.syncedAt ?? null, channel: directory?.channel ?? null, origin: directory?.origin ?? null,
    sha256: directory ? "preview" : null, lastDiff: directory?.diff ?? null, sourceUrl: GSTN_MASTER,
  };
}

function directorySearch(input: { query: string; kind?: string | null; limit?: number; date?: string | null }) {
  const q = String(input.query ?? "").trim().toLowerCase();
  const limit = Math.min(Math.max(input.limit ?? 50, 1), 200);
  const date = input.date ?? new Date().toISOString().slice(0, 10);
  const rateFor = (code: string) => {
    const rule = hsnRule(code);
    const r = rule && hsnRateOn(rule, date);
    return rule && r ? { ruleCode: rule.code, taxCategory: "taxable", rateBp: r.bp, conditional: false } : null;
  };
  const kindOk = (code: string) => !input.kind || (input.kind === "sac") === code.startsWith("99");
  if (!directory) {
    const hits = PREVIEW_HSN.filter((h) => (/^\d+$/.test(q) ? h.code.startsWith(q) || q.startsWith(h.code) : h.description.toLowerCase().includes(q)))
      .filter((h) => !input.kind || (input.kind === "sac") === (h.kind === "service"))
      .map((h) => ({ code: h.code, kind: h.kind, description: h.description, delisted: false, chapter: null, heading: null, subheading: null, rate: rateFor(h.code) }));
    return { hits: hits.slice(0, limit), more: hits.length > limit, source: "rule_pack" };
  }
  const digits = q.replace(/[\s.]/g, "");
  const parent = (code: string, len: number) =>
    code.length > len && directoryMap.has(code.slice(0, len)) ? { code: code.slice(0, len), description: directoryMap.get(code.slice(0, len))! } : null;
  const context = (code: string) => [2, 4, 6].map((l) => parent(code, l)?.description ?? "").join(" ").toLowerCase();
  const words = q.split(/[^a-z0-9]+/).filter(Boolean);
  const matches = DIRECTORY_SAMPLE.filter(([code, desc]) => {
    if (!kindOk(code)) return false;
    if (/^\d+$/.test(digits)) return code.startsWith(digits);
    const hay = `${desc} ${context(code)}`.toLowerCase().split(/[^a-z0-9]+/);
    return words.every((w) => hay.some((t) => t.startsWith(w)));
  });
  if (/^\d+$/.test(digits)) matches.sort((a, b) => a[0].localeCompare(b[0]));
  else matches.sort((a, b) => {
    const score = ([, d]: [string, string]) => -words.filter((w) => d.toLowerCase().includes(w)).length;
    return score(a) - score(b) || b[0].length - a[0].length || a[0].localeCompare(b[0]);
  });
  const hits = matches.slice(0, limit).map(([code, description]) => ({
    code, kind: code.startsWith("99") ? "service" : "goods", description, delisted: false,
    chapter: parent(code, 2), heading: parent(code, 4), subheading: parent(code, 6), rate: rateFor(code),
  }));
  return { hits, more: matches.length > limit, source: "directory" };
}

async function directorySync(channel: "gstn" | "file", origin: string, onProgress?: (p: unknown) => void) {
  if (channel === "gstn") {
    const total = 668_474;
    for (let got = 0; got < total; got = Math.min(total, got + 120_000)) {
      onProgress?.({ stage: "downloading", received: got, total });
      await new Promise((r) => setTimeout(r, 140));
    }
    onProgress?.({ stage: "updating", received: total, total });
    await new Promise((r) => setTimeout(r, 400));
  }
  const unchanged = !!directory;
  const diff = { added: unchanged ? 0 : DIRECTORY_SAMPLE.length, changed: 0, delisted: 0, restored: 0, unchanged: unchanged ? DIRECTORY_SAMPLE.length : 0 };
  directory = { syncedAt: new Date().toISOString(), channel, origin, diff: unchanged ? directory!.diff : diff };
  if (!unchanged) log("hsn_directory.synced", null, { channel, origin, codes: DIRECTORY_SAMPLE.length, diff });
  return { unchanged, diff, skippedRows: 0, status: directoryStatus() };
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
          label: pct(basisPoints),
          from: RATE_DATES[basisPoints]?.from ?? null,
          to: RATE_DATES[basisPoints]?.to ?? null,
          note: basisPoints === 1200 || basisPoints === 2800 ? "Withdrawn from 22 Sep 2025" : null,
        })),
      };
    case "hsn_lookup": {
      const q = String(args.query ?? "").trim().toLowerCase();
      if (!q) return [];
      const date = args.date ?? new Date().toISOString().slice(0, 10);
      const words = q.split(/\s+/).filter((w) => w.length >= 3);
      const hit = (h: PreviewHsn) =>
        /^\d+$/.test(q)
          ? h.code.startsWith(q) || q.startsWith(h.code)
          : words.some((w) => h.description.toLowerCase().split(/[^a-z0-9]+/).some((s) => s.startsWith(w)));
      return PREVIEW_HSN.filter(hit)
        .map((h) => {
          const r = hsnRateOn(h, date);
          return r && { code: h.code, kind: h.kind, description: h.description, conditional: false, taxCategory: "taxable", rateBp: r.bp, since: r.from ?? null, source: r.from ? "Notification 9/2025-Central Tax (Rate)" : "Notification 1/2017-Central Tax (Rate)" };
        })
        .filter(Boolean);
    }
    case "hsn_directory_status":
      return directoryStatus();
    case "search_hsn_directory":
      return directorySearch(args.input ?? {});
    case "sync_hsn_directory":
      return directorySync("gstn", GSTN_MASTER, args.onProgress);
    case "import_hsn_master_file":
      return directorySync("file", "HSN_SAC.xlsx");
    case "rule_pack_info":
      return {
        version: PREVIEW_PACK, published: "2026-11-20", hsnEntries: 46,
        note: "Starter pack for small businesses (browser preview shows a sample).",
        sources: [
          { id: "G2017", title: "Notification 1/2017-Central Tax (Rate) and amendments", detail: "Goods rate schedules from 1 Jul 2017" },
          { id: "R2025", title: "Notification 9/2025-Central Tax (Rate)", detail: "Rate rationalisation from 22 Sep 2025" },
        ],
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

// Rule pack sample for the preview (the real pack lives in crates/gst-engine/rules).
const PREVIEW_PACK = "2026.11.1";
const RATE_DATES: Record<number, { from?: string; to?: string }> = {
  600: { to: "2025-09-21" }, 1200: { to: "2025-09-21" }, 2800: { to: "2025-09-21" }, 4000: { from: "2025-09-22" },
};
type PreviewHsn = { code: string; kind: "goods" | "service"; description: string; rates: { bp: number; from?: string; to?: string }[] };
const CHANGE = [{ to: "2025-09-21" }, { from: "2025-09-22" }];
const PREVIEW_HSN: PreviewHsn[] = [
  { code: "0405", kind: "goods", description: "Butter, ghee and other milk fats", rates: [{ bp: 1200, ...CHANGE[0] }, { bp: 500, ...CHANGE[1] }] },
  { code: "3401", kind: "goods", description: "Soap", rates: [{ bp: 1800, ...CHANGE[0] }, { bp: 500, ...CHANGE[1] }] },
  { code: "8415", kind: "goods", description: "Air conditioners", rates: [{ bp: 2800, ...CHANGE[0] }, { bp: 1800, ...CHANGE[1] }] },
  { code: "8418", kind: "goods", description: "Refrigerators and freezers", rates: [{ bp: 1800 }] },
  { code: "8471", kind: "goods", description: "Computers and peripherals", rates: [{ bp: 1800 }] },
  { code: "8517", kind: "goods", description: "Mobile phones and network equipment", rates: [{ bp: 1800 }] },
  { code: "9983", kind: "service", description: "Professional, technical and business services (incl. IT)", rates: [{ bp: 1800 }] },
  { code: "9987", kind: "service", description: "Maintenance, repair and installation", rates: [{ bp: 1800 }] },
];
const pct = (bp: number) => `${bp / 100}%`;
const fmtDate = (d: string) => new Date(d + "T00:00:00").toLocaleDateString("en-IN", { day: "numeric", month: "short", year: "numeric" });
const hsnRateOn = (h: PreviewHsn, date: string) => h.rates.find((r) => (!r.from || date >= r.from) && (!r.to || date <= r.to));
const hsnRule = (code?: string | null) =>
  code ? PREVIEW_HSN.filter((h) => code.startsWith(h.code)).sort((a, b) => b.code.length - a.code.length)[0] : undefined;

function previewWarnings(input: SalesDocumentInput, s: ReturnType<typeof supplyFor>): TaxWarning[] {
  const out: TaxWarning[] = [];
  const date = s.original?.docDate ?? input.docDate;
  if (!date) return out;
  const home = s.party?.gstin ? Number(s.party.gstin.slice(0, 2)) : null;
  if (home && s.pos && s.pos !== 96 && home !== s.pos)
    out.push({ code: "place_of_supply", field: "placeOfSupply", message: `Place of supply is state ${s.pos}, but the customer's GSTIN is registered in state ${home}. Check that this is right.` });
  input.lines.forEach((l, i) => {
    if (l.taxCategory !== "taxable") return;
    const bp = toBp(l.gstRate);
    const hsn = hsnRule(l.hsnSac ?? items.find((it) => it.id === l.itemId)?.hsnSac);
    const want = hsn && hsnRateOn(hsn, date);
    const d = RATE_DATES[bp];
    if (d && ((d.to && date > d.to) || (d.from && date < d.from))) {
      let message = `${pct(bp)} GST applies ${d.to ? `only up to ${fmtDate(d.to)}` : `only from ${fmtDate(d.from!)}`}.`;
      if (hsn && want) message += ` The rule pack has ${pct(want.bp)} for HSN/SAC ${hsn.code} (${hsn.description}).`;
      out.push({ code: "rate_not_in_force", field: `lines.${i}.gstRate`, message });
    } else if (hsn && want && want.bp !== bp) {
      out.push({ code: "rate_differs", field: `lines.${i}.gstRate`, message: `HSN/SAC ${hsn.code} (${hsn.description}) is ${pct(want.bp)}${want.from ? ` from ${fmtDate(want.from)}` : ""} in the rule pack; this line uses ${pct(bp)}. Check the rate.` });
    }
  });
  return out;
}

const GSTR1_LABELS: Record<Gstr1Section, string> = {
  b2b: "B2B (table 4A)", b2cl: "B2C large (table 5)", b2cs: "B2C small (table 7)",
  expWpay: "Export with payment of tax (table 6A)", expWopay: "Export without payment of tax (table 6A)",
  sezWpay: "SEZ with payment of tax (table 6B)", sezWopay: "SEZ without payment of tax (table 6B)",
  cdnr: "Credit/debit notes, registered (table 9B)", cdnur: "Credit/debit notes, unregistered (table 9B)",
};

function previewGstr1(d: { docType: string; partyGstType: string; placeOfSupply: number; supplyKind: string; zeroRated: boolean; totalPaise: number }, taxable: boolean, other: boolean): Gstr1Info {
  const overseas = d.partyGstType === "overseas" || d.placeOfSupply === 96;
  const registered = d.partyGstType === "regular" || d.partyGstType === "composition";
  const sez = d.partyGstType === "sez";
  let section: Gstr1Section | null = null;
  if (d.docType === "credit_note") section = registered || sez ? "cdnr" : "cdnur";
  else if (d.docType !== "bill_of_supply" && (taxable || overseas)) {
    if (overseas) section = d.zeroRated ? "expWopay" : "expWpay";
    else if (sez) section = d.zeroRated ? "sezWopay" : "sezWpay";
    else if (registered) section = "b2b";
    else section = d.supplyKind === "inter_state" && d.totalPaise > 10_000_000 ? "b2cl" : "b2cs";
  }
  return { section, label: section ? GSTR1_LABELS[section] : null, nilTable: !overseas && (other || (d.docType === "bill_of_supply" && taxable)) };
}

// ---------------------------------------------------------------------------
// GST returns (preview; the Rust workpaper is the real one)
// ---------------------------------------------------------------------------

const zeroTotals = (): ReturnTotals => ({ taxablePaise: 0, igstPaise: 0, cgstPaise: 0, sgstPaise: 0, cessPaise: 0, taxPaise: 0 });

type TotalsLike = { taxablePaise: number; igstPaise: number; cgstPaise: number; sgstPaise: number; utgstPaise?: number; cessPaise: number };

function addTotals(t: ReturnTotals, l: TotalsLike, sign = 1) {
  t.taxablePaise += sign * l.taxablePaise;
  t.igstPaise += sign * l.igstPaise;
  t.cgstPaise += sign * l.cgstPaise;
  t.sgstPaise += sign * (l.sgstPaise + (l.utgstPaise ?? 0));
  t.cessPaise += sign * l.cessPaise;
  t.taxPaise = t.igstPaise + t.cgstPaise + t.sgstPaise + t.cessPaise;
}

function isoLocal(d: Date) {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

function monthEnd(period: string) {
  const [y, m] = period.split("-").map(Number);
  return isoLocal(new Date(y, m, 0));
}

function gstr1Due(period: string) {
  const [y, m] = period.split("-").map(Number);
  return isoLocal(new Date(y, m, 11));
}

function periodStatus(period: string): ReturnPeriod["status"] {
  const today = isoLocal(new Date());
  return today <= monthEnd(period) ? "open" : today <= gstr1Due(period) ? "due" : "past_due";
}

function periodLabel(period: string) {
  const [y, m] = period.split("-").map(Number);
  return new Date(y, m - 1, 1).toLocaleDateString("en-IN", { month: "long", year: "numeric" });
}

const NIL_LABELS: Record<string, string> = {
  inter_registered: "Inter-state to registered persons",
  intra_registered: "Intra-state to registered persons",
  inter_unregistered: "Inter-state to unregistered persons",
  intra_unregistered: "Intra-state to unregistered persons",
};

function previewWorkpaper(regId: string, period: string): Gstr1 {
  const reg = regs.find((r) => r.id === regId);
  if (!reg) throw { code: "not_found", message: "That GST registration wasn't found.", fields: [] } satisfies Fail;
  const from = `${period}-01`;
  const to = monthEnd(period);
  const docs = salesDocs
    .filter((d) => (d.gstRegistrationId ?? regs[0]?.id) === reg.id && d.docDate >= from && d.docDate <= to)
    .sort((a, b) => (a.docDate + (a.number ?? "")).localeCompare(b.docDate + (b.number ?? "")));
  const exceptions: Gstr1["exceptions"] = [];
  const documents: Gstr1["documents"] = [];
  const b2cs = new Map<string, Gstr1["b2cs"][number]>();
  const nil = new Map<string, Gstr1["nil"][number]>();
  const hsn = new Map<string, Gstr1["hsn"][number]>();
  const series = new Map<string, { label: string; numbers: string[]; cancelled: number }>();
  const place = (d: SalesDocument) => (d.placeOfSupply === 96 ? "Outside India" : stateName(d.placeOfSupply));
  const word = (d: SalesDocument) => (d.docType === "credit_note" ? "Credit note" : d.docType === "bill_of_supply" ? "Bill of supply" : "Invoice");
  for (const d of docs) {
    if (d.status === "draft") {
      exceptions.push({ severity: "warning", code: "draft", message: `Draft for ${d.partyName} dated ${d.docDate} isn't issued, so it isn't in this return.`, documentId: d.id, number: null });
      continue;
    }
    const nature = d.docType === "credit_note" ? "Credit notes" : "Invoices for outward supply";
    const sKey = nature + (d.seriesId ?? "");
    const s = series.get(sKey) ?? { label: nature, numbers: [], cancelled: 0 };
    s.numbers.push(d.number ?? "");
    series.set(sKey, s);
    if (d.status === "cancelled") {
      s.cancelled += 1;
      continue;
    }
    const credit = d.docType === "credit_note";
    const sign = credit ? -1 : 1;
    const original = salesDocs.find((o) => o.id === d.originalDocumentId);
    const taxable = d.lines.some((l) => l.taxCategory === "taxable");
    const other = d.lines.some((l) => l.taxCategory !== "taxable");
    let section = previewGstr1(d, taxable, other).section;
    if (credit && section === "cdnur" && original && previewGstr1(original, true, false).section === "b2cs") section = "b2cs";
    const registered = ["regular", "composition", "sez"].includes(d.partyGstType);
    const inter = d.supplyKind === "inter_state";
    for (const [i, l] of d.lines.entries()) {
      if (!l.hsnSac) {
        exceptions.push({ severity: "error", code: "missing_hsn", message: `Line ${i + 1} of ${word(d)} ${d.number} has no HSN/SAC code; table 12 needs one.`, documentId: d.id, number: d.number });
      }
      if (l.taxCategory !== "taxable" && d.placeOfSupply !== 96) {
        const key = `${inter ? "inter" : "intra"}_${registered ? "registered" : "unregistered"}`;
        const n = nil.get(key) ?? { supply: key, label: NIL_LABELS[key], nilPaise: 0, exemptPaise: 0, nonGstPaise: 0 };
        if (l.taxCategory === "nil_rated") n.nilPaise += sign * l.taxablePaise;
        else if (l.taxCategory === "non_gst") n.nonGstPaise += sign * l.taxablePaise;
        else n.exemptPaise += sign * l.taxablePaise;
        nil.set(key, n);
      }
      if (l.hsnSac) {
        const code = l.hsnSac;
        const service = code.startsWith("99");
        const uqc = service ? "NA" : l.unit && /^[A-Z]{3}$/.test(l.unit) ? l.unit : "OTH";
        const rate = l.taxCategory === "taxable" ? l.gstRateBp : 0;
        const group: "b2b" | "b2c" = registered ? "b2b" : "b2c";
        const key = [group, code, uqc, rate].join("|");
        const rule = PREVIEW_HSN.filter((h) => code.startsWith(h.code)).sort((a, b) => b.code.length - a.code.length)[0];
        const h = hsn.get(key) ?? { group, hsn: code, description: rule?.description ?? l.description, uqc, quantity: "0", rateBp: rate, totals: zeroTotals() };
        if (!service) h.quantity = String(Number(h.quantity) + sign * Number(l.quantity));
        addTotals(h.totals, l, sign);
        hsn.set(key, h);
      }
    }
    if (!section) continue;
    const lines = d.lines.filter((l) => l.taxCategory === "taxable" || d.placeOfSupply === 96);
    if (section === "b2cs") {
      for (const l of lines) {
        const key = `${d.placeOfSupply}|${l.gstRateBp}|${!inter}`;
        const row = b2cs.get(key) ?? { placeOfSupply: d.placeOfSupply, placeName: place(d), intraState: !inter, rateBp: l.gstRateBp, totals: zeroTotals(), documents: [] };
        addTotals(row.totals, l, sign);
        if (!row.documents.some((x) => x.id === d.id)) row.documents.push({ id: d.id, number: d.number ?? "", docType: d.docType });
        b2cs.set(key, row);
      }
      continue;
    }
    const totals = zeroTotals();
    for (const l of lines) addTotals(totals, l);
    const key: Gstr1["documents"][number]["section"] = section.startsWith("exp") ? "exp" : section.startsWith("sez") ? "b2b" : (section as "b2b" | "b2cl" | "cdnr" | "cdnur");
    const labels: Record<string, string> = { expWpay: "Export with payment", expWopay: "Export under LUT", sezWpay: "SEZ with payment", sezWopay: "SEZ under LUT", b2cl: "B2C large" };
    documents.push({
      id: d.id,
      section: key,
      docType: d.docType,
      number: d.number ?? "",
      docDate: d.docDate,
      partyName: d.partyName,
      gstin: registered ? d.partyGstin : null,
      placeOfSupply: d.placeOfSupply,
      placeName: place(d),
      kindLabel: labels[section] ?? "Regular",
      reverseCharge: d.reverseCharge,
      valuePaise: d.totalPaise,
      totals,
      originalNumber: original?.number ?? null,
      originalId: original?.id ?? null,
    });
  }
  if (documents.some((r) => r.section === "exp")) {
    exceptions.push({ severity: "warning", code: "shipping_bill", message: "Exports of goods need the shipping bill number, date and port code. AccountsVU doesn't record them yet; add them on the portal before filing.", documentId: null, number: null });
  }
  for (const h of hsn.values()) {
    if (h.uqc === "OTH") exceptions.push({ severity: "warning", code: "uqc", message: `HSN ${h.hsn} at ${h.rateBp / 100}% (${h.group}) has lines without a standard unit, so it is reported in OTH (others).`, documentId: null, number: null });
  }
  const sum = (key: string) => {
    const t = zeroTotals();
    let value = 0;
    const rows = documents.filter((r) => r.section === key);
    for (const r of rows) {
      addTotals(t, r.totals);
      value += r.valuePaise;
    }
    return { count: rows.length, totals: t, valuePaise: value };
  };
  const net = zeroTotals();
  const sections: Gstr1["sections"] = [];
  const main = [
    ["b2b", "B2B, SEZ and deemed exports", "4A, 4B, 6B, 6C"],
    ["b2cl", "B2C large", "5"],
    ["exp", "Exports", "6A"],
  ] as const;
  for (const [key, title, table] of main) {
    const x = sum(key);
    addTotals(net, x.totals);
    sections.push({ key, title, table, ...x });
  }
  const b2csTotals = zeroTotals();
  for (const r of b2cs.values()) addTotals(b2csTotals, r.totals);
  addTotals(net, b2csTotals);
  sections.push({ key: "b2cs", title: "B2C small", table: "7", count: [...b2cs.values()].reduce((a, r) => a + r.documents.length, 0), totals: b2csTotals, valuePaise: 0 });
  const nilRows = [...nil.values()];
  sections.push({ key: "nil", title: "Nil-rated, exempt and non-GST", table: "8", count: nilRows.length, totals: zeroTotals(), valuePaise: nilRows.reduce((a, n) => a + n.nilPaise + n.exemptPaise + n.nonGstPaise, 0) });
  const notes = [
    ["cdnr", "Credit notes, registered"],
    ["cdnur", "Credit notes, unregistered"],
  ] as const;
  for (const [key, title] of notes) {
    const x = sum(key);
    addTotals(net, x.totals, -1);
    sections.push({ key, title, table: "9B", ...x });
  }
  const hsnTotals = zeroTotals();
  for (const h of hsn.values()) addTotals(hsnTotals, h.totals);
  sections.push({ key: "hsn", title: "HSN summary", table: "12", count: hsn.size, totals: hsnTotals, valuePaise: 0 });
  const seriesRows = [...series.values()].map((s) => {
    const sorted = [...s.numbers].sort();
    return { label: s.label, from: sorted[0], to: sorted[sorted.length - 1], total: s.numbers.length, cancelled: s.cancelled, net: s.numbers.length - s.cancelled };
  });
  sections.push({ key: "docs", title: "Documents issued", table: "13", count: seriesRows.reduce((a, s) => a + s.total, 0), totals: zeroTotals(), valuePaise: 0 });
  exceptions.sort((a, b) => (a.severity === "error" ? 0 : 1) - (b.severity === "error" ? 0 : 1));
  return {
    registrationId: reg.id,
    gstin: reg.gstin,
    stateName: stateName(reg.stateCode),
    period,
    label: periodLabel(period),
    from,
    to,
    dueDate: gstr1Due(period),
    status: periodStatus(period),
    rulePackVersion: PREVIEW_PACK,
    formatVersion: "gstr1-json/1",
    sections,
    totals: net,
    documents,
    b2cs: [...b2cs.values()],
    nil: nilRows,
    hsn: [...hsn.values()],
    series: seriesRows,
    exceptions,
  };
}

// GSTR-3B (preview; the Rust workpaper is the real one).
const H0 = (): Heads => ({ igst: 0, cgst: 0, sgst: 0, cess: 0 });
const hPlus = (a: Heads, b: Heads, sign = 1): Heads => ({ igst: a.igst + sign * b.igst, cgst: a.cgst + sign * b.cgst, sgst: a.sgst + sign * b.sgst, cess: a.cess + sign * b.cess });
const hFloor = (a: Heads): Heads => ({ igst: Math.max(0, a.igst), cgst: Math.max(0, a.cgst), sgst: Math.max(0, a.sgst), cess: Math.max(0, a.cess) });
const hSum = (a: Heads) => a.igst + a.cgst + a.sgst + a.cess;
const lineHeads = (l: { igstPaise: number; cgstPaise: number; sgstPaise: number; utgstPaise: number; cessPaise: number }): Heads => ({ igst: l.igstPaise, cgst: l.cgstPaise, sgst: l.sgstPaise + l.utgstPaise, cess: l.cessPaise });

function previewPropose(liability: Heads, credit: Heads, rc: Heads): OffsetProposal {
  const l = hFloor(liability);
  const c = hFloor(credit);
  const left = { ...l };
  const pay = (liab: number): HeadPayment => ({ liability: liab, byCredit: H0(), cash: 0 });
  const out = { igst: pay(l.igst), cgst: pay(l.cgst), sgst: pay(l.sgst), cess: pay(l.cess) };
  const use = (from: keyof Heads, to: keyof Heads, cap = Infinity) => {
    const x = Math.min(c[from], left[to], cap);
    out[to].byCredit[from] += x;
    c[from] -= x;
    left[to] -= x;
  };
  use("igst", "igst");
  const shortC = Math.max(0, left.cgst - c.cgst);
  const shortS = Math.max(0, left.sgst - c.sgst);
  use("igst", "cgst", shortC);
  use("igst", "sgst", shortS);
  use("igst", "cgst");
  use("igst", "sgst");
  use("cgst", "cgst");
  use("sgst", "sgst");
  use("cgst", "igst");
  use("sgst", "igst");
  use("cess", "cess");
  for (const k of ["igst", "cgst", "sgst", "cess"] as const) out[k].cash = left[k];
  const r = hFloor(rc);
  return { ...out, reverseCharge: r, carryForward: c, cashTotal: hSum(left) + hSum(r) };
}

function previewGstr3bOne(regId: string, period: string, broughtForward: Heads): Gstr3b {
  const g1 = previewWorkpaper(regId, period);
  const ours = (d: { gstRegistrationId: string | null; docDate: string }) => (d.gstRegistrationId ?? regs[0]?.id) === regId && d.docDate >= g1.from && d.docDate <= g1.to;
  const row = (key: string, label: string): Gstr3bRow => ({ key, label, taxablePaise: 0, tax: H0() });
  const outward = [
    row("a", "(a) Outward taxable supplies (other than zero-rated, nil-rated and exempted)"),
    row("b", "(b) Outward taxable supplies (zero-rated)"),
    row("c", "(c) Other outward supplies (nil-rated, exempted)"),
    row("d", "(d) Inward supplies (liable to reverse charge)"),
    row("e", "(e) Non-GST outward supplies"),
  ];
  const add = (rows: Gstr3bRow[], key: string, taxable: number, tax: Heads) => {
    const r = rows.find((x) => x.key === key)!;
    r.taxablePaise += taxable;
    r.tax = hPlus(r.tax, tax);
  };
  const interState = new Map<number, Gstr3b["interState"][number]>();
  for (const d of salesDocs.filter((x) => x.status === "issued" && ours(x))) {
    const sign = d.docType === "credit_note" ? -1 : 1;
    const target = salesDocs.find((o) => o.id === d.originalDocumentId) ?? d;
    const sec = previewGstr1(target, true, false).section ?? "";
    const zero = sec.startsWith("exp") || sec.startsWith("sez");
    for (const l of d.lines) {
      if (l.taxCategory === "taxable" && d.docType !== "bill_of_supply") {
        const tax = hPlus(H0(), lineHeads(l), sign);
        add(outward, zero ? "b" : "a", sign * l.taxablePaise, tax);
        const unreg = !["regular", "sez", "overseas"].includes(d.partyGstType) && d.placeOfSupply !== 96;
        if (!zero && unreg && d.supplyKind === "inter_state") {
          const r = interState.get(d.placeOfSupply) ?? { placeOfSupply: d.placeOfSupply, placeName: stateName(d.placeOfSupply), unregisteredTaxablePaise: 0, unregisteredIgstPaise: 0, compositionTaxablePaise: 0, compositionIgstPaise: 0 };
          if (d.partyGstType === "composition") {
            r.compositionTaxablePaise += sign * l.taxablePaise;
            r.compositionIgstPaise += tax.igst;
          } else {
            r.unregisteredTaxablePaise += sign * l.taxablePaise;
            r.unregisteredIgstPaise += tax.igst;
          }
          interState.set(d.placeOfSupply, r);
        }
      } else add(outward, l.taxCategory === "non_gst" ? "e" : "c", sign * l.taxablePaise, H0());
    }
  }
  const available = [row("1", "(1) Import of goods"), row("2", "(2) Import of services"), row("3", "(3) Inward supplies liable to reverse charge (other than 1 & 2)"), row("4", "(4) Inward supplies from ISD"), row("5", "(5) All other ITC")];
  const reversed = [row("1", "(1) As per rules 38, 42 & 43 of CGST Rules and section 17(5)"), row("2", "(2) Others")];
  const itcOther = [row("1", "(1) ITC reclaimed which was reversed under 4B(2) earlier"), row("2", "(2) Ineligible ITC under section 16(4) & ITC restricted due to PoS rules")];
  const inward = [
    { key: "exempt", label: "From a supplier under composition scheme, exempt and nil-rated supply", interStatePaise: 0, intraStatePaise: 0 },
    { key: "non_gst", label: "Non-GST supply", interStatePaise: 0, intraStatePaise: 0 },
  ];
  const itcLines: Gstr3b["itcLines"] = [];
  let pending = H0();
  let pendingLines = 0;
  let common = H0();
  let commonLines = 0;
  const mine = (d: { gstRegistrationId: string | null }) => (d.gstRegistrationId ?? regs[0]?.id) === regId;
  for (const d of purchaseDocs.filter((x) => x.status === "recorded" && mine(x) && (ours(x) || x.lines.some((l) => l.itcPeriod === period)))) {
    const sign = d.docType === "debit_note" ? -1 : 1;
    const inMonth = ours(d);
    const docMonth = d.docDate.slice(0, 7);
    for (const l of d.lines) {
      const tax = lineHeads(l);
      const claim = l.itcPeriod ?? docMonth;
      if (inMonth) {
        if (d.reverseCharge && l.taxCategory === "taxable") add(outward, "d", sign * l.taxablePaise, hPlus(H0(), tax, sign));
        const col = l.taxCategory === "non_gst" ? 1 : ["nil_rated", "exempt"].includes(l.taxCategory) ? 0 : -1;
        if (col >= 0) {
          if (d.supplyKind === "inter_state") inward[col].interStatePaise += sign * l.taxablePaise;
          else inward[col].intraStatePaise += sign * l.taxablePaise;
        }
      }
      if (claim !== period || !hSum(tax) || l.itcStatus === "na" || (!d.partyGstin && !d.reverseCharge)) continue;
      const table = d.reverseCharge ? (d.partyGstType === "overseas" ? "2" : "3") : "5";
      const total = hSum(tax);
      const eligible = l.itcStatus === "eligible" ? tax : l.itcStatus === "partial" ? { igst: Math.round((tax.igst * l.itcEligiblePaise) / total), cgst: Math.round((tax.cgst * l.itcEligiblePaise) / total), sgst: Math.round((tax.sgst * l.itcEligiblePaise) / total), cess: 0 } : H0();
      if (l.itcStatus === "pending") {
        pending = hPlus(pending, tax, sign);
        pendingLines += 1;
      } else {
        add(available, table, 0, hPlus(H0(), tax, sign));
        add(reversed, "1", 0, hPlus(H0(), hPlus(tax, eligible, -1), sign));
        if (l.itcCommon) {
          common = hPlus(common, eligible, sign);
          commonLines += 1;
        }
      }
      itcLines.push({
        documentId: d.id, docType: d.docType, supplierInvoice: d.supplierInvoiceNumber ?? d.number, docDate: d.docDate, partyName: d.partyName ?? "", gstin: d.partyGstin,
        description: l.description, table: l.itcStatus === "pending" ? "pending" : `4A(${table})`, status: l.itcStatus, reason: l.itcReason, common: !!l.itcCommon, reverseCharge: d.reverseCharge,
        tax: hPlus(H0(), tax, sign), eligible: hPlus(H0(), eligible, sign), movedFrom: claim !== docMonth ? docMonth : null,
      });
    }
  }
  const val = (k: string) => outward.find((r) => r.key === k)!;
  const exempt = val("c").taxablePaise + val("e").taxablePaise;
  const turnover = val("a").taxablePaise + val("b").taxablePaise + exempt;
  const f = (x: number) => (turnover > 0 && exempt > 0 ? Math.round((Math.max(0, x) * Math.min(exempt, turnover)) / turnover) : 0);
  const reversal = { igst: f(common.igst), cgst: f(common.cgst), sgst: f(common.sgst), cess: f(common.cess) };
  if (hSum(reversal)) add(reversed, "1", 0, reversal);
  const gross = available.reduce((h, r) => hPlus(h, r.tax), H0());
  const net = hPlus(gross, reversed.reduce((h, r) => hPlus(h, r.tax), H0()), -1);
  let liability = hPlus(val("a").tax, val("b").tax);
  let credit = hPlus(broughtForward, net);
  const negative = hFloor(hPlus(H0(), credit, -1));
  liability = hPlus(liability, negative);
  credit = hFloor(credit);
  const offset = previewPropose(liability, credit, val("d").tax);
  const nilG1 = g1.nil.reduce((a, n) => a + n.nilPaise + n.exemptPaise + n.nonGstPaise, 0);
  const ab = (fn: (r: Gstr3bRow) => number) => fn(val("a")) + fn(val("b"));
  const crossCheck = ([
    ["Taxable value (3.1 a + b)", g1.totals.taxablePaise, ab((r) => r.taxablePaise)],
    ["IGST", g1.totals.igstPaise, ab((r) => r.tax.igst)],
    ["CGST", g1.totals.cgstPaise, ab((r) => r.tax.cgst)],
    ["SGST/UTGST", g1.totals.sgstPaise, ab((r) => r.tax.sgst)],
    ["Cess", g1.totals.cessPaise, ab((r) => r.tax.cess)],
    ["Nil-rated, exempt and non-GST (3.1 c + e)", nilG1, exempt],
  ] as const).map(([label, a, b]) => ({ label, gstr1Paise: a, gstr3bPaise: b, differencePaise: b - a }));
  const exceptions: Gstr3b["exceptions"] = [];
  const rs = (p: number) => `₹${(p / 100).toLocaleString("en-IN", { minimumFractionDigits: 2 })}`;
  if (pendingLines) exceptions.push({ severity: "warning", code: "itc_pending", message: `${pendingLines} purchase ${pendingLines === 1 ? "line" : "lines"} with ${rs(hSum(pending))} of GST ${pendingLines === 1 ? "is" : "are"} awaiting ITC review, so not claimed. Review them in Purchases to claim.`, documentId: null, number: null });
  if (hSum(reversal) > 0) exceptions.push({ severity: "warning", code: "rule42", message: `Rule 42: ${rs(hSum(reversal))} of common credit is reversed in 4B(1) for exempt supplies (${rs(exempt)} of ${rs(turnover)} turnover). This is the monthly figure; it is trued up for the year.`, documentId: null, number: null });
  for (const c of crossCheck) if (c.differencePaise) exceptions.push({ severity: c.label.startsWith("Nil") ? "warning" : "error", code: c.label.startsWith("Nil") ? "gstr1_nil_mismatch" : "gstr1_mismatch", message: `${c.label}: GSTR-1 shows ${rs(c.gstr1Paise)}, GSTR-3B ${rs(c.gstr3bPaise)}.`, documentId: null, number: null });
  const [y, m] = period.split("-").map(Number);
  const due = isoLocal(new Date(y, m, 20));
  const today = isoLocal(new Date());
  return {
    registrationId: g1.registrationId, gstin: g1.gstin, stateName: g1.stateName, period, label: g1.label, from: g1.from, to: g1.to,
    dueDate: due, status: today <= g1.to ? "open" : today <= due ? "due" : "past_due",
    outward, interState: [...interState.values()], itcAvailable: available, itcReversed: reversed, itcNet: net, itcOther,
    inwardExempt: inward, rule42: { commonLines, commonCredit: common, exemptTurnoverPaise: exempt, totalTurnoverPaise: turnover, reversal },
    liability, reverseCharge: val("d").tax, creditBroughtForward: broughtForward, creditAvailable: credit, offset, crossCheck, itcLines, pendingItc: pending,
    exceptions: exceptions.sort((a, b) => (a.severity === "error" ? 0 : 1) - (b.severity === "error" ? 0 : 1)),
  };
}

/** GSTR-3B for a month, chaining credit from the start of the books. */
function previewGstr3b(regId: string, period: string): Gstr3b {
  const start = (org?.booksStartDate ?? `${period}-01`).slice(0, 7);
  let p = start <= period ? start : period;
  let carry = H0();
  for (;;) {
    const v = previewGstr3bOne(regId, p, carry);
    if (p === period) return v;
    carry = v.offset.carryForward;
    const [y, m] = p.split("-").map(Number);
    p = m === 12 ? `${y + 1}-01` : `${y}-${String(m + 1).padStart(2, "0")}`;
  }
}

// --- GSTR-2B (Phase 2.4): a small mirror of the Rust reconciliation, for the browser preview ---

type Recon2bLine = { section: string; gstin: string; name: string; kind: ReconRow["kind"]; number: string; date: string; value: number; a: ReconAmounts; pos: string | null; rev: boolean; itc: boolean; rsn: string | null; filed: string | null; supPeriod: string | null };
const recon2b = new Map<string, { fileName: string; importedAt: string; generatedOn: string | null; skipped: string | null; lines: Recon2bLine[] }>();
const recon2bDecisions = new Map<string, { decision: ReconDecision; note: string | null; purchaseId: string | null }>();

const normNo = (n: string) =>
  (n.toUpperCase().match(/[A-Z]+|[0-9]+/g) ?? []).map((r) => (/^[0-9]+$/.test(r) ? r.replace(/^0+(?=.)/, "") : r)).join("");
const tailDigits = (n: string) => ((n.match(/[0-9]+/g) ?? []).pop() ?? "").replace(/^0+(?=.)/, "");
const amt = (taxable: number, igst: number, cgst: number, sgst: number, cess: number): ReconAmounts => ({ taxablePaise: taxable, igstPaise: igst, cgstPaise: cgst, sgstPaise: sgst, cessPaise: cess, taxPaise: igst + cgst + sgst + cess });
const r2bPaise = (v: unknown) => {
  const m = String(v ?? 0).trim().match(/^(-?)(\d*)(?:\.(\d*))?$/);
  if (!m) throw { code: "invalid_input", message: `${String(v)} isn't an amount`, fields: [] } satisfies Fail;
  const f = (m[3] ?? "").padEnd(3, "0");
  const p = Number(m[2] || "0") * 100 + Number(f.slice(0, 2)) + (Number(f[2]) >= 5 ? 1 : 0);
  return m[1] ? -p : p;
};
const dmy = (d: string) => {
  const [dd, mm, yy] = d.split(/[-/]/);
  return `${yy}-${mm.padStart(2, "0")}-${dd.padStart(2, "0")}`;
};
const mmyyyy = (p: string) => `${p.slice(2)}-${p.slice(0, 2)}`;

function previewImportGstr2b(regId: string, fileName: string, bytes: number[]): Gstr2bImportResult {
  const fail = (message: string) => ({ code: "invalid_input", message, fields: [] }) satisfies Fail;
  const reg = regs.find((r) => r.id === regId);
  if (!reg) throw { code: "not_found", message: "That GST registration wasn't found.", fields: [] } satisfies Fail;
  if (/\.(xlsx?|zip)$/i.test(fileName)) throw fail("choose the JSON download of GSTR-2B (Excel and ZIP files aren't read yet; unzip the download first)");
  let root: Record<string, unknown>;
  try {
    root = JSON.parse(new TextDecoder().decode(new Uint8Array(bytes)).replace(/^﻿/, ""));
  } catch {
    throw fail("this isn't a GSTR-2B JSON file: not valid JSON");
  }
  const data = (root.data ?? root) as Record<string, unknown>;
  const gstin = String(data.gstin ?? "").toUpperCase();
  const rtn = String(data.rtnprd ?? "");
  const docdata = data.docdata as Record<string, unknown> | undefined;
  if (!gstin || !/^\d{6}$/.test(rtn) || !docdata) throw fail("this isn't a GSTR-2B JSON file");
  if (gstin !== reg.gstin.toUpperCase()) throw fail(`this GSTR-2B is for ${gstin}, not ${reg.gstin}`);
  const lines: Recon2bLine[] = [];
  const skipped: string[] = [];
  for (const [section, key] of [["b2b", "inv"], ["b2ba", "inv"], ["cdnr", "nt"], ["cdnra", "nt"]] as const) {
    for (const s of (docdata[section] as Record<string, unknown>[] | undefined) ?? []) {
      for (const d of (s[key] as Record<string, unknown>[] | undefined) ?? []) {
        const notes = section.startsWith("cdn");
        const items = (d.items as Record<string, unknown>[] | undefined) ?? [];
        const head = (k: string) => (k in d ? r2bPaise(d[k]) : items.reduce((a, it) => a + r2bPaise(it[k]), 0));
        lines.push({
          section, gstin: String(s.ctin ?? "").toUpperCase(), name: String(s.trdnm ?? ""),
          kind: notes ? (d.typ === "D" ? "debit_note" : "credit_note") : "invoice",
          number: String(d[notes ? "ntnum" : "inum"] ?? ""), date: dmy(String(d.dt ?? "")), value: r2bPaise(d.val),
          a: amt(head("txval"), head("igst"), head("cgst"), head("sgst"), head("cess")),
          pos: d.pos ? String(d.pos) : null, rev: d.rev === "Y", itc: d.itcavl !== "N", rsn: d.rsn ? String(d.rsn) : null,
          filed: s.supfildt ? dmy(String(s.supfildt)) : null, supPeriod: s.supprd ? mmyyyy(String(s.supprd)) : null,
        });
      }
    }
  }
  for (const [k, v] of Object.entries(docdata)) if (!["b2b", "b2ba", "cdnr", "cdnra"].includes(k) && Array.isArray(v) && v.length) skipped.push(`${k.toUpperCase()} ${v.length}`);
  const period = mmyyyy(rtn);
  const replaced = recon2b.has(`${regId}|${period}`);
  recon2b.set(`${regId}|${period}`, { fileName, importedAt: new Date().toISOString(), generatedOn: data.gendt ? dmy(String(data.gendt)) : null, skipped: skipped.join(", ") || null, lines });
  return { period, documentCount: lines.length, skippedSummary: skipped.join(", ") || null, replaced };
}

function previewGstr2b(regId: string, period: string): Gstr2b {
  const reg = regs.find((r) => r.id === regId);
  if (!reg) throw { code: "not_found", message: "That GST registration wasn't found.", fields: [] } satisfies Fail;
  const first = `${period}-01`;
  const last = monthEnd(period);
  const st = recon2b.get(`${regId}|${period}`);
  const lines = st?.lines ?? [];
  const key2b = (gstin: string, kind: string, number: string) => `2b:${gstin}|${kind}|${normNo(number)}`;
  const elsewhere = new Map<string, string>();
  for (const [k, s] of recon2b)
    if (k.startsWith(`${regId}|`) && !k.endsWith(`|${period}`))
      for (const l of s.lines) if (!elsewhere.has(key2b(l.gstin, l.kind, l.number))) elsewhere.set(key2b(l.gstin, l.kind, l.number), k.split("|")[1]);
  const decided = (key: string) => recon2bDecisions.get(`${regId}|${period}|${key}`);
  const deferredBefore = (key: string) =>
    [...recon2bDecisions.entries()].some(([k, d]) => k.startsWith(`${regId}|`) && k.split("|")[1] < period && k.endsWith(`|${key}`) && d.decision === "deferred");

  type Book = { side: ReconBookSide; gstin: string; kind: ReconRow["kind"]; name: string };
  const books: Book[] = [];
  const foundIn: { b: Book; at: string }[] = [];
  for (const d of purchaseDocs) {
    if (d.status !== "recorded" || !d.partyGstin || (d.gstRegistrationId ?? regs[0]?.id) !== regId) continue;
    const kind = d.docType === "debit_note" ? "credit_note" : "invoice";
    const number = (d.docType === "debit_note" ? d.number : (d.supplierInvoiceNumber ?? d.number)) ?? "";
    if (!number) continue;
    const date = d.supplierInvoiceDate ?? d.docDate;
    if (date > last) continue;
    let pendingLines = 0;
    let claim = 0;
    for (const l of d.lines) {
      const t = hSum(lineHeads(l));
      if (!t) continue;
      if (l.itcStatus === "pending") pendingLines++;
      else if (l.itcStatus === "eligible") claim += t;
      else if (l.itcStatus === "partial") claim += l.itcEligiblePaise;
    }
    const b: Book = {
      gstin: d.partyGstin.toUpperCase(),
      kind,
      name: d.partyName ?? "",
      side: {
        purchaseId: d.id, docType: d.docType as ReconBookSide["docType"], number, date,
        amounts: amt(d.taxablePaise, d.igstPaise, d.cgstPaise, d.sgstPaise + d.utgstPaise, d.cessPaise),
        itcPendingLines: pendingLines, itcClaimablePaise: claim, carried: date < first,
      },
    };
    const at = elsewhere.get(key2b(b.gstin, kind, number));
    if (at) {
      if (date >= first) foundIn.push({ b, at });
      continue;
    }
    books.push(b);
  }
  const used = new Set<number>();
  const done = new Set<number>();
  const rows: ReconRow[] = [];
  const agree = (x: ReconAmounts, y: ReconAmounts) => (["taxablePaise", "igstPaise", "cgstPaise", "sgstPaise", "cessPaise"] as const).every((k) => Math.abs(x[k] - y[k]) <= 100);
  const diffs = (b: Book, l: Recon2bLine): ReconDifference[] => {
    const out: ReconDifference[] = [];
    if (normNo(b.side.number) !== normNo(l.number)) out.push({ field: "number", books: b.side.number, portal: l.number, deltaPaise: null });
    if (b.side.date !== l.date) out.push({ field: "date", books: b.side.date, portal: l.date, deltaPaise: null });
    for (const [f, k] of [["taxable", "taxablePaise"], ["igst", "igstPaise"], ["cgst", "cgstPaise"], ["sgst", "sgstPaise"], ["cess", "cessPaise"]] as const)
      if (Math.abs(l.a[k] - b.side.amounts[k]) > 100) out.push({ field: f, books: String(b.side.amounts[k]), portal: String(l.a[k]), deltaPaise: l.a[k] - b.side.amounts[k] });
    return out;
  };
  const portalSide = (l: Recon2bLine): ReconPortalSide => ({
    section: l.section, number: l.number, date: l.date, valuePaise: l.value, amounts: l.a,
    itcAvailable: l.itc, itcReason: l.rsn, reverseCharge: l.rev, supplierFiledOn: l.filed, supplierPeriod: l.supPeriod,
  });
  const push = (key: string, status: ReconStatus, gstin: string, name: string, kind: ReconRow["kind"], b: Book | null, l: Recon2bLine | null, differences: ReconDifference[] = [], found: string | null = null) => {
    const d = found ? undefined : decided(key);
    const ok = d && b && d.purchaseId && d.purchaseId !== b.side.purchaseId ? undefined : d;
    rows.push({
      key, status, decision: ok?.decision ?? null, note: ok?.note ?? null, needsReview: !found && status !== "exact" && !ok,
      supplierGstin: gstin, supplierName: name, kind, books: b?.side ?? null, portal: l ? portalSide(l) : null, differences, foundIn: found,
    });
  };
  lines.forEach((l, i) => {
    const firstAt = lines.findIndex((x) => x.gstin === l.gstin && x.kind === l.kind && normNo(x.number) === normNo(l.number));
    if (firstAt < i) {
      done.add(i);
      push(`${key2b(l.gstin, l.kind, l.number)}#${i + 1}`, "duplicate", l.gstin, l.name, l.kind, null, l);
    }
  });
  lines.forEach((l, i) => {
    if (done.has(i)) return;
    const bi = books.findIndex((b, j) => !used.has(j) && b.gstin === l.gstin && b.kind === l.kind && normNo(b.side.number) === normNo(l.number));
    if (bi < 0) return;
    used.add(bi);
    done.add(i);
    const d = diffs(books[bi], l);
    push(key2b(l.gstin, l.kind, l.number), d.length ? "mismatch" : "exact", l.gstin, l.name || books[bi].name, l.kind, books[bi], l, d);
  });
  lines.forEach((l, i) => {
    if (done.has(i)) return;
    const bi = books.findIndex(
      (b, j) => !used.has(j) && b.gstin === l.gstin && b.kind === l.kind && agree(b.side.amounts, l.a) && (b.side.date === l.date || (!!tailDigits(l.number) && tailDigits(b.side.number) === tailDigits(l.number))),
    );
    if (bi < 0) return;
    used.add(bi);
    done.add(i);
    push(key2b(l.gstin, l.kind, l.number), "likely", l.gstin, l.name || books[bi].name, l.kind, books[bi], l, diffs(books[bi], l));
  });
  lines.forEach((l, i) => {
    if (!done.has(i)) push(key2b(l.gstin, l.kind, l.number), "missing_in_books", l.gstin, l.name, l.kind, null, l);
  });
  books.forEach((b, j) => {
    if (used.has(j)) return;
    if (b.side.carried && !deferredBefore(`book:${b.side.purchaseId}`)) return;
    push(`book:${b.side.purchaseId}`, "missing_in_2b", b.gstin, b.name, b.kind, b, null);
  });
  for (const f of foundIn) push(`book:${f.b.side.purchaseId}`, "missing_in_2b", f.b.gstin, f.b.name, f.b.kind, f.b, null, [], f.at);
  const dateOf = (r: ReconRow) => r.portal?.date ?? r.books?.date ?? "";
  rows.sort((a, b) => a.supplierName.toLowerCase().localeCompare(b.supplierName.toLowerCase()) || dateOf(a).localeCompare(dateOf(b)));

  const labels: [ReconStatus, string][] = [
    ["exact", "Exact match"], ["likely", "Likely match"], ["mismatch", "Mismatch"],
    ["missing_in_2b", "Missing in 2B"], ["missing_in_books", "Missing in books"], ["duplicate", "Duplicate in 2B"],
  ];
  const zero = () => amt(0, 0, 0, 0, 0);
  const addA = (t: ReconAmounts, a: ReconAmounts, s: number) => (Object.keys(t) as (keyof ReconAmounts)[]).forEach((k) => (t[k] += s * a[k]));
  const summary = labels.map(([status, label]) => ({ status, label, count: 0, needsReview: 0, books: zero(), portal: zero() }));
  const itc = { portalAvailablePaise: 0, portalNotAvailablePaise: 0, booksClaimablePaise: 0, booksClaimedNotIn2bPaise: 0 };
  for (const r of rows) {
    const s = r.kind === "credit_note" ? -1 : 1;
    const x = summary.find((y) => y.status === r.status)!;
    x.count++;
    x.needsReview += r.needsReview ? 1 : 0;
    if (r.books) addA(x.books, r.books.amounts, s);
    if (r.portal) addA(x.portal, r.portal.amounts, s);
    if (r.status === "duplicate" || r.foundIn) continue;
    if (r.portal) itc[r.portal.itcAvailable ? "portalAvailablePaise" : "portalNotAvailablePaise"] += s * r.portal.amounts.taxPaise;
    if (r.books) {
      itc.booksClaimablePaise += s * r.books.itcClaimablePaise;
      if (!r.portal) itc.booksClaimedNotIn2bPaise += s * r.books.itcClaimablePaise;
    }
  }
  return {
    registrationId: regId,
    gstin: reg.gstin,
    period,
    label: periodLabel(period),
    statement: st ? { fileName: st.fileName, importedAt: st.importedAt, generatedOn: st.generatedOn, documentCount: st.lines.length, skippedSummary: st.skipped } : null,
    rows,
    summary,
    needsReview: rows.filter((r) => r.needsReview).length,
    itc,
    tolerancePaise: 100,
  };
}

function previewDecideGstr2b(regId: string, period: string, input: Gstr2bDecisionInput): Gstr2b {
  const v = previewGstr2b(regId, period);
  const row = v.rows.find((r) => r.key === input.rowKey && !r.foundIn);
  if (!row) throw { code: "not_found", message: "That reconciliation row wasn't found.", fields: [] } satisfies Fail;
  const k = `${regId}|${period}|${row.key}`;
  if (!input.decision) recon2bDecisions.delete(k);
  else {
    if (row.status === "exact") throw { code: "invalid_input", message: "an exact match needs no decision", fields: [] } satisfies Fail;
    recon2bDecisions.set(k, { decision: input.decision, note: input.note ?? null, purchaseId: row.books?.purchaseId ?? null });
  }
  return previewGstr2b(regId, period);
}


// ---------------------------------------------------------------------------
// Offline filing (Phase 2.5)
// ---------------------------------------------------------------------------

type PreviewFiling = {
  id: string;
  regId: string;
  returnType: FilingReturnType;
  period: string;
  status: "filed" | "removed";
  arn: string;
  filedOn: string;
  note: string | null;
  summary: Record<string, unknown>;
  warningCount: number;
  recordedAt: string;
  files: FilingFile[];
  removedAt: string | null;
  reason: string | null;
};
let filings: PreviewFiling[] = [];

const RETURN_LABEL: Record<FilingReturnType, string> = { gstr1: "GSTR-1", gstr3b: "GSTR-3B" };
const FILING_ERRORS = new Set(["period_open", "missing_gstin", "missing_hsn", "hsn_mismatch", "gstr1_mismatch"]);

function filedRecord(regId: string, period: string, rt: FilingReturnType) {
  return filings.find((f) => f.status === "filed" && f.regId === regId && f.period === period && f.returnType === rt);
}

function filedBadge(regId: string, period: string, rt: FilingReturnType) {
  const f = filedRecord(regId, period, rt);
  return f ? { arn: f.arn, filedOn: f.filedOn } : null;
}

function monthName(period: string) {
  return new Date(`${period}-01T00:00:00`).toLocaleDateString("en-IN", { month: "long", year: "numeric" });
}

function prevMonth(period: string) {
  const d = new Date(`${period}-01T00:00:00`);
  d.setMonth(d.getMonth() - 1);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
}

function nextMonth(period: string) {
  const d = new Date(`${period}-01T00:00:00`);
  d.setMonth(d.getMonth() + 1);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
}

/** "GSTR-1 for October 2026" when a filed return locks the family on that date. */
function previewGstLock(regId: string | null, date: string, family: "sales" | "purchases") {
  const reg = regId ?? regs[0]?.id;
  if (!reg) return null;
  const month = date.slice(0, 7);
  const f = (["gstr1", "gstr3b"] as const)
    .filter((rt) => family === "sales" || rt === "gstr3b")
    .map((rt) => filedRecord(reg, month, rt))
    .find(Boolean);
  return f ? `${RETURN_LABEL[f.returnType]} for ${monthName(month)}` : null;
}

function previewReturnFiling(regId: string, period: string, rt: FilingReturnType): FilingView {
  const reg = regs.find((r) => r.id === regId) ?? nf("GST registration not found");
  const checks: Gstr1["exceptions"] = [];
  let summary: Record<string, unknown>;
  let label: string;
  let due: string;
  if (rt === "gstr1") {
    const w = previewWorkpaper(regId, period);
    checks.push(...w.exceptions);
    summary = { documents: w.documents.length, totals: w.totals };
    label = w.label;
    due = w.dueDate;
  } else {
    const v = previewGstr3b(regId, period);
    checks.push(...v.exceptions);
    summary = { liability: v.liability, reverseCharge: v.reverseCharge, itcNet: v.itcNet, cashPayable: v.offset.cashTotal, carryForward: v.offset.carryForward };
    label = v.label;
    due = v.dueDate;
    if (!filedRecord(regId, period, "gstr1"))
      checks.push({ severity: "warning", code: "gstr1_unfiled", message: `GSTR-1 for ${monthName(period)} isn't recorded as filed. The portal expects it before GSTR-3B.`, documentId: null, number: null });
    if (recon2b.has(`${regId}|${period}`)) {
      const n = previewGstr2b(regId, period).needsReview;
      if (n) checks.push({ severity: "warning", code: "gstr2b_review", message: `${n} GSTR-2B ${n === 1 ? "row is" : "rows are"} still to review.`, documentId: null, number: null });
    } else {
      checks.push({ severity: "warning", code: "gstr2b_missing", message: `GSTR-2B for ${monthName(period)} isn't imported, so ITC isn't checked against it.`, documentId: null, number: null });
    }
  }
  const today = isoLocal(new Date());
  if (today < `${nextMonth(period)}-01`)
    checks.push({ severity: "error", code: "period_open", message: `${monthName(period)} isn't over yet; it can be filed from 1 ${monthName(nextMonth(period))}.`, documentId: null, number: null });
  const prev = prevMonth(period);
  if (org && prev >= org.booksStartDate.slice(0, 7) && !filedRecord(regId, prev, rt))
    checks.push({ severity: "warning", code: "previous_unfiled", message: `${RETURN_LABEL[rt]} for ${monthName(prev)} isn't recorded as filed. Returns are filed in order.`, documentId: null, number: null });
  const blocking = (c: { code: string; severity: string }) => FILING_ERRORS.has(c.code) && c.severity === "error";
  const errors = checks.filter(blocking).map((c) => ({ ...c, severity: "error" as const }));
  const warnings = checks.filter((c) => !blocking(c)).map((c) => ({ ...c, severity: "warning" as const }));
  const f = filedRecord(regId, period, rt);
  return {
    registrationId: regId,
    gstin: reg.gstin,
    returnType: rt,
    label: RETURN_LABEL[rt],
    period,
    periodLabel: label,
    dueDate: due,
    filing: f
      ? { id: f.id, channel: "offline", arn: f.arn, filedOn: f.filedOn, note: f.note, returnPackVersion: "2026.11.1", rulePackVersion: "2026.11.1", summary: f.summary, warningCount: f.warningCount, recordedAt: f.recordedAt, recordedBy: "owner", files: f.files }
      : null,
    removed: filings
      .filter((x) => x.status === "removed" && x.regId === regId && x.period === period && x.returnType === rt)
      .map((x) => ({ arn: x.arn, filedOn: x.filedOn, removedAt: x.removedAt, removedBy: "owner", reason: x.reason })),
    validation: { packVersion: "2026.11.1", errors, warnings, canFile: errors.length === 0 },
    summary,
    changedSinceFiling: !!f && JSON.stringify(f.summary) !== JSON.stringify(summary),
  };
}

function previewRecordFiling(regId: string, period: string, rt: FilingReturnType, input: FilingInput): FilingView {
  const v = previewReturnFiling(regId, period, rt);
  const e: [string, string][] = [];
  const arn = input.arn.replace(/\s+/g, "").toUpperCase();
  if (!/^[A-Z0-9]{15}$/.test(arn)) e.push(["arn", "Enter the 15-character ARN from the acknowledgement."]);
  const today = isoLocal(new Date());
  if (!/^\d{4}-\d{2}-\d{2}$/.test(input.filedOn)) e.push(["filedOn", "Enter the filing date."]);
  else if (input.filedOn > today) e.push(["filedOn", "The filing date can't be in the future."]);
  else if (input.filedOn < `${nextMonth(period)}-01`) e.push(["filedOn", `A return for ${monthName(period)} can only be filed after the month ends.`]);
  if (input.acknowledgement) {
    if (!/\.(pdf|png|jpe?g)$/i.test(input.acknowledgement.fileName)) e.push(["acknowledgement", "Attach the acknowledgement as a PDF, PNG or JPG."]);
    else if (input.acknowledgement.bytes.length > 5 * 1024 * 1024) e.push(["acknowledgement", "The acknowledgement must be 5 MB or smaller."]);
  }
  if (e.length) fail(e);
  if (v.filing) conflict(`${v.label} for ${v.periodLabel} is already recorded as filed.`);
  if (!v.validation.canFile) throw { code: "invalid_input", message: `Fix the ${v.validation.errors.length} errors first, so the books match what was filed.`, fields: [] } satisfies Fail;
  const reg = regs.find((r) => r.id === regId)!;
  const stamp = `${period.slice(5)}${period.slice(0, 4)}`;
  const file = (kind: FilingFile["kind"], fileName: string, sizeBytes: number): FilingFile => ({ id: crypto.randomUUID(), kind, fileName, sizeBytes });
  const files =
    rt === "gstr1"
      ? [file("portal_json", `GSTR1_${reg.gstin}_${stamp}.json`, 2048), file("workpaper_json", `GSTR1_workpaper_${reg.gstin}_${stamp}.json`, 8192)]
      : [file("workpaper_xlsx", `GSTR3B_workpaper_${reg.gstin}_${stamp}.xlsx`, 16384), file("workpaper_json", `GSTR3B_workpaper_${reg.gstin}_${stamp}.json`, 8192)];
  if (input.acknowledgement) files.push(file("acknowledgement", input.acknowledgement.fileName, input.acknowledgement.bytes.length));
  const rec: PreviewFiling = {
    id: crypto.randomUUID(), regId, returnType: rt, period, status: "filed", arn, filedOn: input.filedOn, note: input.note?.trim() || null,
    summary: v.summary, warningCount: v.validation.warnings.length, recordedAt: now(), files, removedAt: null, reason: null,
  };
  filings.push(rec);
  log("gst_filing.recorded", rec.id, rec);
  return previewReturnFiling(regId, period, rt);
}

function previewRemoveFiling(regId: string, period: string, rt: FilingReturnType, reason: string): FilingView {
  if (!String(reason ?? "").trim()) fail([["reason", "Say why the filing record is being removed."]]);
  const f = filedRecord(regId, period, rt) ?? nf("Filing record not found");
  Object.assign(f, { status: "removed", removedAt: now(), reason: reason.trim() });
  log("gst_filing.removed", f.id, f);
  return previewReturnFiling(regId, period, rt);
}

function previewReturnCentre(regId: string | null, fyCode: string | null): ReturnCentre {
  const active = regs.filter((r) => r.registrationType !== "composition");
  const reg = active.find((r) => r.id === regId) ?? active.find((r) => r.status === "active") ?? active[0] ?? null;
  const currentStart = Number(fy().code.slice(0, 4));
  const start = fyCode ? Number(fyCode.slice(0, 4)) : currentStart;
  const code = (y: number) => `${y}-${String((y + 1) % 100).padStart(2, "0")}`;
  const today = isoLocal(new Date());
  const periods: ReturnPeriod[] = [];
  if (reg) {
    for (let i = 0; i < 12; i++) {
      const d = new Date(start, 3 + i, 1);
      const p = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
      if (`${p}-01` > today) break;
      const w = previewWorkpaper(reg.id, p);
      const three = previewGstr3b(reg.id, p);
      const inPeriod = salesDocs.filter((x) => (x.gstRegistrationId ?? regs[0]?.id) === reg.id && x.docDate >= w.from && x.docDate <= w.to);
      periods.push({
        period: p,
        label: w.label,
        from: w.from,
        to: w.to,
        gstr1Due: w.dueDate,
        status: w.status,
        documentCount: inPeriod.filter((x) => x.status === "issued").length,
        draftCount: inPeriod.filter((x) => x.status === "draft").length,
        errorCount: w.exceptions.filter((e) => e.severity === "error").length,
        warningCount: w.exceptions.filter((e) => e.severity !== "error").length,
        outputTax: w.totals,
        gstr3bDue: three.dueDate,
        gstr3bStatus: three.status,
        netItcPaise: hSum(three.itcNet),
        cashPayablePaise: three.offset.cashTotal,
        gstr2bImported: recon2b.has(`${reg.id}|${p}`),
        gstr2bNeedsReview: recon2b.has(`${reg.id}|${p}`) ? previewGstr2b(reg.id, p).needsReview : 0,
        gstr1Filed: filedBadge(reg.id, p, "gstr1"),
        gstr3bFiled: filedBadge(reg.id, p, "gstr3b"),
      });
    }
  }
  return {
    registrations: active.map((r) => ({ id: r.id, gstin: r.gstin, stateName: stateName(r.stateCode), filingFrequency: r.filingFrequency as "monthly" | "quarterly" })),
    registrationId: reg?.id ?? null,
    financialYear: code(start),
    fyStart: `${start}-04-01`,
    fyEnd: `${start + 1}-03-31`,
    previousYear: start >= currentStart ? code(start - 1) : null,
    nextYear: `${start + 1}-04-01` <= today ? code(start + 1) : null,
    quarterly: reg?.filingFrequency === "quarterly",
    periods: periods.reverse(),
  };
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
  const gstr1 =
    s.party && s.pos != null
      ? previewGstr1(
          { docType: input.docType, partyGstType: s.party.gstType, placeOfSupply: s.pos, supplyKind: s.kind ?? "intra_state", zeroRated: input.zeroRated, totalPaise: t.totalPaise },
          input.lines.some((l) => l.taxCategory === "taxable"),
          input.lines.some((l) => l.taxCategory !== "taxable"),
        )
      : null;
  return {
    supplyKind: s.kind, placeOfSupply: s.pos, supplierState: s.supplier, lines, totals: t, numberPreview: s.party && sr ? sr.nextPreview : null,
    warnings: s.party ? previewWarnings(input, s) : [], gstr1, rulePackVersion: PREVIEW_PACK,
  };
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
  const credited = salesDocs.filter((c) => c.originalDocumentId === d.id && c.status === "issued").reduce((a, c) => a + c.totalPaise, 0);
  if (d.docType !== "credit_note" && d.paidPaise + credited >= d.totalPaise) return d.paidPaise > 0 ? "paid" : "issued";
  if (d.docType !== "credit_note" && d.paidPaise > 0) return "part_paid";
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
    gstr1: previewGstr1(d, d.lines.some((l) => l.taxCategory === "taxable"), d.lines.some((l) => l.taxCategory !== "taxable")),
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
    rulePackVersion: PREVIEW_PACK,
    lines: input.lines.map((l, i) => ({
      id: crypto.randomUUID(), documentId: docId, lineNo: i + 1, itemId: l.itemId ?? null,
      description: l.description || item(l.itemId)?.name || "", hsnSac: l.hsnSac ?? item(l.itemId)?.hsnSac ?? null, unit: l.unit ?? item(l.itemId)?.unit ?? null,
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
      const lockS = previewGstLock(args.input.gstRegistrationId ?? null, args.input.docDate, "sales");
      if (lockS) fail([["docDate", `${lockS} is recorded as filed, so nothing can be issued in that month. Date it in an open month.`]]);
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
      const lockC = previewGstLock(d.gstRegistrationId, d.docDate, "sales");
      if (lockC) conflict(`${lockC} is recorded as filed with this document, so it can't be cancelled. Issue a credit note in an open month instead.`);
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
  return { supplyKind: s.kind, lines, totals: t, reverseChargeTaxPaise: input.reverseCharge ? tax : 0, numberPreview: input.docType === "debit_note" ? (sr?.nextPreview ?? null) : null, warnings: [], rulePackVersion: PREVIEW_PACK };
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
  const debited = purchaseDocs.filter((n) => n.originalDocumentId === d.id && n.status === "recorded").reduce((a, n) => a + n.totalPaise, 0);
  if (d.paidPaise + debited >= d.totalPaise) return d.paidPaise > 0 ? "paid" : "recorded";
  if (d.paidPaise > 0) return "part_paid";
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
    createdAt: existing?.createdAt ?? now(), updatedAt: now(), recordedAt: null, cancelledAt: null, rulePackVersion: PREVIEW_PACK,
    lines: input.lines.map((l, i) => {
      const lc = c.lines[i]!;
      const tax = lc.cgstPaise + lc.sgstPaise + lc.utgstPaise + lc.igstPaise;
      const itc = (tax ? (l.itcStatus ?? "pending") : "na") as ItcStatus;
      return {
        id: crypto.randomUUID(), documentId: docId, lineNo: i + 1, itemId: l.itemId ?? null, accountId: l.accountId, description: l.description || items.find((x) => x.id === l.itemId)?.name || "",
        hsnSac: l.hsnSac ?? null, unit: l.unit ?? null, quantity: l.quantity, unitPricePaise: toPaise(l.unitPrice) ?? 0, discountPaise: toPaise(l.discount) ?? 0,
        taxCategory: l.taxCategory, gstRateBp: toBp(l.gstRate), cessRateBp: 0, ...lc, itcStatus: itc, itcReason: l.itcReason ?? null,
        itcEligiblePaise: itc === "eligible" ? tax : itc === "partial" ? (toPaise(l.itcEligible) ?? 0) : 0, itcReviewedAt: null, itcReviewedBy: null, itcCommon: !!l.itcCommon && (itc === "eligible" || itc === "partial"), itcPeriod: null,
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
      const lockP = previewGstLock(args.input.gstRegistrationId ?? null, args.input.docDate, "purchases");
      if (lockP) fail([["docDate", `${lockP} is recorded as filed, so nothing can be recorded in that month. Date it in an open month; its ITC is claimed there.`]]);
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
      const regOf = d.gstRegistrationId ?? regs[0]?.id ?? "";
      let claimMonth = l.itcPeriod ?? d.docDate.slice(0, 7);
      if (filedRecord(regOf, claimMonth, "gstr3b")) {
        if (l.itcStatus !== "pending") conflict(`GSTR-3B for ${monthName(claimMonth)} is recorded as filed with this line's ITC, so its review can't change. Correct it with a debit note in an open month.`);
        do claimMonth = nextMonth(claimMonth);
        while (filedRecord(regOf, claimMonth, "gstr3b"));
        l.itcPeriod = claimMonth;
      }
      Object.assign(l, { itcStatus: st, itcReason: args.input.reason || null, itcEligiblePaise: eligible, itcReviewedAt: now(), itcReviewedBy: "owner", itcCommon: !!args.input.common && (st === "eligible" || st === "partial") });
      d.rowVersion += 1;
      log("purchase_line.itc_review", l.id, l);
      return purchaseView(d.id);
    }
    case "cancel_purchase": {
      const d = purchaseDocs.find((x) => x.id === args.id)!;
      const lockX = previewGstLock(d.gstRegistrationId, d.docDate, "purchases");
      if (lockX) conflict(`${lockX} is recorded as filed with this purchase, so it can't be cancelled. Record a debit note in an open month instead.`);
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


// ---------------------------------------------------------------------------
// Banking (Phase 1.5) — a simplified stand-in: balances come from opening
// balances, receipts/payments and cash/bank purchases only.
// ---------------------------------------------------------------------------

type MoneyRec = Omit<MoneyAccount, "ledgerCode" | "systemRole" | "balancePaise" | "openingBalancePaise" | "unmatchedLines" | "lastStatementDate" | "statementBalancePaise">;
type VoucherRec = Omit<VoucherView, "moneyAccountName" | "moneyAccountKind" | "partyName" | "counterAccountName" | "allocations" | "allocatedPaise" | "onAccountPaise" | "statementLineId"> & { allocations: { documentKind: "sales" | "purchase"; documentId: string; amountPaise: number }[] };
type LineRec = Omit<StatementLine, "matched" | "suggestions">;

let moneyAccounts: MoneyRec[] = [];
let vouchers: VoucherRec[] = [];
let statementLines: LineRec[] = [];
const openings = new Map<string, number>();
const today = () => {
  const d = new Date(); // local calendar date, as the Rust core uses
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
};
const nf = (message: string): never => {
  throw { code: "not_found", message, fields: [] } satisfies Fail;
};
const conflict = (message: string): never => {
  throw { code: "conflict", message, fields: [] } satisfies Fail;
};

function ensureMoneyAccounts() {
  if (moneyAccounts.length || !org) return;
  for (const a of accounts.filter((a) => a.systemRole === "cash" || a.systemRole === "bank")) {
    moneyAccounts.push({
      id: id(), organisationId: org.id, accountId: a.id, kind: a.systemRole as MoneyAccountKind, name: a.name,
      bankName: null, accountNumber: null, ifsc: null, statementMapping: null, isActive: true, rowVersion: 1, createdAt: now(), updatedAt: now(),
    });
  }
}

/** Every posting that touches a ledger account, as book lines (oldest first). */
function ledgerLines(accountId: string): Omit<BookLine, "balancePaise">[] {
  const out: Omit<BookLine, "balancePaise">[] = [];
  const opening = openings.get(accountId) ?? 0;
  if (opening)
    out.push({ entryId: `open-${accountId}`, entryDate: org!.booksStartDate, sourceType: "account_opening_balance", sourceId: accountId, narration: "Opening balance", debitPaise: Math.max(opening, 0), creditPaise: Math.max(-opening, 0), partyName: null, postedAt: "", statementLineId: null });
  const money = moneyAccounts.find((m) => m.accountId === accountId);
  for (const v of vouchers) {
    const entries: [string, string, number][] = [[v.journalEntryId, v.voucherDate, 1]];
    if (v.cancelEntryId) entries.push([v.cancelEntryId, v.voucherDate, -1]);
    for (const [entryId, date, sign] of entries) {
      let amt = 0;
      const inflow = v.voucherType === "receipt" ? v.amountPaise : -v.amountPaise;
      if (money && v.moneyAccountId === money.id) amt = inflow;
      else if (v.counterAccountId === accountId) amt = -inflow;
      if (!amt) continue;
      amt *= sign;
      out.push({ entryId, entryDate: date, sourceType: v.voucherType, sourceId: v.id, narration: sign < 0 ? `Cancellation of ${v.number}` : `${v.number}`, debitPaise: Math.max(amt, 0), creditPaise: Math.max(-amt, 0), partyName: parties.find((p) => p.id === v.partyId)?.name ?? null, postedAt: v.createdAt, statementLineId: null });
    }
  }
  if (money?.kind && money.accountId === accounts.find((a) => a.systemRole === money.kind)?.id)
    for (const d of purchaseDocs.filter((d) => d.status !== "draft" && d.paymentMode === money.kind))
      out.push({ entryId: `pur-${d.id}`, entryDate: d.docDate, sourceType: "purchase", sourceId: d.id, narration: d.supplierInvoiceNumber ?? "Purchase", debitPaise: 0, creditPaise: d.totalPaise, partyName: d.partyName, postedAt: d.createdAt, statementLineId: null });
  out.sort((a, b) => (a.entryDate + a.postedAt).localeCompare(b.entryDate + b.postedAt));
  for (const l of out) l.statementLineId = statementLines.find((s) => s.journalEntryId === l.entryId)?.id ?? null;
  return out;
}

function moneyView(m: MoneyRec): MoneyAccount {
  const lines = ledgerLines(m.accountId);
  const mine = statementLines.filter((l) => l.moneyAccountId === m.id).sort((a, b) => b.lineDate.localeCompare(a.lineDate));
  const ledger = accounts.find((a) => a.id === m.accountId);
  return {
    ...m,
    ledgerCode: ledger?.code ?? "",
    systemRole: ledger?.systemRole ?? null,
    balancePaise: lines.reduce((a, l) => a + l.debitPaise - l.creditPaise, 0),
    openingBalancePaise: openings.get(m.accountId) ?? 0,
    unmatchedLines: mine.filter((l) => l.status === "unmatched").length,
    lastStatementDate: mine[0]?.lineDate ?? null,
    statementBalancePaise: mine.find((l) => l.balancePaise != null)?.balancePaise ?? null,
  };
}

function checkMoney(input: MoneyAccountInput, exceptId?: string): [string, string][] {
  const e: [string, string][] = [];
  if (!input.name.trim()) e.push(["name", "Give the account a name."]);
  else if (moneyAccounts.some((m) => m.id !== exceptId && m.name.toLowerCase() === input.name.trim().toLowerCase())) e.push(["name", "Another cash or bank account has this name."]);
  if (input.ifsc && !/^[A-Za-z]{4}0[A-Za-z0-9]{6}$/.test(input.ifsc.trim())) e.push(["ifsc", "An IFSC looks like HDFC0001234."]);
  if (input.openingBalance && toPaise(input.openingBalance) == null) e.push(["openingBalance", "Enter an amount like 1250 or 1250.50."]);
  return e;
}

function openDocs(partyId: string, type: VoucherType): OpenDocument[] {
  if (type === "receipt")
    return salesDocs
      .filter((d) => d.partyId === partyId && d.status === "issued" && d.docType !== "credit_note")
      .map((d) => {
        const adjusted = salesDocs.filter((c) => c.originalDocumentId === d.id && c.status === "issued").reduce((a, c) => a + c.totalPaise, 0);
        return { documentKind: "sales" as const, id: d.id, docType: d.docType, number: d.number, docDate: d.docDate, dueDate: d.dueDate, totalPaise: d.totalPaise, paidPaise: d.paidPaise, adjustedPaise: adjusted, outstandingPaise: d.totalPaise - d.paidPaise - adjusted };
      })
      .filter((d) => d.outstandingPaise > 0)
      .sort((a, b) => a.docDate.localeCompare(b.docDate));
  return purchaseDocs
    .filter((d) => d.partyId === partyId && d.status === "recorded" && d.docType !== "debit_note" && d.paymentMode === "credit")
    .map((d) => {
      const adjusted = purchaseDocs.filter((n) => n.originalDocumentId === d.id && n.status === "recorded").reduce((a, n) => a + n.totalPaise, 0);
      return { documentKind: "purchase" as const, id: d.id, docType: d.docType, number: d.supplierInvoiceNumber ?? d.number, docDate: d.supplierInvoiceDate ?? d.docDate, dueDate: d.dueDate, totalPaise: d.totalPaise, paidPaise: d.paidPaise, adjustedPaise: adjusted, outstandingPaise: d.totalPaise - d.paidPaise - adjusted };
    })
    .filter((d) => d.outstandingPaise > 0)
    .sort((a, b) => a.docDate.localeCompare(b.docDate));
}

function adjustPaid(kind: "sales" | "purchase", docId: string, delta: number) {
  if (kind === "sales") salesDocs = salesDocs.map((d) => (d.id === docId ? { ...d, paidPaise: d.paidPaise + delta, rowVersion: d.rowVersion + 1 } : d));
  else purchaseDocs = purchaseDocs.map((d) => (d.id === docId ? { ...d, paidPaise: d.paidPaise + delta, rowVersion: d.rowVersion + 1 } : d));
}

function voucherView(vid: string): VoucherView {
  const v = vouchers.find((x) => x.id === vid) ?? nf("Receipt or payment not found");
  const m = moneyAccounts.find((x) => x.id === v.moneyAccountId)!;
  const allocations = v.allocations.map((a) => {
    const s = salesDocs.find((d) => d.id === a.documentId);
    const p = purchaseDocs.find((d) => d.id === a.documentId);
    return { ...a, number: s?.number ?? p?.supplierInvoiceNumber ?? p?.number ?? null, docDate: s?.docDate ?? p?.docDate ?? null };
  });
  const allocated = allocations.reduce((a, x) => a + x.amountPaise, 0);
  return {
    ...v,
    moneyAccountName: m.name,
    moneyAccountKind: m.kind,
    partyName: parties.find((p) => p.id === v.partyId)?.name ?? null,
    counterAccountName: accounts.find((a) => a.id === v.counterAccountId)?.name ?? null,
    allocations,
    allocatedPaise: allocated,
    onAccountPaise: v.partyId ? v.amountPaise - allocated : 0,
    statementLineId: statementLines.find((l) => l.journalEntryId === v.journalEntryId)?.id ?? null,
  };
}

function createVoucher(input: VoucherInput): VoucherView {
  const e: [string, string][] = [];
  if (!input.voucherDate) e.push(["voucherDate", "Enter a valid date."]);
  else if (input.voucherDate > today()) e.push(["voucherDate", "The date can't be in the future."]);
  const m = moneyAccounts.find((x) => x.id === input.moneyAccountId);
  if (!m) e.push(["moneyAccountId", "Choose a cash or bank account."]);
  const amount = toPaise(input.amount);
  if (!amount || amount <= 0) e.push(["amount", "Enter an amount."]);
  if (input.partyId && input.counterAccountId) e.push(["partyId", "Choose a party or an account, not both."]);
  if (!input.partyId && !input.counterAccountId) e.push(["partyId", "Choose who the money is from or to."]);
  const open = input.partyId ? openDocs(input.partyId, input.voucherType) : [];
  const allocs: VoucherRec["allocations"] = [];
  input.allocations.forEach((a, i) => {
    const amt = toPaise(a.amount) ?? 0;
    if (!amt) return;
    const d = open.find((x) => x.id === a.documentId);
    if (!d) e.push([`allocations.${i}.amount`, "This document is already settled or isn't this party's."]);
    else if (amt > d.outstandingPaise) e.push([`allocations.${i}.amount`, "More than is outstanding on this document."]);
    else allocs.push({ documentKind: d.documentKind, documentId: d.id, amountPaise: amt });
  });
  if (amount && allocs.reduce((a, x) => a + x.amountPaise, 0) > amount) e.push(["allocations", "The amounts settled add up to more than the total."]);
  const line = input.statementLineId ? statementLines.find((l) => l.id === input.statementLineId) : undefined;
  if (line && amount) {
    const signed = line.depositPaise - line.withdrawalPaise;
    if ((input.voucherType === "receipt" ? amount : -amount) !== signed) e.push(["amount", "The amount and direction must match the statement line."]);
  }
  if (e.length) fail(e);
  const type = input.voucherType;
  const fyCode = shortFy();
  const seq = vouchers.filter((v) => v.voucherType === type && v.financialYear === fyCode).length + 1;
  const v: VoucherRec = {
    id: id(), organisationId: org!.id, voucherType: type, status: "posted",
    number: `${type === "receipt" ? "RCT" : "PMT"}/${fyCode}/${String(seq).padStart(4, "0")}`,
    financialYear: fyCode, seq, voucherDate: input.voucherDate, moneyAccountId: m!.id,
    partyId: input.partyId || null, counterAccountId: input.counterAccountId || null, amountPaise: amount!,
    mode: m!.kind === "cash" ? "cash" : (input.mode ?? "neft"), reference: blank(input.reference), narration: blank(input.narration),
    journalEntryId: id(), cancelEntryId: null, cancelReason: null, rowVersion: 1, createdAt: now(), updatedAt: now(), cancelledAt: null,
    allocations: allocs,
  };
  vouchers.push(v);
  for (const a of allocs) adjustPaid(a.documentKind, a.documentId, a.amountPaise);
  if (line) statementLines = statementLines.map((l) => (l.id === line.id ? { ...l, status: "matched", journalEntryId: v.journalEntryId, matchedAt: now(), matchedBy: "owner" } : l));
  log(`money_voucher.${type}`, v.id, v);
  return voucherView(v.id);
}

// -- Statements ---------------------------------------------------------------

function parseDate(raw: string, order: StatementMapping["dateOrder"]): string | null {
  const s = raw.trim();
  if (/^\d{4}-\d{2}-\d{2}/.test(s)) return s.slice(0, 10);
  const parts = s.split(/\s+/)[0].split(/[/.-]/);
  if (parts.length !== 3) return null;
  const [d, m, y] = order === "mdy" ? [parts[1], parts[0], parts[2]] : order === "ymd" ? [parts[2], parts[1], parts[0]] : parts;
  const months = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
  const mm = /^\d+$/.test(m) ? Number(m) : months.indexOf(m.slice(0, 3).toLowerCase()) + 1;
  const yy = y.length === 2 ? 2000 + Number(y) : Number(y);
  const dd = Number(d);
  if (!(mm >= 1 && mm <= 12 && dd >= 1 && dd <= 31 && yy > 1900)) return null;
  return `${yy}-${String(mm).padStart(2, "0")}-${String(dd).padStart(2, "0")}`;
}

function parseAmount(raw: string): number | null {
  let s = raw.replace(/[,₹\s]/g, "");
  if (!s || s === "-") return null;
  let sign = 1;
  if (/dr$/i.test(s)) { sign = -1; s = s.slice(0, -2); } else if (/cr$/i.test(s)) s = s.slice(0, -2);
  if (s.startsWith("(") && s.endsWith(")")) { sign = -sign; s = s.slice(1, -1); }
  if (s.startsWith("-")) { sign = -sign; s = s.slice(1); }
  const p = toPaise(s);
  return p && !Number.isNaN(p) ? sign * p : null;
}

function suggestMapping(rows: string[][]): StatementMapping {
  let best: StatementMapping = { headerRow: 0, date: null, description: null, reference: null, withdrawal: null, deposit: null, amount: null, drCr: null, balance: null, dateOrder: "dmy" };
  let bestScore = 0;
  rows.slice(0, 40).forEach((row, i) => {
    const m: StatementMapping = { headerRow: i, date: null, description: null, reference: null, withdrawal: null, deposit: null, amount: null, drCr: null, balance: null, dateOrder: "dmy" };
    let score = 0;
    row.forEach((h, c) => {
      const n = norm(h);
      if (!n) return;
      const has = (...k: string[]) => k.some((x) => n.includes(x));
      if (m.date == null && has("date") && !has("value", "posted")) { m.date = c; score += 2; }
      else if (m.description == null && has("description", "narration", "particular", "remark", "details")) { m.description = c; score += 2; }
      else if (m.withdrawal == null && has("withdrawal", "debit")) { m.withdrawal = c; score += 2; }
      else if (m.deposit == null && has("deposit", "credit")) { m.deposit = c; score += 2; }
      else if (m.drCr == null && ["crdr", "drcr", "type"].includes(n)) { m.drCr = c; score += 1; }
      else if (m.balance == null && has("balance")) { m.balance = c; score += 1; }
      else if (m.amount == null && has("amount")) { m.amount = c; score += 2; }
      else if (m.reference == null && has("chq", "cheque", "ref", "utr", "transactionid")) { m.reference = c; score += 1; }
    });
    if (m.date == null) { const d = row.findIndex((h) => norm(h).includes("date")); if (d >= 0) { m.date = d; score += 1; } }
    if (m.date != null && (m.amount != null || m.withdrawal != null || m.deposit != null) && score > bestScore) { best = m; bestScore = score; }
  });
  if (best.amount == null) best.drCr = null;
  return best;
}

function parseStatement(rows: string[][], m: StatementMapping): StatementPreview {
  if (m.date == null) fail([["mapping", "Choose the date column."]]);
  if (m.description == null) fail([["mapping", "Choose the description column."]]);
  if (m.amount == null && m.withdrawal == null && m.deposit == null) fail([["mapping", "Choose the withdrawal and deposit columns, or a single amount column."]]);
  const lines: ParsedStatementLine[] = [];
  const skipped: StatementPreview["skipped"] = [];
  const cell = (r: string[], c: number | null) => (c == null ? "" : (r[c] ?? "").trim());
  rows.forEach((r, i) => {
    if (i <= m.headerRow || r.every((c) => !c.trim())) return;
    const date = parseDate(cell(r, m.date), m.dateOrder);
    if (!date) return void skipped.push({ row: i + 1, reason: "no date" });
    let signed: number | null;
    if (m.amount != null) {
      const a = parseAmount(cell(r, m.amount));
      if (a != null && m.drCr != null) {
        const t = cell(r, m.drCr).toLowerCase();
        signed = t.startsWith("d") ? -Math.abs(a) : t.startsWith("c") ? Math.abs(a) : null;
      } else signed = a;
    } else {
      const w = parseAmount(cell(r, m.withdrawal));
      const d = parseAmount(cell(r, m.deposit));
      signed = w != null && d == null ? -Math.abs(w) : d != null && w == null ? Math.abs(d) : null;
    }
    if (signed == null) return void skipped.push({ row: i + 1, reason: "no amount" });
    const ref = cell(r, m.reference);
    lines.push({
      row: i + 1, lineDate: date, description: cell(r, m.description) || "(no description)", reference: ref && ref !== "-" ? ref : null,
      withdrawalPaise: signed < 0 ? -signed : 0, depositPaise: Math.max(signed, 0),
      balancePaise: m.balance == null ? null : parseAmount(cell(r, m.balance)), duplicate: false,
    });
  });
  return { lines, skipped, newLines: 0, duplicates: 0, depositsPaise: 0, withdrawalsPaise: 0, firstDate: null, lastDate: null };
}

const fingerprint = (moneyId: string, l: ParsedStatementLine, n: number) =>
  [moneyId, l.lineDate, l.depositPaise - l.withdrawalPaise, l.description.toLowerCase().replace(/[^a-z0-9]/g, ""), l.reference ?? "", l.balancePaise ?? "", n].join("|");

function withFingerprints(moneyId: string, lines: ParsedStatementLine[]) {
  const seen = new Map<string, number>();
  return lines.map((l) => {
    const base = fingerprint(moneyId, l, 0);
    const n = (seen.get(base) ?? 0) + 1;
    seen.set(base, n);
    return { line: l, fp: fingerprint(moneyId, l, n) };
  });
}

function candidate(b: Omit<BookLine, "balancePaise">, lineDate?: string): MatchCandidate {
  const gap = lineDate ? Math.round((Date.parse(b.entryDate) - Date.parse(lineDate)) / 86_400_000) : 0;
  return { entryId: b.entryId, entryDate: b.entryDate, sourceType: b.sourceType, sourceId: b.sourceId, narration: b.narration, partyName: b.partyName, amountPaise: b.debitPaise - b.creditPaise, dayGap: gap };
}

function reconciliationView(moneyId: string): Reconciliation {
  const m = moneyAccounts.find((x) => x.id === moneyId) ?? nf("Account not found");
  const book = ledgerLines(m.accountId);
  const free = book.filter((b) => !b.statementLineId && b.debitPaise - b.creditPaise !== 0 && !b.sourceType.endsWith("opening_balance"));
  const lines = statementLines
    .filter((l) => l.moneyAccountId === moneyId)
    .sort((a, b) => b.lineDate.localeCompare(a.lineDate))
    .map((l) => {
      const signed = l.depositPaise - l.withdrawalPaise;
      const matchedEntry = book.find((b) => b.entryId === l.journalEntryId);
      return {
        ...l,
        matched: matchedEntry ? candidate(matchedEntry, l.lineDate) : null,
        suggestions: l.status === "unmatched"
          ? free.filter((b) => b.debitPaise - b.creditPaise === signed).map((b) => candidate(b, l.lineDate)).filter((c) => Math.abs(c.dayGap) <= 7).sort((a, b) => Math.abs(a.dayGap) - Math.abs(b.dayGap)).slice(0, 5)
          : [],
      };
    });
  return { account: moneyView(m), lines, unmatchedEntries: free.reverse().map((b) => candidate(b)) };
}

function lineState(lineId: string, status: StatementLineStatus, entryId: string | null = null) {
  const l = statementLines.find((x) => x.id === lineId) ?? nf("Statement line not found");
  if (status !== "unmatched" && l.status !== "unmatched") conflict("This line is already reconciled or ignored.");
  statementLines = statementLines.map((x) => (x.id === lineId ? { ...x, status, journalEntryId: entryId, matchedAt: status === "unmatched" ? null : now(), matchedBy: status === "unmatched" ? null : "owner" } : x));
  log(`bank_statement_line.${status === "matched" ? "match" : status === "ignored" ? "ignore" : "unmatch"}`, lineId);
  return null;
}

function bankingCommand(command: string, args: Record<string, any>): unknown {
  ensureMoneyAccounts();
  switch (command) {
    case "list_money_accounts":
      return moneyAccounts.map(moneyView);
    case "create_money_account": {
      const input = args.input as MoneyAccountInput;
      const e = checkMoney(input);
      if (e.length) fail(e);
      const used = new Set(accounts.map((a) => a.code));
      let code = input.kind === "cash" ? 1001 : 1011;
      while (used.has(String(code))) code++;
      const ledger: Account = { id: id(), code: String(code), name: input.name.trim(), kind: "asset", systemRole: null, isActive: true };
      accounts.push(ledger);
      const m: MoneyRec = {
        id: id(), organisationId: org!.id, accountId: ledger.id, kind: input.kind, name: input.name.trim(),
        bankName: input.kind === "bank" ? blank(input.bankName) : null, accountNumber: input.kind === "bank" ? (blank(input.accountNumber)?.replace(/\s/g, "") ?? null) : null,
        ifsc: input.kind === "bank" ? (blank(input.ifsc)?.toUpperCase() ?? null) : null, statementMapping: null, isActive: true, rowVersion: 1, createdAt: now(), updatedAt: now(),
      };
      moneyAccounts.push(m);
      const ob = (toPaise(input.openingBalance) ?? 0) * (input.openingBalanceSide === "credit" ? -1 : 1);
      if (ob) openings.set(ledger.id, ob);
      log("money_account.create", m.id, m);
      return moneyView(m);
    }
    case "update_money_account": {
      const input = args.input as MoneyAccountInput;
      const cur = moneyAccounts.find((m) => m.id === args.id) ?? nf("Account not found");
      if (cur.rowVersion !== args.expectedVersion) conflict("This record was changed elsewhere. Reload it and try again.");
      const e = checkMoney(input, cur.id);
      const active = input.isActive ?? cur.isActive;
      if (!active && cur.isActive && moneyView(cur).balancePaise !== 0) e.push(["isActive", "Bring the balance to zero (transfer it) before closing this account."]);
      if (e.length) fail(e);
      const next: MoneyRec = {
        ...cur, name: input.name.trim(), bankName: cur.kind === "bank" ? blank(input.bankName) : null,
        accountNumber: cur.kind === "bank" ? (blank(input.accountNumber)?.replace(/\s/g, "") ?? null) : null,
        ifsc: cur.kind === "bank" ? (blank(input.ifsc)?.toUpperCase() ?? null) : null, isActive: active, rowVersion: cur.rowVersion + 1, updatedAt: now(),
      };
      moneyAccounts = moneyAccounts.map((m) => (m.id === cur.id ? next : m));
      accounts = accounts.map((a) => (a.id === cur.accountId ? { ...a, name: next.name } : a));
      openings.set(cur.accountId, (toPaise(input.openingBalance) ?? 0) * (input.openingBalanceSide === "credit" ? -1 : 1));
      log("money_account.update", cur.id, next, cur);
      return moneyView(next);
    }
    case "list_opening_balances":
      return accounts
        .filter((a) => ["asset", "liability", "equity"].includes(a.kind) && !["receivables", "payables", "opening_balance"].includes(a.systemRole ?? ""))
        .map((a) => ({ accountId: a.id, code: a.code, name: a.name, kind: a.kind, systemRole: a.systemRole, amountPaise: openings.get(a.id) ?? 0 }));
    case "set_opening_balance": {
      const input = args.input as OpeningBalanceInput;
      const a = accounts.find((x) => x.id === input.accountId) ?? nf("Account not found");
      if (input.amount && toPaise(input.amount) == null) fail([["amount", "Enter an amount like 1250 or 1250.50."]]);
      const amt = (toPaise(input.amount) ?? 0) * (input.side === "credit" ? -1 : 1);
      openings.set(a.id, amt);
      log("account.opening_balance", a.id, { amountPaise: amt });
      return { accountId: a.id, code: a.code, name: a.name, kind: a.kind, systemRole: a.systemRole, amountPaise: amt };
    }
    case "voucher_accounts":
      return accounts.filter((a) => a.isActive && !["receivables", "payables", "opening_balance", "round_off"].includes(a.systemRole ?? ""));
    case "open_documents":
      return openDocs(args.partyId, args.voucherType);
    case "list_vouchers":
      return [...vouchers]
        .sort((a, b) => (b.voucherDate + b.createdAt).localeCompare(a.voucherDate + a.createdAt))
        .map((v) => ({
          id: v.id, voucherType: v.voucherType, status: v.status, number: v.number, voucherDate: v.voucherDate,
          moneyAccountId: v.moneyAccountId, moneyAccountName: moneyAccounts.find((m) => m.id === v.moneyAccountId)?.name ?? "",
          partyId: v.partyId, counterparty: parties.find((p) => p.id === v.partyId)?.name ?? accounts.find((a) => a.id === v.counterAccountId)?.name ?? "",
          amountPaise: v.amountPaise, mode: v.mode, reference: v.reference,
        }));
    case "get_voucher":
      return voucherView(args.id);
    case "document_settlements":
      return vouchers
        .filter((v) => v.status === "posted")
        .flatMap((v) => v.allocations.filter((a) => a.documentKind === args.documentKind && a.documentId === args.documentId).map((a) => ({ voucherId: v.id, voucherType: v.voucherType, number: v.number, voucherDate: v.voucherDate, amountPaise: a.amountPaise })));
    case "create_voucher":
      return createVoucher(args.input as VoucherInput);
    case "cancel_voucher": {
      const v = vouchers.find((x) => x.id === args.id) ?? nf("Receipt or payment not found");
      if (v.status !== "posted") conflict("This is already cancelled.");
      if (v.rowVersion !== args.expectedVersion) conflict("This record was changed elsewhere. Reload it and try again.");
      if (!String(args.reason ?? "").trim()) fail([["reason", "Give a reason for cancelling."]]);
      if (statementLines.some((l) => l.journalEntryId === v.journalEntryId)) conflict("This is reconciled with a bank statement line. Unmatch it first.");
      for (const a of v.allocations) adjustPaid(a.documentKind, a.documentId, -a.amountPaise);
      vouchers = vouchers.map((x) => (x.id === v.id ? { ...x, status: "cancelled", cancelEntryId: id(), cancelReason: args.reason, cancelledAt: now(), rowVersion: x.rowVersion + 1 } : x));
      log("money_voucher.cancel", v.id);
      return voucherView(v.id);
    }
    case "money_book": {
      const m = moneyAccounts.find((x) => x.id === args.moneyAccountId) ?? nf("Account not found");
      const all = ledgerLines(m.accountId);
      const from: string | null = args.from ?? null;
      const to: string | null = args.to ?? null;
      const opening = from ? all.filter((l) => l.entryDate < from).reduce((a, l) => a + l.debitPaise - l.creditPaise, 0) : 0;
      let bal = opening;
      const lines = all
        .filter((l) => (!from || l.entryDate >= from) && (!to || l.entryDate <= to))
        .map((l) => ({ ...l, balancePaise: (bal += l.debitPaise - l.creditPaise) }));
      return {
        account: moneyView(m), from, to, openingPaise: opening,
        debitTotalPaise: lines.reduce((a, l) => a + l.debitPaise, 0), creditTotalPaise: lines.reduce((a, l) => a + l.creditPaise, 0),
        closingPaise: bal, lines,
      } satisfies MoneyBook;
    }
    case "reconciliation":
      return reconciliationView(args.moneyAccountId);
    case "match_statement_line": {
      const l = statementLines.find((x) => x.id === args.lineId) ?? nf("Statement line not found");
      const m = moneyAccounts.find((x) => x.id === l.moneyAccountId)!;
      const entry = ledgerLines(m.accountId).find((b) => b.entryId === args.entryId);
      if (!entry) fail([["entryId", "That entry doesn't touch this bank account."]]);
      if (entry!.debitPaise - entry!.creditPaise !== l.depositPaise - l.withdrawalPaise) fail([["entryId", "The entry's amount or direction differs from the statement line."]]);
      return lineState(l.id, "matched", args.entryId);
    }
    case "unmatch_statement_line":
      return lineState(args.lineId, "unmatched");
    case "ignore_statement_line":
      return lineState(args.lineId, "ignored");
    case "undo_statement_import": {
      const before = statementLines.length;
      statementLines = statementLines.filter((l) => !(l.moneyAccountId === args.moneyAccountId && l.importBatch === args.batch && l.status !== "matched"));
      log("bank_statement.undo_import", args.moneyAccountId, { removed: before - statementLines.length });
      return before - statementLines.length;
    }
    default:
      return undefined;
  }
}

async function statementCommand(command: string, args: Record<string, any>): Promise<unknown> {
  switch (command) {
    case "read_statement_file": {
      const name = String(args.fileName);
      if (!/\.(csv|txt)$/i.test(name)) throw { code: "invalid", message: "The browser preview reads .csv statements only; the desktop app also reads .xls and .xlsx.", fields: [] } satisfies Fail;
      const rows = parseCsv(new TextDecoder().decode(new Uint8Array(args.bytes))).filter((r) => r.some((c) => c.trim())).map((r) => r.map((c) => c.trim()));
      const m = moneyAccounts.find((x) => x.id === args.moneyAccountId) ?? nf("Account not found");
      const saved = m.statementMapping ? (JSON.parse(m.statementMapping) as StatementMapping) : null;
      return { fileName: name, rows, mapping: saved ?? suggestMapping(rows), mappingSaved: !!saved } satisfies StatementFile;
    }
    case "preview_statement":
    case "import_statement": {
      const input = args.input as StatementImportInput;
      const m = moneyAccounts.find((x) => x.id === input.moneyAccountId) ?? nf("Account not found");
      const p = parseStatement(input.rows, input.mapping);
      const existing = new Set(statementLines.filter((l) => l.moneyAccountId === m.id).map((l) => l.fingerprint));
      const fps = withFingerprints(m.id, p.lines);
      if (command === "preview_statement") {
        for (const { line, fp } of fps) line.duplicate = existing.has(fp);
        const fresh = p.lines.filter((l) => !l.duplicate);
        const dates = p.lines.map((l) => l.lineDate).sort();
        return { ...p, newLines: fresh.length, duplicates: p.lines.length - fresh.length, depositsPaise: fresh.reduce((a, l) => a + l.depositPaise, 0), withdrawalsPaise: fresh.reduce((a, l) => a + l.withdrawalPaise, 0), firstDate: dates[0] ?? null, lastDate: dates.at(-1) ?? null } satisfies StatementPreview;
      }
      if (m.kind !== "bank") throw { code: "invalid", message: "Statements are imported into bank accounts.", fields: [] } satisfies Fail;
      if (!p.lines.length) throw { code: "invalid", message: "No transactions found with this column mapping.", fields: [] } satisfies Fail;
      const batch = id();
      let imported = 0;
      for (const { line, fp } of fps) {
        if (existing.has(fp)) continue;
        imported++;
        statementLines.push({
          id: id(), organisationId: org!.id, moneyAccountId: m.id, lineDate: line.lineDate, description: line.description, reference: line.reference,
          withdrawalPaise: line.withdrawalPaise, depositPaise: line.depositPaise, balancePaise: line.balancePaise, fingerprint: fp, importBatch: batch,
          sourceFile: input.fileName, status: "unmatched", journalEntryId: null, matchedAt: null, matchedBy: null, createdAt: now(),
        });
      }
      moneyAccounts = moneyAccounts.map((x) => (x.id === m.id ? { ...x, statementMapping: JSON.stringify(input.mapping) } : x));
      log("bank_statement.import", m.id, { file: input.fileName, imported });
      return { batch, imported, duplicates: p.lines.length - imported, skipped: p.skipped.length } satisfies StatementImportResult;
    }
    default:
      return bankingCommand(command, args);
  }
}


// ---------------------------------------------------------------------------
// Ledger and reports (Phase 1.6) — the stand-in rebuilds a journal from the
// in-memory documents so report screens can be reviewed in a browser.
// ---------------------------------------------------------------------------

type PEntry = { id: string; date: string; sourceType: string; sourceId: string | null; narration: string | null; postedAt: string; reverses: string | null; lines: { accountId: string; debit: number; credit: number; partyId: string | null }[] };

let booksLock: BooksLock | null = null;
let fyCloses: FyClose[] = [];
let closingEntries: PEntry[] = [];
let logo: LogoContent | null = null;

const roleId = (role: string) => accounts.find((a) => a.systemRole === role)?.id ?? "";

function mkEntry(id: string, date: string, sourceType: string, sourceId: string | null, narration: string | null, postedAt: string, lines: PEntry["lines"], reverses: string | null = null): PEntry {
  return { id, date, sourceType, sourceId, narration, postedAt, reverses, lines: lines.filter((l) => l.debit || l.credit) };
}
const flip = (lines: PEntry["lines"]) => lines.map((l) => ({ ...l, debit: l.credit, credit: l.debit }));
const tax = (d: { cgstPaise: number; sgstPaise: number; utgstPaise: number; igstPaise: number; cessPaise: number }) => d.cgstPaise + d.sgstPaise + d.utgstPaise + d.igstPaise + d.cessPaise;

function journal(): PEntry[] {
  const out: PEntry[] = [];
  const start = org?.booksStartDate ?? "1970-01-01";
  for (const p of parties) {
    const ob = (p as Party & { openingBalancePaise?: number }).openingBalancePaise ?? 0;
    if (!ob) continue;
    const control = roleId(ob > 0 ? "receivables" : "payables");
    out.push(mkEntry(`pob-${p.id}`, start, "party_opening_balance", p.id, `Opening balance — ${p.name}`, "", [
      { accountId: control, debit: Math.max(ob, 0), credit: Math.max(-ob, 0), partyId: p.id },
      { accountId: roleId("opening_balance"), debit: Math.max(-ob, 0), credit: Math.max(ob, 0), partyId: null },
    ]));
  }
  for (const [accountId, ob] of openings) {
    if (!ob) continue;
    out.push(mkEntry(`aob-${accountId}`, start, "account_opening_balance", accountId, "Opening balance", "", [
      { accountId, debit: Math.max(ob, 0), credit: Math.max(-ob, 0), partyId: null },
      { accountId: roleId("opening_balance"), debit: Math.max(-ob, 0), credit: Math.max(ob, 0), partyId: null },
    ]));
  }
  for (const d of salesDocs.filter((x) => x.status !== "draft")) {
    const credit = d.docType === "credit_note";
    let lines: PEntry["lines"] = [
      { accountId: roleId("receivables"), debit: d.totalPaise, credit: 0, partyId: d.partyId },
      { accountId: roleId("sales"), debit: 0, credit: d.taxablePaise, partyId: null },
      { accountId: roleId("output_igst"), debit: 0, credit: tax(d), partyId: null },
      { accountId: roleId("round_off"), debit: Math.max(-d.roundOffPaise, 0), credit: Math.max(d.roundOffPaise, 0), partyId: null },
    ];
    if (credit) lines = flip(lines);
    const src = credit ? "sales_credit_note" : "sales_invoice";
    out.push(mkEntry(`s-${d.id}`, d.docDate, src, d.id, d.number, d.createdAt, lines));
    if (d.status === "cancelled") out.push(mkEntry(`sc-${d.id}`, d.docDate, src, d.id, `Cancellation of ${d.number}`, d.updatedAt, flip(lines), `s-${d.id}`));
  }
  for (const d of purchaseDocs.filter((x) => x.status !== "draft")) {
    const pay = d.paymentMode === "credit" ? roleId("payables") : roleId(d.paymentMode);
    let lines: PEntry["lines"] = [
      ...d.lines.map((l) => ({ accountId: l.accountId, debit: l.taxablePaise, credit: 0, partyId: null })),
      { accountId: roleId("input_igst"), debit: tax(d), credit: 0, partyId: null },
      { accountId: roleId("round_off"), debit: Math.max(d.roundOffPaise, 0), credit: Math.max(-d.roundOffPaise, 0), partyId: null },
      { accountId: pay, debit: 0, credit: d.totalPaise, partyId: d.paymentMode === "credit" ? d.partyId : null },
    ];
    if (d.docType === "debit_note") lines = flip(lines);
    const src = d.docType === "debit_note" ? "purchase_debit_note" : "purchase";
    out.push(mkEntry(`p-${d.id}`, d.docDate, src, d.id, d.supplierInvoiceNumber ?? d.number, d.createdAt, lines));
    if (d.status === "cancelled") out.push(mkEntry(`pc-${d.id}`, d.docDate, src, d.id, "Cancellation", d.updatedAt, flip(lines), `p-${d.id}`));
  }
  for (const v of vouchers) {
    const m = moneyAccounts.find((x) => x.id === v.moneyAccountId)!;
    const other = v.partyId ? roleId(v.voucherType === "receipt" ? "receivables" : "payables") : v.counterAccountId!;
    const inflow = v.voucherType === "receipt";
    const lines: PEntry["lines"] = [
      { accountId: m.accountId, debit: inflow ? v.amountPaise : 0, credit: inflow ? 0 : v.amountPaise, partyId: null },
      { accountId: other, debit: inflow ? 0 : v.amountPaise, credit: inflow ? v.amountPaise : 0, partyId: v.partyId },
    ];
    out.push(mkEntry(v.journalEntryId, v.voucherDate, v.voucherType, v.id, v.narration ?? v.number, v.createdAt, lines));
    if (v.cancelEntryId) out.push(mkEntry(v.cancelEntryId, v.voucherDate, v.voucherType, v.id, `Cancellation of ${v.number}`, v.cancelledAt ?? "", flip(lines), v.journalEntryId));
  }
  out.push(...closingEntries);
  return out.sort((a, b) => (a.date + a.postedAt + a.id).localeCompare(b.date + b.postedAt + b.id));
}

function totals(from: string | null, to: string | null, exclude?: string) {
  const map = new Map(accounts.map((a) => [a.id, { account: a, opening: 0, debit: 0, credit: 0 }]));
  for (const e of journal()) {
    if (exclude && e.sourceType === exclude) continue;
    for (const l of e.lines) {
      const t = map.get(l.accountId);
      if (!t) continue;
      if (from && e.date < from) t.opening += l.debit - l.credit;
      else if (!to || e.date <= to) {
        t.debit += l.debit;
        t.credit += l.credit;
      }
    }
  }
  return [...map.values()].sort((a, b) => a.account.code.localeCompare(b.account.code));
}

function bookOf(pred: (l: PEntry["lines"][number]) => boolean, from: string | null, to: string | null): Ledger {
  let opening = 0;
  const lines: BookLine[] = [];
  let bal = 0;
  for (const e of journal()) {
    const mine = e.lines.filter(pred);
    if (!mine.length) continue;
    const dr = mine.reduce((a, l) => a + l.debit, 0);
    const cr = mine.reduce((a, l) => a + l.credit, 0);
    if (from && e.date < from) {
      opening += dr - cr;
      continue;
    }
    if (to && e.date > to) continue;
    if (!lines.length) bal = opening;
    bal += dr - cr;
    const partyId = e.lines.find((l) => l.partyId)?.partyId;
    lines.push({ entryId: e.id, entryDate: e.date, sourceType: e.sourceType, sourceId: e.sourceId, narration: e.narration, debitPaise: dr, creditPaise: cr, partyName: parties.find((p) => p.id === partyId)?.name ?? null, postedAt: e.postedAt, statementLineId: null, balancePaise: bal });
  }
  const dr = lines.reduce((a, l) => a + l.debitPaise, 0);
  const cr = lines.reduce((a, l) => a + l.creditPaise, 0);
  return { id: "", title: "", subtitle: "", ledgerType: "account", from, to, openingPaise: opening, debitTotalPaise: dr, creditTotalPaise: cr, closingPaise: opening + dr - cr, lines };
}

const lineOf = (t: { account: Account }, amount: number): ReportLine => ({ accountId: t.account.id, code: t.account.code, name: t.account.name, amountPaise: amount });
const sectionOf = (key: string, title: string, lines: ReportLine[]): ReportSection => ({ key, title, lines, totalPaise: lines.reduce((a, l) => a + l.amountPaise, 0) });

function yearsList(): YearStatus[] {
  if (!org) return [];
  const todayIso = today();
  const startMonth = org.fyStartMonth;
  const [by, bm] = org.booksStartDate.split("-").map(Number);
  let y = bm >= startMonth ? by : by - 1;
  const out: YearStatus[] = [];
  for (;;) {
    const start = `${y}-${String(startMonth).padStart(2, "0")}-01`;
    if (start > todayIso) break;
    const endD = new Date(y + 1, startMonth - 1, 0);
    const end = `${endD.getFullYear()}-${String(endD.getMonth() + 1).padStart(2, "0")}-${String(endD.getDate()).padStart(2, "0")}`;
    const code = startMonth === 1 ? String(y) : `${y}-${String((y + 1) % 100).padStart(2, "0")}`;
    const close = fyCloses.find((c) => c.financialYear === code && c.status === "closed") ?? null;
    out.push({ code, label: `FY ${code}`, startDate: start, endDate: end, status: close ? "closed" : "open", ended: end < todayIso, close });
    y++;
  }
  return out.reverse();
}

const control = (): BooksControl => ({ lock: booksLock, booksStartDate: org?.booksStartDate ?? "", years: yearsList(), history: [...fyCloses].reverse() });

function reportCommand(command: string, args: Record<string, any>): unknown {
  switch (command) {
    case "trial_balance": {
      const rows = totals(args.from, args.to)
        .filter((t) => t.opening || t.debit || t.credit)
        .map((t) => ({ accountId: t.account.id, code: t.account.code, name: t.account.name, kind: t.account.kind, systemRole: t.account.systemRole, openingPaise: t.opening, debitPaise: t.debit, creditPaise: t.credit, closingPaise: t.opening + t.debit - t.credit }));
      const sum = (f: (r: TrialBalanceRow) => number) => rows.reduce((a, r) => a + f(r), 0);
      const r: TrialBalance = {
        from: args.from ?? null, to: args.to ?? null, rows,
        openingDebitPaise: sum((r) => Math.max(r.openingPaise, 0)), openingCreditPaise: sum((r) => Math.max(-r.openingPaise, 0)),
        debitPaise: sum((r) => r.debitPaise), creditPaise: sum((r) => r.creditPaise),
        closingDebitPaise: sum((r) => Math.max(r.closingPaise, 0)), closingCreditPaise: sum((r) => Math.max(-r.closingPaise, 0)), balanced: true,
      };
      r.balanced = r.debitPaise === r.creditPaise && r.closingDebitPaise === r.closingCreditPaise;
      return r;
    }
    case "profit_and_loss": {
      const ts = totals(args.from, args.to, "year_close").filter((t) => t.debit !== t.credit);
      const mv = (t: (typeof ts)[number]) => t.debit - t.credit;
      const sections = [
        sectionOf("trading_income", "Sales", ts.filter((t) => t.account.kind === "income" && t.account.systemRole === "sales").map((t) => lineOf(t, -mv(t)))),
        sectionOf("direct_costs", "Cost of sales", ts.filter((t) => t.account.kind === "expense" && t.account.code.startsWith("5")).map((t) => lineOf(t, mv(t)))),
        sectionOf("other_income", "Other income", ts.filter((t) => t.account.kind === "income" && t.account.systemRole !== "sales").map((t) => lineOf(t, -mv(t)))),
        sectionOf("operating_expenses", "Operating expenses", ts.filter((t) => t.account.kind === "expense" && !t.account.code.startsWith("5")).map((t) => lineOf(t, mv(t)))),
      ];
      const gross = sections[0].totalPaise - sections[1].totalPaise;
      return { from: args.from, to: args.to, sections, grossProfitPaise: gross, netProfitPaise: gross + sections[2].totalPaise - sections[3].totalPaise } satisfies ProfitAndLoss;
    }
    case "balance_sheet": {
      const ts = totals(null, args.asAt);
      const bal = (t: (typeof ts)[number]) => t.debit - t.credit;
      const assets = sectionOf("assets", "Assets", ts.filter((t) => t.account.kind === "asset" && bal(t)).map((t) => lineOf(t, bal(t))));
      const liabilities = sectionOf("liabilities", "Liabilities", ts.filter((t) => t.account.kind === "liability" && bal(t)).map((t) => lineOf(t, -bal(t))));
      const equity = sectionOf("equity", "Capital and reserves", ts.filter((t) => t.account.kind === "equity" && bal(t)).map((t) => lineOf(t, -bal(t))));
      const profit = -ts.filter((t) => t.account.kind === "income" || t.account.kind === "expense").reduce((a, t) => a + bal(t), 0);
      const right = liabilities.totalPaise + equity.totalPaise + profit;
      return { asAt: args.asAt, assets, liabilities, equity, profitPaise: profit, totalAssetsPaise: assets.totalPaise, totalLiabilitiesAndEquityPaise: right, balanced: assets.totalPaise === right } satisfies BalanceSheet;
    }
    case "general_ledger": {
      const a = accounts.find((x) => x.id === args.accountId) ?? nf("Account not found");
      return { ...bookOf((l) => l.accountId === a.id, args.from, args.to), id: a.id, title: a.name, subtitle: `${a.code} · ${a.kind}`, ledgerType: "account" } satisfies Ledger;
    }
    case "party_ledger": {
      const p = parties.find((x) => x.id === args.partyId) ?? nf("Party not found");
      return { ...bookOf((l) => l.partyId === p.id, args.from, args.to), id: p.id, title: p.name, subtitle: p.gstin ?? p.kind, ledgerType: "party" } satisfies Ledger;
    }
    case "journal_register": {
      const entries = journal()
        .filter((e) => (!args.from || e.date >= args.from) && (!args.to || e.date <= args.to))
        .map((e) => ({
          entryId: e.id, entryDate: e.date, sourceType: e.sourceType, sourceId: e.sourceId, narration: e.narration, reversesEntryId: e.reverses, postedAt: e.postedAt, postedBy: "owner",
          totalPaise: e.lines.reduce((a, l) => a + l.debit, 0),
          lines: e.lines.map((l) => {
            const a = accounts.find((x) => x.id === l.accountId);
            return { accountId: l.accountId, accountCode: a?.code ?? "", accountName: a?.name ?? "", debitPaise: l.debit, creditPaise: l.credit, partyName: parties.find((p) => p.id === l.partyId)?.name ?? null };
          }),
        }));
      return { from: args.from ?? null, to: args.to ?? null, entries: entries.slice(0, 1000), truncated: entries.length > 1000 } satisfies JournalRegister;
    }
    case "ageing": {
      const kind = args.kind as "receivables" | "payables";
      const asOf: string = args.asOf;
      const map = new Map<string, AgeingParty>();
      const docs = kind === "receivables"
        ? salesDocs.filter((d) => d.status === "issued" && d.docType !== "credit_note" && d.docDate <= asOf).map((d) => ({ kind: "sales" as const, d, number: d.number, date: d.docDate, adjusted: salesDocs.filter((c) => c.originalDocumentId === d.id && c.status === "issued").reduce((a, c) => a + c.totalPaise, 0) }))
        : purchaseDocs.filter((d) => d.status === "recorded" && d.docType !== "debit_note" && d.paymentMode === "credit" && d.docDate <= asOf).map((d) => ({ kind: "purchase" as const, d, number: d.supplierInvoiceNumber ?? d.number, date: d.supplierInvoiceDate ?? d.docDate, adjusted: purchaseDocs.filter((n) => n.originalDocumentId === d.id && n.status === "recorded").reduce((a, n) => a + n.totalPaise, 0) }));
      for (const { kind: dk, d, number, date, adjusted } of docs) {
        const outstanding = d.totalPaise - d.paidPaise - adjusted;
        if (outstanding <= 0 || !d.partyId) continue;
        const days = Math.round((Date.parse(asOf) - Date.parse(d.dueDate ?? date)) / 86_400_000);
        const bucket = days <= 0 ? 0 : days <= 30 ? 1 : days <= 60 ? 2 : days <= 90 ? 3 : 4;
        const p = map.get(d.partyId) ?? { partyId: d.partyId, partyName: d.partyName ?? "", buckets: [0, 0, 0, 0, 0], unallocatedPaise: 0, totalPaise: 0, documents: [] };
        p.buckets[bucket] += outstanding;
        p.totalPaise += outstanding;
        p.documents.push({ documentKind: dk, id: d.id, number, docDate: date, dueDate: d.dueDate, totalPaise: d.totalPaise, outstandingPaise: outstanding, daysOverdue: days, bucket });
        map.set(d.partyId, p);
      }
      const list = [...map.values()].sort((a, b) => b.totalPaise - a.totalPaise);
      const buckets = [0, 1, 2, 3, 4].map((i) => list.reduce((a, p) => a + p.buckets[i], 0));
      return { kind, asOf, bucketLabels: ["Not due", "1–30 days", "31–60 days", "61–90 days", "Over 90 days"], parties: list, buckets, unallocatedPaise: 0, totalPaise: list.reduce((a, p) => a + p.totalPaise, 0) } satisfies Ageing;
    }
    case "return_centre":
      return previewReturnCentre(args.registrationId ?? null, args.financialYear ?? null);
    case "gstr1":
      return previewWorkpaper(args.registrationId, args.period);
    case "return_filing":
      return previewReturnFiling(args.registrationId, args.period, args.returnType);
    case "record_filing":
      return previewRecordFiling(args.registrationId, args.period, args.returnType, args.input);
    case "remove_filing":
      return previewRemoveFiling(args.registrationId, args.period, args.returnType, args.reason);
    case "save_filing_pack":
    case "save_compliance_pack":
      throw { code: "invalid_input", message: "Saving the pack runs in the desktop app.", fields: [] } satisfies Fail;
    case "gstr2b":
      return previewGstr2b(args.registrationId, args.period);
    case "import_gstr2b":
      return previewImportGstr2b(args.registrationId, args.fileName, args.bytes);
    case "decide_gstr2b":
      return previewDecideGstr2b(args.registrationId, args.period, args.input);
    case "save_gstr2b_xlsx":
      throw { code: "invalid_input", message: "Saving the Excel report runs in the desktop app.", fields: [] } satisfies Fail;
    case "gstr3b":
      return previewGstr3b(args.registrationId, args.period);
    case "save_gstr3b_xlsx":
      throw { code: "invalid_input", message: "Saving the Excel workpaper runs in the desktop app.", fields: [] } satisfies Fail;
    case "save_gstr1_json":
      throw { code: "invalid_input", message: "Saving the GSTR-1 file runs in the desktop app.", fields: [] } satisfies Fail;
    case "books_control":
      return control();
    case "set_books_lock": {
      const until: string | null = args.until ?? null;
      const reopening = !!booksLock && (!until || until < booksLock.lockedUntil);
      if (reopening && !String(args.reason ?? "").trim()) fail([["reason", "Say why the period is being reopened."]]);
      if (until && until > today()) fail([["lockedUntil", "You can't lock a date that hasn't happened yet."]]);
      if (fyCloses.some((c) => c.status === "closed" && (!until || c.endDate > until))) conflict("A closed year stays locked. Reopen it first.");
      booksLock = until ? { organisationId: org!.id, lockedUntil: until, reason: args.reason ?? null, updatedAt: now(), updatedBy: "owner" } : null;
      log(reopening ? "books_lock.reopen" : "books_lock.lock", null, booksLock);
      return control();
    }
    case "close_financial_year": {
      const y = yearsList().find((x) => x.code === args.code) ?? nf("Year not found");
      if (!y.ended) conflict(`${y.code} hasn't ended yet. A year can be closed from the day after it ends.`);
      if (y.status === "closed") conflict(`${y.code} is already closed.`);
      const ts = totals(y.startDate, y.endDate, "year_close").filter((t) => (t.account.kind === "income" || t.account.kind === "expense") && t.debit !== t.credit);
      const profit = -ts.reduce((a, t) => a + t.debit - t.credit, 0);
      const cid = id();
      const lines = ts.map((t) => ({ accountId: t.account.id, debit: Math.max(t.credit - t.debit, 0), credit: Math.max(t.debit - t.credit, 0), partyId: null }));
      lines.push({ accountId: roleId("retained_earnings") || roleId("capital"), debit: Math.max(-profit, 0), credit: Math.max(profit, 0), partyId: null });
      const entry = mkEntry(`yc-${cid}`, y.endDate, "year_close", cid, `Year-end close ${y.code}`, now(), lines);
      closingEntries.push(entry);
      fyCloses.push({ id: cid, organisationId: org!.id, financialYear: y.code, startDate: y.startDate, endDate: y.endDate, status: "closed", entryId: entry.id, profitPaise: profit, closedAt: now(), closedBy: "owner", reopenEntryId: null, reopenReason: null, reopenedAt: null, reopenedBy: null });
      if (!booksLock || booksLock.lockedUntil < y.endDate) booksLock = { organisationId: org!.id, lockedUntil: y.endDate, reason: `Year ${y.code} closed`, updatedAt: now(), updatedBy: "owner" };
      log("financial_year.close", cid, { financialYear: y.code });
      return control();
    }
    case "reopen_financial_year": {
      if (!String(args.reason ?? "").trim()) fail([["reason", "Say why the year is being reopened."]]);
      const c = fyCloses.find((x) => x.financialYear === args.code && x.status === "closed") ?? conflict(`${args.code} isn't closed.`);
      if (fyCloses.some((x) => x.status === "closed" && x.startDate > c.startDate)) conflict("Reopen the later years first; years reopen newest first.");
      const entry = closingEntries.find((e) => e.id === c.entryId);
      if (entry) closingEntries.push(mkEntry(`yr-${c.id}`, c.endDate, "year_close", c.id, `Year ${c.financialYear} reopened`, now(), flip(entry.lines), entry.id));
      fyCloses = fyCloses.map((x) => (x.id === c.id ? { ...x, status: "reopened", reopenEntryId: `yr-${c.id}`, reopenReason: args.reason, reopenedAt: now(), reopenedBy: "owner" } : x));
      const before = new Date(Date.parse(c.startDate) - 86_400_000).toISOString().slice(0, 10);
      booksLock = before >= (org?.booksStartDate ?? "") ? { organisationId: org!.id, lockedUntil: before, reason: `Year ${c.financialYear} reopened: ${args.reason}`, updatedAt: now(), updatedBy: "owner" } : null;
      log("financial_year.reopen", c.id, { reason: args.reason });
      return control();
    }
    case "set_logo": {
      const bytes = args.bytes as number[];
      if (bytes.length > 1024 * 1024) throw { code: "invalid", message: "The logo must be 1 MB or smaller.", fields: [] } satisfies Fail;
      const head = String.fromCharCode(...bytes.slice(0, 200)).toLowerCase();
      const type = bytes[0] === 0x89 && bytes[1] === 0x50 ? "image/png" : bytes[0] === 0xff && bytes[1] === 0xd8 ? "image/jpeg" : head.includes("<svg") ? "image/svg+xml" : null;
      if (!type) throw { code: "invalid", message: "Use a PNG, JPEG or SVG image for the logo.", fields: [] } satisfies Fail;
      logo = { fileName: args.fileName, contentType: type, sizeBytes: bytes.length, bytes };
      log("organisation_logo.update", org?.id ?? null, { fileName: args.fileName });
      return logo;
    }
    case "get_logo":
      return logo;
    case "remove_logo":
      logo = null;
      log("organisation_logo.remove", org?.id ?? null);
      return null;
    default:
      return undefined;
  }
}


// ---------------------------------------------------------------------------
// Phase 1.7 stand-ins: PDFs, backups and the app lock run in the desktop host.
// The preview simulates them so the screens can be exercised in a browser.
// ---------------------------------------------------------------------------

let lockCfg: { pin: string; recovery: string; idleMinutes: number } | null = null;
let locked = false;
let lockFailures = 0;
let backupLog: BackupLog[] = [];
let lastBackupPassphrase: string | null = null;
let restoreChosen = false;
let restoreChecked = false;
const LOCK_FREE = new Set(["app_session", "lock_app", "unlock_app", "unlock_with_recovery"]);

function lockStatus(): LockStatus {
  return { enabled: !!lockCfg, locked, idleMinutes: lockCfg?.idleMinutes ?? 0 };
}

function pinError(pin: string): string | null {
  const digits = /^\d+$/.test(pin);
  if (digits && (pin.length < 4 || pin.length > 12)) return "Use 4 to 12 digits, or a password of at least 6 characters.";
  if (!digits && (pin.length < 6 || pin.length > 64)) return "A password needs 6 to 64 characters; a PIN needs 4 to 12 digits.";
  return null;
}

function backupStatusView() {
  const last = backupLog.find((b) => b.kind === "backup") ?? null;
  const days = last ? Math.floor((Date.now() - Date.parse(last.happenedAt)) / 86_400_000) : null;
  return { lastBackup: last, lastRestore: backupLog.find((b) => b.kind === "restore") ?? null, daysSinceBackup: days, due: days === null || days >= 7 };
}

function checkPin(pin: string, field = "pin") {
  if (lockFailures >= 5) throw { code: "throttled", message: "Too many wrong attempts. Try again in 30 seconds.", fields: [] } satisfies Fail;
  if (!lockCfg || pin !== lockCfg.pin) {
    lockFailures++;
    fail([[field, "That PIN isn't right."]]);
  }
  lockFailures = 0;
}

function businessName(): string {
  return org ? org.tradeName ?? org.legalName : "Khope Traders";
}

function hostCommand(command: string, args: Record<string, any>): unknown {
  switch (command) {
    case "app_session":
      return { lock: lockStatus(), restoredKeptCopy: null };
    case "lock_app":
      if (lockCfg) locked = true;
      return lockStatus();
    case "unlock_app":
      checkPin(args.pin);
      locked = false;
      return lockStatus();
    case "unlock_with_recovery": {
      const norm = (x: string) => x.toUpperCase().replace(/[^A-Z0-9]/g, "");
      if (!lockCfg || norm(args.code ?? "") !== norm(lockCfg.recovery)) fail([["code", "That recovery code isn't right."]]);
      lockCfg = null;
      locked = false;
      return lockStatus();
    }
    case "set_app_lock": {
      if (lockCfg) checkPin(args.currentPin ?? "", "currentPin");
      const e = pinError(args.pin ?? "");
      if (e) fail([["pin", e]]);
      const chars = "23456789ABCDEFGHJKMNPQRSTUVWXYZ";
      const group = () => Array.from({ length: 4 }, () => chars[Math.floor(Math.random() * chars.length)]).join("");
      const code = [group(), group(), group(), group()].join("-");
      lockCfg = { pin: args.pin, recovery: code, idleMinutes: args.idleMinutes };
      return { status: lockStatus(), recoveryCode: code };
    }
    case "set_lock_timeout":
      if (lockCfg) lockCfg.idleMinutes = args.idleMinutes;
      return lockStatus();
    case "remove_app_lock":
      checkPin(args.pin);
      lockCfg = null;
      locked = false;
      return lockStatus();
    case "save_sales_pdf":
    case "open_sales_pdf": {
      const d = salesDocs.find((x) => x.id === args.id);
      if (!d) throw { code: "not_found", message: "Document not found", fields: [] } satisfies Fail;
      if (command === "open_sales_pdf") return null;
      const stem = d.number ?? (d.docType === "credit_note" ? "Credit-note-draft" : "Tax-invoice-draft");
      const fileName = stem.replace(/[^A-Za-z0-9_-]+/g, "-") + ".pdf";
      return { path: "C:\\Users\\you\\Documents\\" + fileName, fileName, sizeBytes: 48_213 };
    }
    case "open_saved_file":
      return null;
    case "backup_status":
      return backupStatusView();
    case "create_backup": {
      if (!org) throw { code: "not_set_up", message: "set up your business first", fields: [] } satisfies Fail;
      if ((args.passphrase ?? "").length < 8) fail([["passphrase", "Use at least 8 characters."]]);
      lastBackupPassphrase = args.passphrase;
      const name = businessName().replace(/[^A-Za-z0-9]+/g, "-").replace(/-+$/, "");
      const t = new Date();
      const pad = (n: number) => String(n).padStart(2, "0");
      const fileName = name + "-" + t.getFullYear() + "-" + pad(t.getMonth() + 1) + "-" + pad(t.getDate()) + "-" + pad(t.getHours()) + pad(t.getMinutes()) + ".avbackup";
      const rec: BackupLog = { id: id(), kind: "backup", happenedAt: now(), fileName, sizeBytes: 912_384, sha256: null, actor: "owner" };
      backupLog.unshift(rec);
      log("backup.create", rec.id, rec);
      return { file: { path: "C:\\Users\\you\\Documents\\" + fileName, fileName, sizeBytes: rec.sizeBytes }, status: backupStatusView() };
    }
    case "choose_backup_file": {
      restoreChosen = true;
      restoreChecked = false;
      const last = backupLog.find((b) => b.kind === "backup");
      return {
        fileName: last?.fileName ?? "Khope-Traders-2026-11-18-1830.avbackup",
        sizeBytes: last?.sizeBytes ?? 912_384,
        header: { format: 1, createdAt: last?.happenedAt ?? now(), appVersion: "0.1.0", schemaVersion: 8, businessName: businessName(), databaseBytes: 901_120 },
      };
    }
    case "verify_backup": {
      if (!restoreChosen) throw { code: "not_found", message: "Choose a backup file first.", fields: [] } satisfies Fail;
      if (!args.passphrase || (lastBackupPassphrase !== null && args.passphrase !== lastBackupPassphrase))
        fail([["passphrase", "That passphrase doesn't open this backup, or the file has been changed."]]);
      restoreChecked = true;
      return {
        header: { format: 1, createdAt: now(), appVersion: "0.1.0", schemaVersion: 8, businessName: businessName(), databaseBytes: 901_120 },
        check: {
          integrity: "ok", foreignKeyProblems: 0, schemaVersion: 8, businessName: businessName(),
          parties: parties.length, items: items.length, salesDocuments: salesDocs.length, purchaseDocuments: purchaseDocs.length,
          vouchers: vouchers.length, journalEntries: salesDocs.filter((d) => d.status !== "draft").length, unbalancedEntries: 0, attachments: 0,
          lastEntryDate: salesDocs.map((d) => d.docDate).sort().at(-1) ?? null,
        },
      };
    }
    case "apply_restore":
      if (!restoreChecked) throw { code: "conflict", message: "Check the backup with its passphrase first.", fields: [] } satisfies Fail;
      throw { code: "invalid_input", message: "The browser preview can't restart. Restoring runs in the desktop app.", fields: [] } satisfies Fail;
    case "check_update":
      return {
        currentVersion: "0.2.8",
        version: "0.3.0",
        date: new Date(Date.now() - 86_400_000).toISOString(),
        notes: "AccountsVU 0.3.0 — sample release notes from the browser preview.",
        canInstall: true,
      };
    case "open_releases_page":
      return null;
    case "cancel_restore":
      restoreChosen = false;
      restoreChecked = false;
      return null;
  }
  return undefined;
}

export async function previewBackend(command: string, args: Record<string, any> = {}): Promise<unknown> {
  await new Promise((r) => setTimeout(r, 120)); // feel like real IPC
  // Mirrors the desktop host's gate: while locked only the unlock commands pass.
  if (locked && !LOCK_FREE.has(command)) throw { code: "locked", message: "AccountsVU is locked. Enter your PIN to continue.", fields: [] } satisfies Fail;
  const hosted = hostCommand(command, args);
  if (hosted !== undefined) return hosted;
  switch (command) {
    case "app_health":
      return { appVersion: "0.2.8", schemaVersion: 12, database: "(browser preview — in memory)", organisationConfigured: !!org };
    case "dashboard_summary":
      return {
        salesThisMonth: "0", purchasesThisMonth: "0",
        cashAndBank: (() => {
          ensureMoneyAccounts();
          const p = moneyAccounts.reduce((a, m) => a + moneyView(m).balancePaise, 0);
          return (p / 100).toFixed(2);
        })(), customerDues: "0", estimatedGstPayable: "0",
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
      const r = masters(command, args) ?? salesCommand(command, args) ?? (await purchaseCommand(command, args)) ?? (await statementCommand(command, args)) ?? reportCommand(command, args);
      if (r !== undefined) return r;
      throw { code: "not_found", message: `Unknown command ${command}`, fields: [] } satisfies Fail;
    }
  }
}

/** Simulated download for the browser preview; it can't install or restart. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function previewInstallUpdate(onProgress: (p: any) => void): Promise<void> {
  const total = 12_400_000;
  onProgress({ event: "started", contentLength: total });
  for (let got = 0; got < total; ) {
    await new Promise((r) => setTimeout(r, 90));
    got = Math.min(total, got + 620_000);
    onProgress({ event: "progress", downloaded: got });
  }
  onProgress({ event: "finished" });
  await new Promise((r) => setTimeout(r, 400));
  throw { code: "invalid_input", message: "The browser preview can't install updates. Updates install in the desktop app.", fields: [] };
}
