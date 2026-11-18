import type { MoneyString } from "./api";

const inr = new Intl.NumberFormat("en-IN", {
  style: "currency",
  currency: "INR",
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

/**
 * Display-only INR formatting with Indian digit grouping (₹1,23,456.00).
 * Converting to Number here is safe for display (exact up to ~₹90 trillion)
 * — never do arithmetic on money in the UI; the Rust core owns it.
 */
export function formatInr(amount: MoneyString): string {
  const n = Number(amount);
  return Number.isFinite(n) ? inr.format(n) : "—";
}

/** Indian financial year label for a date, e.g. "FY 2026–27" (April start). */
export function financialYearLabel(d = new Date()): string {
  const start = d.getMonth() >= 3 ? d.getFullYear() : d.getFullYear() - 1;
  return `FY ${start}–${String((start + 1) % 100).padStart(2, "0")}`;
}

function iso(d: Date): string {
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${m}-${day}`;
}

/** Inclusive ISO bounds of the local calendar month containing `d`. */
export function monthBounds(d = new Date()): { start: string; end: string } {
  return {
    start: iso(new Date(d.getFullYear(), d.getMonth(), 1)),
    end: iso(new Date(d.getFullYear(), d.getMonth() + 1, 0)),
  };
}

export function monthLabel(d = new Date()): string {
  return d.toLocaleDateString("en-IN", { month: "long", year: "numeric" });
}

export function greeting(d = new Date()): string {
  const h = d.getHours();
  if (h < 12) return "Good morning";
  if (h < 17) return "Good afternoon";
  return "Good evening";
}

/** Display paise (integer) as INR, e.g. 125050 → ₹1,250.50. Display only. */
export function formatPaise(paise: number | null | undefined): string {
  if (paise == null) return "—";
  return inr.format(paise / 100);
}

/** Paise → editable decimal string without floating point, e.g. 125050 → "1250.50". */
export function paiseToInput(paise: number | null | undefined): string {
  if (paise == null || paise === 0) return "";
  const abs = Math.abs(paise);
  return `${Math.trunc(abs / 100)}.${String(abs % 100).padStart(2, "0")}`;
}

/** Basis points → percent label, e.g. 1800 → "18%", 25 → "0.25%". */
export function formatRate(bp: number): string {
  const whole = Math.trunc(bp / 100);
  const frac = bp % 100;
  return frac === 0 ? `${whole}%` : `${whole}.${String(frac).padStart(2, "0").replace(/0$/, "")}%`;
}
