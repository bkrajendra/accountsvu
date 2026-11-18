import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router";
import { Building2, Loader2, Lock } from "lucide-react";
import { api, keys, type Organisation, type OrganisationInput } from "../../lib/api";
import { CONSTITUTIONS, MONTHS, STATES } from "../../lib/gst";
import { useSave } from "../../lib/mutations";
import { Button, Card } from "../../components/ui";
import { Field, FormGrid, Select, TextInput, Toggle } from "../../components/form";
import { SettingsSection } from "./SettingsSection";

function toInput(o: Organisation): OrganisationInput {
  return {
    legalName: o.legalName,
    tradeName: o.tradeName ?? "",
    pan: o.pan ?? "",
    constitution: o.constitution ?? "",
    addressLine1: o.addressLine1 ?? "",
    addressLine2: o.addressLine2 ?? "",
    city: o.city ?? "",
    pincode: o.pincode ?? "",
    stateCode: o.stateCode,
    email: o.email ?? "",
    phone: o.phone ?? "",
    fyStartMonth: o.fyStartMonth,
    booksStartDate: o.booksStartDate,
    roundToRupee: o.roundToRupee,
  };
}

export function BusinessTab() {
  const navigate = useNavigate();
  const { data: profile } = useQuery({ queryKey: keys.profile, queryFn: api.businessProfile });
  const org = profile?.organisation;
  const [form, setForm] = useState<OrganisationInput | null>(null);

  useEffect(() => {
    if (org) setForm(toInput(org));
  }, [org]);

  const save = useSave((f: OrganisationInput) => api.updateOrganisation(f, org!.rowVersion), {
    invalidate: [keys.profile],
    success: "Business details saved",
  });

  const dirty = useMemo(() => !!org && !!form && JSON.stringify(toInput(org)) !== JSON.stringify(form), [org, form]);

  if (profile && !org) {
    return (
      <Card className="flex items-center gap-5 p-6">
        <div className="grid size-11 place-items-center rounded-xl bg-accent-soft text-accent">
          <Building2 size={22} strokeWidth={1.8} />
        </div>
        <div className="flex-1">
          <h2 className="text-title">No business set up yet</h2>
          <p className="mt-0.5 text-muted">Set up your business to unlock invoicing and GST.</p>
        </div>
        <Button variant="primary" onClick={() => navigate("/setup")}>
          Set up business
        </Button>
      </Card>
    );
  }
  if (!form) return null;

  const err = save.fieldErrors;
  const set = <K extends keyof OrganisationInput>(k: K, v: OrganisationInput[K]) => {
    setForm((f) => (f ? { ...f, [k]: v } : f));
    save.clearError(k);
  };
  const hasGstin = (profile?.registrations.length ?? 0) > 0;

  const text = (k: keyof OrganisationInput, label: string, o: { optional?: boolean; className?: string; type?: string; disabled?: boolean; hint?: string } = {}) => (
    <Field label={label} error={err[k]} optional={o.optional} className={o.className} hint={o.hint}>
      {(p) => (
        <TextInput
          id={p.id}
          type={o.type ?? "text"}
          invalid={p.invalid}
          aria-describedby={p.describedBy}
          disabled={o.disabled}
          value={String(form[k] ?? "")}
          onChange={(e) => set(k, e.target.value as never)}
        />
      )}
    </Field>
  );

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        save.mutate(form);
      }}
    >
      <SettingsSection title="Identity" description="How the business appears on invoices and returns.">
        <Card className="p-5">
          <FormGrid>
            {text("legalName", "Legal name", { className: "col-span-2" })}
            {text("tradeName", "Trade name", { optional: true })}
            <Field label="Type of business" error={err.constitution}>
              {(p) => (
                <Select id={p.id} invalid={p.invalid} value={form.constitution} onChange={(e) => set("constitution", e.target.value)}>
                  {CONSTITUTIONS.map((c) => (
                    <option key={c.value} value={c.value}>
                      {c.label}
                    </option>
                  ))}
                </Select>
              )}
            </Field>
            {text("pan", "PAN", { optional: !hasGstin, disabled: hasGstin, hint: hasGstin ? "Fixed by your GSTIN" : undefined })}
            <Field label="State" error={err.stateCode}>
              {(p) => (
                <Select id={p.id} invalid={p.invalid} value={form.stateCode} onChange={(e) => set("stateCode", Number(e.target.value))}>
                  {STATES.map((s) => (
                    <option key={s.code} value={s.code}>
                      {String(s.code).padStart(2, "0")} · {s.name}
                    </option>
                  ))}
                </Select>
              )}
            </Field>
          </FormGrid>
        </Card>
      </SettingsSection>

      <SettingsSection title="Address & contact" description="Printed on invoices.">
        <Card className="p-5">
          <FormGrid>
            {text("addressLine1", "Address", { optional: true, className: "col-span-2" })}
            {text("addressLine2", "Address line 2", { optional: true, className: "col-span-2" })}
            {text("city", "City", { optional: true })}
            {text("pincode", "PIN code", { optional: true })}
            {text("email", "E-mail", { optional: true, type: "email" })}
            {text("phone", "Phone", { optional: true, type: "tel" })}
          </FormGrid>
        </Card>
      </SettingsSection>

      <SettingsSection title="Books" description="Financial year and rounding rules.">
        <Card className="space-y-5 p-5">
          <FormGrid>
            {text("booksStartDate", "Books start on", { type: "date" })}
            <Field
              label="Financial year starts in"
              error={err.fyStartMonth}
              hint={
                profile?.hasPostings ? (
                  <span className="flex items-center gap-1">
                    <Lock size={11} /> Locked after transactions are posted
                  </span>
                ) : undefined
              }
            >
              {(p) => (
                <Select
                  id={p.id}
                  invalid={p.invalid}
                  disabled={profile?.hasPostings}
                  value={form.fyStartMonth}
                  onChange={(e) => set("fyStartMonth", Number(e.target.value))}
                >
                  {MONTHS.map((m, i) => (
                    <option key={m} value={i + 1}>
                      {m}
                    </option>
                  ))}
                </Select>
              )}
            </Field>
          </FormGrid>
          <Toggle
            checked={form.roundToRupee}
            onChange={(v) => set("roundToRupee", v)}
            label="Round invoice totals to the nearest rupee"
            description="The difference is posted to the Round-off account."
          />
        </Card>
      </SettingsSection>

      <div className="sticky bottom-0 -mx-8 flex justify-end gap-2 border-t border-line bg-bg/85 px-8 py-3 backdrop-blur">
        <Button disabled={!dirty || save.isPending} onClick={() => org && setForm(toInput(org))}>
          Discard
        </Button>
        <Button type="submit" variant="primary" disabled={!dirty || save.isPending}>
          {save.isPending && <Loader2 size={15} className="animate-spin" />}
          Save changes
        </Button>
      </div>
    </form>
  );
}
