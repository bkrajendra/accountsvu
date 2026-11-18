/**
 * Client-side GST helpers for instant form feedback. The Rust core
 * re-validates everything (crates/domain); these only make typing nicer.
 */

const CHARSET = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

export const STATES: { code: number; name: string }[] = [
  { code: 1, name: "Jammu and Kashmir" },
  { code: 2, name: "Himachal Pradesh" },
  { code: 3, name: "Punjab" },
  { code: 4, name: "Chandigarh" },
  { code: 5, name: "Uttarakhand" },
  { code: 6, name: "Haryana" },
  { code: 7, name: "Delhi" },
  { code: 8, name: "Rajasthan" },
  { code: 9, name: "Uttar Pradesh" },
  { code: 10, name: "Bihar" },
  { code: 11, name: "Sikkim" },
  { code: 12, name: "Arunachal Pradesh" },
  { code: 13, name: "Nagaland" },
  { code: 14, name: "Manipur" },
  { code: 15, name: "Mizoram" },
  { code: 16, name: "Tripura" },
  { code: 17, name: "Meghalaya" },
  { code: 18, name: "Assam" },
  { code: 19, name: "West Bengal" },
  { code: 20, name: "Jharkhand" },
  { code: 21, name: "Odisha" },
  { code: 22, name: "Chhattisgarh" },
  { code: 23, name: "Madhya Pradesh" },
  { code: 24, name: "Gujarat" },
  { code: 26, name: "Dadra and Nagar Haveli and Daman and Diu" },
  { code: 27, name: "Maharashtra" },
  { code: 29, name: "Karnataka" },
  { code: 30, name: "Goa" },
  { code: 31, name: "Lakshadweep" },
  { code: 32, name: "Kerala" },
  { code: 33, name: "Tamil Nadu" },
  { code: 34, name: "Puducherry" },
  { code: 35, name: "Andaman and Nicobar Islands" },
  { code: 36, name: "Telangana" },
  { code: 37, name: "Andhra Pradesh" },
  { code: 38, name: "Ladakh" },
  { code: 97, name: "Other Territory" },
];

export function stateName(code: number | undefined | null): string {
  return STATES.find((s) => s.code === code)?.name ?? (code ? `State ${code}` : "—");
}

export const CONSTITUTIONS: { value: string; label: string; panType?: string[] }[] = [
  { value: "proprietorship", label: "Proprietorship", panType: ["P"] },
  { value: "partnership", label: "Partnership firm", panType: ["F"] },
  { value: "llp", label: "LLP", panType: ["F"] },
  { value: "private_limited", label: "Private limited company", panType: ["C"] },
  { value: "public_limited", label: "Public limited company", panType: ["C"] },
  { value: "huf", label: "Hindu Undivided Family", panType: ["H"] },
  { value: "trust", label: "Trust", panType: ["T", "A"] },
  { value: "society", label: "Society", panType: ["A", "T"] },
  { value: "other", label: "Other" },
];

/** Best-guess constitution from the PAN holder-type letter. */
export function constitutionFromPan(pan: string): string | undefined {
  const t = pan[3];
  return { P: "proprietorship", F: "partnership", C: "private_limited", H: "huf", T: "trust", A: "society" }[t ?? ""];
}

export const DOCUMENT_TYPES: { value: string; label: string }[] = [
  { value: "tax_invoice", label: "Tax invoice" },
  { value: "bill_of_supply", label: "Bill of supply" },
  { value: "export_invoice", label: "Export invoice" },
  { value: "credit_note", label: "Credit note" },
  { value: "debit_note", label: "Debit note" },
  { value: "receipt_voucher", label: "Receipt voucher" },
  { value: "payment_voucher", label: "Payment voucher" },
  { value: "estimate", label: "Estimate" },
];

export function documentTypeLabel(v: string): string {
  return DOCUMENT_TYPES.find((d) => d.value === v)?.label ?? v;
}

export const MONTHS = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];

const PAN_RE = /^[A-Z]{5}[0-9]{4}[A-Z]$/;

export function isValidPan(pan: string): boolean {
  return PAN_RE.test(pan);
}

function gstinCheckChar(first14: string): string {
  let sum = 0;
  for (let i = 0; i < 14; i++) {
    const v = CHARSET.indexOf(first14[i]);
    const p = v * (i % 2 === 0 ? 1 : 2);
    sum += Math.floor(p / 36) + (p % 36);
  }
  return CHARSET[(36 - (sum % 36)) % 36];
}

/** Returns an error message, or null when the GSTIN looks valid. */
export function gstinError(raw: string): string | null {
  const g = raw.trim().toUpperCase();
  if (g.length !== 15) return "A GSTIN has 15 characters.";
  if (![...g].every((c) => CHARSET.includes(c))) return "Use only letters and digits.";
  if (!STATES.some((s) => s.code === Number(g.slice(0, 2)))) return "The first two digits must be a state code.";
  if (!isValidPan(g.slice(2, 12))) return "Characters 3–12 must be a valid PAN.";
  if (gstinCheckChar(g.slice(0, 14)) !== g[14]) return "This GSTIN has a typo — the last character doesn't match.";
  return null;
}

export function gstinParts(raw: string): { stateCode: number; pan: string } | null {
  const g = raw.trim().toUpperCase();
  if (gstinError(g)) return null;
  return { stateCode: Number(g.slice(0, 2)), pan: g.slice(2, 12) };
}
