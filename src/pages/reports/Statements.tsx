import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router";
import clsx from "clsx";
import { AlertTriangle, CheckCircle2 } from "lucide-react";
import { api, keys, type ReportSection } from "../../lib/api";
import { formatPaise } from "../../lib/format";
import { shortDate, todayIso } from "../../lib/sales";
import { Badge, Card } from "../../components/ui";
import { Amount, DrCr, Loading, PeriodBar, ReportHeader, ledgerHref, usePeriod } from "./common";

function BalancedBadge({ ok }: { ok: boolean }) {
  return ok ? (
    <Badge tone="success" icon={<CheckCircle2 size={12} />}>Balanced</Badge>
  ) : (
    <Badge tone="danger" icon={<AlertTriangle size={12} />}>Out of balance</Badge>
  );
}

const period = (from: string, to: string) => (from || to ? `${from ? shortDate(from) : "Start"} – ${to ? shortDate(to) : "today"}` : "All dates");

export function TrialBalancePage() {
  const p = usePeriod();
  const navigate = useNavigate();
  const { data, isFetching } = useQuery({
    queryKey: keys.report("tb", p.from, p.to),
    queryFn: () => api.trialBalance(p.from, p.to),
    enabled: p.ready,
    placeholderData: (prev) => prev,
  });
  return (
    <div>
      <ReportHeader kicker="Report" title="Trial balance" subtitle={period(p.from, p.to)} actions={data && <BalancedBadge ok={data.balanced} />} />
      <PeriodBar period={p} busy={isFetching} />
      {!data ? (
        <Loading />
      ) : (
        <Card className="overflow-hidden">
          <table className="w-full text-[13px]">
            <thead>
              <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
                <th className="w-16 px-4 py-2 font-medium">Code</th>
                <th className="px-4 py-2 font-medium">Account</th>
                <th className="px-4 py-2 text-right font-medium">Opening</th>
                <th className="px-4 py-2 text-right font-medium">Debit</th>
                <th className="px-4 py-2 text-right font-medium">Credit</th>
                <th className="px-4 py-2 text-right font-medium">Closing</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {data.rows.map((r) => {
                const open = () => navigate(ledgerHref(r.accountId, p.from, p.to));
                return (
                  <tr key={r.accountId} tabIndex={0} onClick={open} onKeyDown={(e) => e.key === "Enter" && open()} className="cursor-pointer hover:bg-hover focus-visible:bg-hover focus-visible:outline-none">
                    <td className="num px-4 py-2 text-muted">{r.code}</td>
                    <td className="px-4 py-2">{r.name}</td>
                    <td className="px-4 py-2 text-right"><DrCr paise={r.openingPaise} /></td>
                    <td className="px-4 py-2 text-right"><Amount paise={r.debitPaise} /></td>
                    <td className="px-4 py-2 text-right"><Amount paise={r.creditPaise} /></td>
                    <td className="px-4 py-2 text-right font-medium"><DrCr paise={r.closingPaise} /></td>
                  </tr>
                );
              })}
              {data.rows.length === 0 && (
                <tr>
                  <td colSpan={6} className="px-4 py-10 text-center text-muted">Nothing posted in this period.</td>
                </tr>
              )}
            </tbody>
            <tfoot>
              <tr className="border-t border-line-strong bg-surface-2 font-semibold">
                <td className="px-4 py-2.5" colSpan={2}>Total</td>
                <td className="num px-4 py-2.5 text-right text-[12px] font-normal text-muted">
                  {formatPaise(data.openingDebitPaise)} Dr / {formatPaise(data.openingCreditPaise)} Cr
                </td>
                <td className="num px-4 py-2.5 text-right">{formatPaise(data.debitPaise)}</td>
                <td className="num px-4 py-2.5 text-right">{formatPaise(data.creditPaise)}</td>
                <td className="num px-4 py-2.5 text-right text-[12px] font-normal text-muted">
                  {formatPaise(data.closingDebitPaise)} Dr / {formatPaise(data.closingCreditPaise)} Cr
                </td>
              </tr>
            </tfoot>
          </table>
        </Card>
      )}
    </div>
  );
}

function SectionTable({ s, from, to, sign = 1 }: { s: ReportSection; from?: string; to?: string; sign?: number }) {
  return (
    <div>
      <div className="flex items-baseline justify-between border-b border-line bg-surface-2 px-4 py-2 text-[12px] font-semibold uppercase tracking-[0.04em] text-muted">
        <span>{s.title}</span>
      </div>
      {s.lines.length === 0 ? (
        <div className="px-4 py-2 text-[13px] text-subtle">None</div>
      ) : (
        s.lines.map((l) => (
          <Link key={l.accountId} to={ledgerHref(l.accountId, from, to)} className="flex items-center justify-between px-4 py-1.5 text-[13px] hover:bg-hover">
            <span>
              <span className="num mr-3 text-muted">{l.code}</span>
              {l.name}
            </span>
            <Amount paise={sign * l.amountPaise} zero />
          </Link>
        ))
      )}
      <div className="flex justify-between border-t border-line px-4 py-2 text-[13px] font-semibold">
        <span>Total {s.title.toLowerCase()}</span>
        <Amount paise={sign * s.totalPaise} zero />
      </div>
    </div>
  );
}

function ResultRow({ label, paise, strong }: { label: string; paise: number; strong?: boolean }) {
  return (
    <div className={clsx("flex justify-between border-y border-line-strong bg-accent-soft/40 px-4 py-2.5", strong ? "text-[15px] font-semibold" : "text-[13.5px] font-medium")}>
      <span>{label}</span>
      <span className="num">{paise < 0 ? `(${formatPaise(-paise)})` : formatPaise(paise)}</span>
    </div>
  );
}

export function ProfitAndLossPage() {
  const p = usePeriod();
  const ok = p.ready && !!p.from && !!p.to;
  const { data, isFetching } = useQuery({
    queryKey: keys.report("pl", p.from, p.to),
    queryFn: () => api.profitAndLoss(p.from, p.to),
    enabled: ok,
    placeholderData: (prev) => prev,
  });
  return (
    <div className="max-w-[860px]">
      <ReportHeader kicker="Report" title="Profit & loss" subtitle={period(p.from, p.to)} />
      <PeriodBar period={p} busy={isFetching} />
      {!ok ? (
        <p className="text-[13px] text-muted">Choose a start and end date.</p>
      ) : !data ? (
        <Loading />
      ) : (
        <Card className="overflow-hidden">
          <SectionTable s={data.sections[0]} from={p.from} to={p.to} />
          <SectionTable s={data.sections[1]} from={p.from} to={p.to} />
          <ResultRow label={data.grossProfitPaise >= 0 ? "Gross profit" : "Gross loss"} paise={data.grossProfitPaise} />
          <SectionTable s={data.sections[2]} from={p.from} to={p.to} />
          <SectionTable s={data.sections[3]} from={p.from} to={p.to} />
          <ResultRow label={data.netProfitPaise >= 0 ? "Net profit" : "Net loss"} paise={data.netProfitPaise} strong />
          <p className="px-4 py-3 text-[12px] text-muted">Purchases are expensed as booked; closing stock is not valued yet. Year-end closing entries are left out.</p>
        </Card>
      )}
    </div>
  );
}

export function BalanceSheetPage() {
  const p = usePeriod();
  // A balance sheet is "as on" a date: never later than today.
  const asAt = p.to && p.to < todayIso() ? p.to : todayIso();
  const { data, isFetching } = useQuery({
    queryKey: keys.report("bs", asAt),
    queryFn: () => api.balanceSheet(asAt),
    enabled: p.ready && !!asAt,
    placeholderData: (prev) => prev,
  });
  return (
    <div className="max-w-[1080px]">
      <ReportHeader kicker="Report" title="Balance sheet" subtitle={asAt ? `As on ${shortDate(asAt)}` : undefined} actions={data && <BalancedBadge ok={data.balanced} />} />
      <PeriodBar period={p} single busy={isFetching} />
      {!data ? (
        <Loading />
      ) : (
        <div className="grid grid-cols-2 items-start gap-5">
          <Card className="overflow-hidden">
            <SectionTable s={data.liabilities} to={asAt} />
            <SectionTable s={data.equity} to={asAt} />
            <div className="flex justify-between px-4 py-2 text-[13px]">
              <span>{data.profitPaise >= 0 ? "Profit" : "Loss"} not yet closed into reserves</span>
              <Amount paise={data.profitPaise} zero />
            </div>
            <ResultRow label="Total" paise={data.totalLiabilitiesAndEquityPaise} strong />
          </Card>
          <Card className="overflow-hidden">
            <SectionTable s={data.assets} to={asAt} />
            <ResultRow label="Total" paise={data.totalAssetsPaise} strong />
          </Card>
        </div>
      )}
    </div>
  );
}
