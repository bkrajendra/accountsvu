import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Lock, LockOpen, Loader2 } from "lucide-react";
import { api, keys, type YearStatus } from "../../lib/api";
import { formatPaise } from "../../lib/format";
import { shortDate, todayIso } from "../../lib/sales";
import { useSave } from "../../lib/mutations";
import { Badge, Button, Card } from "../../components/ui";
import { Dialog } from "../../components/Dialog";
import { Field, TextInput } from "../../components/form";
import { SettingsSection } from "./SettingsSection";

const INVALIDATE = [keys.booksControl, ["report"]];

/** Period lock and financial-year close (owner only; reopening needs a reason). */
export function PeriodsTab() {
  const { data } = useQuery({ queryKey: keys.booksControl, queryFn: api.booksControl });
  const [lockOpen, setLockOpen] = useState(false);
  const [closing, setClosing] = useState<YearStatus | null>(null);
  const [reopening, setReopening] = useState<YearStatus | null>(null);

  if (!data) {
    return (
      <div className="grid place-items-center py-16 text-muted">
        <Loader2 className="animate-spin" />
      </div>
    );
  }

  return (
    <>
      <SettingsSection title="Period lock" description="Nothing can be posted, changed or cancelled on or before the lock date — invoices, bills, receipts or opening balances. Use it once a month's GST return is filed.">
        <Card className="flex items-center gap-4 p-5">
          <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">{data.lock ? <Lock size={18} /> : <LockOpen size={18} />}</span>
          <div className="min-w-0 flex-1 text-[13px]">
            {data.lock ? (
              <>
                <div className="font-semibold">Locked up to {shortDate(data.lock.lockedUntil)}</div>
                {data.lock.reason && <div className="text-muted">{data.lock.reason}</div>}
              </>
            ) : (
              <>
                <div className="font-semibold">Books are open</div>
                <div className="text-muted">Entries can be posted on any date from {shortDate(data.booksStartDate)}.</div>
              </>
            )}
          </div>
          <Button onClick={() => setLockOpen(true)}>{data.lock ? "Change lock" : "Lock books"}</Button>
        </Card>
      </SettingsSection>

      <SettingsSection title="Financial years" description="Closing a year moves its profit or loss into retained earnings and locks it. Balances carry forward. Years close in order and reopen newest first.">
        <Card className="divide-y divide-line">
          {data.years.map((y) => (
            <div key={y.code} className="flex items-center gap-4 px-5 py-3 text-[13px]">
              <div className="min-w-0 flex-1">
                <div className="font-medium">
                  {y.label} <span className="font-normal text-muted">· {shortDate(y.startDate)} – {shortDate(y.endDate)}</span>
                </div>
                {y.close && (
                  <div className="text-muted">
                    Closed {shortDate(todayIso(new Date(y.close.closedAt)))} · {y.close.profitPaise >= 0 ? "profit" : "loss"} <span className="num">{formatPaise(Math.abs(y.close.profitPaise))}</span> to retained earnings
                  </div>
                )}
              </div>
              {y.status === "closed" ? <Badge tone="success" icon={<Lock size={11} />}>Closed</Badge> : y.ended ? <Badge tone="warning">Ended, open</Badge> : <Badge tone="accent">Current</Badge>}
              {y.status === "closed" ? (
                <Button variant="ghost" onClick={() => setReopening(y)}>Reopen</Button>
              ) : (
                <Button disabled={!y.ended} title={y.ended ? undefined : "A year can be closed once it has ended."} onClick={() => setClosing(y)}>Close year</Button>
              )}
            </div>
          ))}
        </Card>
        {data.history.some((h) => h.status === "reopened") && (
          <div className="mt-3 text-[12.5px] text-muted">
            {data.history
              .filter((h) => h.status === "reopened")
              .map((h) => (
                <p key={h.id}>
                  {h.financialYear} reopened {shortDate(h.reopenedAt ? todayIso(new Date(h.reopenedAt)) : null)}: {h.reopenReason}
                </p>
              ))}
          </div>
        )}
      </SettingsSection>

      {lockOpen && <LockDialog current={data.lock?.lockedUntil ?? null} onClose={() => setLockOpen(false)} />}
      {closing && <CloseDialog y={closing} onClose={() => setClosing(null)} />}
      {reopening && <ReopenDialog y={reopening} onClose={() => setReopening(null)} />}
    </>
  );
}

function LockDialog({ current, onClose }: { current: string | null; onClose: () => void }) {
  const [until, setUntil] = useState(current ?? "");
  const [reason, setReason] = useState("");
  const save = useSave((x: { until: string | null; reason: string }) => api.setBooksLock(x.until, x.reason), {
    invalidate: INVALIDATE,
    success: "Period lock updated",
    onSuccess: onClose,
  });
  const reopening = !!current && (!until || until < current);
  return (
    <Dialog
      open
      onClose={onClose}
      width={480}
      title="Lock the books"
      description="Choose the last date that can no longer change."
      footer={
        <>
          {current && (
            <Button variant="ghost" className="mr-auto" onClick={() => setUntil("")}>
              Remove lock
            </Button>
          )}
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={save.isPending || (!until && !current)} onClick={() => save.mutate({ until: until || null, reason })}>
            {save.isPending && <Loader2 size={15} className="animate-spin" />} Save
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <Field label="Locked up to and including" error={save.fieldErrors.lockedUntil}>
          {(p) => <TextInput id={p.id} type="date" invalid={p.invalid} max={todayIso()} value={until} onChange={(e) => setUntil(e.target.value)} />}
        </Field>
        {reopening && (
          <Field label="Why reopen?" error={save.fieldErrors.reason} hint="Kept in the activity log.">
            {(p) => <TextInput id={p.id} invalid={p.invalid} autoFocus value={reason} onChange={(e) => setReason(e.target.value)} placeholder="e.g. late supplier bill for October" />}
          </Field>
        )}
      </div>
    </Dialog>
  );
}

function CloseDialog({ y, onClose }: { y: YearStatus; onClose: () => void }) {
  const { data: pl } = useQuery({ queryKey: keys.report("pl", y.startDate, y.endDate), queryFn: () => api.profitAndLoss(y.startDate, y.endDate) });
  const save = useSave(() => api.closeFinancialYear(y.code), {
    invalidate: INVALIDATE,
    success: `${y.label} closed`,
    onSuccess: onClose,
  });
  return (
    <Dialog
      open
      onClose={onClose}
      width={480}
      title={`Close ${y.label}?`}
      description="Income and expenses are moved to retained earnings by one entry on the last day, and the year is locked. You can reopen it later with a reason."
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={save.isPending} onClick={() => save.mutate(undefined)}>
            {save.isPending && <Loader2 size={15} className="animate-spin" />} Close year
          </Button>
        </>
      }
    >
      <div className="flex justify-between rounded-xl bg-surface-2 px-4 py-3 text-[13.5px]">
        <span>{pl && pl.netProfitPaise < 0 ? "Net loss" : "Net profit"}</span>
        <span className="num font-semibold">{pl ? formatPaise(Math.abs(pl.netProfitPaise)) : "…"}</span>
      </div>
    </Dialog>
  );
}

function ReopenDialog({ y, onClose }: { y: YearStatus; onClose: () => void }) {
  const [reason, setReason] = useState("");
  const save = useSave((r: string) => api.reopenFinancialYear(y.code, r), {
    invalidate: INVALIDATE,
    success: `${y.label} reopened`,
    onSuccess: onClose,
  });
  return (
    <Dialog
      open
      onClose={onClose}
      width={480}
      title={`Reopen ${y.label}?`}
      description="The closing entry is reversed and the year unlocked, so entries can be posted in it again. Close it again when you're done."
      footer={
        <>
          <Button onClick={onClose}>Keep closed</Button>
          <Button variant="primary" disabled={save.isPending} onClick={() => save.mutate(reason)}>
            Reopen year
          </Button>
        </>
      }
    >
      <Field label="Why reopen?" error={save.fieldErrors.reason} hint="Kept in the activity log.">
        {(p) => <TextInput id={p.id} invalid={p.invalid} autoFocus value={reason} onChange={(e) => setReason(e.target.value)} placeholder="e.g. auditor's adjustment" />}
      </Field>
    </Dialog>
  );
}
