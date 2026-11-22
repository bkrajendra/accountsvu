import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams, useSearchParams } from "react-router";
import { AlertCircle, ArrowLeft, Loader2, Plus, X } from "lucide-react";
import {
  api,
  keys,
  type Account,
  type ItcStatus,
  type Party,
  type PaymentMode,
  type PurchaseCalculation,
  type PurchaseDocType,
  type PurchaseDocumentInput,
  type PurchaseDocumentView,
  type TaxCategory,
} from "../../lib/api";
import { formatPaise, formatRate, paiseToInput } from "../../lib/format";
import { stateName } from "../../lib/gst";
import { SUPPLY_LABEL, amountInWords, shortDate, todayIso } from "../../lib/sales";
import { PURCHASE_TYPE_LABEL } from "../../lib/purchases";
import { useSave } from "../../lib/mutations";
import { Badge, Button, Card, Segmented } from "../../components/ui";
import { Field, Select, TextInput, Toggle } from "../../components/form";
import { Combobox } from "../../components/Combobox";
import { Dialog } from "../../components/Dialog";
import { WarningList, rateLabel, warningsFor } from "../../components/TaxWarnings";
import { CellInput, useDebounced } from "../../components/editor";
import { AttachmentsPanel } from "../../components/Attachments";

interface LineDraft {
  key: string;
  itemId: string | null;
  accountId: string;
  description: string;
  hsnSac: string;
  unit: string;
  quantity: string;
  unitPrice: string;
  discount: string;
  tax: string;
  itcStatus: Exclude<ItcStatus, "na">;
  itcReason: string;
  itcEligible: string;
  /** Kept from the saved line; set in the ITC review. */
  itcCommon?: boolean;
}

interface Draft {
  docType: PurchaseDocType;
  partyId: string;
  supplierInvoiceNumber: string;
  supplierInvoiceDate: string;
  docDate: string;
  dueDate: string;
  paymentMode: PaymentMode;
  reverseCharge: boolean;
  pricesIncludeTax: boolean;
  originalDocumentId: string;
  reason: string;
  notes: string;
  lines: LineDraft[];
}

const newKey = () => crypto.randomUUID();
const percent = (bp: number) => formatRate(bp).replace("%", "");
const taxOf = (category: TaxCategory, bp: number) => (category === "taxable" ? `taxable:${percent(bp)}` : category);

function blankLine(accountId = ""): LineDraft {
  return { key: newKey(), itemId: null, accountId, description: "", hsnSac: "", unit: "", quantity: "1", unitPrice: "", discount: "", tax: "taxable:18", itcStatus: "pending", itcReason: "", itcEligible: "" };
}

function fromView(v: PurchaseDocumentView, asDebitNote = false): Draft {
  return {
    docType: asDebitNote ? "debit_note" : v.docType,
    partyId: v.partyId ?? "",
    supplierInvoiceNumber: asDebitNote ? "" : (v.supplierInvoiceNumber ?? ""),
    supplierInvoiceDate: asDebitNote ? "" : (v.supplierInvoiceDate ?? ""),
    docDate: asDebitNote ? todayIso() : v.docDate,
    dueDate: asDebitNote ? "" : (v.dueDate ?? ""),
    paymentMode: asDebitNote ? "credit" : v.paymentMode,
    reverseCharge: v.reverseCharge,
    pricesIncludeTax: v.pricesIncludeTax,
    originalDocumentId: asDebitNote ? v.id : (v.originalDocumentId ?? ""),
    reason: asDebitNote ? "" : (v.reason ?? ""),
    notes: asDebitNote ? "" : (v.notes ?? ""),
    lines: v.lines.map((l) => ({
      key: newKey(),
      itemId: l.itemId,
      accountId: l.accountId,
      description: l.description,
      hsnSac: l.hsnSac ?? "",
      unit: l.unit ?? "",
      quantity: l.quantity,
      unitPrice: paiseToInput(l.unitPricePaise) || "0",
      discount: paiseToInput(l.discountPaise),
      tax: taxOf(l.taxCategory, l.gstRateBp),
      itcStatus: l.itcStatus === "na" ? "pending" : l.itcStatus,
      itcReason: l.itcReason ?? "",
      itcEligible: paiseToInput(l.itcEligiblePaise),
      itcCommon: l.itcCommon,
    })),
  };
}

const kept = (d: Draft) => d.lines.flatMap((l, i) => (l.itemId || l.description.trim() || l.unitPrice.trim() ? [i] : []));

function toInput(d: Draft): PurchaseDocumentInput {
  return {
    docType: d.docType,
    partyId: d.partyId || undefined,
    supplierInvoiceNumber: d.supplierInvoiceNumber || undefined,
    supplierInvoiceDate: d.supplierInvoiceDate || undefined,
    docDate: d.docDate,
    dueDate: d.dueDate || undefined,
    paymentMode: d.paymentMode,
    reverseCharge: d.reverseCharge,
    pricesIncludeTax: d.pricesIncludeTax,
    originalDocumentId: d.originalDocumentId || undefined,
    reason: d.reason || undefined,
    notes: d.notes || undefined,
    lines: kept(d).map((i) => {
      const l = d.lines[i];
      const [category, rate] = l.tax.split(":");
      return {
        itemId: l.itemId ?? undefined,
        accountId: l.accountId,
        description: l.description,
        hsnSac: l.hsnSac || undefined,
        unit: l.unit || undefined,
        quantity: l.quantity,
        unitPrice: l.unitPrice,
        discount: l.discount || undefined,
        taxCategory: category as TaxCategory,
        gstRate: rate,
        itcStatus: l.itcStatus,
        itcReason: l.itcReason || undefined,
        itcEligible: l.itcEligible || undefined,
        itcCommon: l.itcCommon || undefined,
      };
    }),
  };
}

export function PurchaseEditor() {
  const { id } = useParams();
  const [params] = useSearchParams();
  const fromId = params.get("from");
  const type = (params.get("type") as PurchaseDocType | null) ?? "bill";
  const { data: existing } = useQuery({ queryKey: keys.purchase(id ?? ""), queryFn: () => api.getPurchase(id!), enabled: !!id });
  const { data: original } = useQuery({ queryKey: keys.purchase(fromId ?? ""), queryFn: () => api.getPurchase(fromId!), enabled: !!fromId });
  const { data: accounts } = useQuery({ queryKey: keys.purchaseAccounts, queryFn: api.purchaseAccounts });

  if ((id && !existing) || (fromId && !original) || !accounts) {
    return (
      <div className="grid place-items-center py-24 text-muted">
        <Loader2 className="animate-spin" />
      </div>
    );
  }
  if (existing && existing.status !== "draft") {
    return (
      <Card className="p-8 text-center">
        <p className="text-muted">This purchase is recorded and can't be edited.</p>
        <Link to={`/purchases/${existing.id}`} className="mt-3 inline-block text-accent">
          Open it
        </Link>
      </Card>
    );
  }
  const defaultAccount =
    (type === "expense" ? accounts.find((a) => a.name === "Office expenses") : accounts.find((a) => a.systemRole === "purchases"))?.id ?? "";
  const initial: Draft = existing
    ? fromView(existing)
    : original
      ? fromView(original, true)
      : {
          docType: type,
          partyId: "",
          supplierInvoiceNumber: "",
          supplierInvoiceDate: "",
          docDate: todayIso(),
          dueDate: "",
          paymentMode: type === "expense" ? "cash" : "credit",
          reverseCharge: false,
          pricesIncludeTax: false,
          originalDocumentId: "",
          reason: "",
          notes: "",
          lines: [blankLine(defaultAccount)],
        };
  return <Editor key={id ?? fromId ?? type} initial={initial} existing={existing} original={original} accounts={accounts} defaultAccount={defaultAccount} />;
}

function Editor({
  initial,
  existing,
  original,
  accounts,
  defaultAccount,
}: {
  initial: Draft;
  existing?: PurchaseDocumentView;
  original?: PurchaseDocumentView;
  accounts: Account[];
  defaultAccount: string;
}) {
  const navigate = useNavigate();
  const { data: parties = [] } = useQuery({ queryKey: keys.parties, queryFn: api.listParties });
  const { data: items = [] } = useQuery({ queryKey: keys.items, queryFn: api.listItems });
  const { data: options } = useQuery({ queryKey: keys.itemOptions, queryFn: api.itemOptions, staleTime: Infinity });
  const [d, setD] = useState<Draft>(initial);
  const [confirming, setConfirming] = useState(false);
  const grid = useRef<HTMLTableSectionElement>(null);
  const isDn = d.docType === "debit_note";
  const party: Party | undefined = parties.find((p) => p.id === d.partyId);

  const input = useMemo(() => toInput(d), [d]);
  const keptIdx = useMemo(() => kept(d), [d]);
  const debounced = useDebounced(input, 150);
  const { data: calc } = useQuery({ queryKey: ["purchaseCalc", debounced], queryFn: () => api.calculatePurchase(debounced), placeholderData: keepPreviousData });

  const opts = { invalidate: [keys.purchases, keys.parties, keys.series] };
  const save = useSave((x: PurchaseDocumentInput) => api.savePurchaseDraft(existing?.id ?? null, x, existing?.rowVersion), {
    ...opts,
    success: "Draft saved",
    onSuccess: (v) => navigate(`/purchases/${v.id}/edit`, { replace: true }),
  });
  const record = useSave((x: PurchaseDocumentInput) => api.recordPurchase(existing?.id ?? null, x, existing?.rowVersion), {
    ...opts,
    success: isDn ? "Debit note recorded" : "Purchase recorded",
    onSuccess: (v) => navigate(`/purchases/${v.id}`, { replace: true }),
  });
  const err = useMemo(() => {
    const raw = { ...save.fieldErrors, ...record.fieldErrors };
    return Object.fromEntries(
      Object.entries(raw).map(([k, v]) => {
        const m = /^lines\.(\d+)\.(.+)$/.exec(k);
        return [m ? `lines.${keptIdx[Number(m[1])] ?? m[1]}.${m[2]}` : k, v];
      }),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [save.fieldErrors, record.fieldErrors]);
  const busy = save.isPending || record.isPending;
  const clear = (k: string) => {
    save.clearError(k);
    record.clearError(k);
  };
  const set = <K extends keyof Draft>(k: K, v: Draft[K]) => {
    setD((x) => ({ ...x, [k]: v }));
    clear(k);
  };
  const setLine = (i: number, patch: Partial<LineDraft>) => {
    setD((x) => ({ ...x, lines: x.lines.map((l, n) => (n === i ? { ...l, ...patch } : l)) }));
    const n = keptIdx.indexOf(i);
    if (n >= 0) for (const k of Object.keys(patch)) clear(`lines.${n}.${k === "tax" ? "gstRate" : k}`);
  };

  const focusLine = useRef<string | null>(null);
  useEffect(() => {
    if (!focusLine.current) return;
    grid.current?.querySelector<HTMLInputElement>(`tr[data-line="${focusLine.current}"] input`)?.focus();
    focusLine.current = null;
  }, [d.lines]);
  function addLine() {
    const line = blankLine(d.lines.at(-1)?.accountId || defaultAccount);
    focusLine.current = line.key;
    setD((x) => ({ ...x, lines: [...x.lines, line] }));
  }
  function onGridKey(e: KeyboardEvent<HTMLTableSectionElement>) {
    if (e.key !== "Enter" || e.defaultPrevented || !(e.target instanceof HTMLInputElement)) return;
    const rows = [...(grid.current?.querySelectorAll("tr[data-line]") ?? [])];
    const idx = rows.indexOf(e.target.closest("tr[data-line]")!);
    e.preventDefault();
    if (idx === rows.length - 1) addLine();
    else rows[idx + 1]?.querySelector<HTMLInputElement>("input")?.focus();
  }

  function pickItem(i: number, itemId: string | null) {
    const it = items.find((x) => x.id === itemId);
    if (!it) return setLine(i, { itemId: null });
    setLine(i, {
      itemId: it.id,
      description: it.name,
      hsnSac: it.hsnSac ?? "",
      unit: it.unit,
      unitPrice: paiseToInput(it.purchasePricePaise) || d.lines[i].unitPrice,
      tax: taxOf(it.taxCategory, it.gstRateBp),
    });
  }

  const supplierOptions = parties
    .filter((p) => p.kind !== "customer" && (p.isActive || p.id === d.partyId))
    .map((p) => ({ id: p.id, label: p.name, detail: p.gstin ?? stateName(p.stateCode) }));
  const itemOptions = items.filter((i) => i.isActive).map((i) => ({ id: i.id, label: i.name, detail: [i.code, i.hsnSac].filter(Boolean).join(" · ") }));
  const taxOptions = [
    ...(options?.gstRates.filter((r) => r.basisPoints > 0).map((r) => ({ value: `taxable:${percent(r.basisPoints)}`, label: rateLabel(r, d.supplierInvoiceDate || d.docDate) })) ?? []),
    { value: "exempt", label: "Exempt" },
    { value: "nil_rated", label: "Nil-rated" },
    { value: "non_gst", label: "Non-GST" },
  ];
  const lineErrors = (i: number) => Object.entries(err).filter(([k]) => k.startsWith(`lines.${i}.`)).map(([, m]) => m);
  const title = existing ? `Edit draft ${PURCHASE_TYPE_LABEL[d.docType].toLowerCase()}` : `New ${PURCHASE_TYPE_LABEL[d.docType].toLowerCase()}`;
  const back = original ? `/purchases/${original.id}` : "/purchases";

  return (
    <div>
      <div className="mb-6 flex items-end justify-between gap-4">
        <div>
          <Link to={back} className="mb-2 inline-flex items-center gap-1 text-[13px] text-muted hover:text-fg">
            <ArrowLeft size={14} /> {original ? (original.supplierInvoiceNumber ?? "Bill") : "Purchases"}
          </Link>
          <h1 className="text-display">{title}</h1>
        </div>
        {calc?.numberPreview && (
          <div className="text-right text-[12.5px] text-muted">
            Will be numbered
            <div className="num text-[15px] font-semibold text-fg">{calc.numberPreview}</div>
          </div>
        )}
      </div>

      <div className="grid grid-cols-[1fr_300px] items-start gap-5">
        <div className="min-w-0 space-y-5">
          <Card className="p-5">
            <div className="grid grid-cols-6 gap-4">
              <Field label="Supplier" optional={d.docType === "expense" && d.paymentMode !== "credit"} error={err.partyId} className="col-span-3">
                {(p) =>
                  isDn ? (
                    <TextInput id={p.id} disabled value={original?.partyName ?? existing?.partyName ?? ""} />
                  ) : (
                    <Combobox id={p.id} invalid={p.invalid} autoFocus={!existing} options={supplierOptions} value={d.partyId || null} onSelect={(v) => {
                      const sp = parties.find((x) => x.id === v);
                      setD((x) => ({ ...x, partyId: v ?? "", reverseCharge: sp && !sp.gstin ? x.reverseCharge : false }));
                      clear("partyId");
                    }} placeholder="Search suppliers" />
                  )
                }
              </Field>
              {isDn ? (
                <Field label="Reason" error={err.reason} className="col-span-3">
                  {(p) => <TextInput id={p.id} invalid={p.invalid} autoFocus value={d.reason} placeholder="e.g. Goods returned to supplier" onChange={(e) => set("reason", e.target.value)} />}
                </Field>
              ) : (
                <div className="col-span-3">
                  <div className="mb-1.5 text-[12.5px] font-medium">Paid</div>
                  <Segmented<PaymentMode>
                    label="Paid"
                    value={d.paymentMode}
                    onChange={(v) => set("paymentMode", v)}
                    options={[
                      { value: "credit", label: "On credit" },
                      { value: "cash", label: "Cash" },
                      { value: "bank", label: "Bank" },
                    ]}
                  />
                  {err.paymentMode && <p className="mt-1 text-[12px] text-danger">{err.paymentMode}</p>}
                </div>
              )}
              {!isDn && (
                <>
                  <Field label="Supplier's invoice no." optional={d.docType === "expense"} error={err.supplierInvoiceNumber} className="col-span-2">
                    {(p) => <TextInput id={p.id} invalid={p.invalid} className="num uppercase" value={d.supplierInvoiceNumber} onChange={(e) => set("supplierInvoiceNumber", e.target.value)} />}
                  </Field>
                  <Field label="Invoice date" optional={d.docType === "expense"} error={err.supplierInvoiceDate} className="col-span-2">
                    {(p) => <TextInput id={p.id} invalid={p.invalid} type="date" max={d.docDate} value={d.supplierInvoiceDate} onChange={(e) => set("supplierInvoiceDate", e.target.value)} />}
                  </Field>
                </>
              )}
              <Field label={isDn ? "Date" : "Received on"} error={err.docDate} className="col-span-2">
                {(p) => <TextInput id={p.id} invalid={p.invalid} type="date" max={todayIso()} value={d.docDate} onChange={(e) => set("docDate", e.target.value)} />}
              </Field>
              {!isDn && d.paymentMode === "credit" && (
                <Field label="Due" optional error={err.dueDate} className="col-span-2">
                  {(p) => <TextInput id={p.id} invalid={p.invalid} type="date" value={d.dueDate} onChange={(e) => set("dueDate", e.target.value)} />}
                </Field>
              )}
            </div>
            {party && (
              <p className="mt-3 text-[12.5px] text-muted">
                {party.gstin ? <span className="num">{party.gstin}</span> : "No GSTIN — input tax can't be claimed"} · {stateName(party.stateCode)}
                {party.paymentTermsDays ? ` · ${party.paymentTermsDays}-day terms` : ""}
              </p>
            )}
            {original && (
              <p className="mt-3 text-[12.5px] text-muted">
                Against bill {original.supplierInvoiceNumber} of {shortDate(original.supplierInvoiceDate)} · {formatPaise(original.totalPaise - original.debitedPaise)} left.
              </p>
            )}
            {!isDn && (
              <div className="mt-4 flex flex-wrap gap-x-8 gap-y-3 border-t border-line pt-4">
                <div className="w-[260px]">
                  <Toggle checked={d.pricesIncludeTax} onChange={(v) => set("pricesIncludeTax", v)} label="Prices include GST" />
                </div>
                <div className="w-[340px]">
                  <Toggle checked={d.reverseCharge} onChange={(v) => set("reverseCharge", v)} label="Reverse charge" description="You pay the GST to the government, not the supplier (e.g. transport, legal services)." />
                  {err.reverseCharge && <p className="mt-1 text-[12px] text-danger">{err.reverseCharge}</p>}
                </div>
              </div>
            )}
          </Card>

          <Card className="overflow-visible">
            <table className="w-full text-[13px]">
              <thead>
                <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
                  <th className="w-8 py-2 pl-4 font-medium">#</th>
                  <th className="px-2 py-2 font-medium">Item or description</th>
                  <th className="w-[150px] px-2 py-2 font-medium">Book to</th>
                  <th className="w-[84px] px-2 py-2 font-medium">HSN/SAC</th>
                  <th className="w-[64px] px-2 py-2 font-medium">Qty</th>
                  <th className="w-[96px] px-2 py-2 font-medium">Rate ₹</th>
                  <th className="w-[92px] px-2 py-2 font-medium">GST</th>
                  <th className="w-[118px] px-2 py-2 font-medium">ITC</th>
                  <th className="w-[100px] px-2 py-2 text-right font-medium">Amount</th>
                  <th className="w-9" />
                </tr>
              </thead>
              <tbody ref={grid} onKeyDown={onGridKey}>
                {d.lines.map((l, i) => {
                  const lc = keptIdx.includes(i) ? (calc?.lines[keptIdx.indexOf(i)] ?? null) : null;
                  const has = (f: string) => !!err[`lines.${i}.${f}`];
                  const taxed = l.tax.startsWith("taxable");
                  const errs = lineErrors(i);
                  const warns = keptIdx.includes(i) ? warningsFor(calc?.warnings, keptIdx.indexOf(i)) : [];
                  return [
                    <tr key={l.key} data-line={l.key} className="align-top">
                      <td className="num py-2 pl-4 pt-4 text-subtle">{i + 1}</td>
                      <td className="px-2 py-2">
                        <Combobox ariaLabel={`Line ${i + 1} item`} freeText invalid={has("description")} options={itemOptions} value={l.itemId} text={l.description}
                          onSelect={(v) => pickItem(i, v)} onText={(t) => setLine(i, { description: t })} placeholder="Search items or describe" />
                      </td>
                      <td className="px-2 py-2">
                        <Select aria-label={`Line ${i + 1} account`} invalid={has("accountId")} value={l.accountId} onChange={(e) => setLine(i, { accountId: e.target.value })} className="px-2">
                          <option value="">Choose…</option>
                          {accounts.map((a) => (
                            <option key={a.id} value={a.id}>
                              {a.name}
                            </option>
                          ))}
                        </Select>
                      </td>
                      <td className="px-2 py-2">
                        <CellInput label={`Line ${i + 1} HSN/SAC`} invalid={has("hsnSac")} value={l.hsnSac} onChange={(v) => setLine(i, { hsnSac: v.replace(/\D/g, "") })} />
                      </td>
                      <td className="px-2 py-2">
                        <CellInput label={`Line ${i + 1} quantity`} invalid={has("quantity")} value={l.quantity} onChange={(v) => setLine(i, { quantity: v })} numeric />
                      </td>
                      <td className="px-2 py-2">
                        <CellInput label={`Line ${i + 1} rate`} invalid={has("unitPrice")} value={l.unitPrice} onChange={(v) => setLine(i, { unitPrice: v })} numeric />
                      </td>
                      <td className="px-2 py-2">
                        <Select aria-label={`Line ${i + 1} GST`} invalid={has("gstRate")} value={l.tax} onChange={(e) => setLine(i, { tax: e.target.value })} className="px-2">
                          {taxOptions.map((o) => (
                            <option key={o.value} value={o.value}>
                              {o.label}
                            </option>
                          ))}
                        </Select>
                      </td>
                      <td className="px-2 py-2">
                        {taxed ? (
                          <Select aria-label={`Line ${i + 1} input tax credit`} invalid={has("itcStatus")} value={l.itcStatus} onChange={(e) => setLine(i, { itcStatus: e.target.value as LineDraft["itcStatus"] })} className="px-2">
                            <option value="pending">Review later</option>
                            <option value="eligible">Eligible</option>
                            <option value="partial">Partial</option>
                            <option value="ineligible">Not eligible</option>
                          </Select>
                        ) : (
                          <span className="block pt-2 text-[12px] text-subtle">No GST</span>
                        )}
                      </td>
                      <td className="num px-2 py-2 pt-4 text-right font-medium">{lc ? formatPaise(lc.amountPaise) : <span className="text-subtle">—</span>}</td>
                      <td className="py-2 pr-2 pt-3">
                        <button type="button" aria-label={`Remove line ${i + 1}`} disabled={d.lines.length === 1}
                          onClick={() => setD((x) => ({ ...x, lines: x.lines.filter((_, n) => n !== i) }))}
                          className="grid size-7 place-items-center rounded-md text-subtle hover:bg-hover hover:text-fg disabled:invisible">
                          <X size={14} />
                        </button>
                      </td>
                    </tr>,
                    taxed && (l.itcStatus === "partial" || l.itcStatus === "ineligible") && (
                      <tr key={`${l.key}-itc`}>
                        <td />
                        <td colSpan={9} className="px-2 pb-2">
                          <div className="flex gap-3">
                            <TextInput aria-label={`Line ${i + 1} ITC reason`} invalid={has("itcReason")} value={l.itcReason} onChange={(e) => setLine(i, { itcReason: e.target.value })}
                              placeholder={l.itcStatus === "partial" ? "Why only part, e.g. common credit under rule 42" : "Why not, e.g. blocked credit u/s 17(5) — food & beverages"} />
                            {l.itcStatus === "partial" && (
                              <TextInput aria-label={`Line ${i + 1} claimable ITC`} invalid={has("itcEligible")} value={l.itcEligible} onChange={(e) => setLine(i, { itcEligible: e.target.value })}
                                placeholder="Claimable ₹" className="num w-[140px] text-right" inputMode="decimal" />
                            )}
                          </div>
                        </td>
                      </tr>
                    ),
                    (errs.length > 0 || warns.length > 0) && (
                      <tr key={`${l.key}-err`}>
                        <td />
                        <td colSpan={9} className="space-y-0.5 px-2 pb-2">
                          {errs.map((m) => (
                            <p key={m} className="flex items-center gap-1.5 text-[12px] text-danger" role="alert">
                              <AlertCircle size={12} /> {m}
                            </p>
                          ))}
                          <WarningList warnings={warns} className="space-y-0.5" />
                        </td>
                      </tr>
                    ),
                  ];
                })}
              </tbody>
            </table>
            <div className="flex items-center justify-between border-t border-line px-4 py-2.5">
              <Button variant="ghost" onClick={addLine}>
                <Plus size={14} /> Add line
              </Button>
              <span className="text-[12px] text-subtle">Input tax credit is never assumed — choose it per line, or review it later.</span>
            </div>
            {err.lines && <p className="flex items-center gap-1.5 px-4 pb-3 text-[12.5px] text-danger" role="alert"><AlertCircle size={13} /> {err.lines}</p>}
          </Card>

          <Card className="p-5">
            <Field label="Notes" optional error={err.notes}>
              {(p) => (
                <textarea id={p.id} rows={2} value={d.notes} onChange={(e) => set("notes", e.target.value)}
                  className="w-full rounded-[9px] border border-line-strong bg-surface px-3 py-2 text-[13.5px] focus:border-accent-fill focus:shadow-[var(--focus-ring)] focus:outline-none" />
              )}
            </Field>
          </Card>
        </div>

        <div className="space-y-4">
          <Summary calc={calc} reverseCharge={d.reverseCharge} />
          {existing ? (
            <AttachmentsPanel documentId={existing.id} attachments={existing.attachments} editable />
          ) : (
            <Card className="p-4 text-[12.5px] text-muted">Save as a draft to attach the supplier's bill.</Card>
          )}
        </div>
      </div>

      <div className="material sticky bottom-0 z-20 -mx-8 mt-6 flex items-center justify-end gap-2 border-t border-line px-8 py-3">
        <Link to={back}>
          <Button>Cancel</Button>
        </Link>
        <Button disabled={busy} onClick={() => save.mutate(toInput(d))}>
          {save.isPending && <Loader2 size={15} className="animate-spin" />} Save draft
        </Button>
        <Button variant="primary" disabled={busy} onClick={() => setConfirming(true)}>
          {record.isPending && <Loader2 size={15} className="animate-spin" />} Record {PURCHASE_TYPE_LABEL[d.docType].toLowerCase()}
        </Button>
      </div>

      {confirming && (
        <Dialog
          open
          onClose={() => setConfirming(false)}
          title={`Record this ${PURCHASE_TYPE_LABEL[d.docType].toLowerCase()}?`}
          description={`${formatPaise(calc?.totals.totalPaise ?? 0)}${party ? ` from ${party.name}` : ""}.`}
          width={460}
          footer={
            <>
              <Button onClick={() => setConfirming(false)}>Not yet</Button>
              <Button variant="primary" autoFocus data-autofocus onClick={() => {
                setConfirming(false);
                record.mutate(toInput(d));
              }}>
                Record and post
              </Button>
            </>
          }
        >
          <p className="text-[13.5px] text-muted">
            It's posted to your books and can't be edited afterwards — corrections are made with a debit note or by cancelling it. Input tax credit can still be
            reviewed line by line.
          </p>
        </Dialog>
      )}
    </div>
  );
}

function Summary({ calc, reverseCharge }: { calc?: PurchaseCalculation; reverseCharge: boolean }) {
  const t = calc?.totals;
  const row = (label: string, paise: number | undefined, show = true) =>
    show && (
      <div className="flex justify-between py-1 text-[13px]">
        <span className="text-muted">{label}</span>
        <span className="num">{formatPaise(paise ?? 0)}</span>
      </div>
    );
  return (
    <Card className="sticky top-4 p-5">
      <h2 className="text-[12px] font-semibold uppercase tracking-[0.04em] text-muted">Summary</h2>
      {calc?.supplyKind && <p className="mt-1 text-[12.5px] text-muted">{SUPPLY_LABEL[calc.supplyKind]}</p>}
      <div className="mt-3 border-t border-line pt-2">
        {row("Taxable value", t?.taxablePaise)}
        {row("CGST", t?.cgstPaise, !!t?.cgstPaise)}
        {row("SGST", t?.sgstPaise, !!t?.sgstPaise)}
        {row("UTGST", t?.utgstPaise, !!t?.utgstPaise)}
        {row("IGST", t?.igstPaise, !!t?.igstPaise)}
        {row("Cess", t?.cessPaise, !!t?.cessPaise)}
        {row("Round-off", t?.roundOffPaise, !!t?.roundOffPaise)}
      </div>
      <div className="mt-2 flex items-baseline justify-between border-t border-line pt-3">
        <span className="text-[13px] font-semibold">{reverseCharge ? "Payable to supplier" : "Total"}</span>
        <span className="num text-[22px] font-semibold tracking-[-0.01em]">{formatPaise(t?.totalPaise ?? 0)}</span>
      </div>
      {!!t?.totalPaise && <p className="mt-2 text-[12px] leading-snug text-muted">{amountInWords(t.totalPaise)}</p>}
      {reverseCharge && !!calc?.reverseChargeTaxPaise && (
        <p className="mt-3 rounded-lg bg-warning-soft p-2.5 text-[12.5px] text-warning">
          You pay {formatPaise(calc.reverseChargeTaxPaise)} GST yourself under reverse charge.
        </p>
      )}
      {!!calc?.warnings.length && (
        <p className="mt-3 text-[11.5px] text-subtle">
          {calc.warnings.length === 1 ? "1 rate" : `${calc.warnings.length} rates`} to check against the supplier's bill. They don't stop you recording it.
        </p>
      )}
      <div className="mt-4">
        <Badge>{calc ? `GST engine · rules ${calc.rulePackVersion}` : "Calculated by the GST engine"}</Badge>
      </div>
    </Card>
  );
}
