import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";
import { api, keys, type VoucherView } from "../../lib/api";
import { formatPaise, paiseToInput } from "../../lib/format";
import { shortDate, todayIso } from "../../lib/sales";
import { useSave } from "../../lib/mutations";
import { Button } from "../../components/ui";
import { Dialog } from "../../components/Dialog";
import { Field, TextInput } from "../../components/form";
import { inputToPaise } from "./VoucherEditor";

/**
 * Adjust what is left of an advance against the party's open invoices (a
 * receipt) or bills (a payment). `documentId` puts that document first.
 */
export function AdjustAdvanceDialog({ v, documentId, onClose }: { v: VoucherView; documentId?: string; onClose: () => void }) {
  const receipt = v.voucherType === "receipt";
  const { data: openAll = [], isLoading } = useQuery({
    queryKey: keys.openDocuments(v.partyId ?? "", v.voucherType),
    queryFn: () => api.openDocuments(v.partyId!, v.voucherType),
    enabled: !!v.partyId,
  });
  const open = useMemo(() => {
    const first = openAll.filter((d) => d.id === documentId);
    return [...first, ...openAll.filter((d) => d.id !== documentId)];
  }, [openAll, documentId]);
  // Oldest first (or the named document first), until the advance runs out.
  const fifo = useMemo(() => {
    let left = v.onAccountPaise;
    const out: Record<string, string> = {};
    for (const d of open) {
      const take = Math.min(left, d.outstandingPaise);
      out[d.id] = take > 0 ? paiseToInput(take) : "";
      left -= take;
    }
    return out;
  }, [open, v.onAccountPaise]);
  const [edited, setEdited] = useState<Record<string, string> | null>(null);
  const amounts = edited ?? fifo;
  const latest = open.reduce((m, d) => (d.docDate > m ? d.docDate : m), v.voucherDate);
  const [date, setDate] = useState(todayIso() < latest ? latest : todayIso());
  const total = open.reduce((a, d) => a + (inputToPaise(amounts[d.id]) ?? 0), 0);

  const save = useSave(() => api.adjustAdvance(v.id, { date, allocations: open.map((d) => ({ documentId: d.id, amount: amounts[d.id] ?? "" })).filter((a) => a.amount) }), {
    invalidate: [keys.vouchers, keys.sales, keys.purchases, keys.parties, ["openDocuments"], ["partyAdvances"], ["settlements"]],
    success: "Advance adjusted",
    onSuccess: onClose,
  });
  const err = save.fieldErrors;
  const rowError = Object.entries(err).find(([k]) => k.startsWith("allocations."))?.[1];

  return (
    <Dialog
      open
      onClose={onClose}
      width={640}
      title={`Adjust ${v.number}`}
      description={`${formatPaise(v.onAccountPaise)} of this advance is left. Use it to settle ${receipt ? "invoices" : "bills"} raised since.${v.advanceTax ? " The GST paid on the part you adjust is taken back (GSTR-1 table 11B), as the invoice charges it." : ""}`}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={save.isPending || total <= 0} onClick={() => save.mutate(undefined)}>
            {save.isPending && <Loader2 size={15} className="animate-spin" />} Adjust {total > 0 ? formatPaise(total) : ""}
          </Button>
        </>
      }
    >
      <div className="grid gap-4">
        <Field label="Adjustment date" error={err.date} hint="On or after the advance and the documents it settles." className="max-w-[220px]">
          {(p) => <TextInput id={p.id} type="date" invalid={p.invalid} value={date} max={todayIso()} onChange={(e) => setDate(e.target.value)} />}
        </Field>
        {isLoading ? (
          <div className="grid place-items-center py-6 text-muted">
            <Loader2 className="animate-spin" size={18} />
          </div>
        ) : open.length === 0 ? (
          <p className="rounded-lg bg-surface-2 px-4 py-6 text-center text-[13px] text-muted">
            {v.partyName} has no {receipt ? "invoices" : "bills"} to settle.
          </p>
        ) : (
          <div className="overflow-hidden rounded-xl border border-line">
            <table className="w-full text-[13px]">
              <thead>
                <tr className="bg-surface-2 text-left text-[12px] text-muted">
                  <th className="px-3 py-2 font-medium">Number</th>
                  <th className="px-3 py-2 font-medium">Date</th>
                  <th className="px-3 py-2 text-right font-medium">Outstanding</th>
                  <th className="w-[150px] px-3 py-2 text-right font-medium">Adjust</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {open.map((d, i) => (
                  <tr key={d.id}>
                    <td className="num px-3 py-2">{d.number ?? "—"}</td>
                    <td className="num px-3 py-2 text-muted">{shortDate(d.docDate)}</td>
                    <td className="num px-3 py-2 text-right">{formatPaise(d.outstandingPaise)}</td>
                    <td className="px-3 py-1.5">
                      <TextInput
                        aria-label={`Adjust against ${d.number ?? "document"}`}
                        invalid={!!err[`allocations.${i}.amount`]}
                        className="num h-8 text-right"
                        inputMode="decimal"
                        placeholder="0.00"
                        value={amounts[d.id] ?? ""}
                        onChange={(e) => {
                          setEdited({ ...amounts, [d.id]: e.target.value });
                          save.clearError(`allocations.${i}.amount`);
                        }}
                      />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        <p className="flex justify-between text-[13px] text-muted">
          <span className={total > v.onAccountPaise ? "text-danger" : undefined}>
            Adjusting <span className="num font-medium text-fg">{formatPaise(total)}</span> of {formatPaise(v.onAccountPaise)}
          </span>
          <span>
            Left as advance <span className="num font-medium text-fg">{formatPaise(Math.max(v.onAccountPaise - total, 0))}</span>
          </span>
        </p>
        {(err.allocations || rowError) && <p className="text-[12.5px] text-danger">{err.allocations ?? rowError}</p>}
      </div>
    </Dialog>
  );
}
