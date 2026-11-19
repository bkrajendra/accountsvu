import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { previewBackend, previewInstallUpdate } from "./preview";

/** Money crosses IPC as a decimal string (never a JS float) — see crates/domain/src/money.rs. */
export type MoneyString = string;

export interface HealthInfo {
  appVersion: string;
  schemaVersion: number;
  database: string;
  organisationConfigured: boolean;
}

export interface GstContext {
  gstin: string;
  filingFrequency: "monthly" | "quarterly";
}

export interface DashboardSummary {
  salesThisMonth: MoneyString;
  purchasesThisMonth: MoneyString;
  cashAndBank: MoneyString;
  customerDues: MoneyString;
  estimatedGstPayable: MoneyString;
  registrations: GstContext[];
  organisationConfigured: boolean;
}

export interface Organisation {
  id: string;
  legalName: string;
  tradeName: string | null;
  pan: string | null;
  constitution: string | null;
  addressLine1: string | null;
  addressLine2: string | null;
  city: string | null;
  pincode: string | null;
  stateCode: number;
  email: string | null;
  phone: string | null;
  fyStartMonth: number;
  booksStartDate: string;
  roundToRupee: boolean;
  rowVersion: number;
  createdAt: string;
  updatedAt: string;
}

export interface Registration {
  id: string;
  organisationId: string;
  gstin: string;
  stateCode: number;
  registrationType: string;
  filingFrequency: "monthly" | "quarterly";
  signatoryName: string | null;
  effectiveFrom: string;
  effectiveTo: string | null;
  status: "active" | "cancelled" | "suspended";
  rowVersion: number;
  createdAt: string;
  updatedAt: string | null;
}

export interface FinancialYearInfo {
  code: string;
  label: string;
  startDate: string;
  endDate: string;
}

export interface BusinessProfile {
  organisation: Organisation | null;
  registrations: Registration[];
  currentFinancialYear: FinancialYearInfo;
  nextFinancialYear: FinancialYearInfo;
  accountCount: number;
  hasPostings: boolean;
}

export interface Series {
  id: string;
  organisationId: string;
  gstRegistrationId: string | null;
  documentType: string;
  financialYear: string;
  prefix: string;
  suffix: string;
  padding: number;
  nextNumber: number;
  isDefault: boolean;
  rowVersion: number;
  createdAt: string;
  updatedAt: string;
  gstin: string | null;
  nextPreview: string;
  locked: boolean;
}

export interface Account {
  id: string;
  code: string;
  name: string;
  kind: "asset" | "liability" | "equity" | "income" | "expense";
  systemRole: string | null;
  isActive: boolean;
}

export interface AuditEvent {
  id: string;
  occurredAt: string;
  actor: string;
  action: string;
  entityType: string;
  entityId: string | null;
  reason: string | null;
  beforeJson: string | null;
  afterJson: string | null;
}

export interface OrganisationInput {
  legalName: string;
  tradeName?: string;
  pan?: string;
  constitution: string;
  addressLine1?: string;
  addressLine2?: string;
  city?: string;
  pincode?: string;
  stateCode: number;
  email?: string;
  phone?: string;
  fyStartMonth: number;
  booksStartDate: string;
  roundToRupee: boolean;
}

export interface RegistrationInput {
  gstin: string;
  registrationType: string;
  filingFrequency: string;
  effectiveFrom: string;
  signatoryName?: string;
}

export interface UpdateRegistrationInput {
  registrationType: string;
  filingFrequency: string;
  effectiveFrom: string;
  effectiveTo?: string;
  signatoryName?: string;
  status: string;
}

export interface SetupBusinessInput {
  organisation: OrganisationInput;
  registration?: RegistrationInput;
  invoicePrefix?: string;
}

export interface SeriesInput {
  gstRegistrationId?: string;
  documentType: string;
  financialYear: string;
  prefix: string;
  suffix: string;
  padding: number;
  nextNumber: number;
  isDefault: boolean;
}

export type SeriesUpdateInput = Pick<SeriesInput, "prefix" | "suffix" | "padding" | "nextNumber" | "isDefault">;

// ---------------------------------------------------------------------------
// Masters (Phase 1.2)
// ---------------------------------------------------------------------------

export type PartyKind = "customer" | "supplier" | "both";
export type PartyGstType = "regular" | "composition" | "unregistered" | "consumer" | "overseas" | "sez";

export interface Party {
  id: string;
  organisationId: string;
  kind: PartyKind;
  name: string;
  legalName: string | null;
  gstType: PartyGstType;
  gstin: string | null;
  pan: string | null;
  stateCode: number;
  addressLine1: string | null;
  addressLine2: string | null;
  city: string | null;
  pincode: string | null;
  shippingSame: boolean;
  shippingLine1: string | null;
  shippingLine2: string | null;
  shippingCity: string | null;
  shippingPincode: string | null;
  shippingStateCode: number | null;
  contactPerson: string | null;
  email: string | null;
  phone: string | null;
  paymentTermsDays: number | null;
  creditLimitPaise: number | null;
  /** Signed: positive = they owe us. */
  openingBalancePaise: number;
  openingEntryId: string | null;
  notes: string | null;
  isActive: boolean;
  rowVersion: number;
  createdAt: string;
  updatedAt: string;
  /** Current ledger balance in paise; positive = they owe us. */
  balancePaise: number;
}

export interface PartyInput {
  kind: PartyKind;
  name: string;
  legalName?: string;
  gstType: PartyGstType;
  gstin?: string;
  pan?: string;
  stateCode?: number;
  addressLine1?: string;
  addressLine2?: string;
  city?: string;
  pincode?: string;
  shippingSame: boolean;
  shippingLine1?: string;
  shippingLine2?: string;
  shippingCity?: string;
  shippingPincode?: string;
  shippingStateCode?: number;
  contactPerson?: string;
  email?: string;
  phone?: string;
  paymentTermsDays?: number;
  creditLimit?: MoneyString;
  openingBalance?: MoneyString;
  openingBalanceSide?: "receivable" | "payable";
  notes?: string;
  isActive: boolean;
}

export type ItemKind = "goods" | "service";
export type TaxCategory = "taxable" | "exempt" | "nil_rated" | "non_gst";

export interface Item {
  id: string;
  organisationId: string;
  kind: ItemKind;
  name: string;
  code: string | null;
  description: string | null;
  hsnSac: string | null;
  unit: string;
  taxCategory: TaxCategory;
  gstRateBp: number;
  cessRateBp: number;
  salesPricePaise: number | null;
  purchasePricePaise: number | null;
  priceIncludesTax: boolean;
  trackStock: boolean;
  isActive: boolean;
  rowVersion: number;
  createdAt: string;
  updatedAt: string;
}

export interface ItemInput {
  kind: ItemKind;
  name: string;
  code?: string;
  description?: string;
  hsnSac?: string;
  unit?: string;
  taxCategory: TaxCategory;
  /** Percent as a decimal string, e.g. "18". */
  gstRate?: string;
  cessRate?: string;
  salesPrice?: MoneyString;
  purchasePrice?: MoneyString;
  priceIncludesTax: boolean;
  trackStock: boolean;
  isActive: boolean;
}

export interface ItemOptions {
  units: { code: string; label: string }[];
  gstRates: { basisPoints: number; legacy: boolean }[];
}

// ---------------------------------------------------------------------------
// Import (Phase 1.2)
// ---------------------------------------------------------------------------

export type ImportKind = "parties" | "items";

export interface ImportField {
  key: string;
  label: string;
  required: boolean;
  hint: string;
}

export interface ImportTable {
  fileName: string;
  headers: string[];
  rows: string[][];
}

/** Field key → zero-based column index. */
export type ImportMapping = Record<string, number>;

export interface ParsedImport {
  table: ImportTable;
  fields: ImportField[];
  mapping: ImportMapping;
}

export interface RowIssue {
  /** Spreadsheet row number; the header is row 1. */
  row: number;
  label: string;
  errors: { field: string; message: string }[];
}

export interface ImportPreview {
  total: number;
  valid: number;
  issues: RowIssue[];
}

export interface ImportResult {
  created: number;
  skipped: RowIssue[];
}

export interface ImportTemplate {
  fields: ImportField[];
  fileName: string;
  csv: string;
}

// ---------------------------------------------------------------------------
// Sales (Phase 1.3)
// ---------------------------------------------------------------------------

export type SalesDocType = "tax_invoice" | "bill_of_supply" | "credit_note";
export type SalesDisplayStatus = "draft" | "issued" | "overdue" | "part_paid" | "paid" | "cancelled";
export type SupplyKind = "intra_state" | "intra_union_territory" | "inter_state";

export interface SalesDocument {
  id: string;
  organisationId: string;
  docType: SalesDocType;
  status: "draft" | "issued" | "cancelled";
  gstRegistrationId: string | null;
  seriesId: string | null;
  financialYear: string | null;
  number: string | null;
  docDate: string;
  dueDate: string | null;
  partyId: string;
  partyName: string;
  partyGstin: string | null;
  partyGstType: PartyGstType;
  partyAddress: string | null;
  placeOfSupply: number;
  supplierState: number;
  supplyKind: SupplyKind;
  zeroRated: boolean;
  reverseCharge: boolean;
  pricesIncludeTax: boolean;
  originalDocumentId: string | null;
  reason: string | null;
  notes: string | null;
  taxablePaise: number;
  cgstPaise: number;
  sgstPaise: number;
  utgstPaise: number;
  igstPaise: number;
  cessPaise: number;
  roundOffPaise: number;
  totalPaise: number;
  paidPaise: number;
  journalEntryId: string | null;
  cancelEntryId: string | null;
  cancelReason: string | null;
  rowVersion: number;
  createdAt: string;
  updatedAt: string;
  issuedAt: string | null;
  cancelledAt: string | null;
}

export interface SalesLine {
  id: string;
  documentId: string;
  lineNo: number;
  itemId: string | null;
  description: string;
  hsnSac: string | null;
  unit: string | null;
  quantity: string;
  unitPricePaise: number;
  discountPaise: number;
  taxCategory: TaxCategory;
  gstRateBp: number;
  cessRateBp: number;
  amountPaise: number;
  taxablePaise: number;
  cgstPaise: number;
  sgstPaise: number;
  utgstPaise: number;
  igstPaise: number;
  cessPaise: number;
  totalPaise: number;
}

export interface SalesSummary extends SalesDocument {
  displayStatus: SalesDisplayStatus;
}

export interface DocumentRef {
  id: string;
  docType: SalesDocType;
  number: string | null;
  docDate: string;
  status: SalesDocument["status"];
  totalPaise: number;
}

export interface SalesDocumentView extends SalesDocument {
  displayStatus: SalesDisplayStatus;
  lines: SalesLine[];
  original: DocumentRef | null;
  creditNotes: DocumentRef[];
  creditedPaise: number;
}

export interface SalesLineInput {
  itemId?: string;
  description: string;
  hsnSac?: string;
  unit?: string;
  quantity: string;
  unitPrice: string;
  discount?: string;
  taxCategory: TaxCategory;
  gstRate?: string;
  cessRate?: string;
}

export interface SalesDocumentInput {
  docType: SalesDocType;
  partyId: string;
  docDate: string;
  dueDate?: string;
  gstRegistrationId?: string;
  placeOfSupply?: number;
  zeroRated: boolean;
  pricesIncludeTax: boolean;
  originalDocumentId?: string;
  reason?: string;
  notes?: string;
  lines: SalesLineInput[];
}

export interface LineCalc {
  amountPaise: number;
  taxablePaise: number;
  cgstPaise: number;
  sgstPaise: number;
  utgstPaise: number;
  igstPaise: number;
  cessPaise: number;
  totalPaise: number;
}

export interface TotalsCalc {
  taxablePaise: number;
  cgstPaise: number;
  sgstPaise: number;
  utgstPaise: number;
  igstPaise: number;
  cessPaise: number;
  roundOffPaise: number;
  totalPaise: number;
}

export interface SalesCalculation {
  supplyKind: SupplyKind | null;
  placeOfSupply: number | null;
  supplierState: number | null;
  lines: (LineCalc | null)[];
  totals: TotalsCalc;
  numberPreview: string | null;
}

// ---------------------------------------------------------------------------
// Purchases (Phase 1.4)
// ---------------------------------------------------------------------------

export type PurchaseDocType = "bill" | "debit_note" | "expense";
export type PurchaseStatus = "draft" | "unpaid" | "overdue" | "part_paid" | "paid" | "recorded" | "cancelled";
export type PaymentMode = "credit" | "cash" | "bank";
export type ItcStatus = "na" | "pending" | "eligible" | "ineligible" | "partial";

export interface PurchaseDocument {
  id: string;
  organisationId: string;
  docType: PurchaseDocType;
  status: "draft" | "recorded" | "cancelled";
  gstRegistrationId: string | null;
  seriesId: string | null;
  number: string | null;
  financialYear: string | null;
  supplierInvoiceNumber: string | null;
  supplierInvoiceDate: string | null;
  docDate: string;
  dueDate: string | null;
  partyId: string | null;
  partyName: string | null;
  partyGstin: string | null;
  partyGstType: PartyGstType | null;
  placeOfSupply: number;
  supplierState: number;
  supplyKind: SupplyKind;
  reverseCharge: boolean;
  pricesIncludeTax: boolean;
  paymentMode: PaymentMode;
  originalDocumentId: string | null;
  reason: string | null;
  notes: string | null;
  taxablePaise: number;
  cgstPaise: number;
  sgstPaise: number;
  utgstPaise: number;
  igstPaise: number;
  cessPaise: number;
  roundOffPaise: number;
  totalPaise: number;
  paidPaise: number;
  journalEntryId: string | null;
  cancelEntryId: string | null;
  cancelReason: string | null;
  rowVersion: number;
  createdAt: string;
  updatedAt: string;
  recordedAt: string | null;
  cancelledAt: string | null;
}

export interface PurchaseLine {
  id: string;
  documentId: string;
  lineNo: number;
  itemId: string | null;
  accountId: string;
  description: string;
  hsnSac: string | null;
  unit: string | null;
  quantity: string;
  unitPricePaise: number;
  discountPaise: number;
  taxCategory: TaxCategory;
  gstRateBp: number;
  cessRateBp: number;
  amountPaise: number;
  taxablePaise: number;
  cgstPaise: number;
  sgstPaise: number;
  utgstPaise: number;
  igstPaise: number;
  cessPaise: number;
  totalPaise: number;
  itcStatus: ItcStatus;
  itcReason: string | null;
  itcEligiblePaise: number;
  itcReviewedAt: string | null;
  itcReviewedBy: string | null;
}

export interface PurchaseSummary extends PurchaseDocument {
  displayStatus: PurchaseStatus;
  itcPendingLines: number;
}

export interface PurchaseRef {
  id: string;
  docType: PurchaseDocType;
  number: string | null;
  supplierInvoiceNumber: string | null;
  docDate: string;
  status: PurchaseDocument["status"];
  totalPaise: number;
}

export interface Attachment {
  id: string;
  entityType: string;
  entityId: string;
  fileName: string;
  contentType: string;
  sizeBytes: number;
  sha256: string;
  uploadedAt: string;
  uploadedBy: string;
}

export interface PurchaseDocumentView extends PurchaseDocument {
  displayStatus: PurchaseStatus;
  lines: PurchaseLine[];
  original: PurchaseRef | null;
  debitNotes: PurchaseRef[];
  debitedPaise: number;
  attachments: Attachment[];
}

export interface PurchaseLineInput {
  itemId?: string;
  accountId: string;
  description: string;
  hsnSac?: string;
  unit?: string;
  quantity: string;
  unitPrice: string;
  discount?: string;
  taxCategory: TaxCategory;
  gstRate?: string;
  cessRate?: string;
  itcStatus?: Exclude<ItcStatus, "na">;
  itcReason?: string;
  itcEligible?: string;
}

export interface PurchaseDocumentInput {
  docType: PurchaseDocType;
  partyId?: string;
  supplierInvoiceNumber?: string;
  supplierInvoiceDate?: string;
  docDate: string;
  dueDate?: string;
  gstRegistrationId?: string;
  paymentMode?: PaymentMode;
  reverseCharge: boolean;
  pricesIncludeTax: boolean;
  originalDocumentId?: string;
  reason?: string;
  notes?: string;
  lines: PurchaseLineInput[];
}

export interface PurchaseCalculation {
  supplyKind: SupplyKind | null;
  lines: (LineCalc | null)[];
  totals: TotalsCalc;
  reverseChargeTaxPaise: number;
  numberPreview: string | null;
}

export interface AttachmentContent {
  fileName: string;
  contentType: string;
  bytes: number[];
}

export interface ItcReviewInput {
  status: Exclude<ItcStatus, "na">;
  reason?: string;
  eligible?: string;
}

// ---------------------------------------------------------------------------
// Banking (Phase 1.5)
// ---------------------------------------------------------------------------

export type MoneyAccountKind = "cash" | "bank";
export type VoucherType = "receipt" | "payment";
export type VoucherMode = "cash" | "upi" | "neft" | "rtgs" | "imps" | "cheque" | "card" | "other";
export type StatementLineStatus = "unmatched" | "matched" | "ignored";

export interface MoneyAccount {
  id: string;
  organisationId: string;
  accountId: string;
  kind: MoneyAccountKind;
  name: string;
  bankName: string | null;
  accountNumber: string | null;
  ifsc: string | null;
  statementMapping: string | null;
  isActive: boolean;
  rowVersion: number;
  createdAt: string;
  updatedAt: string;
  ledgerCode: string;
  systemRole: string | null;
  balancePaise: number;
  openingBalancePaise: number;
  unmatchedLines: number;
  lastStatementDate: string | null;
  statementBalancePaise: number | null;
}

export interface MoneyAccountInput {
  kind: MoneyAccountKind;
  name: string;
  bankName?: string;
  accountNumber?: string;
  ifsc?: string;
  openingBalance?: string;
  openingBalanceSide?: "debit" | "credit";
  isActive?: boolean;
}

export interface OpeningBalance {
  accountId: string;
  code: string;
  name: string;
  kind: string;
  systemRole: string | null;
  amountPaise: number;
}

export interface OpeningBalanceInput {
  accountId: string;
  amount?: string;
  side?: "debit" | "credit";
}

export interface OpenDocument {
  documentKind: "sales" | "purchase";
  id: string;
  docType: string;
  number: string | null;
  docDate: string;
  dueDate: string | null;
  totalPaise: number;
  paidPaise: number;
  adjustedPaise: number;
  outstandingPaise: number;
}

export interface AllocationInput {
  documentId: string;
  amount: string;
}

export interface VoucherInput {
  voucherType: VoucherType;
  voucherDate: string;
  moneyAccountId: string;
  partyId?: string;
  counterAccountId?: string;
  amount: string;
  mode?: VoucherMode;
  reference?: string;
  narration?: string;
  allocations: AllocationInput[];
  statementLineId?: string;
}

export interface VoucherSummary {
  id: string;
  voucherType: VoucherType;
  status: "posted" | "cancelled";
  number: string;
  voucherDate: string;
  moneyAccountId: string;
  moneyAccountName: string;
  partyId: string | null;
  counterparty: string;
  amountPaise: number;
  mode: VoucherMode;
  reference: string | null;
}

export interface AllocationView {
  documentKind: "sales" | "purchase";
  documentId: string;
  number: string | null;
  docDate: string | null;
  amountPaise: number;
}

export interface VoucherView {
  id: string;
  organisationId: string;
  voucherType: VoucherType;
  status: "posted" | "cancelled";
  number: string;
  financialYear: string;
  seq: number;
  voucherDate: string;
  moneyAccountId: string;
  partyId: string | null;
  counterAccountId: string | null;
  amountPaise: number;
  mode: VoucherMode;
  reference: string | null;
  narration: string | null;
  journalEntryId: string;
  cancelEntryId: string | null;
  cancelReason: string | null;
  rowVersion: number;
  createdAt: string;
  updatedAt: string;
  cancelledAt: string | null;
  moneyAccountName: string;
  moneyAccountKind: MoneyAccountKind;
  partyName: string | null;
  counterAccountName: string | null;
  allocations: AllocationView[];
  allocatedPaise: number;
  onAccountPaise: number;
  statementLineId: string | null;
}

export interface Settlement {
  voucherId: string;
  voucherType: VoucherType;
  number: string;
  voucherDate: string;
  amountPaise: number;
}

export interface BookLine {
  entryId: string;
  entryDate: string;
  sourceType: string;
  sourceId: string | null;
  narration: string | null;
  debitPaise: number;
  creditPaise: number;
  partyName: string | null;
  postedAt: string;
  statementLineId: string | null;
  balancePaise: number;
}

export interface MoneyBook {
  account: MoneyAccount;
  from: string | null;
  to: string | null;
  openingPaise: number;
  debitTotalPaise: number;
  creditTotalPaise: number;
  closingPaise: number;
  lines: BookLine[];
}

export interface StatementMapping {
  headerRow: number;
  date: number | null;
  description: number | null;
  reference: number | null;
  withdrawal: number | null;
  deposit: number | null;
  amount: number | null;
  drCr: number | null;
  balance: number | null;
  dateOrder: "dmy" | "mdy" | "ymd";
}

export interface StatementFile {
  fileName: string;
  rows: string[][];
  mapping: StatementMapping;
  mappingSaved: boolean;
}

export interface StatementImportInput {
  moneyAccountId: string;
  fileName: string;
  rows: string[][];
  mapping: StatementMapping;
}

export interface ParsedStatementLine {
  row: number;
  lineDate: string;
  description: string;
  reference: string | null;
  withdrawalPaise: number;
  depositPaise: number;
  balancePaise: number | null;
  duplicate: boolean;
}

export interface StatementPreview {
  lines: ParsedStatementLine[];
  skipped: { row: number; reason: string }[];
  newLines: number;
  duplicates: number;
  depositsPaise: number;
  withdrawalsPaise: number;
  firstDate: string | null;
  lastDate: string | null;
}

export interface StatementImportResult {
  batch: string;
  imported: number;
  duplicates: number;
  skipped: number;
}

export interface MatchCandidate {
  entryId: string;
  entryDate: string;
  sourceType: string;
  sourceId: string | null;
  narration: string | null;
  partyName: string | null;
  amountPaise: number;
  dayGap: number;
}

export interface StatementLine {
  id: string;
  organisationId: string;
  moneyAccountId: string;
  lineDate: string;
  description: string;
  reference: string | null;
  withdrawalPaise: number;
  depositPaise: number;
  balancePaise: number | null;
  fingerprint: string;
  importBatch: string;
  sourceFile: string | null;
  status: StatementLineStatus;
  journalEntryId: string | null;
  matchedAt: string | null;
  matchedBy: string | null;
  createdAt: string;
  matched: MatchCandidate | null;
  suggestions: MatchCandidate[];
}

export interface Reconciliation {
  account: MoneyAccount;
  lines: StatementLine[];
  unmatchedEntries: MatchCandidate[];
}

// ---------------------------------------------------------------------------
// Ledger and reports (Phase 1.6)
// ---------------------------------------------------------------------------

export interface TrialBalanceRow {
  accountId: string;
  code: string;
  name: string;
  kind: Account["kind"];
  systemRole: string | null;
  openingPaise: number;
  debitPaise: number;
  creditPaise: number;
  closingPaise: number;
}

export interface TrialBalance {
  from: string | null;
  to: string | null;
  rows: TrialBalanceRow[];
  openingDebitPaise: number;
  openingCreditPaise: number;
  debitPaise: number;
  creditPaise: number;
  closingDebitPaise: number;
  closingCreditPaise: number;
  balanced: boolean;
}

export interface ReportLine {
  accountId: string;
  code: string;
  name: string;
  amountPaise: number;
}

export interface ReportSection {
  key: string;
  title: string;
  lines: ReportLine[];
  totalPaise: number;
}

export interface ProfitAndLoss {
  from: string;
  to: string;
  sections: ReportSection[];
  grossProfitPaise: number;
  netProfitPaise: number;
}

export interface BalanceSheet {
  asAt: string;
  assets: ReportSection;
  liabilities: ReportSection;
  equity: ReportSection;
  profitPaise: number;
  totalAssetsPaise: number;
  totalLiabilitiesAndEquityPaise: number;
  balanced: boolean;
}

export interface Ledger {
  id: string;
  title: string;
  subtitle: string;
  ledgerType: "account" | "party";
  from: string | null;
  to: string | null;
  openingPaise: number;
  debitTotalPaise: number;
  creditTotalPaise: number;
  closingPaise: number;
  lines: BookLine[];
}

export interface JournalEntryView {
  entryId: string;
  entryDate: string;
  sourceType: string;
  sourceId: string | null;
  narration: string | null;
  reversesEntryId: string | null;
  postedAt: string;
  postedBy: string | null;
  totalPaise: number;
  lines: { accountId: string; accountCode: string; accountName: string; debitPaise: number; creditPaise: number; partyName: string | null }[];
}

export interface JournalRegister {
  from: string | null;
  to: string | null;
  entries: JournalEntryView[];
  truncated: boolean;
}

export interface AgeingDocument {
  documentKind: "sales" | "purchase";
  id: string;
  number: string | null;
  docDate: string;
  dueDate: string | null;
  totalPaise: number;
  outstandingPaise: number;
  daysOverdue: number;
  bucket: number;
}

export interface AgeingParty {
  partyId: string;
  partyName: string;
  buckets: number[];
  unallocatedPaise: number;
  totalPaise: number;
  documents: AgeingDocument[];
}

export interface Ageing {
  kind: "receivables" | "payables";
  asOf: string;
  bucketLabels: string[];
  parties: AgeingParty[];
  buckets: number[];
  unallocatedPaise: number;
  totalPaise: number;
}

export interface BooksLock {
  organisationId: string;
  lockedUntil: string;
  reason: string | null;
  updatedAt: string;
  updatedBy: string;
}

export interface FyClose {
  id: string;
  organisationId: string;
  financialYear: string;
  startDate: string;
  endDate: string;
  status: "closed" | "reopened";
  entryId: string | null;
  profitPaise: number;
  closedAt: string;
  closedBy: string;
  reopenEntryId: string | null;
  reopenReason: string | null;
  reopenedAt: string | null;
  reopenedBy: string | null;
}

export interface YearStatus extends FinancialYearInfo {
  status: "open" | "closed";
  ended: boolean;
  close: FyClose | null;
}

export interface BooksControl {
  lock: BooksLock | null;
  booksStartDate: string;
  years: YearStatus[];
  history: FyClose[];
}

export interface LogoContent {
  fileName: string;
  contentType: string;
  sizeBytes: number;
  bytes: number[];
}

/** Error shape produced by src-tauri/src/commands.rs. */
export class ApiError extends Error {
  readonly code: string;
  readonly fields: Record<string, string>;

  constructor(raw: { code?: string; message?: string; fields?: { field: string; message: string }[] }) {
    super(raw.message ?? "Something went wrong.");
    this.code = raw.code ?? "unknown";
    this.fields = Object.fromEntries((raw.fields ?? []).map((f) => [f.field, f.message]));
  }
}

export const runningInTauri = isTauri();

/**
 * Typed IPC wrapper. Outside the desktop shell (plain `pnpm dev` in a
 * browser) an in-memory preview backend answers instead, so the UI can be
 * developed standalone. It is never bundled into a code path the desktop uses.
 */
// ---------------------------------------------------------------------------
// Phase 1.7: documents, backup, app lock
// ---------------------------------------------------------------------------

export interface SavedFile {
  path: string;
  fileName: string;
  sizeBytes: number;
}

export interface BackupLog {
  id: string;
  kind: "backup" | "restore";
  happenedAt: string;
  fileName: string;
  sizeBytes: number;
  sha256: string | null;
  actor: string | null;
}

export interface BackupStatus {
  lastBackup: BackupLog | null;
  lastRestore: BackupLog | null;
  daysSinceBackup: number | null;
  due: boolean;
}

export interface BackupHeader {
  format: number;
  createdAt: string;
  appVersion: string;
  schemaVersion: number;
  businessName: string;
  databaseBytes: number;
}

export interface ChosenBackup {
  fileName: string;
  sizeBytes: number;
  header: BackupHeader;
}

export interface DatabaseCheck {
  integrity: string;
  foreignKeyProblems: number;
  schemaVersion: number;
  businessName: string | null;
  parties: number;
  items: number;
  salesDocuments: number;
  purchaseDocuments: number;
  vouchers: number;
  journalEntries: number;
  unbalancedEntries: number;
  attachments: number;
  lastEntryDate: string | null;
}

export interface RestoreCheck {
  header: BackupHeader;
  check: DatabaseCheck;
}

export interface LockStatus {
  enabled: boolean;
  locked: boolean;
  idleMinutes: number;
}

export interface AppSession {
  lock: LockStatus;
  /** Set once after a restore: the file the replaced data was kept in. */
  restoredKeptCopy: string | null;
}

export const IDLE_CHOICES = [0, 1, 5, 15, 30, 60] as const;

export interface UpdateInfo {
  currentVersion: string;
  version: string;
  date: string | null;
  notes: string | null;
  /** False where the app can't replace itself (Linux .deb/.rpm). */
  canInstall: boolean;
}

export type UpdateProgress =
  | { event: "started"; contentLength: number | null }
  | { event: "progress"; downloaded: number }
  | { event: "finished" };

/** Fired when the host reports the app is locked. */
export const LOCKED_EVENT = "accountsvu:locked";

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    if (!runningInTauri) return (await previewBackend(command, args)) as T;
    return await invoke<T>(command, args);
  } catch (e) {
    const err =
      e instanceof ApiError
        ? e
        : typeof e === "object" && e !== null
          ? new ApiError(e as ConstructorParameters<typeof ApiError>[0])
          : new ApiError({ message: String(e) });
    // The host refuses every command while the app is locked; show the lock screen.
    if (err.code === "locked") window.dispatchEvent(new Event(LOCKED_EVENT));
    throw err;
  }
}

export const api = {
  health: () => call<HealthInfo>("app_health"),
  dashboardSummary: (monthStart: string, monthEnd: string) =>
    call<DashboardSummary>("dashboard_summary", { monthStart, monthEnd }),

  businessProfile: () => call<BusinessProfile>("business_profile"),
  setupBusiness: (input: SetupBusinessInput) => call<BusinessProfile>("setup_business", { input }),
  updateOrganisation: (input: OrganisationInput, expectedVersion: number) =>
    call<BusinessProfile>("update_organisation", { input, expectedVersion }),
  addGstRegistration: (input: RegistrationInput) => call<BusinessProfile>("add_gst_registration", { input }),
  updateGstRegistration: (id: string, input: UpdateRegistrationInput, expectedVersion: number) =>
    call<BusinessProfile>("update_gst_registration", { id, input, expectedVersion }),

  listAccounts: () => call<Account[]>("list_accounts"),
  listSeries: () => call<Series[]>("list_series"),
  createSeries: (input: SeriesInput) => call<Series[]>("create_series", { input }),
  updateSeries: (id: string, input: SeriesUpdateInput, expectedVersion: number) =>
    call<Series[]>("update_series", { id, input, expectedVersion }),

  listAuditEvents: (limit = 100) => call<AuditEvent[]>("list_audit_events", { limit }),

  listParties: () => call<Party[]>("list_parties"),
  createParty: (input: PartyInput) => call<Party>("create_party", { input }),
  updateParty: (id: string, input: PartyInput, expectedVersion: number) =>
    call<Party>("update_party", { id, input, expectedVersion }),

  itemOptions: () => call<ItemOptions>("item_options"),
  listItems: () => call<Item[]>("list_items"),
  createItem: (input: ItemInput) => call<Item>("create_item", { input }),
  updateItem: (id: string, input: ItemInput, expectedVersion: number) =>
    call<Item>("update_item", { id, input, expectedVersion }),

  importTemplate: (kind: ImportKind) => call<ImportTemplate>("import_template", { kind }),
  /** Bytes cross IPC as a number array; the core caps files at 5 MB. */
  parseImport: (kind: ImportKind, fileName: string, bytes: Uint8Array) =>
    call<ParsedImport>("parse_import", { kind, fileName, bytes: Array.from(bytes) }),
  previewImport: (kind: ImportKind, table: ImportTable, mapping: ImportMapping) =>
    call<ImportPreview>("preview_import", { kind, table, mapping }),
  commitImport: (kind: ImportKind, table: ImportTable, mapping: ImportMapping) =>
    call<ImportResult>("commit_import", { kind, table, mapping }),

  listSales: () => call<SalesSummary[]>("list_sales"),
  getSalesDocument: (id: string) => call<SalesDocumentView>("get_sales_document", { id }),
  calculateSales: (input: SalesDocumentInput) => call<SalesCalculation>("calculate_sales", { input }),
  saveSalesDraft: (id: string | null, input: SalesDocumentInput, expectedVersion?: number) =>
    call<SalesDocumentView>("save_sales_draft", { id, input, expectedVersion }),
  issueSalesDocument: (id: string | null, input: SalesDocumentInput, expectedVersion?: number) =>
    call<SalesDocumentView>("issue_sales_document", { id, input, expectedVersion }),
  cancelSalesDocument: (id: string, reason: string, expectedVersion: number) =>
    call<SalesDocumentView>("cancel_sales_document", { id, reason, expectedVersion }),
  deleteSalesDraft: (id: string, expectedVersion: number) => call<null>("delete_sales_draft", { id, expectedVersion }),

  purchaseAccounts: () => call<Account[]>("purchase_accounts"),
  listPurchases: () => call<PurchaseSummary[]>("list_purchases"),
  getPurchase: (id: string) => call<PurchaseDocumentView>("get_purchase", { id }),
  calculatePurchase: (input: PurchaseDocumentInput) => call<PurchaseCalculation>("calculate_purchase", { input }),
  savePurchaseDraft: (id: string | null, input: PurchaseDocumentInput, expectedVersion?: number) =>
    call<PurchaseDocumentView>("save_purchase_draft", { id, input, expectedVersion }),
  recordPurchase: (id: string | null, input: PurchaseDocumentInput, expectedVersion?: number) =>
    call<PurchaseDocumentView>("record_purchase", { id, input, expectedVersion }),
  reviewItc: (documentId: string, lineId: string, input: ItcReviewInput, expectedVersion: number) =>
    call<PurchaseDocumentView>("review_itc", { documentId, lineId, input, expectedVersion }),
  cancelPurchase: (id: string, reason: string, expectedVersion: number) =>
    call<PurchaseDocumentView>("cancel_purchase", { id, reason, expectedVersion }),
  deletePurchaseDraft: (id: string, expectedVersion: number) => call<null>("delete_purchase_draft", { id, expectedVersion }),
  /** Bytes cross IPC as a number array; the core caps attachments at 5 MB. */
  addAttachment: (documentId: string, fileName: string, bytes: Uint8Array) =>
    call<Attachment>("add_attachment", { documentId, fileName, bytes: Array.from(bytes) }),
  attachmentContent: (id: string) => call<AttachmentContent>("attachment_content", { id }),
  removeAttachment: (id: string) => call<null>("remove_attachment", { id }),

  listMoneyAccounts: () => call<MoneyAccount[]>("list_money_accounts"),
  createMoneyAccount: (input: MoneyAccountInput) => call<MoneyAccount>("create_money_account", { input }),
  updateMoneyAccount: (id: string, input: MoneyAccountInput, expectedVersion: number) =>
    call<MoneyAccount>("update_money_account", { id, input, expectedVersion }),
  listOpeningBalances: () => call<OpeningBalance[]>("list_opening_balances"),
  setOpeningBalance: (input: OpeningBalanceInput) => call<OpeningBalance>("set_opening_balance", { input }),
  voucherAccounts: () => call<Account[]>("voucher_accounts"),
  openDocuments: (partyId: string, voucherType: VoucherType) =>
    call<OpenDocument[]>("open_documents", { partyId, voucherType }),
  listVouchers: () => call<VoucherSummary[]>("list_vouchers"),
  getVoucher: (id: string) => call<VoucherView>("get_voucher", { id }),
  documentSettlements: (documentKind: "sales" | "purchase", documentId: string) =>
    call<Settlement[]>("document_settlements", { documentKind, documentId }),
  createVoucher: (input: VoucherInput) => call<VoucherView>("create_voucher", { input }),
  cancelVoucher: (id: string, reason: string, expectedVersion: number) =>
    call<VoucherView>("cancel_voucher", { id, reason, expectedVersion }),
  moneyBook: (moneyAccountId: string, from?: string, to?: string) =>
    call<MoneyBook>("money_book", { moneyAccountId, from: from || null, to: to || null }),
  /** Bytes cross IPC as a number array; the core caps files at 5 MB. */
  readStatementFile: (moneyAccountId: string, fileName: string, bytes: Uint8Array) =>
    call<StatementFile>("read_statement_file", { moneyAccountId, fileName, bytes: Array.from(bytes) }),
  previewStatement: (input: StatementImportInput) => call<StatementPreview>("preview_statement", { input }),
  importStatement: (input: StatementImportInput) => call<StatementImportResult>("import_statement", { input }),
  reconciliation: (moneyAccountId: string) => call<Reconciliation>("reconciliation", { moneyAccountId }),
  matchStatementLine: (lineId: string, entryId: string) => call<null>("match_statement_line", { lineId, entryId }),
  unmatchStatementLine: (lineId: string) => call<null>("unmatch_statement_line", { lineId }),
  ignoreStatementLine: (lineId: string) => call<null>("ignore_statement_line", { lineId }),
  undoStatementImport: (moneyAccountId: string, batch: string) =>
    call<number>("undo_statement_import", { moneyAccountId, batch }),

  trialBalance: (from?: string, to?: string) => call<TrialBalance>("trial_balance", { from: from || null, to: to || null }),
  profitAndLoss: (from: string, to: string) => call<ProfitAndLoss>("profit_and_loss", { from, to }),
  balanceSheet: (asAt: string) => call<BalanceSheet>("balance_sheet", { asAt }),
  generalLedger: (accountId: string, from?: string, to?: string) =>
    call<Ledger>("general_ledger", { accountId, from: from || null, to: to || null }),
  partyLedger: (partyId: string, from?: string, to?: string) =>
    call<Ledger>("party_ledger", { partyId, from: from || null, to: to || null }),
  journalRegister: (from?: string, to?: string) => call<JournalRegister>("journal_register", { from: from || null, to: to || null }),
  ageing: (kind: "receivables" | "payables", asOf: string) => call<Ageing>("ageing", { kind, asOf }),
  booksControl: () => call<BooksControl>("books_control"),
  setBooksLock: (until: string | null, reason?: string) => call<BooksControl>("set_books_lock", { until, reason: reason || null }),
  closeFinancialYear: (code: string) => call<BooksControl>("close_financial_year", { code }),
  reopenFinancialYear: (code: string, reason: string) => call<BooksControl>("reopen_financial_year", { code, reason }),
  /** Bytes cross IPC as a number array; the core caps logos at 1 MB. */
  setLogo: (fileName: string, bytes: Uint8Array) => call<LogoContent>("set_logo", { fileName, bytes: Array.from(bytes) }),
  getLogo: () => call<LogoContent | null>("get_logo"),
  removeLogo: () => call<null>("remove_logo"),

  /** Renders the PDF and asks where to save it; null when cancelled (desktop only). */
  saveSalesPdf: (id: string) => call<SavedFile | null>("save_sales_pdf", { id }),
  /** Opens the PDF in the default viewer, e.g. to print. */
  openSalesPdf: (id: string) => call<null>("open_sales_pdf", { id }),
  /** Shows a file this session saved in its folder. */
  openSavedFile: (path: string) => call<null>("open_saved_file", { path }),

  backupStatus: () => call<BackupStatus>("backup_status"),
  createBackup: (passphrase: string) => call<{ file: SavedFile; status: BackupStatus } | null>("create_backup", { passphrase }),
  chooseBackupFile: () => call<ChosenBackup | null>("choose_backup_file"),
  verifyBackup: (passphrase: string) => call<RestoreCheck>("verify_backup", { passphrase }),
  /** Replaces the data with the checked backup and restarts the app. */
  applyRestore: () => call<null>("apply_restore"),
  cancelRestore: () => call<null>("cancel_restore"),

  appSession: () => call<AppSession>("app_session"),

  checkUpdate: () => call<UpdateInfo | null>("check_update"),
  /** Downloads, installs and restarts; resolves only if something went wrong in preview. */
  installUpdate: (onProgress: (p: UpdateProgress) => void) => {
    if (!runningInTauri)
      return previewInstallUpdate(onProgress).catch((e: ConstructorParameters<typeof ApiError>[0]) => {
        throw new ApiError(e);
      });
    const channel = new Channel<UpdateProgress>();
    channel.onmessage = onProgress;
    return call<void>("install_update", { onProgress: channel });
  },
  openReleasesPage: () => call<void>("open_releases_page"),
  lockApp: () => call<LockStatus>("lock_app"),
  unlockApp: (pin: string) => call<LockStatus>("unlock_app", { pin }),
  unlockWithRecovery: (code: string) => call<LockStatus>("unlock_with_recovery", { code }),
  setAppLock: (pin: string, idleMinutes: number, currentPin?: string) =>
    call<{ status: LockStatus; recoveryCode: string }>("set_app_lock", { pin, idleMinutes, currentPin: currentPin || null }),
  setLockTimeout: (idleMinutes: number) => call<LockStatus>("set_lock_timeout", { idleMinutes }),
  removeAppLock: (pin: string) => call<LockStatus>("remove_app_lock", { pin }),
};

/** Query keys shared by readers and the mutations that invalidate them. */
export const keys = {
  profile: ["profile"] as const,
  dashboard: ["dashboard"] as const,
  series: ["series"] as const,
  accounts: ["accounts"] as const,
  audit: ["audit"] as const,
  health: ["health"] as const,
  parties: ["parties"] as const,
  items: ["items"] as const,
  itemOptions: ["itemOptions"] as const,
  sales: ["sales"] as const,
  salesDocument: (id: string) => ["sales", id] as const,
  purchases: ["purchases"] as const,
  purchase: (id: string) => ["purchases", id] as const,
  purchaseAccounts: ["purchaseAccounts"] as const,
  moneyAccounts: ["moneyAccounts"] as const,
  openingBalances: ["openingBalances"] as const,
  voucherAccounts: ["voucherAccounts"] as const,
  vouchers: ["vouchers"] as const,
  voucher: (id: string) => ["vouchers", id] as const,
  openDocuments: (partyId: string, type: string) => ["openDocuments", partyId, type] as const,
  settlements: (kind: string, id: string) => ["settlements", kind, id] as const,
  moneyBook: (id: string, from: string, to: string) => ["moneyBook", id, from, to] as const,
  reconciliation: (id: string) => ["reconciliation", id] as const,
  /** Every report shares this prefix, so one invalidation refreshes them all. */
  report: (...parts: string[]) => ["report", ...parts] as const,
  booksControl: ["booksControl"] as const,
  logo: ["logo"] as const,
  backupStatus: ["backupStatus"] as const,
  session: ["session"] as const,
};
