import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router";
import clsx from "clsx";
import { ArrowLeft, CheckCircle2, GitCompareArrows, Loader2 } from "lucide-react";
import { api, keys } from "../../lib/api";
import { formatPaise } from "../../lib/format";
import { shortDate } from "../../lib/sales";
import { sourceLabel, sourceLink } from "../../lib/banking";
import { Button, Card } from "../../components/ui";
import { Field, TextInput } from "../../components/form";

/** Cash book / bank book: every entry through one account, with running balance. */
export function MoneyBook() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const { data: book, isFetching } = useQuery({
    queryKey: keys.moneyBook(id, from, to),
    queryFn: () => api.moneyBook(id, from, to),
    placeholderData: (prev) => prev,
  });

  if (!book) {
    return (
      <div className="grid place-items-center py-24 text-muted">
        <Loader2 className="animate-spin" />
      </div>
    );
  }
  const m = book.account;
  const bank = m.kind === "bank";

  return (
    <div>
      <Link to="/banking" className="mb-2 inline-flex items-center gap-1 text-[13px] text-muted hover:text-fg">
        <ArrowLeft size={14} /> Banking
      </Link>
      <div className="mb-6 flex items-end justify-between gap-4">
        <div>
          <div className="text-[12px] font-semibold uppercase tracking-[0.08em] text-accent">{bank ? "Bank book" : "Cash book"}</div>
          <h1 className="text-display">{m.name}</h1>
          <p className="mt-1 text-[14px] text-muted">
            Balance in books <span className="num font-semibold text-fg">{formatPaise(m.balancePaise)}</span>
          </p>
        </div>
        {bank && (
          <Link to={`/banking/accounts/${m.id}/reconcile`}>
            <Button>
              <GitCompareArrows size={15} /> Reconcile
            </Button>
          </Link>
        )}
      </div>

      <div className="mb-4 flex items-end gap-3">
        <Field label="From" className="w-[170px]">
          {(p) => <TextInput id={p.id} type="date" value={from} onChange={(e) => setFrom(e.target.value)} />}
        </Field>
        <Field label="To" className="w-[170px]">
          {(p) => <TextInput id={p.id} type="date" value={to} min={from || undefined} onChange={(e) => setTo(e.target.value)} />}
        </Field>
        {(from || to) && (
          <Button variant="ghost" onClick={() => (setFrom(""), setTo(""))}>
            All dates
          </Button>
        )}
        {isFetching && <Loader2 size={16} className="mb-2.5 animate-spin text-muted" />}
      </div>

      <Card className="overflow-hidden">
        <table className="w-full text-[13px]">
          <thead>
            <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
              <th className="px-4 py-2 font-medium">Date</th>
              <th className="px-4 py-2 font-medium">Particulars</th>
              <th className="px-4 py-2 font-medium">Source</th>
              <th className="px-4 py-2 text-right font-medium">{bank ? "Deposits" : "Received"}</th>
              <th className="px-4 py-2 text-right font-medium">{bank ? "Withdrawals" : "Paid"}</th>
              <th className="px-4 py-2 text-right font-medium">Balance</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            <tr className="bg-surface-2/50">
              <td className="num px-4 py-2 text-muted">{from ? shortDate(from) : "—"}</td>
              <td className="px-4 py-2 font-medium" colSpan={4}>
                Opening balance
              </td>
              <td className="num px-4 py-2 text-right font-medium">{formatPaise(book.openingPaise)}</td>
            </tr>
            {book.lines.map((l) => {
              const link = sourceLink(l.sourceType, l.sourceId);
              const open = link ? () => navigate(link) : undefined;
              return (
                <tr key={l.entryId} tabIndex={open ? 0 : undefined} onClick={open} onKeyDown={(e) => e.key === "Enter" && open?.()} className={clsx(open && "cursor-pointer hover:bg-hover focus-visible:bg-hover focus-visible:outline-none")}>
                  <td className="num px-4 py-2.5 text-muted">{shortDate(l.entryDate)}</td>
                  <td className="px-4 py-2.5">
                    {l.partyName ?? l.narration ?? "—"}
                    {l.partyName && l.narration && <div className="text-[12px] text-muted">{l.narration}</div>}
                  </td>
                  <td className="px-4 py-2.5 text-muted">
                    <span className={clsx(link && "text-accent")}>{sourceLabel(l.sourceType)}</span>
                    {l.statementLineId && <CheckCircle2 size={13} className="ml-1.5 inline text-success" aria-label="Reconciled" />}
                  </td>
                  <td className="num px-4 py-2.5 text-right">{l.debitPaise ? formatPaise(l.debitPaise) : ""}</td>
                  <td className="num px-4 py-2.5 text-right">{l.creditPaise ? formatPaise(l.creditPaise) : ""}</td>
                  <td className={clsx("num px-4 py-2.5 text-right", l.balancePaise < 0 && "text-danger")}>{formatPaise(l.balancePaise)}</td>
                </tr>
              );
            })}
            {book.lines.length === 0 && (
              <tr>
                <td colSpan={6} className="px-4 py-10 text-center text-muted">
                  No entries in this period.
                </td>
              </tr>
            )}
          </tbody>
          <tfoot>
            <tr className="border-t border-line-strong bg-surface-2 font-semibold">
              <td className="px-4 py-2.5" colSpan={3}>
                Closing balance
              </td>
              <td className="num px-4 py-2.5 text-right">{formatPaise(book.debitTotalPaise)}</td>
              <td className="num px-4 py-2.5 text-right">{formatPaise(book.creditTotalPaise)}</td>
              <td className={clsx("num px-4 py-2.5 text-right", book.closingPaise < 0 && "text-danger")}>{formatPaise(book.closingPaise)}</td>
            </tr>
          </tfoot>
        </table>
      </Card>
    </div>
  );
}
