import type { SalesDisplayStatus, SalesDocType, SupplyKind } from "./api";

export const DOC_TYPE_LABEL: Record<SalesDocType, string> = {
  tax_invoice: "Tax invoice",
  bill_of_supply: "Bill of supply",
  credit_note: "Credit note",
};

export const STATUS: Record<SalesDisplayStatus, { label: string; tone: "neutral" | "accent" | "success" | "warning" | "danger" }> = {
  draft: { label: "Draft", tone: "neutral" },
  issued: { label: "Issued", tone: "accent" },
  overdue: { label: "Overdue", tone: "danger" },
  part_paid: { label: "Part paid", tone: "warning" },
  paid: { label: "Paid", tone: "success" },
  cancelled: { label: "Cancelled", tone: "neutral" },
};

export const SUPPLY_LABEL: Record<SupplyKind, string> = {
  intra_state: "Within the state · CGST + SGST",
  intra_union_territory: "Within the UT · CGST + UTGST",
  inter_state: "Between states · IGST",
};

export function shortDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString("en-IN", { day: "numeric", month: "short", year: "numeric" });
}

export function todayIso(d = new Date()): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

const ONES = ["", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve", "thirteen",
  "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen"];
const TENS = ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];

function belowHundred(n: number): string {
  return n < 20 ? ONES[n] : `${TENS[Math.trunc(n / 10)]}${n % 10 ? `-${ONES[n % 10]}` : ""}`;
}

function belowThousand(n: number): string {
  const h = Math.trunc(n / 100);
  const r = n % 100;
  return [h ? `${ONES[h]} hundred` : "", r ? belowHundred(r) : ""].filter(Boolean).join(" and ");
}

/** Indian numbering in words: 1,23,45,678 → "one crore twenty-three lakh…". Integer maths only. */
function integerWords(n: number): string {
  if (n === 0) return "zero";
  const parts: string[] = [];
  const crore = Math.trunc(n / 1_00_00_000);
  const lakh = Math.trunc((n % 1_00_00_000) / 1_00_000);
  const thousand = Math.trunc((n % 1_00_000) / 1000);
  const rest = n % 1000;
  if (crore) parts.push(`${integerWords(crore)} crore`);
  if (lakh) parts.push(`${belowHundred(lakh)} lakh`);
  if (thousand) parts.push(`${belowHundred(thousand)} thousand`);
  if (rest) parts.push(belowThousand(rest));
  return parts.join(" ");
}

/** "Rupees two thousand nine hundred and forty-nine only" (paise → words). */
export function amountInWords(paise: number): string {
  const abs = Math.abs(paise);
  const rupees = Math.trunc(abs / 100);
  const p = abs % 100;
  const words = `Rupees ${integerWords(rupees)}${p ? ` and ${belowHundred(p)} paise` : ""} only`;
  return words.charAt(0).toUpperCase() + words.slice(1);
}
