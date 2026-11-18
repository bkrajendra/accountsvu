import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { BadgeCheck, CalendarDays, Loader2, Pencil, Plus, UserRound } from "lucide-react";
import { api, keys, type Registration, type RegistrationInput, type UpdateRegistrationInput } from "../../lib/api";
import { gstinParts, stateName } from "../../lib/gst";
import { useSave } from "../../lib/mutations";
import { Badge, Button, Card } from "../../components/ui";
import { Dialog } from "../../components/Dialog";
import { ChoiceCards, Field, FormGrid, Select, TextInput } from "../../components/form";
import { SettingsSection } from "./SettingsSection";

const TYPE_LABEL: Record<string, string> = { regular: "Regular", sez_unit: "SEZ unit", casual: "Casual", composition: "Composition" };

export function GstTab() {
  const { data: profile } = useQuery({ queryKey: keys.profile, queryFn: api.businessProfile });
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<Registration | null>(null);
  const regs = profile?.registrations ?? [];

  return (
    <SettingsSection
      title="GST registrations"
      description="One business can hold a GSTIN in each state it operates from. Returns are prepared per GSTIN."
      actions={
        profile?.organisation && (
          <Button onClick={() => setAdding(true)}>
            <Plus size={15} /> Add GSTIN
          </Button>
        )
      }
    >
      {regs.length === 0 ? (
        <Card className="p-6 text-center">
          <p className="font-medium">No GSTIN added</p>
          <p className="mt-1 text-[13px] text-muted">
            {profile?.organisation ? "Add your GSTIN to issue tax invoices." : "Set up your business first."}
          </p>
        </Card>
      ) : (
        <div className="space-y-3">
          {regs.map((r) => (
            <Card key={r.id} className="flex items-center gap-4 p-4">
              <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
                <BadgeCheck size={20} strokeWidth={1.8} />
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="num text-[15px] font-semibold tracking-[0.02em]">{r.gstin}</span>
                  {r.status === "active" ? <Badge tone="success">Active</Badge> : <Badge tone="danger">{r.status === "cancelled" ? "Cancelled" : "Suspended"}</Badge>}
                </div>
                <div className="mt-1 flex flex-wrap gap-x-4 gap-y-1 text-[12.5px] text-muted">
                  <span>{stateName(r.stateCode)}</span>
                  <span>{TYPE_LABEL[r.registrationType] ?? r.registrationType}</span>
                  <span>{r.filingFrequency === "monthly" ? "Monthly returns" : "Quarterly (QRMP)"}</span>
                  <span className="flex items-center gap-1">
                    <CalendarDays size={12} /> From {r.effectiveFrom}
                    {r.effectiveTo && ` to ${r.effectiveTo}`}
                  </span>
                  {r.signatoryName && (
                    <span className="flex items-center gap-1">
                      <UserRound size={12} /> {r.signatoryName}
                    </span>
                  )}
                </div>
              </div>
              <Button variant="ghost" onClick={() => setEditing(r)} aria-label={`Edit ${r.gstin}`}>
                <Pencil size={14} /> Edit
              </Button>
            </Card>
          ))}
        </div>
      )}
      <AddRegistrationDialog open={adding} onClose={() => setAdding(false)} orgPan={profile?.organisation?.pan ?? null} />
      {editing && <EditRegistrationDialog reg={editing} onClose={() => setEditing(null)} />}
    </SettingsSection>
  );
}

function AddRegistrationDialog({ open, onClose, orgPan }: { open: boolean; onClose: () => void; orgPan: string | null }) {
  const blank: RegistrationInput = { gstin: "", registrationType: "regular", filingFrequency: "monthly", effectiveFrom: "", signatoryName: "" };
  const [f, setF] = useState(blank);
  const close = () => {
    setF(blank);
    save.reset();
    onClose();
  };
  const save = useSave(api.addGstRegistration, { invalidate: [keys.profile, keys.series], success: "GSTIN added", onSuccess: close });
  const err = save.fieldErrors;
  const parts = gstinParts(f.gstin);
  const set = <K extends keyof RegistrationInput>(k: K, v: RegistrationInput[K]) => {
    setF((x) => ({ ...x, [k]: v }));
    save.clearError(k);
  };

  return (
    <Dialog
      open={open}
      onClose={close}
      title="Add a GSTIN"
      description={orgPan ? `It must belong to your PAN ${orgPan}. Default number series are created for it.` : undefined}
      footer={
        <>
          <Button onClick={close}>Cancel</Button>
          <Button variant="primary" disabled={save.isPending} onClick={() => save.mutate(f)}>
            {save.isPending && <Loader2 size={15} className="animate-spin" />} Add GSTIN
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <Field label="GSTIN" error={err.gstin} hint={parts ? `${stateName(parts.stateCode)} · PAN ${parts.pan}` : undefined}>
          {(p) => (
            <TextInput
              id={p.id}
              invalid={p.invalid}
              aria-describedby={p.describedBy}
              autoFocus
              maxLength={15}
              spellCheck={false}
              className="num font-medium tracking-[0.04em]"
              value={f.gstin}
              onChange={(e) => set("gstin", e.target.value.toUpperCase().replace(/\s/g, ""))}
            />
          )}
        </Field>
        <ChoiceCards
          name="Filing frequency"
          value={f.filingFrequency as "monthly" | "quarterly"}
          onChange={(v) => set("filingFrequency", v)}
          options={[
            { value: "monthly", title: "Monthly" },
            { value: "quarterly", title: "Quarterly (QRMP)" },
          ]}
        />
        <FormGrid>
          <Field label="Registration type" error={err.registrationType}>
            {(p) => (
              <Select id={p.id} invalid={p.invalid} value={f.registrationType} onChange={(e) => set("registrationType", e.target.value)}>
                <option value="regular">Regular</option>
                <option value="sez_unit">SEZ unit</option>
                <option value="casual">Casual taxable person</option>
              </Select>
            )}
          </Field>
          <Field label="Registered from" error={err.effectiveFrom}>
            {(p) => <TextInput id={p.id} type="date" invalid={p.invalid} value={f.effectiveFrom} onChange={(e) => set("effectiveFrom", e.target.value)} />}
          </Field>
          <Field label="Authorised signatory" optional error={err.signatoryName} className="col-span-2">
            {(p) => <TextInput id={p.id} invalid={p.invalid} value={f.signatoryName} onChange={(e) => set("signatoryName", e.target.value)} />}
          </Field>
        </FormGrid>
      </div>
    </Dialog>
  );
}

function EditRegistrationDialog({ reg, onClose }: { reg: Registration; onClose: () => void }) {
  const [f, setF] = useState<UpdateRegistrationInput>({
    registrationType: reg.registrationType,
    filingFrequency: reg.filingFrequency,
    effectiveFrom: reg.effectiveFrom,
    effectiveTo: reg.effectiveTo ?? "",
    signatoryName: reg.signatoryName ?? "",
    status: reg.status,
  });
  const save = useSave((x: UpdateRegistrationInput) => api.updateGstRegistration(reg.id, x, reg.rowVersion), {
    invalidate: [keys.profile],
    success: "Registration updated",
    onSuccess: onClose,
  });
  const err = save.fieldErrors;
  const set = <K extends keyof UpdateRegistrationInput>(k: K, v: UpdateRegistrationInput[K]) => {
    setF((x) => ({ ...x, [k]: v }));
    save.clearError(k);
  };

  return (
    <Dialog
      open
      onClose={onClose}
      title={`Edit ${reg.gstin}`}
      description="The GSTIN itself can't change. If it was cancelled, mark it cancelled and add the new one."
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={save.isPending} onClick={() => save.mutate(f)}>
            {save.isPending && <Loader2 size={15} className="animate-spin" />} Save
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <ChoiceCards
          name="Filing frequency"
          value={f.filingFrequency as "monthly" | "quarterly"}
          onChange={(v) => set("filingFrequency", v)}
          options={[
            { value: "monthly", title: "Monthly" },
            { value: "quarterly", title: "Quarterly (QRMP)" },
          ]}
        />
        <FormGrid>
          <Field label="Registration type" error={err.registrationType}>
            {(p) => (
              <Select id={p.id} invalid={p.invalid} value={f.registrationType} onChange={(e) => set("registrationType", e.target.value)}>
                <option value="regular">Regular</option>
                <option value="sez_unit">SEZ unit</option>
                <option value="casual">Casual taxable person</option>
              </Select>
            )}
          </Field>
          <Field label="Status" error={err.status}>
            {(p) => (
              <Select id={p.id} invalid={p.invalid} value={f.status} onChange={(e) => set("status", e.target.value)}>
                <option value="active">Active</option>
                <option value="suspended">Suspended</option>
                <option value="cancelled">Cancelled</option>
              </Select>
            )}
          </Field>
          <Field label="Registered from" error={err.effectiveFrom}>
            {(p) => <TextInput id={p.id} type="date" invalid={p.invalid} value={f.effectiveFrom} onChange={(e) => set("effectiveFrom", e.target.value)} />}
          </Field>
          <Field label="Registered until" optional error={err.effectiveTo}>
            {(p) => <TextInput id={p.id} type="date" invalid={p.invalid} value={f.effectiveTo} onChange={(e) => set("effectiveTo", e.target.value)} />}
          </Field>
          <Field label="Authorised signatory" optional error={err.signatoryName} className="col-span-2">
            {(p) => <TextInput id={p.id} invalid={p.invalid} value={f.signatoryName} onChange={(e) => set("signatoryName", e.target.value)} />}
          </Field>
        </FormGrid>
      </div>
    </Dialog>
  );
}
