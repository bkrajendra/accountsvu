import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useSearchParams } from "react-router";
import clsx from "clsx";
import { ArrowLeft, Loader2, Wand2 } from "lucide-react";
import { api, keys, type AdvanceTaxInput, type VoucherInput, type VoucherMode, type VoucherPurpose, type VoucherType } from "../../lib/api";
import { STATES } from "../../lib/gst";
import { rateLabel } from "../../components/TaxWarnings";
import { Badge } from "../../components/ui";
import { formatPaise, paiseToInput } from "../../lib/format";
import { shortDate, todayIso } from "../../lib/sales";
import { MODE_LABEL } from "../../lib/banking";
import { useSave } from "../../lib/mutations";
import { Button, Card, Segmented } from "../../components/ui";
import { Combobox } from "../../components/Combobox";
import { Field, Select, TextInput } from "../../components/form";

/** Rupee text → paise without floating point ("1,250.5" → 125050); null if not a number. */
export function inputToPaise(s: string | undefined): number | null {
  const t = (s ?? "").replace(/[₹,\s]/g, "");
  if (!/^\d+(\.\d{0,2})?$/.test(t)) return null;
  const [r, f = ""] = t.split(".");
  return Number(r) * 100 + Number((f + "00").slice(0, 2));
}

type Side = "party" | "account";

export function VoucherEditor() {
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const lineId = params.get("line") ?? undefined;
  const { data: moneyAccounts = [] } = useQuery({ queryKey: keys.moneyAccounts, queryFn: api.listMoneyAccounts });
  const { data: parties = [] } = useQuery({ queryKey: keys.parties, queryFn: api.listParties });
  const { data: ledgers = [] } = useQuery({ queryKey: keys.voucherAccounts, queryFn: api.voucherAccounts });

  const [type, setType] = useState<VoucherType>(params.get("type") === "payment" ? "payment" : "receipt");
  const [side, setSide] = useState<Side>(params.get("counter") ? "account" : "party");
  const [f, setF] = useState<Omit<VoucherInput, "voucherType" | "allocations">>({
    voucherDate: params.get("date") ?? todayIso(),
    moneyAccountId: params.get("account") ?? "",
    partyId: params.get("party") ?? undefined,
    counterAccountId: params.get("counter") ?? undefined,
    amount: params.get("amount") ?? "",
    mode: undefined,
    reference: params.get("ref") ?? "",
    narration: params.get("narration") ?? "",
    statementLineId: lineId,
  });
  const [purpose, setPurpose] = useState<VoucherPurpose>(params.get("purpose") === "refund" ? "refund" : "settlement");
  const [refundOf, setRefundOf] = useState<string | null>(params.get("refundOf"));
  // Tax on the part kept as an advance (services).
  const [taxOn, setTaxOn] = useState(false);
  const [tax, setTax] = useState<Partial<AdvanceTaxInput>>({});
  // Allocation amounts by document id; `manual` once the user edits one.
  const [alloc, setAlloc] = useState<Record<string, string>>({});
  const [manual, setManual] = useState(false);

  const active = moneyAccounts.filter((m) => m.isActive);
  useEffect(() => {
    if (!f.moneyAccountId && active.length) {
      const bank = active.find((m) => m.kind === "bank");
      setF((x) => ({ ...x, moneyAccountId: (bank ?? active[0]).id }));
    }
  }, [active, f.moneyAccountId]);
  const money = moneyAccounts.find((m) => m.id === f.moneyAccountId);

  const partyId = side === "party" ? f.partyId : undefined;
  const refund = side === "party" && purpose === "refund";
  const { data: openAll = [], isFetching: loadingOpen } = useQuery({
    queryKey: keys.openDocuments(partyId ?? "", type),
    queryFn: () => api.openDocuments(partyId!, type),
    enabled: !!partyId,
  });
  const open = refund ? [] : openAll;
  // A refund gives back an advance that came the other way.
  const other: VoucherType = type === "receipt" ? "payment" : "receipt";
  const { data: advances = [] } = useQuery({
    queryKey: keys.partyAdvances(partyId ?? "", other),
    queryFn: () => api.partyAdvances(partyId!, other),
    enabled: !!partyId && refund,
  });
  const { data: profile } = useQuery({ queryKey: keys.profile, queryFn: api.businessProfile });
  const { data: options } = useQuery({ queryKey: keys.itemOptions, queryFn: api.itemOptions, staleTime: Infinity });
  const activeRegs = profile?.registrations.filter((r) => r.status === "active" && r.registrationType !== "composition") ?? [];
  const party = parties.find((p) => p.id === partyId);

  const amountPaise = inputToPaise(f.amount) ?? 0;
  // Oldest first, until the money runs out.
  const fifo = useMemo(() => {
    let left = amountPaise;
    const out: Record<string, string> = {};
    for (const d of open) {
      const take = Math.min(left, d.outstandingPaise);
      out[d.id] = take > 0 ? paiseToInput(take) : "";
      left -= take;
    }
    return out;
  }, [open, amountPaise]);
  const shown = manual ? alloc : fifo;
  // A document named in the link (e.g. "Record receipt" on an invoice) settles first.
  const preDoc = params.get("doc");
  useEffect(() => {
    if (preDoc && open.length && !f.amount) {
      const d = open.find((x) => x.id === preDoc);
      if (d) {
        setF((x) => ({ ...x, amount: paiseToInput(d.outstandingPaise) }));
        setAlloc({ [d.id]: paiseToInput(d.outstandingPaise) });
        setManual(true);
      }
    }
  }, [preDoc, open, f.amount]);

  const receipt = type === "receipt";
  const allocatedPaise = open.reduce((a, d) => a + (inputToPaise(shown[d.id]) ?? 0), 0);
  const onAccount = amountPaise - allocatedPaise;

  const save = useSave(
    (input: VoucherInput) => api.createVoucher(input),
    {
      invalidate: [keys.vouchers, keys.moneyAccounts, keys.parties, keys.sales, keys.purchases, ["openDocuments"], ["moneyBook"], ["reconciliation"], ["settlements"]],
      success: type === "receipt" ? "Receipt recorded" : "Payment recorded",
      onSuccess: (v) => (lineId ? navigate(-1) : navigate(`/banking/vouchers/${v.id}`, { replace: true })),
    },
  );
  const err = save.fieldErrors;
  const set = <K extends keyof typeof f>(k: K, v: (typeof f)[K]) => {
    setF((x) => ({ ...x, [k]: v }));
    save.clearError(k);
  };

  const taxable = receipt && side === "party" && purpose === "settlement" && !!partyId && onAccount > 0 && activeRegs.length > 0;
  const taxInput: AdvanceTaxInput = {
    registrationId: tax.registrationId ?? activeRegs[0]?.id ?? "",
    placeOfSupply: tax.placeOfSupply ?? party?.stateCode ?? activeRegs[0]?.stateCode ?? 0,
    rateBp: tax.rateBp ?? 1800,
  };
  const taxReg = activeRegs.find((r) => r.id === taxInput.registrationId);
  const taxPreview = (() => {
    if (!taxOn || onAccount <= 0) return 0;
    const t = Math.round((onAccount * 10000) / (10000 + taxInput.rateBp));
    return taxReg?.stateCode === taxInput.placeOfSupply ? 2 * Math.round((t * taxInput.rateBp) / 20000) : Math.round((t * taxInput.rateBp) / 10000);
  })();

  const submit = () =>
    save.mutate({
      ...f,
      voucherType: type,
      purpose: side === "party" ? purpose : "settlement",
      refundOfId: refund ? (refundOf ?? undefined) : undefined,
      advanceTax: taxable && taxOn ? taxInput : undefined,
      partyId: side === "party" ? f.partyId || undefined : undefined,
      counterAccountId: side === "account" ? f.counterAccountId || undefined : undefined,
      mode: money?.kind === "cash" ? "cash" : (f.mode ?? "neft"),
      allocations: side === "party" ? open.map((d) => ({ documentId: d.id, amount: shown[d.id] ?? "" })).filter((a) => a.amount) : [],
    });

  const partyOptions = parties.map((p) => ({
    id: p.id,
    label: p.name,
    detail: [p.gstin, p.kind === "both" ? "Customer & supplier" : p.kind === "customer" ? "Customer" : "Supplier"].filter(Boolean).join(" · "),
  }));
  const ledgerOptions = ledgers
    .filter((a) => a.id !== money?.accountId)
    .map((a) => ({ id: a.id, label: a.name, detail: a.code }));
  return (
    <div className="mx-auto max-w-[880px]">
      <Link to="/banking?tab=vouchers" className="mb-2 inline-flex items-center gap-1 text-[13px] text-muted hover:text-fg">
        <ArrowLeft size={14} /> Banking
      </Link>
      <div className="mb-6 flex items-end justify-between gap-4">
        <div>
          <h1 className="text-display">{refund ? "Record a refund" : receipt ? "Record a receipt" : "Record a payment"}</h1>
          <p className="mt-1 text-[14px] text-muted">
            {lineId ? "From a bank statement line — it is reconciled when you save." : refund ? (receipt ? "Money a supplier paid back to you." : "Money you paid back to a customer.") : receipt ? "Money that came in. Settle invoices, or keep it as an advance." : "Money that went out. Settle bills, or book it to an account."}
          </p>
        </div>
        {!lineId && (
          <Segmented<VoucherType>
            label="Type"
            value={type}
            onChange={(t) => {
              setType(t);
              setManual(false);
            }}
            options={[
              { value: "receipt", label: "Receipt" },
              { value: "payment", label: "Payment" },
            ]}
          />
        )}
      </div>

      <Card className="p-5">
        <div className="grid grid-cols-3 gap-4">
          <Field label="Date" error={err.voucherDate}>
            {(p) => <TextInput id={p.id} type="date" invalid={p.invalid} value={f.voucherDate} max={todayIso()} onChange={(e) => set("voucherDate", e.target.value)} />}
          </Field>
          <Field label={receipt ? "Into" : "Out of"} error={err.moneyAccountId}>
            {(p) => (
              <Select id={p.id} invalid={p.invalid} value={f.moneyAccountId} disabled={!!lineId} onChange={(e) => set("moneyAccountId", e.target.value)}>
                {active.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label="Amount (₹)" error={err.amount}>
            {(p) => (
              <TextInput id={p.id} invalid={p.invalid} className="num text-right" inputMode="decimal" placeholder="0.00" readOnly={!!lineId} autoFocus={!lineId && !!f.partyId} value={f.amount}
                onChange={(e) => {
                  set("amount", e.target.value);
                  setManual(false);
                }} />
            )}
          </Field>

          <div className="col-span-3 flex items-center gap-3 border-t border-line pt-4">
            <span className="text-[12.5px] font-medium">{receipt ? "Received from" : "Paid to"}</span>
            <Segmented<Side>
              label="Counterparty"
              value={side}
              onChange={(s) => {
                setSide(s);
                save.clearError("partyId");
              }}
              options={[
                { value: "party", label: receipt ? "A customer or supplier" : "A supplier or customer" },
                { value: "account", label: "Another account" },
              ]}
            />
            {side === "party" && (
              <Segmented<VoucherPurpose>
                label="Purpose"
                value={purpose}
                onChange={(v) => {
                  setPurpose(v);
                  setRefundOf(null);
                  save.clearError("partyId");
                }}
                options={[
                  { value: "settlement", label: receipt ? "Payment received" : "Payment made" },
                  { value: "refund", label: receipt ? "Refund from a supplier" : "Refund to a customer" },
                ]}
              />
            )}
          </div>
          {side === "party" ? (
            <Field label="Party" error={err.partyId} className="col-span-2">
              {(p) => (
                <Combobox id={p.id} invalid={p.invalid} autoFocus={!f.partyId} options={partyOptions} value={f.partyId ?? null}
                  onSelect={(v) => {
                    set("partyId", v ?? undefined);
                    setManual(false);
                  }} placeholder="Search parties" />
              )}
            </Field>
          ) : (
            <Field label="Account" error={err.counterAccountId ?? err.partyId} className="col-span-2" hint="e.g. Bank charges, Owner's capital, Drawings, GST, or another bank for a transfer.">
              {(p) => <Combobox id={p.id} invalid={p.invalid} options={ledgerOptions} value={f.counterAccountId ?? null} onSelect={(v) => set("counterAccountId", v ?? undefined)} placeholder="Search accounts" />}
            </Field>
          )}
          {money?.kind === "cash" ? (
            <Field label="Mode">{(p) => <TextInput id={p.id} value="Cash" disabled />}</Field>
          ) : (
            <Field label="Mode" error={err.mode}>
              {(p) => (
                <Select id={p.id} value={f.mode ?? "neft"} onChange={(e) => set("mode", e.target.value as VoucherMode)}>
                  {(Object.keys(MODE_LABEL) as VoucherMode[]).map((m) => (
                    <option key={m} value={m}>
                      {MODE_LABEL[m]}
                    </option>
                  ))}
                </Select>
              )}
            </Field>
          )}
          <Field label="Reference" optional error={err.reference} hint="UTR, cheque or transaction number.">
            {(p) => <TextInput id={p.id} invalid={p.invalid} className="num" value={f.reference ?? ""} onChange={(e) => set("reference", e.target.value)} />}
          </Field>
          <Field label="Narration" optional error={err.narration} className="col-span-2">
            {(p) => <TextInput id={p.id} invalid={p.invalid} value={f.narration ?? ""} onChange={(e) => set("narration", e.target.value)} />}
          </Field>
        </div>
      </Card>

      {refund && f.partyId && (
        <Card className="mt-5 overflow-hidden">
          <div className="border-b border-line px-4 py-3">
            <h2 className="text-[14px] font-semibold">Refund of</h2>
            <p className="text-[12.5px] text-muted">
              {receipt ? "The supplier gives back money you paid them in advance." : "You give back money the customer paid in advance."} Link the advance so it shows as used up.
            </p>
          </div>
          <div className="divide-y divide-line text-[13px]">
            <RefundChoice checked={!refundOf} onChange={() => setRefundOf(null)} label="Not linked to an advance" detail="e.g. a refund of an overpayment or a credit note balance" />
            {advances.map((a) => (
              <RefundChoice
                key={a.voucherId}
                checked={refundOf === a.voucherId}
                onChange={() => {
                  setRefundOf(a.voucherId);
                  if (!f.amount) set("amount", paiseToInput(a.onAccountPaise));
                  save.clearError("refundOfId");
                }}
                label={<span className="num">{a.number}</span>}
                detail={`${shortDate(a.voucherDate)} · ${formatPaise(a.onAccountPaise)} left of ${formatPaise(a.amountPaise)}`}
                badge={a.taxedRegistrationId ? <Badge tone="accent">GST paid</Badge> : null}
              />
            ))}
          </div>
          {advances.some((a) => a.voucherId === refundOf && a.taxedRegistrationId) && (
            <p className="border-t border-line px-4 py-2 text-[12.5px] text-muted">The GST paid on the part refunded is taken back (GSTR-1 table 11B).</p>
          )}
          {(err.refundOfId || err.allocations) && <p className="border-t border-line px-4 py-2 text-[12.5px] text-danger">{err.refundOfId ?? err.allocations}</p>}
        </Card>
      )}

      {side === "party" && f.partyId && !refund && (
        <Card className="mt-5 overflow-hidden">
          <div className="flex items-center justify-between border-b border-line px-4 py-3">
            <div>
              <h2 className="text-[14px] font-semibold">{receipt ? "Invoices to settle" : "Bills to settle"}</h2>
              <p className="text-[12.5px] text-muted">Oldest first. Change any amount; what is left stays on the party's account as an advance.</p>
            </div>
            {manual && (
              <Button variant="ghost" onClick={() => setManual(false)}>
                <Wand2 size={15} /> Oldest first
              </Button>
            )}
          </div>
          {loadingOpen && open.length === 0 ? (
            <div className="grid place-items-center py-8 text-muted">
              <Loader2 className="animate-spin" size={18} />
            </div>
          ) : open.length === 0 ? (
            <p className="px-4 py-6 text-center text-[13px] text-muted">Nothing outstanding. The whole amount is kept as an advance.</p>
          ) : (
            <table className="w-full text-[13px]">
              <thead>
                <tr className="bg-surface-2 text-left text-[12px] text-muted">
                  <th className="px-4 py-2 font-medium">Number</th>
                  <th className="px-4 py-2 font-medium">Date</th>
                  <th className="px-4 py-2 font-medium">Due</th>
                  <th className="px-4 py-2 text-right font-medium">Total</th>
                  <th className="px-4 py-2 text-right font-medium">Outstanding</th>
                  <th className="w-[160px] px-4 py-2 text-right font-medium">Settle now</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {open.map((d, i) => (
                  <tr key={d.id}>
                    <td className="num px-4 py-2">{d.number ?? "—"}</td>
                    <td className="num px-4 py-2 text-muted">{shortDate(d.docDate)}</td>
                    <td className="num px-4 py-2 text-muted">{shortDate(d.dueDate)}</td>
                    <td className="num px-4 py-2 text-right">{formatPaise(d.totalPaise)}</td>
                    <td className="num px-4 py-2 text-right">{formatPaise(d.outstandingPaise)}</td>
                    <td className="px-4 py-1.5">
                      <TextInput aria-label={`Settle ${d.number ?? "document"}`} invalid={!!err[`allocations.${i}.amount`]} title={err[`allocations.${i}.amount`]} className="num h-8 text-right" inputMode="decimal" placeholder="0.00" value={shown[d.id] ?? ""}
                        onChange={(e) => {
                          setAlloc({ ...shown, [d.id]: e.target.value });
                          setManual(true);
                          save.clearError(`allocations.${i}.amount`);
                        }} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          <div className="flex justify-end gap-8 border-t border-line px-4 py-3 text-[13px]">
            <span className="text-muted">
              Settled <span className="num ml-1 font-medium text-fg">{formatPaise(allocatedPaise)}</span>
            </span>
            <span className={clsx("text-muted", onAccount < 0 && "text-danger")}>
              {onAccount < 0 ? "Over by" : "Advance"} <span className="num ml-1 font-medium">{formatPaise(Math.abs(onAccount))}</span>
            </span>
          </div>
          {(err.allocations || Object.keys(err).some((k) => k.startsWith("allocations."))) && (
            <p className="border-t border-line px-4 py-2 text-[12.5px] text-danger">{err.allocations ?? Object.entries(err).find(([k]) => k.startsWith("allocations."))?.[1]}</p>
          )}
        </Card>
      )}

      {taxable && (
        <Card className="mt-5 p-4">
          <label className="flex items-start gap-2.5 text-[13.5px]">
            <input type="checkbox" className="mt-[3px] size-4 accent-[var(--accent-fill)]" checked={taxOn} onChange={(e) => { setTaxOn(e.target.checked); save.clearError("advanceTax"); }} />
            <span>
              Pay GST on the advance of {formatPaise(onAccount)}
              <span className="block text-[12.5px] text-muted">
                For services, GST is due when an advance comes in (GSTR-1 table 11A). It is taken back when you adjust the advance against an invoice or refund it. Advances for goods don't need this.
              </span>
            </span>
          </label>
          {taxOn && (
            <div className="mt-4 grid grid-cols-3 gap-4">
              <Field label="GSTIN" error={err["advanceTax.registrationId"]}>
                {(p) => (
                  <Select id={p.id} invalid={p.invalid} value={taxInput.registrationId} onChange={(e) => setTax((t) => ({ ...t, registrationId: e.target.value }))}>
                    {activeRegs.map((r) => (
                      <option key={r.id} value={r.id}>
                        {r.gstin}
                      </option>
                    ))}
                  </Select>
                )}
              </Field>
              <Field label="Place of supply" error={err["advanceTax.placeOfSupply"]}>
                {(p) => (
                  <Select id={p.id} invalid={p.invalid} value={taxInput.placeOfSupply} onChange={(e) => setTax((t) => ({ ...t, placeOfSupply: Number(e.target.value) }))}>
                    {STATES.map((st) => (
                      <option key={st.code} value={st.code}>
                        {String(st.code).padStart(2, "0")} · {st.name}
                      </option>
                    ))}
                  </Select>
                )}
              </Field>
              <Field label="GST rate" error={err["advanceTax.rateBp"]} hint={taxPreview ? `GST in the advance: ${formatPaise(taxPreview)}` : undefined}>
                {(p) => (
                  <Select id={p.id} invalid={p.invalid} value={taxInput.rateBp} onChange={(e) => setTax((t) => ({ ...t, rateBp: Number(e.target.value) }))}>
                    {(options?.gstRates ?? []).filter((r) => r.basisPoints > 0).map((r) => (
                      <option key={r.basisPoints} value={r.basisPoints}>
                        {rateLabel(r, f.voucherDate)}
                      </option>
                    ))}
                  </Select>
                )}
              </Field>
            </div>
          )}
          {err.advanceTax && <p className="mt-2 text-[12.5px] text-danger">{err.advanceTax}</p>}
        </Card>
      )}

      <div className="mt-5 flex justify-end gap-2">
        <Button onClick={() => navigate(-1)}>Cancel</Button>
        <Button variant="primary" disabled={save.isPending} onClick={submit}>
          {save.isPending && <Loader2 size={15} className="animate-spin" />} {refund ? "Record refund" : receipt ? "Record receipt" : "Record payment"}
        </Button>
      </div>
      {err.statementLineId && <p className="mt-2 text-right text-[12.5px] text-danger">{err.statementLineId}</p>}
    </div>
  );
}

function RefundChoice({ checked, onChange, label, detail, badge }: { checked: boolean; onChange: () => void; label: React.ReactNode; detail: string; badge?: React.ReactNode }) {
  return (
    <label className={clsx("flex cursor-pointer items-center gap-3 px-4 py-2.5", checked ? "bg-accent-soft" : "hover:bg-hover")}>
      <input type="radio" name="refund-of" className="size-4 accent-[var(--accent-fill)]" checked={checked} onChange={onChange} />
      <span className="flex-1">
        {label}
        <span className="ml-2 text-[12.5px] text-muted">{detail}</span>
      </span>
      {badge}
    </label>
  );
}
