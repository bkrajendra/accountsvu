import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import clsx from "clsx";
import { AlertCircle, AlertTriangle, Archive, CheckCircle2, FileCheck2, Loader2, Package, Paperclip } from "lucide-react";
import { api, keys, runningInTauri, type FilingView, type FilingReturnType, type SavedFile } from "../../lib/api";
import { shortDate, todayIso } from "../../lib/sales";
import { useSave } from "../../lib/mutations";
import { Badge, Button, Card } from "../../components/ui";
import { Dialog } from "../../components/Dialog";
import { Field, TextInput } from "../../components/form";
import { useToast } from "../../components/Toast";

/** `2026-10` → "October 2026". */
export function monthLabel(period: string) {
  return new Date(`${period}-01T00:00:00`).toLocaleDateString("en-IN", { month: "long", year: "numeric" });
}

export function useFiling(reg: string | null | undefined, period: string, returnType: FilingReturnType) {
  return useQuery({
    queryKey: keys.report("gst", "filing", reg ?? "", period, returnType),
    queryFn: () => api.returnFiling(reg!, period, returnType),
    enabled: !!reg,
  });
}

/** Checks that come from filing itself; the workpaper's own exceptions are listed below the panel. */
const FILING_CHECKS = new Set(["period_open", "previous_unfiled", "gstr1_unfiled", "gstr2b_missing", "gstr2b_review"]);

const FILE_LABEL: Record<string, string> = {
  portal_json: "Portal file",
  workpaper_json: "Workpaper (JSON)",
  workpaper_xlsx: "Workpaper (Excel)",
  acknowledgement: "Acknowledgement",
};

export function FilingPanel({ reg, period, returnType }: { reg: string; period: string; returnType: FilingReturnType }) {
  const { data } = useFiling(reg, period, returnType);
  const [recording, setRecording] = useState(false);
  const [removing, setRemoving] = useState(false);
  if (!data) return null;
  return (
    <>
      {data.filing ? <Filed data={data} onRemove={() => setRemoving(true)} /> : <NotFiled data={data} onRecord={() => setRecording(true)} />}
      {recording && <RecordDialog data={data} onClose={() => setRecording(false)} />}
      {removing && <RemoveDialog data={data} onClose={() => setRemoving(false)} />}
    </>
  );
}

function useSaveFile() {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const run = async (what: string, save: () => Promise<SavedFile | null>) => {
    if (!runningInTauri) {
      toast(`Saving the ${what} runs in the desktop app.`, "info");
      return;
    }
    setBusy(true);
    try {
      const f = await save();
      if (f) toast(`Saved ${f.fileName}`, "success", { label: "Show in folder", onClick: () => void api.openSavedFile(f.path) });
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), "error");
    } finally {
      setBusy(false);
    }
  };
  return { busy, run };
}

function locksText(data: FilingView) {
  return data.returnType === "gstr1" ? "sales documents" : "sales and purchases";
}

function NotFiled({ data, onRecord }: { data: FilingView; onRecord: () => void }) {
  const { busy, run } = useSaveFile();
  const v = data.validation;
  const own = [...v.errors, ...v.warnings].filter((c) => FILING_CHECKS.has(c.code));
  const otherErrors = v.errors.filter((c) => !FILING_CHECKS.has(c.code)).length;
  return (
    <Card className="mb-5 p-4">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="flex min-w-0 items-start gap-3">
          <span className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-lg bg-accent-soft text-accent">
            <FileCheck2 size={16} />
          </span>
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2 text-[14px] font-semibold">
              Not filed yet
              {v.canFile ? (
                <Badge tone="success">Ready to file</Badge>
              ) : (
                <Badge tone="danger" icon={<AlertCircle size={11} />}>
                  {v.errors.length} to fix first
                </Badge>
              )}
              {v.warnings.length > 0 && (
                <Badge tone="warning" icon={<AlertTriangle size={11} />}>
                  {v.warnings.length} to check
                </Badge>
              )}
            </div>
            <p className="mt-1 max-w-[620px] text-[12.5px] text-muted">
              Export the filing pack, file on the GST portal or the offline tool, then record the ARN here. Recording locks{" "}
              {data.periodLabel} for {locksText(data)}.
            </p>
            {(own.length > 0 || otherErrors > 0) && (
              <ul className="mt-2 space-y-1 text-[12.5px]">
                {own.map((c, i) => (
                  <li key={i} className="flex items-start gap-1.5">
                    {c.severity === "error" ? (
                      <AlertCircle size={13} className="mt-[2px] shrink-0 text-danger" aria-label="Error" />
                    ) : (
                      <AlertTriangle size={13} className="mt-[2px] shrink-0 text-warning" aria-label="Warning" />
                    )}
                    {c.message}
                  </li>
                ))}
                {otherErrors > 0 && (
                  <li className="flex items-start gap-1.5">
                    <AlertCircle size={13} className="mt-[2px] shrink-0 text-danger" aria-hidden />
                    {otherErrors} {otherErrors === 1 ? "error" : "errors"} in the return below.
                  </li>
                )}
              </ul>
            )}
          </div>
        </div>
        <div className="flex shrink-0 flex-wrap items-center gap-2">
          <Button
            disabled={!v.canFile || busy}
            onClick={() => void run("filing pack", () => api.saveFilingPack(data.registrationId, data.period, data.returnType))}
          >
            {busy ? <Loader2 size={15} className="animate-spin" /> : <Package size={15} />} Export filing pack
          </Button>
          <Button variant="primary" disabled={!v.canFile} onClick={onRecord}>
            <CheckCircle2 size={15} /> Record as filed
          </Button>
        </div>
      </div>
      {data.removed.length > 0 && <RemovedNote data={data} />}
    </Card>
  );
}

function Filed({ data, onRemove }: { data: FilingView; onRemove: () => void }) {
  const { busy, run } = useSaveFile();
  const f = data.filing!;
  return (
    <Card className="mb-5 p-4">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="flex min-w-0 items-start gap-3">
          <span className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-lg bg-success-soft text-success">
            <CheckCircle2 size={16} />
          </span>
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[14px] font-semibold">
              Filed on {shortDate(f.filedOn)}
              <span className="font-normal text-muted">·</span>
              <span className="text-[13px] font-normal text-muted">
                ARN <span className="num font-medium text-fg">{f.arn}</span>
              </span>
            </div>
            <p className="mt-1 text-[12.5px] text-muted">
              Recorded {shortDate(f.recordedAt.slice(0, 10))} · {data.periodLabel} is locked for {locksText(data)}
              {f.warningCount > 0 && ` · filed with ${f.warningCount} ${f.warningCount === 1 ? "warning" : "warnings"} open`}
            </p>
            {f.note && <p className="mt-1 text-[12.5px]">{f.note}</p>}
            <div className="mt-2 flex flex-wrap gap-1.5">
              {f.files.map((x) => (
                <span key={x.id} className="inline-flex items-center gap-1 rounded-md bg-active px-2 py-0.5 text-[11.5px] text-muted">
                  <Paperclip size={11} aria-hidden /> {FILE_LABEL[x.kind] ?? x.kind}
                </span>
              ))}
            </div>
            {data.changedSinceFiling && (
              <p className="mt-2 flex items-start gap-1.5 text-[12.5px] text-warning">
                <AlertTriangle size={13} className="mt-[2px] shrink-0" aria-hidden />
                The books no longer match the figures filed. The compliance pack keeps the return as filed.
              </p>
            )}
          </div>
        </div>
        <div className="flex shrink-0 flex-wrap items-center gap-2">
          <Button
            disabled={busy}
            onClick={() => void run("compliance pack", () => api.saveCompliancePack(data.registrationId, data.period, data.returnType))}
          >
            {busy ? <Loader2 size={15} className="animate-spin" /> : <Archive size={15} />} Compliance pack
          </Button>
          <Button variant="ghost" className="text-danger hover:bg-danger-soft" onClick={onRemove}>
            Remove record
          </Button>
        </div>
      </div>
      {data.removed.length > 0 && <RemovedNote data={data} />}
    </Card>
  );
}

function RemovedNote({ data }: { data: FilingView }) {
  return (
    <ul className="mt-3 space-y-0.5 border-t border-line pt-2 text-[12px] text-muted">
      {data.removed.map((r, i) => (
        <li key={i}>
          Earlier record removed{r.removedAt ? ` ${shortDate(r.removedAt.slice(0, 10))}` : ""}: ARN <span className="num">{r.arn}</span>
          {r.reason ? ` — ${r.reason}` : ""}
        </li>
      ))}
    </ul>
  );
}

const invalidate = [keys.report(), keys.sales, keys.purchases];

function RecordDialog({ data, onClose }: { data: FilingView; onClose: () => void }) {
  const [arn, setArn] = useState("");
  const [filedOn, setFiledOn] = useState(todayIso());
  const [note, setNote] = useState("");
  const [ack, setAck] = useState<{ fileName: string; bytes: number[] } | null>(null);
  const save = useSave(
    () =>
      api.recordFiling(data.registrationId, data.period, data.returnType, {
        arn,
        filedOn,
        note: note.trim() || null,
        acknowledgement: ack,
      }),
    { invalidate, success: `${data.label} for ${data.periodLabel} recorded as filed`, onSuccess: onClose },
  );
  const warnings = data.validation.warnings.length;
  return (
    <Dialog
      open
      onClose={onClose}
      title={`Record ${data.label} for ${data.periodLabel} as filed`}
      description={`Use the ARN from the portal's acknowledgement. The month is then locked for ${locksText(data)}; the figures and files are kept as filed.`}
      width={520}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={save.isPending} onClick={() => save.mutate(undefined)}>
            {save.isPending && <Loader2 size={15} className="animate-spin" />} Record as filed
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <Field label="ARN" error={save.fieldErrors.arn} hint="15 letters and digits, e.g. AA2710260012345.">
          {(p) => (
            <TextInput
              id={p.id}
              invalid={p.invalid}
              autoFocus
              className="num uppercase"
              maxLength={20}
              value={arn}
              onChange={(e) => {
                setArn(e.target.value);
                save.clearError("arn");
              }}
            />
          )}
        </Field>
        <Field label="Filed on" error={save.fieldErrors.filedOn}>
          {(p) => (
            <TextInput
              id={p.id}
              invalid={p.invalid}
              type="date"
              max={todayIso()}
              value={filedOn}
              onChange={(e) => {
                setFiledOn(e.target.value);
                save.clearError("filedOn");
              }}
            />
          )}
        </Field>
        <Field label="Acknowledgement" optional error={save.fieldErrors.acknowledgement} hint="PDF, PNG or JPG, up to 5 MB. Kept with the filing and in backups.">
          {(p) => (
            <input
              id={p.id}
              type="file"
              accept=".pdf,.png,.jpg,.jpeg"
              aria-invalid={p.invalid || undefined}
              className="text-[13px] file:mr-3 file:h-8 file:rounded-lg file:border file:border-line file:bg-surface file:px-3 file:text-[13px] file:font-medium file:text-fg hover:file:bg-surface-2"
              onChange={async (e) => {
                const file = e.target.files?.[0];
                save.clearError("acknowledgement");
                setAck(file ? { fileName: file.name, bytes: Array.from(new Uint8Array(await file.arrayBuffer())) } : null);
              }}
            />
          )}
        </Field>
        <Field label="Note" optional error={save.fieldErrors.note}>
          {(p) => <TextInput id={p.id} invalid={p.invalid} value={note} maxLength={500} onChange={(e) => setNote(e.target.value)} />}
        </Field>
        {warnings > 0 && (
          <p className={clsx("flex items-start gap-1.5 text-[12.5px] text-muted")}>
            <AlertTriangle size={13} className="mt-[2px] shrink-0 text-warning" aria-hidden />
            {warnings} {warnings === 1 ? "warning is" : "warnings are"} still open; the count is kept with the filing.
          </p>
        )}
      </div>
    </Dialog>
  );
}

function RemoveDialog({ data, onClose }: { data: FilingView; onClose: () => void }) {
  const [reason, setReason] = useState("");
  const save = useSave((r: string) => api.removeFiling(data.registrationId, data.period, data.returnType, r), {
    invalidate,
    success: "Filing record removed",
    onSuccess: onClose,
  });
  return (
    <Dialog
      open
      onClose={onClose}
      title={`Remove the ${data.label} filing record?`}
      description={`${data.periodLabel} opens again for changes. Nothing changes on the GST portal; the record stays in the audit log. Only the owner can do this.`}
      width={480}
      footer={
        <>
          <Button onClick={onClose}>Keep it</Button>
          <Button variant="primary" className="bg-danger hover:bg-danger" disabled={save.isPending} onClick={() => save.mutate(reason)}>
            Remove record
          </Button>
        </>
      }
    >
      <Field label="Reason" error={save.fieldErrors.reason} hint="Kept in the audit log, e.g. ARN entered against the wrong month.">
        {(p) => <TextInput id={p.id} invalid={p.invalid} autoFocus value={reason} onChange={(e) => setReason(e.target.value)} />}
      </Field>
    </Dialog>
  );
}
