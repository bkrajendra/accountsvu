import type { VoucherMode, VoucherType } from "./api";

export const VOUCHER_LABEL: Record<VoucherType, string> = { receipt: "Receipt", payment: "Payment" };

export const MODE_LABEL: Record<VoucherMode, string> = {
  cash: "Cash",
  upi: "UPI",
  neft: "NEFT",
  rtgs: "RTGS",
  imps: "IMPS",
  cheque: "Cheque",
  card: "Card",
  other: "Other",
};

const SOURCE_LABEL: Record<string, string> = {
  sales_invoice: "Invoice",
  sales_credit_note: "Credit note",
  purchase: "Purchase",
  purchase_debit_note: "Debit note",
  itc_review: "ITC review",
  receipt: "Receipt",
  payment: "Payment",
  party_opening_balance: "Opening balance",
  account_opening_balance: "Opening balance",
};

export function sourceLabel(sourceType: string): string {
  return SOURCE_LABEL[sourceType] ?? sourceType.replace(/_/g, " ");
}

/** Where a ledger entry came from, so every figure drills down to its source. */
export function sourceLink(sourceType: string, sourceId: string | null): string | null {
  if (!sourceId) return null;
  if (sourceType.startsWith("sales")) return `/sales/${sourceId}`;
  if (sourceType.startsWith("purchase") || sourceType === "itc_review") return `/purchases/${sourceId}`;
  if (sourceType === "receipt" || sourceType === "payment") return `/banking/vouchers/${sourceId}`;
  if (sourceType === "party_opening_balance") return "/parties";
  return null;
}

/** Signed paise as "+₹x" / "−₹x" helper input: deposits positive. */
export function signedPaise(deposit: number, withdrawal: number): number {
  return deposit - withdrawal;
}

/** Days apart, said plainly. */
export function dayGapLabel(gap: number): string {
  if (gap === 0) return "same day";
  const n = Math.abs(gap);
  return `${n} day${n === 1 ? "" : "s"} ${gap > 0 ? "later" : "earlier"}`;
}
