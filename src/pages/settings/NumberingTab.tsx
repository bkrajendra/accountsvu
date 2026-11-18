import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Loader2, Lock, Pencil, Plus } from "lucide-react";
import { api, keys, type Series, type SeriesInput, type SeriesUpdateInput } from "../../lib/api";
import { DOCUMENT_TYPES, documentTypeLabel } from "../../lib/gst";
import { useSave } from "../../lib/mutations";
import { Badge, Button, Card } from "../../components/ui";
import { Dialog } from "../../components/Dialog";
import { Field, FormGrid, Select, TextInput, Toggle } from "../../components/form";
import { SettingsSection } from "./SettingsSection";

function previewOf(prefix: string, suffix: string, padding: number, next: number) {
  return `${prefix.toUpperCase()}${String(Math.max(1, next || 1)).padStart(padding, "0")}${suffix.toUpperCase()}`;
}

export function NumberingTab() {
  const { data: profile } = useQuery({ queryKey: keys.profile, queryFn: api.businessProfile });
  const { data: series = [] } = useQuery({ queryKey: keys.series, queryFn: api.listSeries });
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<Series | null>(null);

  const years = [...new Set(series.map((s) => s.financialYear))].sort().reverse();

  return (
    <SettingsSection
      title="Document numbering"
      description="Each GSTIN numbers its documents separately per financial year. GST allows up to 16 characters: letters, digits, '-' and '/'."
      actions={
        profile?.organisation && (
          <Button onClick={() => setCreating(true)}>
            <Plus size={15} /> New series
          </Button>
        )
      }
    >
      {series.length === 0 ? (
        <Card className="p-6 text-center text-muted">No number series yet.</Card>
      ) : (
        <div className="space-y-6">
          {years.map((fy) => (
            <div key={fy}>
              <h3 className="mb-2 text-[12px] font-semibold uppercase tracking-[0.04em] text-muted">FY {fy}</h3>
              <Card className="overflow-hidden">
                <table className="w-full text-[13px]">
                  <thead>
                    <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
                      <th className="px-4 py-2 font-medium">Document</th>
                      <th className="px-4 py-2 font-medium">GSTIN</th>
                      <th className="px-4 py-2 font-medium">Next number</th>
                      <th className="px-4 py-2" />
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-line">
                    {series
                      .filter((s) => s.financialYear === fy)
                      .map((s) => (
                        <tr key={s.id} className="hover:bg-hover">
                          <td className="px-4 py-2.5">
                            <span className="font-medium">{documentTypeLabel(s.documentType)}</span>
                            {s.isDefault && (
                              <span className="ml-2">
                                <Badge tone="accent">Default</Badge>
                              </span>
                            )}
                          </td>
                          <td className="num px-4 py-2.5 text-muted">{s.gstin ?? "Unregistered"}</td>
                          <td className="px-4 py-2.5">
                            <span className="num font-semibold tracking-[0.02em]">{s.nextPreview}</span>
                            {s.locked && <Lock size={12} className="ml-1.5 inline text-subtle" aria-label="In use" />}
                          </td>
                          <td className="px-4 py-2.5 text-right">
                            <Button variant="ghost" onClick={() => setEditing(s)} aria-label={`Edit ${documentTypeLabel(s.documentType)} series`}>
                              <Pencil size={14} />
                            </Button>
                          </td>
                        </tr>
                      ))}
                  </tbody>
                </table>
              </Card>
            </div>
          ))}
        </div>
      )}
      {creating && profile && (
        <CreateSeriesDialog
          onClose={() => setCreating(false)}
          registrations={profile.registrations.map((r) => ({ id: r.id, gstin: r.gstin }))}
          years={[profile.currentFinancialYear.code, profile.nextFinancialYear.code]}
        />
      )}
      {editing && <EditSeriesDialog s={editing} onClose={() => setEditing(null)} />}
    </SettingsSection>
  );
}

function CreateSeriesDialog({
  onClose,
  registrations,
  years,
}: {
  onClose: () => void;
  registrations: { id: string; gstin: string }[];
  years: string[];
}) {
  const [f, setF] = useState<SeriesInput>({
    gstRegistrationId: registrations[0]?.id,
    documentType: "tax_invoice",
    financialYear: years[0],
    prefix: "",
    suffix: "",
    padding: 4,
    nextNumber: 1,
    isDefault: false,
  });
  const save = useSave(api.createSeries, { invalidate: [keys.series], success: "Number series created", onSuccess: onClose });
  const err = save.fieldErrors;
  const set = <K extends keyof SeriesInput>(k: K, v: SeriesInput[K]) => {
    setF((x) => ({ ...x, [k]: v }));
    save.clearError(k);
  };
  const preview = previewOf(f.prefix, f.suffix, f.padding, f.nextNumber);
  const setFormat = <K extends keyof SeriesUpdateInput>(k: K, v: SeriesUpdateInput[K]) => set(k, v as SeriesInput[K]);

  return (
    <Dialog
      open
      onClose={onClose}
      title="New number series"
      description="Use a separate series for a branch, counter or showroom."
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={save.isPending} onClick={() => save.mutate(f)}>
            {save.isPending && <Loader2 size={15} className="animate-spin" />} Create series
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <FormGrid>
          <Field label="Document" error={err.documentType}>
            {(p) => (
              <Select id={p.id} invalid={p.invalid} value={f.documentType} onChange={(e) => set("documentType", e.target.value)}>
                {DOCUMENT_TYPES.map((d) => (
                  <option key={d.value} value={d.value}>
                    {d.label}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label="Financial year" error={err.financialYear}>
            {(p) => (
              <Select id={p.id} invalid={p.invalid} value={f.financialYear} onChange={(e) => set("financialYear", e.target.value)}>
                {years.map((y) => (
                  <option key={y} value={y}>
                    FY {y}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          {registrations.length > 0 && (
            <Field label="GSTIN" error={err.gstRegistrationId} className="col-span-2">
              {(p) => (
                <Select id={p.id} invalid={p.invalid} value={f.gstRegistrationId} onChange={(e) => set("gstRegistrationId", e.target.value)}>
                  {registrations.map((r) => (
                    <option key={r.id} value={r.id}>
                      {r.gstin}
                    </option>
                  ))}
                </Select>
              )}
            </Field>
          )}
        </FormGrid>
        <SeriesFormatFields f={f} set={setFormat} err={err} />
        <PreviewLine value={preview} />
        <Toggle checked={f.isDefault} onChange={(v) => set("isDefault", v)} label="Make this the default" description="New documents of this type use it unless you pick another." />
      </div>
    </Dialog>
  );
}

function EditSeriesDialog({ s, onClose }: { s: Series; onClose: () => void }) {
  const [f, setF] = useState<SeriesUpdateInput>({
    prefix: s.prefix,
    suffix: s.suffix,
    padding: s.padding,
    nextNumber: s.nextNumber,
    isDefault: s.isDefault,
  });
  const save = useSave((x: SeriesUpdateInput) => api.updateSeries(s.id, x, s.rowVersion), {
    invalidate: [keys.series],
    success: "Number series saved",
    onSuccess: onClose,
  });
  const err = save.fieldErrors;
  const set = <K extends keyof SeriesUpdateInput>(k: K, v: SeriesUpdateInput[K]) => {
    setF((x) => ({ ...x, [k]: v }));
    save.clearError(k);
  };

  return (
    <Dialog
      open
      onClose={onClose}
      title={`${documentTypeLabel(s.documentType)} · FY ${s.financialYear}`}
      description={s.gstin ?? "Unregistered"}
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
        {s.locked && (
          <p className="flex items-start gap-2 rounded-lg bg-surface-2 p-3 text-[12.5px] text-muted">
            <Lock size={14} className="mt-px shrink-0" />
            Numbers have been issued from this series, so its format is locked. You can only move the next number forward.
          </p>
        )}
        <SeriesFormatFields f={f} set={set} err={err} locked={s.locked} />
        <PreviewLine value={previewOf(f.prefix, f.suffix, f.padding, f.nextNumber)} />
        <Toggle checked={f.isDefault} onChange={(v) => set("isDefault", v)} label="Default for this document type" />
      </div>
    </Dialog>
  );
}

function SeriesFormatFields<T extends SeriesUpdateInput>({
  f,
  set,
  err,
  locked = false,
}: {
  f: T;
  set: <K extends keyof SeriesUpdateInput>(k: K, v: SeriesUpdateInput[K]) => void;
  err: Record<string, string>;
  locked?: boolean;
}) {
  const clean = (v: string) => v.toUpperCase().replace(/[^A-Z0-9/-]/g, "");
  return (
    <div className="grid grid-cols-[1fr_1fr_90px_110px] gap-3">
      <Field label="Prefix" error={err.prefix}>
        {(p) => <TextInput id={p.id} invalid={p.invalid} disabled={locked} className="num" value={f.prefix} placeholder="INV/26-27/" onChange={(e) => set("prefix", clean(e.target.value))} />}
      </Field>
      <Field label="Suffix" optional error={err.suffix}>
        {(p) => <TextInput id={p.id} invalid={p.invalid} disabled={locked} className="num" value={f.suffix} onChange={(e) => set("suffix", clean(e.target.value))} />}
      </Field>
      <Field label="Digits" error={err.padding}>
        {(p) => (
          <Select id={p.id} invalid={p.invalid} disabled={locked} value={f.padding} onChange={(e) => set("padding", Number(e.target.value))}>
            {[1, 2, 3, 4, 5, 6].map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </Select>
        )}
      </Field>
      <Field label="Next number" error={err.nextNumber}>
        {(p) => (
          <TextInput
            id={p.id}
            invalid={p.invalid}
            inputMode="numeric"
            className="num"
            value={f.nextNumber}
            onChange={(e) => set("nextNumber", Number(e.target.value.replace(/\D/g, "")) || 0)}
          />
        )}
      </Field>
    </div>
  );
}

function PreviewLine({ value }: { value: string }) {
  const tooLong = value.length > 16;
  return (
    <div className="flex items-center justify-between rounded-lg bg-surface-2 px-3 py-2.5 text-[13px]">
      <span className="text-muted">Next document will be</span>
      <span className="flex items-center gap-2">
        <span className="num font-semibold tracking-[0.02em]">{value}</span>
        <Badge tone={tooLong ? "danger" : "neutral"}>{value.length}/16</Badge>
      </span>
    </div>
  );
}
