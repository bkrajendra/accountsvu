import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useSearchParams } from "react-router";
import { AlertCircle, AlertTriangle, CalendarClock, CheckCircle2, ChevronLeft, ChevronRight, FileCheck2, Info } from "lucide-react";
import { api, keys, type FiledBadge, type PeriodStatus, type ReturnCentre as Centre, type ReturnPeriod } from "../../lib/api";
import { formatPaise } from "../../lib/format";
import { shortDate, todayIso } from "../../lib/sales";
import { Badge, Button, Card, EmptyState, PageHeader } from "../../components/ui";
import { Loading } from "../reports/common";

export function statusBadge(status: PeriodStatus, due: string, filed?: FiledBadge | null) {
  if (filed)
    return (
      <Badge tone="success" icon={<CheckCircle2 size={11} />}>
        Filed {shortDate(filed.filedOn)}
      </Badge>
    );
  switch (status) {
    case "open":
      return <Badge>In progress</Badge>;
    case "due":
      return <Badge tone="accent">Due {shortDate(due)}</Badge>;
    default:
      return <Badge tone="warning">Not filed · was due {shortDate(due)}</Badge>;
  }
}

/** GSTIN and financial year live in the URL, so drill-down and back keep them. */
export function useReturnScope() {
  const [params, setParams] = useSearchParams();
  const reg = params.get("reg");
  const fy = params.get("fy");
  const set = (next: { reg?: string | null; fy?: string | null }) =>
    setParams(
      (p) => {
        const q = new URLSearchParams(p);
        for (const [k, v] of Object.entries(next)) {
          if (v) q.set(k, v);
          else q.delete(k);
        }
        return q;
      },
      { replace: true },
    );
  return { reg, fy, set };
}

export function ReturnCentre() {
  const scope = useReturnScope();
  const { data, isFetching } = useQuery({
    queryKey: keys.report("gst", "centre", scope.reg ?? "", scope.fy ?? ""),
    queryFn: () => api.returnCentre(scope.reg, scope.fy),
    placeholderData: (prev) => prev,
  });

  if (!data) return <Loading />;
  if (data.registrations.length === 0) {
    return (
      <>
        <PageHeader title="GST returns" />
        <EmptyState
          icon={<FileCheck2 size={22} />}
          title="Add a GSTIN to prepare returns"
          body="Returns are prepared per GST registration. Add your GSTIN in Settings, then issue invoices from it."
          actions={
            <Link to="/settings?tab=gst">
              <Button variant="primary">Open settings</Button>
            </Link>
          }
        />
      </>
    );
  }
  const reg = data.registrations.find((r) => r.id === data.registrationId) ?? data.registrations[0];
  const next = nextDue(data.periods);

  return (
    <div>
      <PageHeader
        title="GST returns"
        subtitle={
          <>
            GSTR-1, GSTR-3B and GSTR-2B by month for <span className="num font-medium text-fg">{reg.gstin}</span> · {reg.stateName}
          </>
        }
        actions={
          <>
            {data.registrations.length > 1 && (
              <select
                aria-label="GSTIN"
                className="h-9 rounded-[10px] border border-line bg-surface px-3 text-[13.5px]"
                value={reg.id}
                onChange={(e) => scope.set({ reg: e.target.value })}
              >
                {data.registrations.map((r) => (
                  <option key={r.id} value={r.id}>
                    {r.gstin} · {r.stateName}
                  </option>
                ))}
              </select>
            )}
            <YearSwitcher data={data} onChange={(fy) => scope.set({ fy })} busy={isFetching} />
          </>
        }
      />

      {data.quarterly && (
        <Card className="mb-4 flex items-start gap-3 px-4 py-3 text-[13px]">
          <Info size={16} className="mt-[1px] shrink-0 text-accent" />
          <p className="text-muted">
            This GSTIN files quarterly (QRMP). Periods are shown by month for now; quarterly returns and IFF come in a later update. Add the
            months of a quarter together when you file.
          </p>
        </Card>
      )}

      {next && (
        <Card className="mb-4 flex items-center gap-3 px-4 py-3">
          <span className="grid size-9 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
            <CalendarClock size={18} />
          </span>
          <div className="min-w-0 flex-1 text-[13.5px]">
            <div className="font-medium">
              {next.kind === "gstr1" ? "GSTR-1" : "GSTR-3B"} for {next.p.label} is due {shortDate(next.due)}
            </div>
            <div className="text-muted">
              {next.kind === "gstr1"
                ? `${next.p.documentCount} documents · output tax ${formatPaise(next.p.outputTax.taxPaise)}`
                : `cash to pay ${formatPaise(next.p.cashPayablePaise)} after ${formatPaise(next.p.netItcPaise)} net ITC`}
              {next.p.errorCount > 0 && ` · ${next.p.errorCount} to fix`}
            </div>
          </div>
          <Link to={`/gst/${next.kind}/${next.p.period}?reg=${reg.id}`}>
            <Button variant="primary">Open {next.kind === "gstr1" ? "GSTR-1" : "GSTR-3B"}</Button>
          </Link>
        </Card>
      )}

      <Card className="overflow-hidden">
        <table className="w-full text-[13px]">
          <thead>
            <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
              <th className="px-4 py-2 font-medium">Period</th>
              <th className="px-3 py-2 font-medium">GSTR-1</th>
              <th className="px-3 py-2 font-medium">GSTR-3B</th>
              <th className="px-3 py-2 font-medium">GSTR-2B</th>
              <th className="px-3 py-2 text-right font-medium">Documents</th>
              <th className="px-3 py-2 text-right font-medium">Output tax</th>
              <th className="px-3 py-2 text-right font-medium">Net ITC</th>
              <th className="px-3 py-2 text-right font-medium">Cash to pay</th>
              <th className="px-3 py-2 font-medium">Checks</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {data.periods.map((p) => (
              <PeriodRow key={p.period} p={p} reg={reg.id} />
            ))}
            {data.periods.length === 0 && (
              <tr>
                <td colSpan={9} className="px-4 py-10 text-center text-muted">
                  No return periods in {data.financialYear} yet.
                </td>
              </tr>
            )}
          </tbody>
        </table>
        <p className="border-t border-line px-4 py-2 text-[12px] text-muted">
          Output tax is invoices less credit notes. Cash to pay is the GSTR-3B proposal after input tax credit, including reverse charge. Due dates
          are for monthly returns (GSTR-1 on the 11th, GSTR-3B on the 20th of the next month); marking a return as filed comes with offline filing.
        </p>
      </Card>
    </div>
  );
}

type Due = { kind: "gstr1" | "gstr3b"; p: ReturnPeriod; due: string };

function nextDue(periods: ReturnPeriod[]): Due | undefined {
  const today = todayIso();
  const all: Due[] = periods.flatMap((p) => [
    ...(p.status === "due" && !p.gstr1Filed && p.gstr1Due >= today ? [{ kind: "gstr1" as const, p, due: p.gstr1Due }] : []),
    ...(p.gstr3bStatus === "due" && !p.gstr3bFiled && p.gstr3bDue >= today ? [{ kind: "gstr3b" as const, p, due: p.gstr3bDue }] : []),
  ]);
  return all.sort((a, b) => a.due.localeCompare(b.due))[0];
}

function PeriodRow({ p, reg }: { p: ReturnPeriod; reg: string }) {
  const navigate = useNavigate();
  const go = (kind: "gstr1" | "gstr3b") => navigate(`/gst/${kind}/${p.period}?reg=${reg}`);
  const cell = (kind: "gstr1" | "gstr3b", status: PeriodStatus, due: string, filed: FiledBadge | null) => (
    <td className="px-3 py-2.5">
      <Link to={`/gst/${kind}/${p.period}?reg=${reg}`} onClick={(e) => e.stopPropagation()} className="group inline-flex items-center gap-1">
        {statusBadge(status, due, filed)}
        <ChevronRight size={14} className="text-subtle group-hover:text-fg" />
      </Link>
    </td>
  );
  return (
    <tr onClick={() => go("gstr1")} className="cursor-pointer hover:bg-hover">
      <td className="px-4 py-2.5 font-medium">{p.label}</td>
      {cell("gstr1", p.status, p.gstr1Due, p.gstr1Filed)}
      {cell("gstr3b", p.gstr3bStatus, p.gstr3bDue, p.gstr3bFiled)}
      <td className="px-3 py-2.5">
        <Link to={`/gst/gstr2b/${p.period}?reg=${reg}`} onClick={(e) => e.stopPropagation()} className="group inline-flex items-center gap-1">
          {!p.gstr2bImported ? (
            <span className="text-[12.5px] text-muted">Not imported</span>
          ) : p.gstr2bNeedsReview > 0 ? (
            <Badge tone="warning">{p.gstr2bNeedsReview} to review</Badge>
          ) : (
            <Badge tone="success">Reconciled</Badge>
          )}
          <ChevronRight size={14} className="text-subtle group-hover:text-fg" />
        </Link>
      </td>
      <td className="num px-3 py-2.5 text-right">
        {p.documentCount}
        {p.draftCount > 0 && <span className="text-[12px] text-muted"> +{p.draftCount} draft</span>}
      </td>
      <td className="num px-3 py-2.5 text-right">{formatPaise(p.outputTax.taxPaise)}</td>
      <td className="num px-3 py-2.5 text-right">{formatPaise(p.netItcPaise)}</td>
      <td className="num px-3 py-2.5 text-right font-medium">{formatPaise(p.cashPayablePaise)}</td>
      <td className="px-3 py-2.5">
        <Checks errors={p.errorCount} warnings={p.warningCount} />
      </td>
    </tr>
  );
}

export function Checks({ errors, warnings }: { errors: number; warnings: number }) {
  if (!errors && !warnings) return <span className="text-[12.5px] text-muted">None</span>;
  return (
    <span className="inline-flex items-center gap-1.5">
      {errors > 0 && (
        <Badge tone="danger" icon={<AlertCircle size={11} />}>
          {errors} to fix
        </Badge>
      )}
      {warnings > 0 && (
        <Badge tone="warning" icon={<AlertTriangle size={11} />}>
          {warnings} to check
        </Badge>
      )}
    </span>
  );
}

function YearSwitcher({ data, onChange, busy }: { data: Centre; onChange: (fy: string) => void; busy: boolean }) {
  return (
    <div className="inline-flex items-center rounded-[10px] border border-line bg-surface shadow-card">
      <button
        type="button"
        aria-label="Previous financial year"
        disabled={!data.previousYear}
        onClick={() => data.previousYear && onChange(data.previousYear)}
        className="pressable grid h-9 w-8 place-items-center text-muted hover:text-fg disabled:opacity-40"
      >
        <ChevronLeft size={16} />
      </button>
      <span className={`num min-w-[84px] text-center text-[13.5px] font-medium ${busy ? "text-muted" : ""}`}>FY {data.financialYear}</span>
      <button
        type="button"
        aria-label="Next financial year"
        disabled={!data.nextYear}
        onClick={() => data.nextYear && onChange(data.nextYear)}
        className="pressable grid h-9 w-8 place-items-center text-muted hover:text-fg disabled:opacity-40"
      >
        <ChevronRight size={16} />
      </button>
    </div>
  );
}
