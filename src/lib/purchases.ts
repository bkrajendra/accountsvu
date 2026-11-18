import type { ItcStatus, PaymentMode, PurchaseDocType, PurchaseStatus } from "./api";

export const PURCHASE_TYPE_LABEL: Record<PurchaseDocType, string> = {
  bill: "Supplier bill",
  debit_note: "Debit note",
  expense: "Expense",
};

export const PURCHASE_STATUS: Record<PurchaseStatus, { label: string; tone: "neutral" | "accent" | "success" | "warning" | "danger" }> = {
  draft: { label: "Draft", tone: "neutral" },
  unpaid: { label: "Unpaid", tone: "accent" },
  overdue: { label: "Overdue", tone: "danger" },
  part_paid: { label: "Part paid", tone: "warning" },
  paid: { label: "Paid", tone: "success" },
  recorded: { label: "Recorded", tone: "accent" },
  cancelled: { label: "Cancelled", tone: "neutral" },
};

export const PAYMENT_LABEL: Record<PaymentMode, string> = {
  credit: "On credit",
  cash: "Paid in cash",
  bank: "Paid from bank",
};

export const ITC_LABEL: Record<ItcStatus, { label: string; tone: "neutral" | "accent" | "success" | "warning" | "danger" }> = {
  na: { label: "No GST", tone: "neutral" },
  pending: { label: "ITC pending", tone: "warning" },
  eligible: { label: "ITC eligible", tone: "success" },
  ineligible: { label: "No ITC", tone: "neutral" },
  partial: { label: "Partial ITC", tone: "accent" },
};

export function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(Math.round((bytes / (1024 * 1024)) * 10) / 10).toFixed(1)} MB`;
}
