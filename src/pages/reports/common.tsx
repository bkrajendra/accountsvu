import type { ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useSearchParams } from "react-router";
import clsx from "clsx";
import { ArrowLeft, Loader2 } from "lucide-react";
import { api, keys } from "../../lib/api";
import { formatPaise } from "../../lib/format";
import { todayIso } from "../../lib/sales";
import { Field, TextInput } from "../../components/form";

export type Preset = "fy" | "lastFy" | "month" | "lastMonth" | "all" | "custom";

const iso = (d: Date) => todayIso(d);

/** Date range presets, from the business's financial year. */
export function presetRange(p: Preset, fyStart: string | undefined, fyEnd: string | undefined): { from: string; to: string } {
  const now = new Date();
  switch (p) {
    case "fy":
      return { from: fyStart ?? "", to: fyEnd ?? "" };
    case "lastFy": {
      if (!fyStart || !fyEnd) return { from: "", to: "" };
      const [y, m, d] = fyStart.split("-").map(Number);
      const [ey, em, ed] = fyEnd.split("-").map(Number);
      return { from: iso(new Date(y - 1, m - 1, d)), to: iso(new Date(ey - 1, em - 1, ed)) };
    }
    case "month":
      return { from: iso(new Date(now.getFullYear(), now.getMonth(), 1)), to: iso(new Date(now.getFullYear(), now.getMonth() + 1, 0)) };
    case "lastMonth":
      return { from: iso(new Date(now.getFullYear(), now.getMonth() - 1, 1)), to: iso(new Date(now.getFullYear(), now.getMonth(), 0)) };
    default:
      return { from: "", to: "" };
  }
}

/**
 * The report period lives in the URL (?from=&to=), so drill-down links and
 * the back button keep it. Defaults to the current financial year.
 */
export function usePeriod(defaultPreset: Preset = "fy") {
  const [params, setParams] = useSearchParams();
  const { data: profile } = useQuery({ queryKey: keys.profile, queryFn: api.businessProfile });
  const fy = profile?.currentFinancialYear;
  const def = presetRange(defaultPreset, fy?.startDate, fy?.endDate);
  const from = params.get("from") ?? def.from;
  const to = params.get("to") ?? def.to;
  const set = (next: { from: string; to: string }) =>
    setParams(
      (p) => {
        const q = new URLSearchParams(p);
        q.set("from", next.from);
        q.set("to", next.to);
        return q;
      },
      { replace: true },
    );
  return { from, to, set, fy, ready: !!profile };
}

const PRESETS: { id: Preset; label: string }[] = [
  { id: "fy", label: "This FY" },
  { id: "lastFy", label: "Last FY" },
  { id: "month", label: "This month" },
  { id: "lastMonth", label: "Last month" },
  { id: "all", label: "All dates" },
];

export function PeriodBar({ period, single, busy }: { period: ReturnType<typeof usePeriod>; single?: boolean; busy?: boolean }) {
  const { from, to, set, fy } = period;
  return (
    <div className="mb-4 flex flex-wrap items-end gap-3">
      {!single && (
        <div role="group" aria-label="Period" className="inline-flex rounded-[10px] bg-active p-0.5">
          {PRESETS.map((p) => {
            const r = presetRange(p.id, fy?.startDate, fy?.endDate);
            const on = r.from === from && r.to === to;
            return (
              <button key={p.id} type="button" aria-pressed={on} onClick={() => set(r)}
                className={clsx("h-8 rounded-[8px] px-3 text-[13px] font-medium", on ? "bg-surface text-fg shadow-card" : "text-muted hover:text-fg")}>
                {p.label}
              </button>
            );
          })}
        </div>
      )}
      {!single && (
        <Field label="From" className="w-[160px]">
          {(p) => <TextInput id={p.id} type="date" value={from} onChange={(e) => set({ from: e.target.value, to })} />}
        </Field>
      )}
      <Field label={single ? "As on" : "To"} className="w-[160px]">
        {(p) => <TextInput id={p.id} type="date" value={to} min={from || undefined} onChange={(e) => set({ from, to: e.target.value })} />}
      </Field>
      {busy && <Loader2 size={16} className="mb-2.5 animate-spin text-muted" />}
    </div>
  );
}

export function ReportHeader({ kicker, title, subtitle, actions }: { kicker?: string; title: string; subtitle?: ReactNode; actions?: ReactNode }) {
  return (
    <>
      <Link to="/reports" className="mb-2 inline-flex items-center gap-1 text-[13px] text-muted hover:text-fg">
        <ArrowLeft size={14} /> Reports
      </Link>
      <div className="mb-5 flex items-end justify-between gap-4">
        <div>
          {kicker && <div className="text-[12px] font-semibold uppercase tracking-[0.08em] text-accent">{kicker}</div>}
          <h1 className="text-display">{title}</h1>
          {subtitle && <p className="mt-1 text-[14px] text-muted">{subtitle}</p>}
        </div>
        {actions && <div className="flex gap-2">{actions}</div>}
      </div>
    </>
  );
}

/** A money cell; blank for zero unless `zero`. */
export function Amount({ paise, zero, className }: { paise: number; zero?: boolean; className?: string }) {
  return <span className={clsx("num", paise < 0 && "text-danger", className)}>{paise || zero ? formatPaise(paise) : ""}</span>;
}

/** Link to an account's ledger for the same period. */
export function ledgerHref(accountId: string, from?: string, to?: string) {
  const q = new URLSearchParams();
  if (from) q.set("from", from);
  if (to) q.set("to", to);
  return `/reports/ledger/${accountId}?${q.toString()}`;
}

export function Loading() {
  return (
    <div className="grid place-items-center py-24 text-muted">
      <Loader2 className="animate-spin" />
    </div>
  );
}

/** Debit/credit balance as "₹x Dr" / "₹x Cr". */
export function DrCr({ paise }: { paise: number }) {
  if (!paise) return <span className="num text-subtle">—</span>;
  return (
    <span className="num">
      {formatPaise(Math.abs(paise))} <span className="text-[11.5px] text-muted">{paise > 0 ? "Dr" : "Cr"}</span>
    </span>
  );
}
