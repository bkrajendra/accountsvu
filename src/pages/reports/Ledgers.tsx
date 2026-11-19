import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams, useSearchParams } from "react-router";
import clsx from "clsx";
import { RotateCcw } from "lucide-react";
import { api, keys, type Ledger } from "../../lib/api";
import { formatPaise } from "../../lib/format";
import { shortDate, todayIso } from "../../lib/sales";
import { sourceLabel, sourceLink } from "../../lib/banking";
import { Badge, Card, Segmented } from "../../components/ui";
import { Combobox } from "../../components/Combobox";
import { Amount, DrCr, Loading, PeriodBar, ReportHeader, usePeriod } from "./common";

function LedgerTable({ l }: { l: Ledger }) {
  const navigate = useNavigate();
  return (
    <Card className="overflow-hidden">
      <table className="w-full text-[13px]">
        <thead>
          <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
            <th className="px-4 py-2 font-medium">Date</th>
            <th className="px-4 py-2 font-medium">Particulars</th>
            <th className="px-4 py-2 font-medium">Source</th>
            <th className="px-4 py-2 text-right font-medium">Debit</th>
            <th className="px-4 py-2 text-right font-medium">Credit</th>
            <th className="px-4 py-2 text-right font-medium">Balance</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          <tr className="bg-surface-2/50">
            <td className="num px-4 py-2 text-muted">{l.from ? shortDate(l.from) : "—"}</td>
            <td className="px-4 py-2 font-medium" colSpan={4}>Opening balance</td>
            <td className="px-4 py-2 text-right font-medium"><DrCr paise={l.openingPaise} /></td>
          </tr>
          {l.lines.map((x) => {
            const link = sourceLink(x.sourceType, x.sourceId);
            const open = link ? () => navigate(link) : undefined;
            return (
              <tr key={x.entryId} tabIndex={open ? 0 : undefined} onClick={open} onKeyDown={(e) => e.key === "Enter" && open?.()} className={clsx(open && "cursor-pointer hover:bg-hover focus-visible:bg-hover focus-visible:outline-none")}>
                <td className="num whitespace-nowrap px-4 py-2.5 text-muted">{shortDate(x.entryDate)}</td>
                <td className="px-4 py-2.5">
                  {l.ledgerType === "account" ? (x.partyName ?? x.narration ?? "—") : (x.narration ?? "—")}
                  {l.ledgerType === "account" && x.partyName && x.narration && <div className="text-[12px] text-muted">{x.narration}</div>}
                </td>
                <td className="px-4 py-2.5">
                  <span className={clsx(link ? "text-accent" : "text-muted")}>{sourceLabel(x.sourceType)}</span>
                </td>
                <td className="px-4 py-2.5 text-right"><Amount paise={x.debitPaise} /></td>
                <td className="px-4 py-2.5 text-right"><Amount paise={x.creditPaise} /></td>
                <td className="px-4 py-2.5 text-right"><DrCr paise={x.balancePaise} /></td>
              </tr>
            );
          })}
          {l.lines.length === 0 && (
            <tr>
              <td colSpan={6} className="px-4 py-10 text-center text-muted">No entries in this period.</td>
            </tr>
          )}
        </tbody>
        <tfoot>
          <tr className="border-t border-line-strong bg-surface-2 font-semibold">
            <td className="px-4 py-2.5" colSpan={3}>Closing balance</td>
            <td className="num px-4 py-2.5 text-right">{formatPaise(l.debitTotalPaise)}</td>
            <td className="num px-4 py-2.5 text-right">{formatPaise(l.creditTotalPaise)}</td>
            <td className="px-4 py-2.5 text-right"><DrCr paise={l.closingPaise} /></td>
          </tr>
        </tfoot>
      </table>
    </Card>
  );
}

/** General ledger of one account (/reports/ledger/:id) or a picker. */
export function GeneralLedgerPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const p = usePeriod();
  const { data: accounts = [] } = useQuery({ queryKey: keys.accounts, queryFn: api.listAccounts });
  const { data, isFetching } = useQuery({
    queryKey: keys.report("gl", id ?? "", p.from, p.to),
    queryFn: () => api.generalLedger(id!, p.from, p.to),
    enabled: !!id && p.ready,
    placeholderData: (prev) => prev,
  });
  const options = accounts.map((a) => ({ id: a.id, label: a.name, detail: a.code }));
  return (
    <div>
      <ReportHeader kicker="General ledger" title={id && data ? data.title : "General ledger"} subtitle={id && data ? data.subtitle : "Choose an account."} />
      <div className="mb-4 w-[360px]">
        <Combobox ariaLabel="Account" options={options} value={id ?? null} placeholder="Search accounts"
          onSelect={(v) => v && navigate(`/reports/ledger/${v}?${params.toString()}`, { replace: !!id })} />
      </div>
      {id && <PeriodBar period={p} busy={isFetching} />}
      {id && (data ? <LedgerTable l={data} /> : <Loading />)}
    </div>
  );
}

/** A party's statement of account (/reports/party/:id) or a picker. */
export function PartyLedgerPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const p = usePeriod("all");
  const { data: parties = [] } = useQuery({ queryKey: keys.parties, queryFn: api.listParties });
  const { data, isFetching } = useQuery({
    queryKey: keys.report("party", id ?? "", p.from, p.to),
    queryFn: () => api.partyLedger(id!, p.from, p.to),
    enabled: !!id && p.ready,
    placeholderData: (prev) => prev,
  });
  const options = parties.map((x) => ({ id: x.id, label: x.name, detail: x.gstin ?? undefined }));
  const owes = data && data.closingPaise > 0 ? "owes you" : "you owe";
  return (
    <div>
      <ReportHeader
        kicker="Party statement"
        title={id && data ? data.title : "Party statement"}
        subtitle={id && data ? (data.closingPaise ? `${data.title} ${owes} ${formatPaise(Math.abs(data.closingPaise))}` : "Settled") : "Choose a customer or supplier."}
      />
      <div className="mb-4 w-[360px]">
        <Combobox ariaLabel="Party" options={options} value={id ?? null} placeholder="Search parties"
          onSelect={(v) => v && navigate(`/reports/party/${v}?${params.toString()}`, { replace: !!id })} />
      </div>
      {id && <PeriodBar period={p} busy={isFetching} />}
      {id && (data ? <LedgerTable l={data} /> : <Loading />)}
    </div>
  );
}

export function JournalPage() {
  const p = usePeriod("month");
  const navigate = useNavigate();
  const { data, isFetching } = useQuery({
    queryKey: keys.report("journal", p.from, p.to),
    queryFn: () => api.journalRegister(p.from, p.to),
    enabled: p.ready,
    placeholderData: (prev) => prev,
  });
  return (
    <div>
      <ReportHeader kicker="Report" title="Journal register" subtitle="Every posting, as it went into the books." />
      <PeriodBar period={p} busy={isFetching} />
      {!data ? (
        <Loading />
      ) : (
        <>
          {data.truncated && <p className="mb-3 text-[12.5px] text-warning">Showing the first 1,000 entries. Narrow the dates to see the rest.</p>}
          <div className="space-y-3">
            {data.entries.map((e) => {
              const link = sourceLink(e.sourceType, e.sourceId);
              return (
                <Card key={e.entryId} className="overflow-hidden">
                  <div className="flex items-center gap-3 border-b border-line bg-surface-2 px-4 py-2 text-[13px]">
                    <span className="num text-muted">{shortDate(e.entryDate)}</span>
                    {link ? (
                      <button type="button" className="font-medium text-accent hover:underline" onClick={() => navigate(link)}>{sourceLabel(e.sourceType)}</button>
                    ) : (
                      <span className="font-medium">{sourceLabel(e.sourceType)}</span>
                    )}
                    <span className="min-w-0 flex-1 truncate text-muted">{e.narration}</span>
                    {e.reversesEntryId && <Badge icon={<RotateCcw size={11} />}>Reversal</Badge>}
                    <span className="num font-medium">{formatPaise(e.totalPaise)}</span>
                  </div>
                  <table className="w-full text-[13px]">
                    <tbody>
                      {e.lines.map((l, i) => (
                        <tr key={i}>
                          <td className={clsx("px-4 py-1.5", l.creditPaise > 0 && "pl-10")}>
                            <Link to={`/reports/ledger/${l.accountId}`} className="hover:text-accent">{l.accountName}</Link>
                            {l.partyName && <span className="text-muted"> · {l.partyName}</span>}
                          </td>
                          <td className="w-[150px] px-4 py-1.5 text-right"><Amount paise={l.debitPaise} /></td>
                          <td className="w-[150px] px-4 py-1.5 text-right"><Amount paise={l.creditPaise} /></td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </Card>
              );
            })}
            {data.entries.length === 0 && <Card className="px-4 py-10 text-center text-[13px] text-muted">Nothing posted in this period.</Card>}
          </div>
        </>
      )}
    </div>
  );
}

export function AgeingPage() {
  const { kind = "receivables" } = useParams();
  const k = kind === "payables" ? "payables" : "receivables";
  const p = usePeriod();
  const navigate = useNavigate();
  const [open, setOpen] = useState<string | null>(null);
  const [view, setView] = useState<"party" | "document">("party");
  const asOf = p.to && p.to < todayIso() ? p.to : todayIso();
  const { data, isFetching } = useQuery({
    queryKey: keys.report("ageing", k, asOf),
    queryFn: () => api.ageing(k, asOf),
    enabled: p.ready,
    placeholderData: (prev) => prev,
  });
  const docLink = (kind: string, id: string) => (kind === "sales" ? `/sales/${id}` : `/purchases/${id}`);
  return (
    <div>
      <ReportHeader
        kicker="Report"
        title={k === "receivables" ? "Receivables ageing" : "Payables ageing"}
        subtitle={`${k === "receivables" ? "What customers owe you" : "What you owe suppliers"}, as on ${shortDate(asOf)}`}
        actions={
          <Segmented<"party" | "document"> label="Group by" value={view} onChange={setView}
            options={[{ value: "party", label: "By party" }, { value: "document", label: "By document" }]} />
        }
      />
      <PeriodBar period={p} single busy={isFetching} />
      {!data ? (
        <Loading />
      ) : (
        <Card className="overflow-hidden">
          <table className="w-full text-[13px]">
            <thead>
              <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
                <th className="px-4 py-2 font-medium">{view === "party" ? "Party" : "Document"}</th>
                {data.bucketLabels.map((b) => (
                  <th key={b} className="px-3 py-2 text-right font-medium">{b}</th>
                ))}
                {view === "party" && <th className="px-3 py-2 text-right font-medium">Unallocated</th>}
                <th className="px-4 py-2 text-right font-medium">Total</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {view === "party"
                ? data.parties.map((x) => (
                    <PartyRows key={x.partyId} x={x} expanded={open === x.partyId} onToggle={() => setOpen(open === x.partyId ? null : x.partyId)} docLink={docLink} />
                  ))
                : data.parties
                    .flatMap((x) => x.documents.map((d) => ({ d, party: x.partyName })))
                    .sort((a, b) => b.d.daysOverdue - a.d.daysOverdue)
                    .map(({ d, party }) => (
                      <tr key={d.id} tabIndex={0} onClick={() => navigate(docLink(d.documentKind, d.id))} onKeyDown={(e) => e.key === "Enter" && navigate(docLink(d.documentKind, d.id))} className="cursor-pointer hover:bg-hover">
                        <td className="px-4 py-2">
                          <span className="num text-accent">{d.number ?? "—"}</span> <span className="text-muted">· {party}</span>
                          <div className="text-[12px] text-muted">{d.daysOverdue > 0 ? `${d.daysOverdue} days overdue` : `Due ${shortDate(d.dueDate ?? d.docDate)}`}</div>
                        </td>
                        {[0, 1, 2, 3, 4].map((i) => (
                          <td key={i} className="px-3 py-2 text-right"><Amount paise={d.bucket === i ? d.outstandingPaise : 0} /></td>
                        ))}
                        <td className="px-4 py-2 text-right font-medium"><Amount paise={d.outstandingPaise} /></td>
                      </tr>
                    ))}
              {data.parties.length === 0 && (
                <tr>
                  <td colSpan={8} className="px-4 py-10 text-center text-muted">Nothing outstanding.</td>
                </tr>
              )}
            </tbody>
            <tfoot>
              <tr className="border-t border-line-strong bg-surface-2 font-semibold">
                <td className="px-4 py-2.5">Total</td>
                {data.buckets.map((b, i) => (
                  <td key={i} className="px-3 py-2.5 text-right"><Amount paise={b} zero /></td>
                ))}
                {view === "party" && <td className="px-3 py-2.5 text-right"><Amount paise={data.unallocatedPaise} zero /></td>}
                <td className="px-4 py-2.5 text-right"><Amount paise={view === "party" ? data.totalPaise : data.buckets.reduce((a, b) => a + b, 0)} zero /></td>
              </tr>
            </tfoot>
          </table>
          <p className="border-t border-line px-4 py-2 text-[12px] text-muted">
            Ages count from the due date. "Unallocated" is opening balances and advances not tied to a document. {isFetching && "Updating…"}
          </p>
        </Card>
      )}
    </div>
  );
}

function PartyRows({ x, expanded, onToggle, docLink }: { x: import("../../lib/api").AgeingParty; expanded: boolean; onToggle: () => void; docLink: (k: string, id: string) => string }) {
  const navigate = useNavigate();
  return (
    <>
      <tr tabIndex={0} onClick={onToggle} onKeyDown={(e) => e.key === "Enter" && onToggle()} aria-expanded={expanded} className="cursor-pointer hover:bg-hover">
        <td className="px-4 py-2 font-medium">
          {x.partyName}
          <Link to={`/reports/party/${x.partyId}`} onClick={(e) => e.stopPropagation()} className="ml-2 text-[12px] font-normal text-accent hover:underline">Statement</Link>
        </td>
        {x.buckets.map((b, i) => (
          <td key={i} className="px-3 py-2 text-right"><Amount paise={b} /></td>
        ))}
        <td className="px-3 py-2 text-right"><Amount paise={x.unallocatedPaise} /></td>
        <td className="px-4 py-2 text-right font-semibold"><Amount paise={x.totalPaise} zero /></td>
      </tr>
      {expanded &&
        x.documents.map((d) => (
          <tr key={d.id} onClick={() => navigate(docLink(d.documentKind, d.id))} className="cursor-pointer bg-surface-2/40 text-[12.5px] hover:bg-hover">
            <td className="py-1.5 pl-8 pr-4">
              <span className="num text-accent">{d.number ?? "—"}</span>
              <span className="ml-2 text-muted">{d.daysOverdue > 0 ? `${d.daysOverdue} days overdue` : `due ${shortDate(d.dueDate ?? d.docDate)}`}</span>
            </td>
            {[0, 1, 2, 3, 4].map((i) => (
              <td key={i} className="px-3 py-1.5 text-right"><Amount paise={d.bucket === i ? d.outstandingPaise : 0} /></td>
            ))}
            <td />
            <td className="px-4 py-1.5 text-right"><Amount paise={d.outstandingPaise} /></td>
          </tr>
        ))}
    </>
  );
}
