import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router";
import clsx from "clsx";
import { ArrowLeft, Ban, FileMinus2, Loader2, Pencil, Trash2 } from "lucide-react";
import { api, keys, type ItcReviewInput, type PurchaseDocumentView, type PurchaseLine } from "../../lib/api";
import { formatPaise, formatRate, paiseToInput } from "../../lib/format";
import { stateName } from "../../lib/gst";
import { SUPPLY_LABEL, amountInWords, shortDate } from "../../lib/sales";
import { ITC_LABEL, PAYMENT_LABEL, PURCHASE_STATUS, PURCHASE_TYPE_LABEL } from "../../lib/purchases";
import { useSave } from "../../lib/mutations";
import { Badge, Button, Card } from "../../components/ui";
import { Dialog } from "../../components/Dialog";
import { ChoiceCards, Field, TextInput } from "../../components/form";
import { AttachmentsPanel } from "../../components/Attachments";

const tax = (l: PurchaseLine) => l.cgstPaise + l.sgstPaise + l.utgstPaise + l.igstPaise + l.cessPaise;

export function PurchaseDocument() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const { data: doc } = useQuery({ queryKey: keys.purchase(id), queryFn: () => api.getPurchase(id) });
  const { data: accounts = [] } = useQuery({ queryKey: keys.purchaseAccounts, queryFn: api.purchaseAccounts });
  const [cancelling, setCancelling] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [reviewing, setReviewing] = useState<PurchaseLine | null>(null);

  if (!doc) {
    return (
      <div className="grid place-items-center py-24 text-muted">
        <Loader2 className="animate-spin" />
      </div>
    );
  }
  const st = PURCHASE_STATUS[doc.displayStatus];
  const title = doc.docType === "debit_note" ? (doc.number ?? "Draft debit note") : (doc.supplierInvoiceNumber ?? PURCHASE_TYPE_LABEL[doc.docType]);
  const recorded = doc.status === "recorded";
  const accountName = (aid: string) => accounts.find((a) => a.id === aid)?.name ?? "—";
  const rcmTax = doc.reverseCharge ? doc.cgstPaise + doc.sgstPaise + doc.utgstPaise + doc.igstPaise + doc.cessPaise : 0;
  const left = doc.totalPaise - doc.debitedPaise;

  return (
    <div>
      <Link to="/purchases" className="mb-2 inline-flex items-center gap-1 text-[13px] text-muted hover:text-fg">
        <ArrowLeft size={14} /> Purchases
      </Link>
      <div className="mb-6 flex items-end justify-between gap-4">
        <div>
          <div className="text-[12px] font-semibold uppercase tracking-[0.08em] text-accent">{PURCHASE_TYPE_LABEL[doc.docType]}</div>
          <div className="flex items-center gap-3">
            <h1 className={clsx("text-display num", doc.status === "cancelled" && "line-through decoration-2")}>{title}</h1>
            <Badge tone={st.tone}>{st.label}</Badge>
          </div>
          <p className="mt-1 text-[14px] text-muted">{doc.partyName ?? "No supplier"}</p>
        </div>
        <div className="flex gap-2">
          {doc.status === "draft" && (
            <>
              <Button onClick={() => setDeleting(true)}>
                <Trash2 size={15} /> Delete
              </Button>
              <Link to={`/purchases/${doc.id}/edit`}>
                <Button variant="primary">
                  <Pencil size={15} /> Edit draft
                </Button>
              </Link>
            </>
          )}
          {recorded && (
            <Button onClick={() => setCancelling(true)}>
              <Ban size={15} /> Cancel
            </Button>
          )}
          {recorded && doc.docType === "bill" && doc.partyId && left > 0 && (
            <Button variant="primary" onClick={() => navigate(`/purchases/new?type=debit_note&from=${doc.id}`)}>
              <FileMinus2 size={15} /> Debit note
            </Button>
          )}
        </div>
      </div>

      <div className="grid grid-cols-[1fr_290px] items-start gap-5">
        <div className="min-w-0 space-y-5">
          <Card className="grid grid-cols-4 gap-4 p-5 text-[13px]">
            <Info label="Supplier GSTIN" value={doc.partyGstin ?? "None"} mono />
            <Info label={doc.docType === "debit_note" ? "Date" : "Invoice date"} value={shortDate(doc.supplierInvoiceDate ?? doc.docDate)} />
            <Info label="Received" value={shortDate(doc.docDate)} />
            <Info label={doc.paymentMode === "credit" ? "Due" : "Paid"} value={doc.paymentMode === "credit" ? shortDate(doc.dueDate) : PAYMENT_LABEL[doc.paymentMode]} />
            <Info label="Supply" value={SUPPLY_LABEL[doc.supplyKind]} />
            <Info label="Supplier's state" value={doc.supplierState === 96 ? "Outside India" : stateName(doc.supplierState)} />
            {doc.reverseCharge && <Info label="Reverse charge" value={`You pay ${formatPaise(rcmTax)} GST`} />}
            {doc.original && (
              <Info label="Against bill" value={<Link to={`/purchases/${doc.original.id}`} className="num text-accent">{doc.original.supplierInvoiceNumber}</Link>} />
            )}
            {doc.reason && <Info label="Reason" value={doc.reason} />}
          </Card>

          <Card className="overflow-hidden">
            <table className="w-full text-[13px]">
              <thead>
                <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
                  <th className="px-4 py-2 font-medium">Description</th>
                  <th className="px-2 py-2 font-medium">Booked to</th>
                  <th className="px-2 py-2 text-right font-medium">Taxable</th>
                  <th className="px-2 py-2 text-right font-medium">GST</th>
                  <th className="px-4 py-2 font-medium">Input tax credit</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {doc.lines.map((l) => (
                  <tr key={l.id} className="align-top">
                    <td className="px-4 py-2.5">
                      {l.description}
                      <div className="num text-[12px] text-muted">
                        {l.quantity} {l.unit} × {formatPaise(l.unitPricePaise)} {l.hsnSac && `· ${l.hsnSac}`}
                      </div>
                    </td>
                    <td className="px-2 py-2.5 text-muted">{accountName(l.accountId)}</td>
                    <td className="num px-2 py-2.5 text-right">{formatPaise(l.taxablePaise)}</td>
                    <td className="num px-2 py-2.5 text-right">
                      {tax(l) ? formatPaise(tax(l)) : "—"}
                      {l.taxCategory === "taxable" && <div className="text-[11.5px] text-muted">{formatRate(l.gstRateBp)}</div>}
                    </td>
                    <td className="px-4 py-2.5">
                      <div className="flex items-center gap-2">
                        <Badge tone={ITC_LABEL[l.itcStatus].tone}>{ITC_LABEL[l.itcStatus].label}</Badge>
                        {recorded && l.itcStatus !== "na" && (
                          <button type="button" className="text-[12.5px] text-accent hover:underline" onClick={() => setReviewing(l)}>
                            {l.itcStatus === "pending" ? "Review" : "Change"}
                          </button>
                        )}
                      </div>
                      {l.itcStatus === "partial" && <div className="num mt-0.5 text-[12px] text-muted">{formatPaise(l.itcEligiblePaise)} claimable</div>}
                      {l.itcReason && <div className="mt-0.5 text-[12px] text-muted">{l.itcReason}</div>}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div className="flex justify-end border-t border-line p-4">
              <div className="w-[280px] text-[13px]">
                <Row label="Taxable value" paise={doc.taxablePaise} always />
                <Row label="CGST" paise={doc.cgstPaise} />
                <Row label="SGST" paise={doc.sgstPaise} />
                <Row label="UTGST" paise={doc.utgstPaise} />
                <Row label="IGST" paise={doc.igstPaise} />
                <Row label="Cess" paise={doc.cessPaise} />
                {doc.reverseCharge && <Row label="Less GST under reverse charge" paise={-rcmTax} />}
                <Row label="Round-off" paise={doc.roundOffPaise} />
                <div className="mt-2 flex items-baseline justify-between border-t border-line pt-2">
                  <span className="font-semibold">{doc.reverseCharge ? "Payable to supplier" : "Total"}</span>
                  <span className="num text-[18px] font-semibold">{formatPaise(doc.totalPaise)}</span>
                </div>
                <p className="mt-1 text-right text-[12px] text-muted">{amountInWords(doc.totalPaise)}</p>
              </div>
            </div>
          </Card>
          {doc.notes && <Card className="whitespace-pre-line p-4 text-[13px] text-muted">{doc.notes}</Card>}
        </div>

        <div className="space-y-4">
          {doc.status === "cancelled" && (
            <Card className="p-4 text-[13px]">
              <div className="font-semibold">Cancelled {shortDate(doc.cancelledAt?.slice(0, 10))}</div>
              <p className="mt-1 text-muted">{doc.cancelReason}</p>
              <p className="mt-2 text-[12px] text-subtle">A reversing entry was posted, including any ITC decisions.</p>
            </Card>
          )}
          <AttachmentsPanel documentId={doc.id} attachments={doc.attachments} editable={doc.status === "draft"} />
          {doc.docType === "bill" && doc.status !== "draft" && (
            <Card className="p-4 text-[13px]">
              <div className="mb-2 text-[12px] font-semibold uppercase tracking-[0.04em] text-muted">Debit notes</div>
              {doc.debitNotes.length === 0 ? (
                <p className="text-muted">None</p>
              ) : (
                doc.debitNotes.map((n) => (
                  <Link key={n.id} to={n.status === "draft" ? `/purchases/${n.id}/edit` : `/purchases/${n.id}`} className="flex justify-between py-1 hover:text-accent">
                    <span className={clsx("num", n.status === "cancelled" && "line-through text-muted")}>{n.number ?? "Draft"}</span>
                    <span className="num">{formatPaise(n.totalPaise)}</span>
                  </Link>
                ))
              )}
            </Card>
          )}
        </div>
      </div>

      {reviewing && <ItcDialog doc={doc} line={reviewing} onClose={() => setReviewing(null)} />}
      {cancelling && <CancelDialog doc={doc} onClose={() => setCancelling(false)} />}
      {deleting && <DeleteDialog doc={doc} onClose={() => setDeleting(false)} />}
    </div>
  );
}

function Info({ label, value, mono }: { label: string; value: React.ReactNode; mono?: boolean }) {
  return (
    <div>
      <div className="text-[11.5px] font-semibold uppercase tracking-[0.05em] text-muted">{label}</div>
      <div className={clsx("mt-0.5", mono && "num")}>{value}</div>
    </div>
  );
}

function Row({ label, paise, always }: { label: string; paise: number; always?: boolean }) {
  if (!paise && !always) return null;
  return (
    <div className="flex justify-between py-0.5">
      <span className="text-muted">{label}</span>
      <span className="num">{formatPaise(paise)}</span>
    </div>
  );
}

function ItcDialog({ doc, line, onClose }: { doc: PurchaseDocumentView; line: PurchaseLine; onClose: () => void }) {
  const [f, setF] = useState<ItcReviewInput>({
    status: line.itcStatus === "na" ? "pending" : line.itcStatus,
    reason: line.itcReason ?? "",
    eligible: paiseToInput(line.itcEligiblePaise),
  });
  const save = useSave((x: ItcReviewInput) => api.reviewItc(doc.id, line.id, x, doc.rowVersion), {
    invalidate: [keys.purchases, keys.purchase(doc.id)],
    success: "Input tax credit updated",
    onSuccess: onClose,
  });
  const err = save.fieldErrors;
  return (
    <Dialog
      open
      onClose={onClose}
      width={560}
      title="Input tax credit"
      description={`${line.description} · GST ${formatPaise(tax(line))}`}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={save.isPending} onClick={() => save.mutate(f)}>
            {save.isPending && <Loader2 size={15} className="animate-spin" />} Save decision
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <ChoiceCards<ItcReviewInput["status"]>
          name="ITC status"
          value={f.status}
          onChange={(status) => {
            setF((x) => ({ ...x, status }));
            save.clearError("itcStatus");
          }}
          options={[
            { value: "eligible", title: "Eligible", description: "Claim it in full" },
            { value: "partial", title: "Partial", description: "Claim part of it" },
            { value: "ineligible", title: "Not eligible", description: "Blocked or not used for business" },
            { value: "pending", title: "Undecided", description: "Keep it pending" },
          ]}
        />
        {err.itcStatus && <p className="text-[12.5px] text-danger">{err.itcStatus}</p>}
        {f.status === "partial" && (
          <Field label="Claimable amount (₹)" error={err.itcEligible} hint={`Out of ${formatPaise(tax(line))}.`}>
            {(p) => <TextInput id={p.id} invalid={p.invalid} className="num" inputMode="decimal" value={f.eligible ?? ""} onChange={(e) => setF((x) => ({ ...x, eligible: e.target.value }))} />}
          </Field>
        )}
        {(f.status === "partial" || f.status === "ineligible") && (
          <Field label="Reason" error={err.itcReason}>
            {(p) => <TextInput id={p.id} invalid={p.invalid} value={f.reason ?? ""} placeholder="e.g. Blocked credit u/s 17(5)" onChange={(e) => setF((x) => ({ ...x, reason: e.target.value }))} />}
          </Field>
        )}
        <p className="text-[12.5px] text-muted">Saving posts an adjusting entry that moves the tax between input tax, pending and the line's cost. Matching with GSTR-2B comes in Phase 2.</p>
      </div>
    </Dialog>
  );
}

function CancelDialog({ doc, onClose }: { doc: PurchaseDocumentView; onClose: () => void }) {
  const [reason, setReason] = useState("");
  const save = useSave((r: string) => api.cancelPurchase(doc.id, r, doc.rowVersion), {
    invalidate: [keys.purchases, keys.purchase(doc.id), keys.parties],
    success: "Purchase cancelled",
    onSuccess: onClose,
  });
  return (
    <Dialog
      open
      onClose={onClose}
      title="Cancel this purchase?"
      description="A reversing entry is posted, including any input tax credit. This can't be undone."
      width={480}
      footer={
        <>
          <Button onClick={onClose}>Keep it</Button>
          <Button variant="primary" className="bg-danger hover:bg-danger" disabled={save.isPending} onClick={() => save.mutate(reason)}>
            Cancel purchase
          </Button>
        </>
      }
    >
      <Field label="Reason" error={save.fieldErrors.reason} hint="Kept in the audit log, e.g. entered twice.">
        {(p) => <TextInput id={p.id} invalid={p.invalid} autoFocus value={reason} onChange={(e) => setReason(e.target.value)} />}
      </Field>
    </Dialog>
  );
}

function DeleteDialog({ doc, onClose }: { doc: PurchaseDocumentView; onClose: () => void }) {
  const navigate = useNavigate();
  const save = useSave(() => api.deletePurchaseDraft(doc.id, doc.rowVersion), {
    invalidate: [keys.purchases],
    success: "Draft deleted",
    onSuccess: () => navigate("/purchases", { replace: true }),
  });
  return (
    <Dialog
      open
      onClose={onClose}
      title="Delete this draft?"
      description="Nothing was posted. Its attachments are removed too. This can't be undone."
      width={440}
      footer={
        <>
          <Button onClick={onClose}>Keep it</Button>
          <Button variant="primary" className="bg-danger hover:bg-danger" disabled={save.isPending} onClick={() => save.mutate(undefined)}>
            Delete draft
          </Button>
        </>
      }
    >
      <p className="text-[13px] text-muted">
        {doc.partyName ?? "No supplier"} · {formatPaise(doc.totalPaise)}
      </p>
    </Dialog>
  );
}
