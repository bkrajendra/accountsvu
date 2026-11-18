import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useParams } from "react-router";
import clsx from "clsx";
import { ArrowLeft, Ban, CheckCircle2, Loader2 } from "lucide-react";
import { api, keys, type VoucherView } from "../../lib/api";
import { formatPaise } from "../../lib/format";
import { amountInWords, shortDate } from "../../lib/sales";
import { MODE_LABEL, VOUCHER_LABEL } from "../../lib/banking";
import { useSave } from "../../lib/mutations";
import { Badge, Button, Card } from "../../components/ui";
import { Dialog } from "../../components/Dialog";
import { Field, TextInput } from "../../components/form";

export function VoucherDocument() {
  const { id = "" } = useParams();
  const { data: v } = useQuery({ queryKey: keys.voucher(id), queryFn: () => api.getVoucher(id) });
  const [cancelling, setCancelling] = useState(false);

  if (!v) {
    return (
      <div className="grid place-items-center py-24 text-muted">
        <Loader2 className="animate-spin" />
      </div>
    );
  }
  const cancelled = v.status === "cancelled";
  const receipt = v.voucherType === "receipt";
  const who = v.partyName ?? v.counterAccountName ?? "—";

  return (
    <div className="mx-auto max-w-[880px]">
      <Link to="/banking?tab=vouchers" className="mb-2 inline-flex items-center gap-1 text-[13px] text-muted hover:text-fg">
        <ArrowLeft size={14} /> Receipts & payments
      </Link>
      <div className="mb-6 flex items-end justify-between gap-4">
        <div>
          <div className="text-[12px] font-semibold uppercase tracking-[0.08em] text-accent">{VOUCHER_LABEL[v.voucherType]}</div>
          <div className="flex items-center gap-3">
            <h1 className={clsx("text-display num", cancelled && "line-through decoration-2")}>{v.number}</h1>
            {cancelled ? <Badge>Cancelled</Badge> : <Badge tone="success">Posted</Badge>}
            {v.statementLineId && <Badge tone="accent" icon={<CheckCircle2 size={12} />}>Reconciled</Badge>}
          </div>
          <p className="mt-1 text-[14px] text-muted">
            {receipt ? "From" : "To"} {who}
          </p>
        </div>
        {!cancelled && (
          <Button onClick={() => setCancelling(true)}>
            <Ban size={15} /> Cancel
          </Button>
        )}
      </div>

      <Card className="grid grid-cols-4 gap-4 p-5 text-[13px]">
        <Info label="Date" value={shortDate(v.voucherDate)} />
        <Info label={receipt ? "Into" : "Out of"} value={<Link to={`/banking/accounts/${v.moneyAccountId}`} className="text-accent">{v.moneyAccountName}</Link>} />
        <Info label="Mode" value={MODE_LABEL[v.mode]} />
        <Info label="Reference" value={v.reference ?? "—"} mono />
        {v.narration && <Info label="Narration" value={v.narration} wide />}
      </Card>

      <Card className="mt-5 overflow-hidden">
        <div className="flex items-baseline justify-between border-b border-line px-4 py-3">
          <h2 className="text-[14px] font-semibold">{v.partyId ? (receipt ? "Invoices settled" : "Bills settled") : "Booked to"}</h2>
          <span className="num text-[18px] font-semibold">{formatPaise(v.amountPaise)}</span>
        </div>
        {v.partyId ? (
          <div className="divide-y divide-line text-[13px]">
            {v.allocations.map((a) => (
              <Link key={a.documentId} to={`/${a.documentKind === "sales" ? "sales" : "purchases"}/${a.documentId}`} className="flex items-center justify-between px-4 py-2.5 hover:bg-hover">
                <span>
                  <span className="num text-accent">{a.number ?? "Document"}</span>
                  <span className="ml-2 text-muted">{shortDate(a.docDate)}</span>
                </span>
                <span className="num">{formatPaise(a.amountPaise)}</span>
              </Link>
            ))}
            {v.onAccountPaise > 0 && (
              <div className="flex items-center justify-between px-4 py-2.5">
                <span className="text-muted">Advance, kept on {v.partyName}'s account</span>
                <span className="num">{formatPaise(v.onAccountPaise)}</span>
              </div>
            )}
          </div>
        ) : (
          <div className="flex justify-between px-4 py-2.5 text-[13px]">
            <span>{v.counterAccountName}</span>
            <span className="num">{formatPaise(v.amountPaise)}</span>
          </div>
        )}
        <p className="border-t border-line px-4 py-2 text-right text-[12px] text-muted">{amountInWords(v.amountPaise)}</p>
      </Card>

      {cancelled && (
        <Card className="mt-5 p-4 text-[13px]">
          <div className="font-semibold">Cancelled {shortDate(v.cancelledAt?.slice(0, 10))}</div>
          <p className="mt-1 text-muted">{v.cancelReason}</p>
          <p className="mt-2 text-[12px] text-subtle">A reversing entry was posted and the documents it settled are open again.</p>
        </Card>
      )}

      {cancelling && <CancelDialog v={v} onClose={() => setCancelling(false)} />}
    </div>
  );
}

function Info({ label, value, mono, wide }: { label: string; value: React.ReactNode; mono?: boolean; wide?: boolean }) {
  return (
    <div className={clsx(wide && "col-span-4")}>
      <div className="text-[11.5px] font-semibold uppercase tracking-[0.05em] text-muted">{label}</div>
      <div className={clsx("mt-0.5", mono && "num")}>{value}</div>
    </div>
  );
}

function CancelDialog({ v, onClose }: { v: VoucherView; onClose: () => void }) {
  const [reason, setReason] = useState("");
  const save = useSave((r: string) => api.cancelVoucher(v.id, r, v.rowVersion), {
    invalidate: [keys.vouchers, keys.voucher(v.id), keys.moneyAccounts, keys.parties, keys.sales, keys.purchases, ["moneyBook"], ["openDocuments"], ["settlements"], ["reconciliation"]],
    success: `${VOUCHER_LABEL[v.voucherType]} cancelled`,
    onSuccess: onClose,
  });
  return (
    <Dialog
      open
      onClose={onClose}
      title={`Cancel ${v.number}?`}
      description="A reversing entry is posted and the invoices or bills it settled become due again. This can't be undone."
      width={480}
      footer={
        <>
          <Button onClick={onClose}>Keep it</Button>
          <Button variant="primary" className="bg-danger hover:bg-danger" disabled={save.isPending} onClick={() => save.mutate(reason)}>
            Cancel {VOUCHER_LABEL[v.voucherType].toLowerCase()}
          </Button>
        </>
      }
    >
      <Field label="Reason" error={save.fieldErrors.reason} hint="Kept in the audit log, e.g. cheque bounced.">
        {(p) => <TextInput id={p.id} invalid={p.invalid} autoFocus value={reason} onChange={(e) => setReason(e.target.value)} />}
      </Field>
    </Dialog>
  );
}
