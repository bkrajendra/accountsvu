import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Loader2, Pencil } from "lucide-react";
import { api, keys, type OpeningBalance, type OpeningBalanceInput } from "../../lib/api";
import { formatPaise, paiseToInput } from "../../lib/format";
import { shortDate } from "../../lib/sales";
import { useSave } from "../../lib/mutations";
import { Button, Card } from "../../components/ui";
import { Dialog } from "../../components/Dialog";
import { Field, Select, TextInput } from "../../components/form";

/**
 * Opening balances of balance-sheet ledgers (cash, bank, capital, loans,
 * assets) as on the books start date. Customer and supplier balances live on
 * each party.
 */
export function OpeningBalances() {
  const { data: rows = [] } = useQuery({ queryKey: keys.openingBalances, queryFn: api.listOpeningBalances });
  const { data: profile } = useQuery({ queryKey: keys.profile, queryFn: api.businessProfile });
  const [editing, setEditing] = useState<OpeningBalance | null>(null);
  if (!rows.length) return null;
  const debit = rows.reduce((a, r) => a + Math.max(r.amountPaise, 0), 0);
  const credit = rows.reduce((a, r) => a + Math.max(-r.amountPaise, 0), 0);
  const start = profile?.organisation?.booksStartDate;

  return (
    <div className="mt-8">
      <h3 className="mb-1 text-[12px] font-semibold uppercase tracking-[0.04em] text-muted">Opening balances</h3>
      <p className="mb-3 text-[12.5px] text-muted">
        As on {start ? shortDate(start) : "the day your books start"}. Customer and supplier balances are set on each party. Any difference sits in Opening balance adjustment.
      </p>
      <Card className="overflow-hidden">
        <table className="w-full text-[13px]">
          <thead>
            <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
              <th className="w-16 px-4 py-2 font-medium">Code</th>
              <th className="px-4 py-2 font-medium">Account</th>
              <th className="px-4 py-2 text-right font-medium">Debit</th>
              <th className="px-4 py-2 text-right font-medium">Credit</th>
              <th className="w-12 px-2 py-2" />
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {rows.map((r) => (
              <tr key={r.accountId}>
                <td className="num px-4 py-2 text-muted">{r.code}</td>
                <td className="px-4 py-2">{r.name}</td>
                <td className="num px-4 py-2 text-right">{r.amountPaise > 0 ? formatPaise(r.amountPaise) : ""}</td>
                <td className="num px-4 py-2 text-right">{r.amountPaise < 0 ? formatPaise(-r.amountPaise) : ""}</td>
                <td className="px-2 py-1 text-right">
                  <button type="button" aria-label={`Set opening balance of ${r.name}`} onClick={() => setEditing(r)} className="pressable grid size-7 place-items-center rounded-lg text-muted hover:bg-hover hover:text-fg">
                    <Pencil size={14} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr className="border-t border-line-strong bg-surface-2 font-semibold">
              <td className="px-4 py-2" colSpan={2}>
                Total
              </td>
              <td className="num px-4 py-2 text-right">{formatPaise(debit)}</td>
              <td className="num px-4 py-2 text-right">{formatPaise(credit)}</td>
              <td />
            </tr>
          </tfoot>
        </table>
      </Card>
      {editing && <OpeningDialog row={editing} onClose={() => setEditing(null)} />}
    </div>
  );
}

function OpeningDialog({ row, onClose }: { row: OpeningBalance; onClose: () => void }) {
  const natural = row.kind === "asset" ? "debit" : "credit";
  const [f, setF] = useState<OpeningBalanceInput>({
    accountId: row.accountId,
    amount: row.amountPaise ? paiseToInput(Math.abs(row.amountPaise)) : "",
    side: row.amountPaise ? (row.amountPaise > 0 ? "debit" : "credit") : natural,
  });
  const save = useSave((x: OpeningBalanceInput) => api.setOpeningBalance(x), {
    invalidate: [keys.openingBalances, keys.moneyAccounts, ["moneyBook"]],
    success: "Opening balance saved",
    onSuccess: onClose,
  });
  return (
    <Dialog
      open
      onClose={onClose}
      width={460}
      title={`Opening balance — ${row.name}`}
      description="Changing it later reverses the old opening entry and posts the new one."
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={save.isPending} onClick={() => save.mutate(f)}>
            {save.isPending && <Loader2 size={15} className="animate-spin" />} Save
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-2 gap-4">
        <Field label="Amount (₹)" error={save.fieldErrors.amount}>
          {(p) => <TextInput id={p.id} invalid={p.invalid} autoFocus className="num" inputMode="decimal" placeholder="0.00" value={f.amount ?? ""} onChange={(e) => setF((x) => ({ ...x, amount: e.target.value }))} />}
        </Field>
        <Field label="Side" error={save.fieldErrors.side}>
          {(p) => (
            <Select id={p.id} value={f.side} onChange={(e) => setF((x) => ({ ...x, side: e.target.value as "debit" | "credit" }))}>
              <option value="debit">Debit{natural === "debit" ? " (usual)" : ""}</option>
              <option value="credit">Credit{natural === "credit" ? " (usual)" : ""}</option>
            </Select>
          )}
        </Field>
      </div>
    </Dialog>
  );
}
