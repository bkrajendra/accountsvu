import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router";
import clsx from "clsx";
import { ArrowLeft, Ban, CheckCircle2, Link2, Loader2, Undo2 } from "lucide-react";
import { api, keys, type VoucherView } from "../../lib/api";
import { formatPaise, formatRate } from "../../lib/format";
import { AdjustAdvanceDialog } from "./AdjustAdvanceDialog";
import { PdfActions } from "../../components/PdfActions";
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
  const [adjusting, setAdjusting] = useState(false);
  const navigate = useNavigate();

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
  const refund = v.purpose === "refund";
  const label = refund ? (receipt ? "Refund received" : "Refund paid") : VOUCHER_LABEL[v.voucherType];
  const canUse = !cancelled && v.onAccountPaise > 0;
  const refundHref = `/banking/vouchers/new?type=${receipt ? "payment" : "receipt"}&party=${v.partyId}&purpose=refund&refundOf=${v.id}&amount=${(v.onAccountPaise / 100).toFixed(2)}`;

  return (
    <div className="mx-auto max-w-[880px]">
      <Link to="/banking?tab=vouchers" className="mb-2 inline-flex items-center gap-1 text-[13px] text-muted hover:text-fg">
        <ArrowLeft size={14} /> Receipts & payments
      </Link>
      <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
        <div>
          <div className="text-[12px] font-semibold uppercase tracking-[0.08em] text-accent">{label}</div>
          <div className="flex items-center gap-3">
            <h1 className={clsx("text-display num whitespace-nowrap", cancelled && "line-through decoration-2")}>{v.number}</h1>
            {cancelled ? <Badge>Cancelled</Badge> : <Badge tone="success">Posted</Badge>}
            {v.statementLineId && <Badge tone="accent" icon={<CheckCircle2 size={12} />}>Reconciled</Badge>}
          </div>
          <p className="mt-1 text-[14px] text-muted">
            {receipt ? "From" : "To"} {who}
            {v.refundOfNumber && v.refundOfId && (
              <>
                {" "}· refund of{" "}
                <Link to={`/banking/vouchers/${v.refundOfId}`} className="num text-accent hover:underline">
                  {v.refundOfNumber}
                </Link>
              </>
            )}
          </p>
        </div>
        <div className="flex shrink-0 flex-wrap justify-end gap-2">
          {canUse && (
            <>
              <Button onClick={() => setAdjusting(true)}>
                <Link2 size={15} /> {receipt ? "Adjust against invoices" : "Adjust against bills"}
              </Button>
              <Button onClick={() => navigate(refundHref)}>
                <Undo2 size={15} /> Refund
              </Button>
            </>
          )}
          <PdfActions open={() => api.openVoucherPdf(v.id)} save={() => api.saveVoucherPdf(v.id)} />
          {!cancelled && (
            <Button onClick={() => setCancelling(true)}>
              <Ban size={15} /> Cancel
            </Button>
          )}
        </div>
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
          <h2 className="text-[14px] font-semibold">{refund ? "Refund" : v.partyId ? (receipt ? "Invoices settled" : "Bills settled") : "Booked to"}</h2>
          <span className="num text-[18px] font-semibold">{formatPaise(v.amountPaise)}</span>
        </div>
        {v.partyId ? (
          <div className="divide-y divide-line text-[13px]">
            {v.allocations.map((a) => (
              <Link key={a.documentId} to={`/${a.documentKind === "sales" ? "sales" : "purchases"}/${a.documentId}`} className="flex items-center justify-between px-4 py-2.5 hover:bg-hover">
                <span>
                  <span className="num text-accent">{a.number ?? "Document"}</span>
                  <span className="ml-2 text-muted">{shortDate(a.docDate)}</span>
                  {a.allocatedOn && <span className="ml-2 text-[12px] text-subtle">adjusted {shortDate(a.allocatedOn)}</span>}
                </span>
                <span className="num">{formatPaise(a.amountPaise)}</span>
              </Link>
            ))}
            {v.refunds.map((r) => (
              <Link key={r.voucherId} to={`/banking/vouchers/${r.voucherId}`} className="flex items-center justify-between px-4 py-2.5 hover:bg-hover">
                <span>
                  Refunded by <span className="num text-accent">{r.number}</span>
                  <span className="ml-2 text-muted">{shortDate(r.voucherDate)}</span>
                </span>
                <span className="num">{formatPaise(r.amountPaise)}</span>
              </Link>
            ))}
            {refund && (
              <div className="flex items-center justify-between px-4 py-2.5">
                <span className="text-muted">{receipt ? `Refund from ${v.partyName}` : `Refund to ${v.partyName}`}</span>
                <span className="num">{formatPaise(v.amountPaise)}</span>
              </div>
            )}
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

      {v.advanceTax && (
        <Card className="mt-5 overflow-hidden text-[13px]">
          <div className="flex items-baseline justify-between border-b border-line px-4 py-3">
            <div>
              <h2 className="text-[14px] font-semibold">GST on the advance</h2>
              <p className="text-[12.5px] text-muted">
                <span className="num">{v.advanceTax.gstin}</span> · place of supply {v.advanceTax.placeName} · {formatRate(v.advanceTax.rateBp)} · GSTR-1 table 11A
              </p>
            </div>
            <span className="num text-[16px] font-semibold">{formatPaise(v.advanceTax.taxPaise)}</span>
          </div>
          <div className="divide-y divide-line">
            <div className="flex justify-between px-4 py-2">
              <span className="text-muted">Advance {formatPaise(v.advanceTax.grossPaise)} less GST</span>
              <span className="num">{formatPaise(v.advanceTax.taxablePaise)}</span>
            </div>
            {v.advanceAdjustments.map((a) => (
              <div key={a.id} className={clsx("flex justify-between px-4 py-2", a.status === "cancelled" && "text-subtle line-through")}>
                <span>
                  GST taken back · {a.reason === "invoice" ? "adjusted against an invoice" : "refunded"} {shortDate(a.adjustDate)}
                  <span className="ml-2 text-[12px] text-muted">on {formatPaise(a.grossPaise)} · table 11B</span>
                </span>
                <span className="num">−{formatPaise(a.taxPaise)}</span>
              </div>
            ))}
            <div className="flex justify-between px-4 py-2 font-medium">
              <span>GST still held on the advance</span>
              <span className="num">{formatPaise(v.advanceTax.remainingTaxPaise)}</span>
            </div>
          </div>
        </Card>
      )}

      {cancelled && (
        <Card className="mt-5 p-4 text-[13px]">
          <div className="font-semibold">Cancelled {shortDate(v.cancelledAt?.slice(0, 10))}</div>
          <p className="mt-1 text-muted">{v.cancelReason}</p>
          <p className="mt-2 text-[12px] text-subtle">A reversing entry was posted and the documents it settled are open again.</p>
        </Card>
      )}

      {cancelling && <CancelDialog v={v} onClose={() => setCancelling(false)} />}
      {adjusting && <AdjustAdvanceDialog v={v} onClose={() => setAdjusting(false)} />}
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
    invalidate: [keys.vouchers, keys.voucher(v.id), keys.moneyAccounts, keys.parties, keys.sales, keys.purchases, ["moneyBook"], ["openDocuments"], ["partyAdvances"], ["settlements"], ["reconciliation"], ["report"]],
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
