import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Loader2, ShoppingBag, Store, Users } from "lucide-react";
import { api, keys, type Party, type PartyGstType, type PartyInput, type PartyKind } from "../../lib/api";
import { gstinError, gstinParts, STATES, stateName } from "../../lib/gst";
import { paiseToInput } from "../../lib/format";
import { useSave } from "../../lib/mutations";
import { Button } from "../../components/ui";
import { Dialog } from "../../components/Dialog";
import { ChoiceCards, Field, FormGrid, Select, TextInput, Toggle } from "../../components/form";

export const GST_TYPES: { value: PartyGstType; label: string; hint: string }[] = [
  { value: "regular", label: "Registered (regular)", hint: "Has a GSTIN — B2B invoices, ITC flows" },
  { value: "composition", label: "Registered (composition)", hint: "Has a GSTIN, can't claim ITC" },
  { value: "unregistered", label: "Unregistered business", hint: "No GSTIN" },
  { value: "consumer", label: "Consumer", hint: "Individual buyer — B2C" },
  { value: "overseas", label: "Overseas", hint: "Outside India — exports/imports" },
  { value: "sez", label: "SEZ unit or developer", hint: "Has a GSTIN, zero-rated supplies" },
];

const HAS_GSTIN: PartyGstType[] = ["regular", "composition", "sez"];

type Draft = Required<Omit<PartyInput, "stateCode" | "shippingStateCode" | "paymentTermsDays">> & {
  stateCode: string;
  shippingStateCode: string;
  paymentTermsDays: string;
};

function draftFrom(p?: Party): Draft {
  return {
    kind: p?.kind ?? "customer",
    name: p?.name ?? "",
    legalName: p?.legalName ?? "",
    gstType: p?.gstType ?? "regular",
    gstin: p?.gstin ?? "",
    pan: p?.pan ?? "",
    stateCode: p ? String(p.stateCode) : "",
    addressLine1: p?.addressLine1 ?? "",
    addressLine2: p?.addressLine2 ?? "",
    city: p?.city ?? "",
    pincode: p?.pincode ?? "",
    shippingSame: p?.shippingSame ?? true,
    shippingLine1: p?.shippingLine1 ?? "",
    shippingLine2: p?.shippingLine2 ?? "",
    shippingCity: p?.shippingCity ?? "",
    shippingPincode: p?.shippingPincode ?? "",
    shippingStateCode: p?.shippingStateCode ? String(p.shippingStateCode) : "",
    contactPerson: p?.contactPerson ?? "",
    email: p?.email ?? "",
    phone: p?.phone ?? "",
    paymentTermsDays: p?.paymentTermsDays != null ? String(p.paymentTermsDays) : "",
    creditLimit: paiseToInput(p?.creditLimitPaise),
    openingBalance: paiseToInput(p?.openingBalancePaise),
    openingBalanceSide: p ? (p.openingBalancePaise < 0 ? "payable" : "receivable") : "receivable",
    notes: p?.notes ?? "",
    isActive: p?.isActive ?? true,
  };
}

function toInput(d: Draft): PartyInput {
  const hasGstin = HAS_GSTIN.includes(d.gstType);
  return {
    ...d,
    gstin: hasGstin ? d.gstin.trim().toUpperCase() : undefined,
    pan: hasGstin ? undefined : d.pan.trim().toUpperCase(),
    stateCode: d.stateCode ? Number(d.stateCode) : undefined,
    shippingStateCode: d.shippingStateCode ? Number(d.shippingStateCode) : undefined,
    paymentTermsDays: d.paymentTermsDays.trim() ? Number(d.paymentTermsDays) : undefined,
  };
}

export function PartyDialog({ party, onClose }: { party?: Party; onClose: () => void }) {
  const { data: profile } = useQuery({ queryKey: keys.profile, queryFn: api.businessProfile });
  const [d, setD] = useState<Draft>(() => draftFrom(party));
  const [touchedGstin, setTouchedGstin] = useState(false);
  const save = useSave(
    (input: PartyInput) => (party ? api.updateParty(party.id, input, party.rowVersion) : api.createParty(input)),
    { invalidate: [keys.parties], success: party ? "Party saved" : "Party added", onSuccess: onClose },
  );
  const err = save.fieldErrors;
  const set = <K extends keyof Draft>(k: K, v: Draft[K]) => {
    setD((x) => ({ ...x, [k]: v }));
    save.clearError(k);
  };

  const hasGstin = HAS_GSTIN.includes(d.gstType);
  const parts = hasGstin ? gstinParts(d.gstin) : null;
  const localGstinError = hasGstin && touchedGstin && d.gstin ? gstinError(d.gstin) : null;
  const overseas = d.gstType === "overseas";
  const booksStart = profile?.organisation?.booksStartDate;

  return (
    <Dialog
      open
      onClose={onClose}
      width={680}
      title={party ? party.name : "New party"}
      description={party ? "Edit customer or supplier details." : "A customer you sell to, a supplier you buy from, or both."}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={save.isPending} onClick={() => save.mutate(toInput(d))}>
            {save.isPending && <Loader2 size={15} className="animate-spin" />} {party ? "Save" : "Add party"}
          </Button>
        </>
      }
    >
      <div className="space-y-6">
        <ChoiceCards<PartyKind>
          name="Party type"
          columns={3}
          value={d.kind}
          onChange={(v) => {
            set("kind", v);
            if (!party && !d.openingBalance) set("openingBalanceSide", v === "supplier" ? "payable" : "receivable");
          }}
          options={[
            { value: "customer", title: "Customer", icon: <Users size={17} /> },
            { value: "supplier", title: "Supplier", icon: <ShoppingBag size={17} /> },
            { value: "both", title: "Both", icon: <Store size={17} /> },
          ]}
        />

        <Section title="Name">
          <FormGrid>
            <Field label="Display name" hint="Shown on invoices and in lists." error={err.name}>
              {(p) => <TextInput id={p.id} invalid={p.invalid} autoFocus value={d.name} onChange={(e) => set("name", e.target.value)} />}
            </Field>
            <Field label="Legal name" optional error={err.legalName}>
              {(p) => <TextInput id={p.id} invalid={p.invalid} value={d.legalName} onChange={(e) => set("legalName", e.target.value)} />}
            </Field>
          </FormGrid>
        </Section>

        <Section title="GST">
          <FormGrid>
            <Field label="GST status" error={err.gstType} hint={GST_TYPES.find((g) => g.value === d.gstType)?.hint}>
              {(p) => (
                <Select id={p.id} invalid={p.invalid} value={d.gstType} onChange={(e) => set("gstType", e.target.value as PartyGstType)}>
                  {GST_TYPES.map((g) => (
                    <option key={g.value} value={g.value}>
                      {g.label}
                    </option>
                  ))}
                </Select>
              )}
            </Field>
            {hasGstin ? (
              <Field
                label="GSTIN"
                error={err.gstin ?? localGstinError ?? undefined}
                hint={parts ? `${stateName(parts.stateCode)} · PAN ${parts.pan}` : undefined}
              >
                {(p) => (
                  <TextInput
                    id={p.id}
                    invalid={p.invalid}
                    className="num uppercase tracking-[0.04em]"
                    maxLength={15}
                    value={d.gstin}
                    onBlur={() => setTouchedGstin(true)}
                    onChange={(e) => set("gstin", e.target.value.toUpperCase())}
                  />
                )}
              </Field>
            ) : (
              <Field label="PAN" optional error={err.pan}>
                {(p) => (
                  <TextInput id={p.id} invalid={p.invalid} className="num uppercase" maxLength={10} value={d.pan} onChange={(e) => set("pan", e.target.value.toUpperCase())} />
                )}
              </Field>
            )}
            {!hasGstin && !overseas && (
              <Field label="State" hint="Decides CGST + SGST or IGST." error={err.stateCode}>
                {(p) => (
                  <Select id={p.id} invalid={p.invalid} value={d.stateCode} onChange={(e) => set("stateCode", e.target.value)}>
                    <option value="">Choose…</option>
                    {STATES.map((s) => (
                      <option key={s.code} value={s.code}>
                        {s.name}
                      </option>
                    ))}
                  </Select>
                )}
              </Field>
            )}
          </FormGrid>
        </Section>

        <Section title="Billing address">
          <FormGrid>
            <Field label="Address line 1" optional error={err.addressLine1} className="col-span-2">
              {(p) => <TextInput id={p.id} invalid={p.invalid} value={d.addressLine1} onChange={(e) => set("addressLine1", e.target.value)} />}
            </Field>
            <Field label="Address line 2" optional error={err.addressLine2} className="col-span-2">
              {(p) => <TextInput id={p.id} invalid={p.invalid} value={d.addressLine2} onChange={(e) => set("addressLine2", e.target.value)} />}
            </Field>
            <Field label="City" optional error={err.city}>
              {(p) => <TextInput id={p.id} invalid={p.invalid} value={d.city} onChange={(e) => set("city", e.target.value)} />}
            </Field>
            <Field label={overseas ? "Postal code" : "PIN code"} optional error={err.pincode}>
              {(p) => <TextInput id={p.id} invalid={p.invalid} className="num" inputMode={overseas ? "text" : "numeric"} value={d.pincode} onChange={(e) => set("pincode", e.target.value)} />}
            </Field>
          </FormGrid>
          <div className="mt-4">
            <Toggle checked={!d.shippingSame} onChange={(v) => set("shippingSame", !v)} label="Ship to a different address" />
          </div>
          {!d.shippingSame && (
            <div className="rise-in mt-4">
              <FormGrid>
                <Field label="Shipping address line 1" error={err.shippingLine1} className="col-span-2">
                  {(p) => <TextInput id={p.id} invalid={p.invalid} value={d.shippingLine1} onChange={(e) => set("shippingLine1", e.target.value)} />}
                </Field>
                <Field label="Shipping address line 2" optional error={err.shippingLine2} className="col-span-2">
                  {(p) => <TextInput id={p.id} invalid={p.invalid} value={d.shippingLine2} onChange={(e) => set("shippingLine2", e.target.value)} />}
                </Field>
                <Field label="City" optional error={err.shippingCity}>
                  {(p) => <TextInput id={p.id} invalid={p.invalid} value={d.shippingCity} onChange={(e) => set("shippingCity", e.target.value)} />}
                </Field>
                <Field label="PIN code" optional error={err.shippingPincode}>
                  {(p) => <TextInput id={p.id} invalid={p.invalid} className="num" inputMode="numeric" value={d.shippingPincode} onChange={(e) => set("shippingPincode", e.target.value)} />}
                </Field>
                <Field label="State" error={err.shippingStateCode}>
                  {(p) => (
                    <Select id={p.id} invalid={p.invalid} value={d.shippingStateCode} onChange={(e) => set("shippingStateCode", e.target.value)}>
                      <option value="">Choose…</option>
                      {STATES.map((s) => (
                        <option key={s.code} value={s.code}>
                          {s.name}
                        </option>
                      ))}
                    </Select>
                  )}
                </Field>
              </FormGrid>
            </div>
          )}
        </Section>

        <Section title="Contact and terms">
          <FormGrid>
            <Field label="Contact person" optional error={err.contactPerson}>
              {(p) => <TextInput id={p.id} invalid={p.invalid} value={d.contactPerson} onChange={(e) => set("contactPerson", e.target.value)} />}
            </Field>
            <Field label="Phone" optional error={err.phone}>
              {(p) => <TextInput id={p.id} invalid={p.invalid} className="num" inputMode="tel" value={d.phone} onChange={(e) => set("phone", e.target.value)} />}
            </Field>
            <Field label="E-mail" optional error={err.email} className="col-span-2">
              {(p) => <TextInput id={p.id} invalid={p.invalid} type="email" value={d.email} onChange={(e) => set("email", e.target.value)} />}
            </Field>
            <Field label="Payment terms" optional hint="Days until an invoice is due." error={err.paymentTermsDays}>
              {(p) => (
                <TextInput id={p.id} invalid={p.invalid} className="num" inputMode="numeric" placeholder="e.g. 30" value={d.paymentTermsDays} onChange={(e) => set("paymentTermsDays", e.target.value.replace(/\D/g, ""))} />
              )}
            </Field>
            <Field label="Credit limit (₹)" optional error={err.creditLimit}>
              {(p) => <TextInput id={p.id} invalid={p.invalid} className="num" inputMode="decimal" value={d.creditLimit} onChange={(e) => set("creditLimit", e.target.value)} />}
            </Field>
          </FormGrid>
        </Section>

        <Section title="Opening balance">
          <FormGrid>
            <Field label="Amount (₹)" optional error={err.openingBalance} hint={booksStart ? `As on ${new Date(booksStart).toLocaleDateString("en-IN", { day: "numeric", month: "short", year: "numeric" })}, when your books start.` : undefined}>
              {(p) => <TextInput id={p.id} invalid={p.invalid} className="num" inputMode="decimal" placeholder="0.00" value={d.openingBalance} onChange={(e) => set("openingBalance", e.target.value)} />}
            </Field>
            <Field label="Who owes whom" error={err.openingBalanceSide}>
              {(p) => (
                <Select id={p.id} invalid={p.invalid} value={d.openingBalanceSide} onChange={(e) => set("openingBalanceSide", e.target.value as Draft["openingBalanceSide"])}>
                  <option value="receivable">They owe you</option>
                  <option value="payable">You owe them</option>
                </Select>
              )}
            </Field>
          </FormGrid>
          {party?.openingEntryId && (
            <p className="mt-2 text-[12px] text-muted">Changing it posts a correcting entry; the original stays in the ledger.</p>
          )}
        </Section>

        <Section title="More">
          <div className="space-y-4">
            <Field label="Notes" optional error={err.notes}>
              {(p) => (
                <textarea
                  id={p.id}
                  rows={2}
                  value={d.notes}
                  onChange={(e) => set("notes", e.target.value)}
                  className="w-full rounded-[9px] border border-line-strong bg-surface px-3 py-2 text-[13.5px] focus:border-accent-fill focus:shadow-[var(--focus-ring)] focus:outline-none"
                />
              )}
            </Field>
            {party && <Toggle checked={d.isActive} onChange={(v) => set("isActive", v)} label="Active" description="Inactive parties stay in reports but aren't offered on new documents." />}
          </div>
        </Section>
      </div>
    </Dialog>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <h3 className="mb-3 text-[12px] font-semibold uppercase tracking-[0.04em] text-muted">{title}</h3>
      {children}
    </section>
  );
}
