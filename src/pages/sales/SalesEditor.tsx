import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams, useSearchParams } from "react-router";
import { AlertCircle, ArrowLeft, Loader2, Plus, X } from "lucide-react";
import {
  api,
  keys,
  type Item,
  type Party,
  type SalesCalculation,
  type SalesDocType,
  type SalesDocumentInput,
  type SalesDocumentView,
  type TaxCategory,
} from "../../lib/api";
import { formatPaise, formatRate, paiseToInput } from "../../lib/format";
import { STATES, stateName } from "../../lib/gst";
import { DOC_TYPE_LABEL, SUPPLY_LABEL, amountInWords, shortDate, todayIso } from "../../lib/sales";
import { useSave } from "../../lib/mutations";
import { Badge, Button, Card } from "../../components/ui";
import { Field, Select, TextInput, Toggle } from "../../components/form";
import { Combobox } from "../../components/Combobox";
import { CellInput, useDebounced } from "../../components/editor";
import { Dialog } from "../../components/Dialog";
import { WarningList, documentWarnings, rateLabel, warningsFor } from "../../components/TaxWarnings";

interface LineDraft {
  key: string;
  itemId: string | null;
  description: string;
  hsnSac: string;
  unit: string;
  quantity: string;
  unitPrice: string;
  discount: string;
  /** "taxable:18", "exempt", "nil_rated" or "non_gst". */
  tax: string;
}

interface Draft {
  docType: SalesDocType;
  partyId: string;
  docDate: string;
  dueDate: string;
  gstRegistrationId: string;
  placeOfSupply: string;
  zeroRated: boolean;
  pricesIncludeTax: boolean;
  originalDocumentId: string;
  reason: string;
  notes: string;
  lines: LineDraft[];
}

const newKey = () => crypto.randomUUID();
const percent = (bp: number) => formatRate(bp).replace("%", "");
const blankLine = (): LineDraft => ({ key: newKey(), itemId: null, description: "", hsnSac: "", unit: "", quantity: "1", unitPrice: "", discount: "", tax: "taxable:18" });

function taxOf(category: TaxCategory, bp: number): string {
  return category === "taxable" ? `taxable:${percent(bp)}` : category;
}

function fromView(v: SalesDocumentView, asCreditNote = false): Draft {
  return {
    docType: asCreditNote ? "credit_note" : v.docType,
    partyId: v.partyId,
    docDate: asCreditNote ? todayIso() : v.docDate,
    dueDate: asCreditNote ? "" : (v.dueDate ?? ""),
    gstRegistrationId: v.gstRegistrationId ?? "",
    placeOfSupply: String(v.placeOfSupply),
    zeroRated: v.zeroRated,
    pricesIncludeTax: v.pricesIncludeTax,
    originalDocumentId: asCreditNote ? v.id : (v.originalDocumentId ?? ""),
    reason: asCreditNote ? "" : (v.reason ?? ""),
    notes: asCreditNote ? "" : (v.notes ?? ""),
    lines: v.lines.map((l) => ({
      key: newKey(),
      itemId: l.itemId,
      description: l.description,
      hsnSac: l.hsnSac ?? "",
      unit: l.unit ?? "",
      quantity: l.quantity,
      unitPrice: paiseToInput(l.unitPricePaise) || "0",
      discount: paiseToInput(l.discountPaise),
      tax: taxOf(l.taxCategory, l.gstRateBp),
    })),
  };
}

/** Lines that have something in them; blank rows are not sent. */
function keptLines(d: Draft): number[] {
  return d.lines.flatMap((l, i) => (l.itemId || l.description.trim() || l.unitPrice.trim() ? [i] : []));
}

function toInput(d: Draft): SalesDocumentInput {
  const kept = keptLines(d);
  return {
    docType: d.docType,
    partyId: d.partyId,
    docDate: d.docDate,
    dueDate: d.dueDate || undefined,
    gstRegistrationId: d.gstRegistrationId || undefined,
    placeOfSupply: d.placeOfSupply ? Number(d.placeOfSupply) : undefined,
    zeroRated: d.zeroRated,
    pricesIncludeTax: d.pricesIncludeTax,
    originalDocumentId: d.originalDocumentId || undefined,
    reason: d.reason || undefined,
    notes: d.notes || undefined,
    lines: kept
      .map((i) => d.lines[i])
      .map((l) => {
        const [category, rate] = l.tax.split(":");
        return {
          itemId: l.itemId ?? undefined,
          description: l.description,
          hsnSac: l.hsnSac || undefined,
          unit: l.unit || undefined,
          quantity: l.quantity,
          unitPrice: l.unitPrice,
          discount: l.discount || undefined,
          taxCategory: category as TaxCategory,
          gstRate: rate,
        };
      }),
  };
}

function addDays(iso: string, days: number): string {
  const [y, m, d] = iso.split("-").map(Number);
  return todayIso(new Date(y, m - 1, d + days));
}


export function SalesEditor() {
  const { id } = useParams();
  const [params] = useSearchParams();
  const fromId = params.get("from");
  const { data: existing } = useQuery({ queryKey: keys.salesDocument(id ?? ""), queryFn: () => api.getSalesDocument(id!), enabled: !!id });
  const { data: original } = useQuery({ queryKey: keys.salesDocument(fromId ?? ""), queryFn: () => api.getSalesDocument(fromId!), enabled: !!fromId });

  if (id && !existing) return <Loading />;
  if (fromId && !original) return <Loading />;
  if (existing && existing.status !== "draft") {
    return (
      <Card className="p-8 text-center">
        <p className="text-muted">This document has been issued and can't be edited.</p>
        <Link to={`/sales/${existing.id}`} className="mt-3 inline-block text-accent">
          Open it
        </Link>
      </Card>
    );
  }
  const initial: Draft = existing
    ? fromView(existing)
    : original
      ? fromView(original, true)
      : {
          docType: "tax_invoice",
          partyId: "",
          docDate: todayIso(),
          dueDate: "",
          gstRegistrationId: "",
          placeOfSupply: "",
          zeroRated: false,
          pricesIncludeTax: false,
          originalDocumentId: "",
          reason: "",
          notes: "",
          lines: [blankLine()],
        };
  return <Editor key={id ?? fromId ?? "new"} initial={initial} existing={existing} original={original} />;
}

function Loading() {
  return (
    <div className="grid place-items-center py-24 text-muted">
      <Loader2 className="animate-spin" />
    </div>
  );
}

function Editor({ initial, existing, original }: { initial: Draft; existing?: SalesDocumentView; original?: SalesDocumentView }) {
  const navigate = useNavigate();
  const { data: profile } = useQuery({ queryKey: keys.profile, queryFn: api.businessProfile });
  const { data: parties = [] } = useQuery({ queryKey: keys.parties, queryFn: api.listParties });
  const { data: items = [] } = useQuery({ queryKey: keys.items, queryFn: api.listItems });
  const { data: options } = useQuery({ queryKey: keys.itemOptions, queryFn: api.itemOptions, staleTime: Infinity });
  const [d, setD] = useState<Draft>(initial);
  const [confirming, setConfirming] = useState(false);
  const grid = useRef<HTMLTableSectionElement>(null);

  const isCredit = d.docType === "credit_note";
  const registered = (profile?.registrations.filter((r) => r.status === "active").length ?? 0) > 0;
  const activeRegs = profile?.registrations.filter((r) => r.status === "active") ?? [];
  const party: Party | undefined = parties.find((p) => p.id === d.partyId);
  const exportOrSez = d.placeOfSupply === "96" || party?.gstType === "sez";

  // Live tax from the Rust core (the UI never does money arithmetic).
  const input = useMemo(() => toInput(d), [d]);
  const kept = useMemo(() => keptLines(d), [d]);
  const debounced = useDebounced(input, 150);
  const { data: calc } = useQuery({
    queryKey: ["salesCalc", debounced],
    queryFn: () => api.calculateSales(debounced),
    placeholderData: keepPreviousData,
    enabled: !!profile?.organisation,
  });

  const opts = { invalidate: [keys.sales, keys.parties, keys.series] };
  const saveDraft = useSave((x: SalesDocumentInput) => api.saveSalesDraft(existing?.id ?? null, x, existing?.rowVersion), {
    ...opts,
    success: "Draft saved",
    onSuccess: (v) => navigate(`/sales/${v.id}/edit`, { replace: true }),
  });
  const issue = useSave((x: SalesDocumentInput) => api.issueSalesDocument(existing?.id ?? null, x, existing?.rowVersion), {
    ...opts,
    success: isCredit ? "Credit note issued" : "Invoice issued",
    onSuccess: (v) => navigate(`/sales/${v.id}`, { replace: true }),
  });
  const err = useMemo(() => {
    const raw = { ...saveDraft.fieldErrors, ...issue.fieldErrors };
    // Server line indexes count only the lines sent; map them back to rows.
    return Object.fromEntries(
      Object.entries(raw).map(([k, v]) => {
        const m = /^lines\.(\d+)\.(.+)$/.exec(k);
        return [m ? `lines.${kept[Number(m[1])] ?? m[1]}.${m[2]}` : k, v];
      }),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [saveDraft.fieldErrors, issue.fieldErrors]);
  const busy = saveDraft.isPending || issue.isPending;
  const clear = (k: string) => {
    saveDraft.clearError(k);
    issue.clearError(k);
  };

  const set = <K extends keyof Draft>(k: K, v: Draft[K]) => {
    setD((x) => ({ ...x, [k]: v }));
    clear(k);
  };
  const setLine = (i: number, patch: Partial<LineDraft>) => {
    setD((x) => ({ ...x, lines: x.lines.map((l, n) => (n === i ? { ...l, ...patch } : l)) }));
    const n = kept.indexOf(i);
    if (n >= 0) for (const k of Object.keys(patch)) clear(`lines.${n}.${k === "tax" ? "gstRate" : k}`);
  };

  function pickParty(pid: string | null) {
    const p = parties.find((x) => x.id === pid);
    setD((x) => ({
      ...x,
      partyId: pid ?? "",
      placeOfSupply: p ? String(p.stateCode) : "",
      zeroRated: false,
      dueDate: p?.paymentTermsDays ? addDays(x.docDate, p.paymentTermsDays) : x.dueDate,
    }));
    clear("partyId");
  }

  function pickItem(i: number, itemId: string | null) {
    const it = items.find((x) => x.id === itemId);
    if (!it) return setLine(i, { itemId: null });
    setLine(i, {
      itemId: it.id,
      description: it.name,
      hsnSac: it.hsnSac ?? "",
      unit: it.unit,
      unitPrice: paiseToInput(it.salesPricePaise) || d.lines[i].unitPrice,
      tax: taxOf(it.taxCategory, it.gstRateBp),
    });
  }

  // Focus the new row once it has rendered.
  const focusLine = useRef<string | null>(null);
  useEffect(() => {
    if (!focusLine.current) return;
    grid.current?.querySelector<HTMLInputElement>(`tr[data-line="${focusLine.current}"] input`)?.focus();
    focusLine.current = null;
  }, [d.lines]);

  function addLine() {
    const line = blankLine();
    focusLine.current = line.key;
    setD((x) => ({ ...x, lines: [...x.lines, line] }));
  }

  /** Enter moves down the grid; on the last row it adds a line. */
  function onGridKey(e: KeyboardEvent<HTMLTableSectionElement>) {
    if (e.key !== "Enter" || e.defaultPrevented || !(e.target instanceof HTMLInputElement)) return;
    const row = e.target.closest("tr[data-line]");
    const rows = [...(grid.current?.querySelectorAll("tr[data-line]") ?? [])];
    const idx = row ? rows.indexOf(row) : -1;
    e.preventDefault();
    if (idx === rows.length - 1) addLine();
    else rows[idx + 1]?.querySelector<HTMLInputElement>("input")?.focus();
  }

  const partyOptions = parties
    .filter((p) => p.kind !== "supplier" && (p.isActive || p.id === d.partyId))
    .map((p) => ({ id: p.id, label: p.name, detail: p.gstin ?? stateName(p.stateCode), keywords: p.phone ?? "" }));
  const itemOptions = items
    .filter((i: Item) => i.isActive)
    .map((i) => ({ id: i.id, label: i.name, detail: [i.code, i.hsnSac, i.taxCategory === "taxable" ? formatRate(i.gstRateBp) : null].filter(Boolean).join(" · "), keywords: i.description ?? "" }));
  const taxOptions = [
    ...(options?.gstRates.filter((r) => r.basisPoints > 0).map((r) => ({ value: `taxable:${percent(r.basisPoints)}`, label: rateLabel(r, d.docDate) })) ?? []),
    { value: "exempt", label: "Exempt" },
    { value: "nil_rated", label: "Nil-rated" },
    { value: "non_gst", label: "Non-GST" },
  ];
  const lineErrors = (i: number) => Object.entries(err).filter(([k]) => k.startsWith(`lines.${i}.`)).map(([, m]) => m);
  const title = existing ? `Edit draft ${DOC_TYPE_LABEL[d.docType].toLowerCase()}` : isCredit ? "New credit note" : `New ${DOC_TYPE_LABEL[d.docType].toLowerCase()}`;

  return (
    <div>
      <div className="mb-6 flex items-end justify-between gap-4">
        <div>
          <Link to={original ? `/sales/${original.id}` : "/sales"} className="mb-2 inline-flex items-center gap-1 text-[13px] text-muted hover:text-fg">
            <ArrowLeft size={14} /> {original ? original.number : "Sales"}
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
              <Field label={isCredit ? "Customer" : "Bill to"} error={err.partyId} className="col-span-3">
                {(p) =>
                  isCredit ? (
                    <TextInput id={p.id} disabled value={original?.partyName ?? existing?.partyName ?? ""} />
                  ) : (
                    <Combobox
                      id={p.id}
                      invalid={p.invalid}
                      autoFocus={!existing}
                      options={partyOptions}
                      value={d.partyId || null}
                      onSelect={pickParty}
                      placeholder="Search customers"
                    />
                  )
                }
              </Field>
              {!isCredit && (
                <Field label="Document" error={err.docType} className="col-span-3">
                  {(p) => (
                    <Select id={p.id} invalid={p.invalid} value={d.docType} onChange={(e) => set("docType", e.target.value as SalesDocType)}>
                      {registered && <option value="tax_invoice">Tax invoice</option>}
                      <option value="bill_of_supply">Bill of supply — exempt or non-GST only</option>
                    </Select>
                  )}
                </Field>
              )}
              {isCredit && (
                <Field label="Reason" error={err.reason} className="col-span-3">
                  {(p) => <TextInput id={p.id} invalid={p.invalid} autoFocus value={d.reason} placeholder="e.g. Goods returned" onChange={(e) => set("reason", e.target.value)} />}
                </Field>
              )}
              <Field label="Date" error={err.docDate} className="col-span-2">
                {(p) => <TextInput id={p.id} invalid={p.invalid} type="date" value={d.docDate} max={todayIso()} onChange={(e) => set("docDate", e.target.value)} />}
              </Field>
              {!isCredit && (
                <Field label="Due" optional error={err.dueDate} className="col-span-2">
                  {(p) => <TextInput id={p.id} invalid={p.invalid} type="date" value={d.dueDate} min={d.docDate} onChange={(e) => set("dueDate", e.target.value)} />}
                </Field>
              )}
              <Field label="Place of supply" error={err.placeOfSupply} className="col-span-2">
                {(p) => (
                  <Select id={p.id} invalid={p.invalid} disabled={isCredit} value={d.placeOfSupply} onChange={(e) => set("placeOfSupply", e.target.value)}>
                    <option value="">From customer</option>
                    {STATES.map((s) => (
                      <option key={s.code} value={s.code}>
                        {s.name}
                      </option>
                    ))}
                    <option value="96">Outside India</option>
                  </Select>
                )}
              </Field>
              {activeRegs.length > 1 && !isCredit && (
                <Field label="Issued from GSTIN" error={err.gstRegistrationId} className="col-span-2">
                  {(p) => (
                    <Select id={p.id} invalid={p.invalid} value={d.gstRegistrationId} onChange={(e) => set("gstRegistrationId", e.target.value)}>
                      {activeRegs.map((r) => (
                        <option key={r.id} value={r.id}>
                          {r.gstin}
                        </option>
                      ))}
                    </Select>
                  )}
                </Field>
              )}
            </div>
            {party && (
              <p className="mt-3 text-[12.5px] text-muted">
                {party.gstin ? <span className="num">{party.gstin}</span> : "No GSTIN"} · {party.addressLine1 ?? stateName(party.stateCode)}
                {party.paymentTermsDays ? ` · ${party.paymentTermsDays}-day terms` : ""}
              </p>
            )}
            {original && (
              <p className="mt-3 text-[12.5px] text-muted">
                Against {original.number} of {shortDate(original.docDate)} · {formatPaise(original.totalPaise - original.creditedPaise)} left to credit. Place of
                supply and GST treatment follow the invoice.
              </p>
            )}
            {!isCredit && (
              <div className="mt-4 flex flex-wrap gap-x-8 gap-y-3 border-t border-line pt-4">
                <div className="w-[260px]">
                  <Toggle checked={d.pricesIncludeTax} onChange={(v) => set("pricesIncludeTax", v)} label="Prices include GST" />
                </div>
                {exportOrSez && (
                  <div className="w-[320px]">
                    <Toggle checked={d.zeroRated} onChange={(v) => set("zeroRated", v)} label="Zero-rated under LUT/bond" description="No GST charged on this export or SEZ supply." />
                    {err.zeroRated && <p className="mt-1 text-[12px] text-danger">{err.zeroRated}</p>}
                  </div>
                )}
              </div>
            )}
          </Card>

          <Card className="overflow-visible">
            <table className="w-full text-[13px]">
              <thead>
                <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
                  <th className="w-8 py-2 pl-4 font-medium">#</th>
                  <th className="px-2 py-2 font-medium">Item or description</th>
                  <th className="w-[92px] px-2 py-2 font-medium">HSN/SAC</th>
                  <th className="w-[76px] px-2 py-2 font-medium">Qty</th>
                  <th className="w-[96px] px-2 py-2 font-medium">Rate ₹</th>
                  <th className="w-[84px] px-2 py-2 font-medium">Disc. ₹</th>
                  <th className="w-[96px] px-2 py-2 font-medium">GST</th>
                  <th className="w-[104px] px-2 py-2 text-right font-medium">Amount</th>
                  <th className="w-9" />
                </tr>
              </thead>
              <tbody ref={grid} onKeyDown={onGridKey}>
                {d.lines.map((l, i) => {
                  const lc = kept.includes(i) ? (calc?.lines[kept.indexOf(i)] ?? null) : null;
                  const errs = lineErrors(i);
                  const warns = kept.includes(i) ? warningsFor(calc?.warnings, kept.indexOf(i)) : [];
                  const has = (f: string) => !!err[`lines.${i}.${f}`];
                  return [
                    <tr key={l.key} data-line={l.key} className="align-top">
                      <td className="num py-2 pl-4 pt-4 text-subtle">{i + 1}</td>
                      <td className="px-2 py-2">
                        <Combobox
                          ariaLabel={`Line ${i + 1} item`}
                          freeText
                          invalid={has("description") || has("itemId")}
                          options={itemOptions}
                          value={l.itemId}
                          text={l.description}
                          onSelect={(v) => pickItem(i, v)}
                          onText={(t) => setLine(i, { description: t })}
                          placeholder="Search items or type a description"
                        />
                      </td>
                      <td className="px-2 py-2">
                        <CellInput label={`Line ${i + 1} HSN/SAC`} invalid={has("hsnSac")} value={l.hsnSac} onChange={(v) => setLine(i, { hsnSac: v.replace(/\D/g, "") })} />
                      </td>
                      <td className="px-2 py-2">
                        <CellInput label={`Line ${i + 1} quantity`} invalid={has("quantity")} value={l.quantity} onChange={(v) => setLine(i, { quantity: v })} numeric />
                        {l.unit && <div className="mt-0.5 text-right text-[11px] text-subtle">{l.unit}</div>}
                      </td>
                      <td className="px-2 py-2">
                        <CellInput label={`Line ${i + 1} rate`} invalid={has("unitPrice")} value={l.unitPrice} onChange={(v) => setLine(i, { unitPrice: v })} numeric />
                      </td>
                      <td className="px-2 py-2">
                        <CellInput label={`Line ${i + 1} discount`} invalid={has("discount")} value={l.discount} onChange={(v) => setLine(i, { discount: v })} numeric />
                      </td>
                      <td className="px-2 py-2">
                        <Select aria-label={`Line ${i + 1} GST`} invalid={has("gstRate") || has("taxCategory")} value={l.tax} onChange={(e) => setLine(i, { tax: e.target.value })} className="px-2">
                          {taxOptions.map((o) => (
                            <option key={o.value} value={o.value}>
                              {o.label}
                            </option>
                          ))}
                        </Select>
                      </td>
                      <td className="num px-2 py-2 pt-4 text-right font-medium">{lc ? formatPaise(lc.amountPaise) : <span className="text-subtle">—</span>}</td>
                      <td className="py-2 pr-2 pt-3">
                        <button
                          type="button"
                          aria-label={`Remove line ${i + 1}`}
                          disabled={d.lines.length === 1}
                          onClick={() => setD((x) => ({ ...x, lines: x.lines.filter((_, n) => n !== i) }))}
                          className="grid size-7 place-items-center rounded-md text-subtle hover:bg-hover hover:text-fg disabled:invisible"
                        >
                          <X size={14} />
                        </button>
                      </td>
                    </tr>,
                    (errs.length > 0 || warns.length > 0) && (
                      <tr key={`${l.key}-err`}>
                        <td />
                        <td colSpan={8} className="space-y-0.5 px-2 pb-2">
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
              <Button variant="ghost" onClick={() => addLine()}>
                <Plus size={14} /> Add line
              </Button>
              <span className="text-[12px] text-subtle">Enter moves to the next line</span>
            </div>
            {err.lines && <p className="flex items-center gap-1.5 px-4 pb-3 text-[12.5px] text-danger" role="alert"><AlertCircle size={13} /> {err.lines}</p>}
          </Card>

          <Card className="p-5">
            <Field label="Notes" optional error={err.notes}>
              {(p) => (
                <textarea
                  id={p.id}
                  rows={2}
                  value={d.notes}
                  placeholder="Shown on the invoice, e.g. bank details or delivery terms"
                  onChange={(e) => set("notes", e.target.value)}
                  className="w-full rounded-[9px] border border-line-strong bg-surface px-3 py-2 text-[13.5px] placeholder:text-subtle focus:border-accent-fill focus:shadow-[var(--focus-ring)] focus:outline-none"
                />
              )}
            </Field>
          </Card>
        </div>

        <Summary calc={calc} credit={isCredit} />
      </div>

      <div className="material sticky bottom-0 z-20 -mx-8 mt-6 flex items-center justify-end gap-2 border-t border-line px-8 py-3">
        <Link to={original ? `/sales/${original.id}` : "/sales"}>
          <Button>Cancel</Button>
        </Link>
        <Button disabled={busy} onClick={() => saveDraft.mutate(toInput(d))}>
          {saveDraft.isPending && <Loader2 size={15} className="animate-spin" />} Save draft
        </Button>
        <Button variant="primary" disabled={busy} onClick={() => setConfirming(true)}>
          {issue.isPending && <Loader2 size={15} className="animate-spin" />} Issue {isCredit ? "credit note" : DOC_TYPE_LABEL[d.docType].toLowerCase()}
        </Button>
      </div>

      {confirming && (
        <Dialog
          open
          onClose={() => setConfirming(false)}
          title={`Issue ${calc?.numberPreview ?? DOC_TYPE_LABEL[d.docType].toLowerCase()}?`}
          description={`${formatPaise(calc?.totals.totalPaise ?? 0)} to ${party?.name ?? original?.partyName ?? "the customer"}.`}
          width={460}
          footer={
            <>
              <Button onClick={() => setConfirming(false)}>Not yet</Button>
              <Button
                variant="primary"
                autoFocus
                onClick={() => {
                  setConfirming(false);
                  issue.mutate(toInput(d));
                }}
              >
                Issue and post
              </Button>
            </>
          }
        >
          <p className="text-[13.5px] text-muted">
            Issuing gives it the next number and posts it to your books. After that it can't be edited — corrections are made with a credit note or by
            cancelling it.
          </p>
        </Dialog>
      )}
    </div>
  );
}


function Summary({ calc, credit }: { calc?: SalesCalculation; credit: boolean }) {
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
      <h2 className="text-[12px] font-semibold uppercase tracking-[0.04em] text-muted">{credit ? "Credit" : "Summary"}</h2>
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
        <span className="text-[13px] font-semibold">Total</span>
        <span className="num text-[22px] font-semibold tracking-[-0.01em]">{formatPaise(t?.totalPaise ?? 0)}</span>
      </div>
      {!!t?.totalPaise && <p className="mt-2 text-[12px] leading-snug text-muted">{amountInWords(t.totalPaise)}</p>}
      {calc && !calc.supplyKind && <p className="mt-3 text-[12px] text-subtle">Choose a customer to work out CGST/SGST or IGST.</p>}
      {calc?.gstr1 && (calc.gstr1.label || calc.gstr1.nilTable) && (
        <div className="mt-4 border-t border-line pt-3 text-[12.5px]">
          <div className="text-muted">Reported in GSTR-1</div>
          <div className="mt-0.5 font-medium">
            {[calc.gstr1.label, calc.gstr1.nilTable ? "Nil/exempt/non-GST (table 8)" : null].filter(Boolean).join(" · ")}
          </div>
        </div>
      )}
      <WarningList warnings={documentWarnings(calc?.warnings)} className="mt-3 space-y-1" />
      {!!calc?.warnings.length && (
        <p className="mt-2 text-[11.5px] text-subtle">
          {calc.warnings.length === 1 ? "1 point" : `${calc.warnings.length} points`} to check. They don't stop you issuing.
        </p>
      )}
      <div className="mt-4">
        <Badge>{calc ? `GST engine · rules ${calc.rulePackVersion}` : "Calculated by the GST engine"}</Badge>
      </div>
    </Card>
  );
}
