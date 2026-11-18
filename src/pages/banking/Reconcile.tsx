import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router";
import clsx from "clsx";
import { ArrowLeft, BookOpen, Check, CheckCheck, EyeOff, Link2, Loader2, Plus, Undo2, Upload } from "lucide-react";
import { api, keys, type MatchCandidate, type StatementImportResult, type StatementLine } from "../../lib/api";
import { formatPaise, paiseToInput } from "../../lib/format";
import { shortDate } from "../../lib/sales";
import { dayGapLabel, sourceLabel, sourceLink } from "../../lib/banking";
import { useToast } from "../../components/Toast";
import { Badge, Button, Card, EmptyState, Segmented } from "../../components/ui";
import { StatementImportDialog } from "./StatementImportDialog";

type Filter = "open" | "matched" | "ignored" | "all";

export function Reconcile() {
  const { id = "" } = useParams();
  const qc = useQueryClient();
  const toast = useToast();
  const { data: rec } = useQuery({ queryKey: keys.reconciliation(id), queryFn: () => api.reconciliation(id) });
  const [importing, setImporting] = useState(false);
  const [lastImport, setLastImport] = useState<StatementImportResult | null>(null);
  const [filter, setFilter] = useState<Filter>("open");

  const refresh = () => {
    for (const k of [keys.reconciliation(id), keys.moneyAccounts, ["moneyBook"], ["audit"], keys.vouchers]) void qc.invalidateQueries({ queryKey: k });
  };
  const act = useMutation({
    mutationFn: (f: () => Promise<unknown>) => f(),
    onSuccess: refresh,
    onError: (e) => toast(e.message, "error"),
  });
  const undo = useMutation({
    mutationFn: (batch: string) => api.undoStatementImport(id, batch),
    onSuccess: (n) => {
      toast(`${n} line${n === 1 ? "" : "s"} removed`);
      setLastImport(null);
      refresh();
    },
    onError: (e) => toast(e.message, "error"),
  });

  // Lines whose best suggestion is unambiguous: one candidate, within 3 days,
  // and not wanted by any other line.
  const sure = useMemo(() => {
    if (!rec) return [];
    const open = rec.lines.filter((l) => l.status === "unmatched" && l.suggestions.length === 1 && Math.abs(l.suggestions[0].dayGap) <= 3);
    const wanted = new Map<string, number>();
    for (const l of open) wanted.set(l.suggestions[0].entryId, (wanted.get(l.suggestions[0].entryId) ?? 0) + 1);
    return open.filter((l) => wanted.get(l.suggestions[0].entryId) === 1);
  }, [rec]);
  const matchAll = useMutation({
    mutationFn: async () => {
      for (const l of sure) await api.matchStatementLine(l.id, l.suggestions[0].entryId);
      return sure.length;
    },
    onSuccess: (n) => {
      toast(`${n} line${n === 1 ? "" : "s"} matched`);
      refresh();
    },
    onError: (e) => {
      toast(e.message, "error");
      refresh();
    },
  });

  if (!rec) {
    return (
      <div className="grid place-items-center py-24 text-muted">
        <Loader2 className="animate-spin" />
      </div>
    );
  }
  const m = rec.account;
  const MATCH: Record<Filter, (l: StatementLine) => boolean> = {
    open: (l) => l.status === "unmatched",
    matched: (l) => l.status === "matched",
    ignored: (l) => l.status === "ignored",
    all: () => true,
  };
  const shown = rec.lines.filter(MATCH[filter]);
  const diff = m.statementBalancePaise != null ? m.balancePaise - m.statementBalancePaise : null;

  return (
    <div>
      <Link to="/banking" className="mb-2 inline-flex items-center gap-1 text-[13px] text-muted hover:text-fg">
        <ArrowLeft size={14} /> Banking
      </Link>
      <div className="mb-6 flex items-end justify-between gap-4">
        <div>
          <div className="text-[12px] font-semibold uppercase tracking-[0.08em] text-accent">Reconciliation</div>
          <h1 className="text-display">{m.name}</h1>
        </div>
        <div className="flex gap-2">
          <Link to={`/banking/accounts/${m.id}`}>
            <Button>
              <BookOpen size={15} /> Bank book
            </Button>
          </Link>
          <Button variant="primary" onClick={() => setImporting(true)}>
            <Upload size={15} /> Import statement
          </Button>
        </div>
      </div>

      <div className="mb-5 grid grid-cols-3 gap-4">
        <Stat label="Balance in books" value={formatPaise(m.balancePaise)} />
        <Stat label={m.lastStatementDate ? `Bank balance on ${shortDate(m.lastStatementDate)}` : "Bank balance"} value={m.statementBalancePaise != null ? formatPaise(m.statementBalancePaise) : "—"} note={diff ? `Differs by ${formatPaise(Math.abs(diff))}` : diff === 0 ? "Matches the books" : undefined} warn={!!diff} />
        <Stat label="Lines to reconcile" value={String(m.unmatchedLines)} note={rec.unmatchedEntries.length ? `${rec.unmatchedEntries.length} book ${rec.unmatchedEntries.length === 1 ? "entry" : "entries"} not on a statement` : undefined} />
      </div>

      {lastImport && (
        <Card className="mb-4 flex items-center justify-between gap-4 px-4 py-3 text-[13px]">
          <span>
            Imported {lastImport.imported} line{lastImport.imported === 1 ? "" : "s"}
            {lastImport.duplicates > 0 && `, skipped ${lastImport.duplicates} already there`}.
          </span>
          <Button variant="ghost" disabled={undo.isPending} onClick={() => undo.mutate(lastImport.batch)}>
            <Undo2 size={15} /> Undo import
          </Button>
        </Card>
      )}

      {rec.lines.length === 0 ? (
        <EmptyState
          icon={<Upload size={22} />}
          title="Import a bank statement"
          body="Download the statement from net banking as Excel or CSV. Each line is matched to receipts, payments, invoices or bills already in your books — or recorded from the line itself."
          actions={
            <Button variant="primary" onClick={() => setImporting(true)}>
              Import statement
            </Button>
          }
        />
      ) : (
        <>
          <div className="mb-3 flex items-center justify-between gap-4">
            <Segmented<Filter>
              label="Show"
              value={filter}
              onChange={setFilter}
              options={(["open", "matched", "ignored", "all"] as Filter[]).map((f) => ({
                value: f,
                label: { open: "To reconcile", matched: "Matched", ignored: "Ignored", all: "All" }[f],
                count: rec.lines.filter(MATCH[f]).length,
              }))}
            />
            {sure.length > 0 && (
              <Button disabled={matchAll.isPending} onClick={() => matchAll.mutate()}>
                {matchAll.isPending ? <Loader2 size={15} className="animate-spin" /> : <CheckCheck size={15} />} Match {sure.length} sure match{sure.length === 1 ? "" : "es"}
              </Button>
            )}
          </div>
          <Card className="overflow-hidden">
            <table className="w-full text-[13px]">
              <thead>
                <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
                  <th className="px-4 py-2 font-medium">Date</th>
                  <th className="px-4 py-2 font-medium">Statement line</th>
                  <th className="px-4 py-2 text-right font-medium">Withdrawal</th>
                  <th className="px-4 py-2 text-right font-medium">Deposit</th>
                  <th className="w-[38%] px-4 py-2 font-medium">In your books</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {shown.map((l) => (
                  <LineRow key={l.id} l={l} accountId={m.id} busy={act.isPending} act={(f) => act.mutate(f)} />
                ))}
                {shown.length === 0 && (
                  <tr>
                    <td colSpan={5} className="px-4 py-10 text-center text-muted">
                      {filter === "open" ? "Everything is reconciled." : "Nothing here."}
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </Card>
          {rec.unmatchedEntries.length > 0 && (
            <details className="mt-5">
              <summary className="cursor-pointer text-[13px] font-medium text-muted hover:text-fg">
                In your books but not on any imported statement ({rec.unmatchedEntries.length})
              </summary>
              <Card className="mt-2 divide-y divide-line text-[13px]">
                {rec.unmatchedEntries.map((c) => (
                  <EntryRow key={c.entryId} c={c} />
                ))}
              </Card>
              <p className="mt-2 text-[12px] text-muted">Usually cheques not yet cleared or entries dated after the statement.</p>
            </details>
          )}
        </>
      )}

      {importing && <StatementImportDialog account={m} onClose={() => setImporting(false)} onImported={setLastImport} />}
    </div>
  );
}

function Stat({ label, value, note, warn }: { label: string; value: string; note?: string; warn?: boolean }) {
  return (
    <Card className="p-4">
      <div className="text-[11.5px] font-semibold uppercase tracking-[0.05em] text-muted">{label}</div>
      <div className="num mt-1 text-[22px] font-semibold">{value}</div>
      {note && <div className={clsx("mt-0.5 text-[12px]", warn ? "text-warning" : "text-muted")}>{note}</div>}
    </Card>
  );
}

function EntryRow({ c, children }: { c: MatchCandidate; children?: React.ReactNode }) {
  const link = sourceLink(c.sourceType, c.sourceId);
  return (
    <div className="flex items-center gap-3 px-4 py-2">
      <div className="min-w-0 flex-1">
        <div className="truncate">
          {link ? (
            <Link to={link} className="text-accent hover:underline">
              {sourceLabel(c.sourceType)}
            </Link>
          ) : (
            sourceLabel(c.sourceType)
          )}
          {c.partyName && <span> · {c.partyName}</span>}
        </div>
        <div className="num text-[12px] text-muted">
          {shortDate(c.entryDate)} · {formatPaise(Math.abs(c.amountPaise))} {c.amountPaise >= 0 ? "in" : "out"}
        </div>
      </div>
      {children}
    </div>
  );
}

function LineRow({ l, accountId, busy, act }: { l: StatementLine; accountId: string; busy: boolean; act: (f: () => Promise<unknown>) => void }) {
  const navigate = useNavigate();
  const deposit = l.depositPaise > 0;
  const create = () => {
    const p = new URLSearchParams({
      type: deposit ? "receipt" : "payment",
      line: l.id,
      account: accountId,
      date: l.lineDate,
      amount: paiseToInput(Math.max(l.depositPaise, l.withdrawalPaise)),
      narration: l.description.slice(0, 200),
    });
    if (l.reference) p.set("ref", l.reference);
    navigate(`/banking/vouchers/new?${p.toString()}`);
  };
  return (
    <tr className={clsx("align-top", l.status !== "unmatched" && "text-muted")}>
      <td className="num whitespace-nowrap px-4 py-2.5">{shortDate(l.lineDate)}</td>
      <td className="max-w-[300px] px-4 py-2.5">
        <div className="break-words text-fg">{l.description}</div>
        {l.reference && <div className="num text-[12px] text-muted">{l.reference}</div>}
      </td>
      <td className="num px-4 py-2.5 text-right">{l.withdrawalPaise ? formatPaise(l.withdrawalPaise) : ""}</td>
      <td className="num px-4 py-2.5 text-right">{l.depositPaise ? formatPaise(l.depositPaise) : ""}</td>
      <td className="px-2 py-1">
        {l.status === "matched" && l.matched ? (
          <EntryRow c={l.matched}>
            <Badge tone="success" icon={<Check size={12} />}>Matched</Badge>
            <button type="button" disabled={busy} className="text-[12.5px] text-accent hover:underline" onClick={() => act(() => api.unmatchStatementLine(l.id))}>
              Unmatch
            </button>
          </EntryRow>
        ) : l.status === "ignored" ? (
          <div className="flex items-center gap-3 px-2 py-2">
            <Badge>Ignored</Badge>
            <button type="button" disabled={busy} className="text-[12.5px] text-accent hover:underline" onClick={() => act(() => api.unmatchStatementLine(l.id))}>
              Restore
            </button>
          </div>
        ) : (
          <div>
            {l.suggestions.map((s) => (
              <EntryRow key={s.entryId} c={s}>
                <span className="text-[12px] text-subtle">{dayGapLabel(s.dayGap)}</span>
                <Button className="h-7 px-2.5 text-[12.5px]" disabled={busy} onClick={() => act(() => api.matchStatementLine(l.id, s.entryId))}>
                  <Link2 size={13} /> Match
                </Button>
              </EntryRow>
            ))}
            <div className="flex items-center gap-3 px-2 py-2">
              {l.suggestions.length === 0 && <span className="text-[12.5px] text-muted">No matching entry.</span>}
              <button type="button" className="inline-flex items-center gap-1 text-[12.5px] font-medium text-accent hover:underline" onClick={create}>
                <Plus size={13} /> Record {deposit ? "receipt" : "payment"}
              </button>
              <button type="button" disabled={busy} className="inline-flex items-center gap-1 text-[12.5px] text-muted hover:text-fg" onClick={() => act(() => api.ignoreStatementLine(l.id))}>
                <EyeOff size={13} /> Ignore
              </button>
            </div>
          </div>
        )}
      </td>
    </tr>
  );
}
