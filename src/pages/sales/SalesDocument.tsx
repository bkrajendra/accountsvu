import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router";
import clsx from "clsx";
import { ArrowLeft, Ban, FilePlus2, Loader2, Pencil, Trash2 } from "lucide-react";
import { api, keys, type SalesDocumentView } from "../../lib/api";
import { formatPaise, formatRate } from "../../lib/format";
import { stateName } from "../../lib/gst";
import { DOC_TYPE_LABEL, STATUS, SUPPLY_LABEL, amountInWords, shortDate } from "../../lib/sales";
import { useSave } from "../../lib/mutations";
import { Badge, Button, Card } from "../../components/ui";
import { Dialog } from "../../components/Dialog";
import { Field, TextInput } from "../../components/form";
import { SettlementsCard } from "../banking/Settlements";
import { useLogoUrl } from "../../lib/logo";
import { PdfActions } from "../../components/PdfActions";
import { TaxOverrideNote } from "../../components/TaxOverride";

export function SalesDocument() {
  const { id = "" } = useParams();
  const { data: doc } = useQuery({ queryKey: keys.salesDocument(id), queryFn: () => api.getSalesDocument(id) });
  const { data: profile } = useQuery({ queryKey: keys.profile, queryFn: api.businessProfile });
  const logoUrl = useLogoUrl();
  const [cancelling, setCancelling] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const navigate = useNavigate();

  if (!doc) {
    return (
      <div className="grid place-items-center py-24 text-muted">
        <Loader2 className="animate-spin" />
      </div>
    );
  }
  const org = profile?.organisation;
  const reg = profile?.registrations.find((r) => r.id === doc.gstRegistrationId);
  const credit = doc.docType === "credit_note";
  const status = STATUS[doc.displayStatus];
  const left = doc.totalPaise - doc.creditedPaise;

  return (
    <div>
      <Link to="/sales" className="mb-2 inline-flex items-center gap-1 text-[13px] text-muted hover:text-fg">
        <ArrowLeft size={14} /> Sales
      </Link>
      <div className="mb-6 flex items-end justify-between gap-4">
        <div className="flex items-center gap-3">
          <h1 className={clsx("text-display num", doc.status === "cancelled" && "line-through decoration-2")}>{doc.number ?? "Draft"}</h1>
          <Badge tone={status.tone}>{status.label}</Badge>
        </div>
        <div className="flex gap-2">
          <PdfActions open={() => api.openSalesPdf(doc.id)} save={() => api.saveSalesPdf(doc.id)} printTitle={doc.status === "draft" ? "Opens with a DRAFT watermark" : undefined} />
          {doc.status === "draft" && (
            <>
              <Button onClick={() => setDeleting(true)}>
                <Trash2 size={15} /> Delete
              </Button>
              <Link to={`/sales/${doc.id}/edit`}>
                <Button variant="primary">
                  <Pencil size={15} /> Edit draft
                </Button>
              </Link>
            </>
          )}
          {doc.status === "issued" && doc.paidPaise === 0 && (
            <Button onClick={() => setCancelling(true)}>
              <Ban size={15} /> Cancel
            </Button>
          )}
          {doc.status === "issued" && !credit && left > 0 && (
            <Button variant="primary" onClick={() => navigate(`/sales/new?type=credit_note&from=${doc.id}`)}>
              <FilePlus2 size={15} /> Credit note
            </Button>
          )}
        </div>
      </div>

      <div className="grid grid-cols-[1fr_280px] items-start gap-5">
        <Card className="paper p-8">
          <div className="flex items-start justify-between gap-6 border-b border-line pb-6">
            <div className="flex items-start gap-4">
              {logoUrl && <img src={logoUrl} alt="" className="max-h-14 max-w-[140px] shrink-0 object-contain" />}
              <div>
              <div className="text-[17px] font-semibold">{org?.tradeName ?? org?.legalName}</div>
              {org?.tradeName && <div className="text-[12.5px] text-muted">{org.legalName}</div>}
              <div className="mt-1 text-[12.5px] text-muted">{[org?.addressLine1, org?.city, org?.pincode].filter(Boolean).join(", ")}</div>
              {reg && <div className="num mt-1 text-[12.5px]">GSTIN {reg.gstin}</div>}
              </div>
            </div>
            <div className="text-right">
              <div className="text-[12px] font-semibold uppercase tracking-[0.08em] text-accent">{DOC_TYPE_LABEL[doc.docType]}</div>
              <div className="num mt-1 text-[15px] font-semibold">{doc.number ?? "Draft"}</div>
              <div className="mt-1 text-[12.5px] text-muted">Dated {shortDate(doc.docDate)}</div>
              {!credit && doc.dueDate && <div className="text-[12.5px] text-muted">Due {shortDate(doc.dueDate)}</div>}
            </div>
          </div>

          <div className="grid grid-cols-2 gap-6 border-b border-line py-5 text-[13px]">
            <div>
              <div className="mb-1 text-[11.5px] font-semibold uppercase tracking-[0.06em] text-muted">Bill to</div>
              <div className="font-medium">{doc.partyName}</div>
              {doc.partyAddress && <div className="text-muted">{doc.partyAddress}</div>}
              {doc.partyGstin && <div className="num">GSTIN {doc.partyGstin}</div>}
            </div>
            <div className="text-right">
              <div className="mb-1 text-[11.5px] font-semibold uppercase tracking-[0.06em] text-muted">Place of supply</div>
              <div>{doc.placeOfSupply === 96 ? "Outside India" : `${stateName(doc.placeOfSupply)} (${String(doc.placeOfSupply).padStart(2, "0")})`}</div>
              <div className="text-[12.5px] text-muted">{SUPPLY_LABEL[doc.supplyKind]}</div>
              {doc.zeroRated && <div className="text-[12.5px] text-muted">Zero-rated under LUT/bond</div>}
              {doc.original && (
                <div className="mt-1 text-[12.5px]">
                  Against <Link to={`/sales/${doc.original.id}`} className="num text-accent">{doc.original.number}</Link> of {shortDate(doc.original.docDate)}
                </div>
              )}
            </div>
          </div>

          <table className="mt-4 w-full text-[13px]">
            <thead>
              <tr className="border-b border-line text-left text-[11.5px] uppercase tracking-[0.04em] text-muted">
                <th className="py-2 pr-2 font-medium">#</th>
                <th className="py-2 pr-2 font-medium">Description</th>
                <th className="py-2 pr-2 font-medium">HSN/SAC</th>
                <th className="py-2 pr-2 text-right font-medium">Qty</th>
                <th className="py-2 pr-2 text-right font-medium">Rate</th>
                <th className="py-2 pr-2 text-right font-medium">GST</th>
                <th className="py-2 text-right font-medium">Taxable</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {doc.lines.map((l) => (
                <tr key={l.id}>
                  <td className="num py-2.5 pr-2 text-muted">{l.lineNo}</td>
                  <td className="py-2.5 pr-2">
                    {l.description}
                    {l.discountPaise > 0 && <div className="text-[12px] text-muted">Less discount {formatPaise(l.discountPaise)}</div>}
                  </td>
                  <td className="num py-2.5 pr-2 text-muted">{l.hsnSac ?? "—"}</td>
                  <td className="num py-2.5 pr-2 text-right">
                    {l.quantity} <span className="text-[11.5px] text-muted">{l.unit}</span>
                  </td>
                  <td className="num py-2.5 pr-2 text-right">{formatPaise(l.unitPricePaise)}</td>
                  <td className="num py-2.5 pr-2 text-right text-muted">{l.taxCategory === "taxable" ? formatRate(l.gstRateBp) : l.taxCategory.replace("_", "-")}</td>
                  <td className="num py-2.5 text-right">{formatPaise(l.taxablePaise)}</td>
                </tr>
              ))}
            </tbody>
          </table>

          <div className="mt-4 flex justify-end">
            <div className="w-[300px] text-[13px]">
              <Line label="Taxable value" paise={doc.taxablePaise} />
              <Line label="CGST" paise={doc.cgstPaise} />
              <Line label="SGST" paise={doc.sgstPaise} />
              <Line label="UTGST" paise={doc.utgstPaise} />
              <Line label="IGST" paise={doc.igstPaise} />
              <Line label="Cess" paise={doc.cessPaise} />
              <Line label="Round-off" paise={doc.roundOffPaise} />
              <div className="mt-2 flex items-baseline justify-between border-t border-line pt-2">
                <span className="font-semibold">Total</span>
                <span className="num text-[18px] font-semibold">{formatPaise(doc.totalPaise)}</span>
              </div>
            </div>
          </div>
          <p className="mt-3 text-right text-[12.5px] text-muted">{amountInWords(doc.totalPaise)}</p>
          {doc.reason && <p className="mt-6 text-[13px]"><span className="text-muted">Reason:</span> {doc.reason}</p>}
          {doc.notes && <p className="mt-4 whitespace-pre-line text-[13px] text-muted">{doc.notes}</p>}
        </Card>

        <div className="space-y-4">
          {doc.status === "cancelled" && (
            <Card className="p-4 text-[13px]">
              <div className="font-semibold">Cancelled {shortDate(doc.cancelledAt?.slice(0, 10))}</div>
              <p className="mt-1 text-muted">{doc.cancelReason}</p>
              <p className="mt-2 text-[12px] text-subtle">The number stays used and a reversing entry was posted.</p>
            </Card>
          )}
          {!credit && doc.status === "issued" && (
            <SettlementsCard kind="sales" documentId={doc.id} partyId={doc.partyId} totalPaise={doc.totalPaise} paidPaise={doc.paidPaise} adjustedPaise={doc.creditedPaise} open />
          )}
          {!credit && doc.status !== "draft" && (
            <Card className="p-4 text-[13px]">
              <div className="mb-2 text-[12px] font-semibold uppercase tracking-[0.04em] text-muted">Credit notes</div>
              {doc.creditNotes.length === 0 ? (
                <p className="text-muted">None</p>
              ) : (
                doc.creditNotes.map((c) => (
                  <Link key={c.id} to={c.status === "draft" ? `/sales/${c.id}/edit` : `/sales/${c.id}`} className="flex justify-between py-1 hover:text-accent">
                    <span className={clsx("num", c.status === "cancelled" && "line-through text-muted")}>{c.number ?? "Draft"}</span>
                    <span className="num">{formatPaise(c.totalPaise)}</span>
                  </Link>
                ))
              )}
              {doc.creditedPaise > 0 && (
                <div className="mt-2 flex justify-between border-t border-line pt-2">
                  <span className="text-muted">Left after credits</span>
                  <span className="num font-medium">{formatPaise(left)}</span>
                </div>
              )}
            </Card>
          )}
          <TaxOverrideNote doc={doc} />
          {doc.status !== "cancelled" && (doc.gstr1.label || doc.gstr1.nilTable) && (
            <Card className="p-4 text-[13px]">
              <div className="mb-2 text-[12px] font-semibold uppercase tracking-[0.04em] text-muted">GST return</div>
              <div className="text-muted">Reported in GSTR-1</div>
              <div className="mt-0.5 font-medium">
                {[doc.gstr1.label, doc.gstr1.nilTable ? "Nil/exempt/non-GST (table 8)" : null].filter(Boolean).join(" · ")}
              </div>
              <p className="mt-2 text-[12px] text-subtle">
                {doc.rulePackVersion ? `Tax worked out with GST rules ${doc.rulePackVersion}.` : "Tax worked out before versioned GST rules."}
              </p>
            </Card>
          )}
        </div>
      </div>

      {cancelling && <CancelDialog doc={doc} onClose={() => setCancelling(false)} />}
      {deleting && <DeleteDialog doc={doc} onClose={() => setDeleting(false)} />}
    </div>
  );
}

function Line({ label, paise }: { label: string; paise: number }) {
  if (!paise && label !== "Taxable value") return null;
  return (
    <div className="flex justify-between py-0.5">
      <span className="text-muted">{label}</span>
      <span className="num">{formatPaise(paise)}</span>
    </div>
  );
}

function CancelDialog({ doc, onClose }: { doc: SalesDocumentView; onClose: () => void }) {
  const [reason, setReason] = useState("");
  const save = useSave((r: string) => api.cancelSalesDocument(doc.id, r, doc.rowVersion), {
    invalidate: [keys.sales, keys.salesDocument(doc.id), keys.parties],
    success: `${doc.number} cancelled`,
    onSuccess: onClose,
  });
  return (
    <Dialog
      open
      onClose={onClose}
      title={`Cancel ${doc.number}?`}
      description="A reversing entry is posted and the number is never reused. This can't be undone."
      width={480}
      footer={
        <>
          <Button onClick={onClose}>Keep it</Button>
          <Button variant="primary" disabled={save.isPending} onClick={() => save.mutate(reason)} className="bg-danger hover:bg-danger">
            {save.isPending && <Loader2 size={15} className="animate-spin" />} Cancel {doc.docType === "credit_note" ? "credit note" : "invoice"}
          </Button>
        </>
      }
    >
      <Field label="Reason" error={save.fieldErrors.reason} hint="Kept in the audit log, e.g. raised to the wrong customer.">
        {(p) => <TextInput id={p.id} invalid={p.invalid} autoFocus value={reason} onChange={(e) => setReason(e.target.value)} />}
      </Field>
      {doc.docType !== "credit_note" && (
        <p className="mt-3 text-[12.5px] text-muted">To reduce the amount instead of cancelling, issue a credit note.</p>
      )}
    </Dialog>
  );
}

function DeleteDialog({ doc, onClose }: { doc: SalesDocumentView; onClose: () => void }) {
  const navigate = useNavigate();
  const save = useSave(() => api.deleteSalesDraft(doc.id, doc.rowVersion), {
    invalidate: [keys.sales],
    success: "Draft deleted",
    onSuccess: () => navigate("/sales", { replace: true }),
  });
  return (
    <Dialog
      open
      onClose={onClose}
      title="Delete this draft?"
      description="It was never issued, so nothing was posted. This can't be undone."
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
        {doc.partyName} · {formatPaise(doc.totalPaise)}
      </p>
    </Dialog>
  );
}
