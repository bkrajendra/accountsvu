import { useRef, useState, type DragEvent } from "react";
import { useQueryClient } from "@tanstack/react-query";
import clsx from "clsx";
import { AlertTriangle, CheckCircle2, Download, FileSpreadsheet, Loader2, Upload } from "lucide-react";
import { api, ApiError, keys, runningInTauri, type ImportKind, type ImportMapping, type ImportPreview, type ImportResult, type ParsedImport } from "../lib/api";
import { Badge, Button } from "./ui";
import { Dialog } from "./Dialog";
import { Select } from "./form";
import { useToast } from "./Toast";

const NOUN: Record<ImportKind, { plural: string; title: string }> = {
  parties: { plural: "parties", title: "Import parties" },
  items: { plural: "items", title: "Import items" },
};

type Step = "file" | "map" | "review" | "done";
const ORDER: Step[] = ["file", "map", "review", "done"];

/**
 * Spreadsheet import (spec §5.1, D15): choose a file → match columns →
 * review problems → import. The Rust core parses the file and validates every
 * row with the same rules as the forms; nothing is saved until "Import".
 */
export function ImportDialog({ kind, onClose }: { kind: ImportKind; onClose: () => void }) {
  const qc = useQueryClient();
  const toast = useToast();
  const [step, setStep] = useState<Step>("file");
  const direction = useRef<"forward" | "back">("forward");
  const [parsed, setParsed] = useState<ParsedImport | null>(null);
  const [mapping, setMapping] = useState<ImportMapping>({});
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [result, setResult] = useState<ImportResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [mappingErrors, setMappingErrors] = useState<Record<string, string>>({});
  const noun = NOUN[kind];

  const go = (to: Step) => {
    direction.current = ORDER.indexOf(to) >= ORDER.indexOf(step) ? "forward" : "back";
    setError(null);
    setStep(to);
  };

  async function run<T>(fn: () => Promise<T>): Promise<T | undefined> {
    setBusy(true);
    setError(null);
    try {
      return await fn();
    } catch (e) {
      if (e instanceof ApiError && Object.keys(e.fields).length) {
        setMappingErrors(Object.fromEntries(Object.entries(e.fields).map(([k, v]) => [k.replace(/^mapping\./, ""), v])));
      } else {
        setError(e instanceof Error ? e.message : String(e));
      }
      return undefined;
    } finally {
      setBusy(false);
    }
  }

  async function chooseFile(file: File | undefined) {
    if (!file) return;
    const p = await run(async () => api.parseImport(kind, file.name, new Uint8Array(await file.arrayBuffer())));
    if (p) {
      setParsed(p);
      setMapping(p.mapping);
      setMappingErrors({});
      go("map");
    }
  }

  async function downloadTemplate() {
    if (runningInTauri) {
      const f = await run(() => api.saveImportTemplate(kind));
      if (f) toast(`Saved ${f.fileName}`, "success", { label: "Open", onClick: () => void api.openSavedFile(f.path) });
      return;
    }
    const t = await run(() => api.importTemplate(kind));
    if (!t) return;
    const url = URL.createObjectURL(new Blob(["﻿", t.csv], { type: "text/csv;charset=utf-8" }));
    const a = Object.assign(document.createElement("a"), { href: url, download: t.fileName });
    a.click();
    URL.revokeObjectURL(url);
    toast(`Downloaded ${t.fileName} to your browser's downloads`, "success");
  }

  async function review() {
    if (!parsed) return;
    const p = await run(() => api.previewImport(kind, parsed.table, mapping));
    if (p) {
      setPreview(p);
      go("review");
    }
  }

  async function commit() {
    if (!parsed) return;
    const r = await run(() => api.commitImport(kind, parsed.table, mapping));
    if (r) {
      setResult(r);
      for (const k of [kind === "parties" ? keys.parties : keys.items, keys.audit, keys.dashboard, keys.profile])
        void qc.invalidateQueries({ queryKey: k });
      toast(`Imported ${r.created} ${noun.plural}`);
      go("done");
    }
  }

  const footer =
    step === "file" ? (
      <Button onClick={onClose}>Cancel</Button>
    ) : step === "map" ? (
      <>
        <Button onClick={() => go("file")}>Back</Button>
        <Button variant="primary" disabled={busy} onClick={review}>
          {busy && <Loader2 size={15} className="animate-spin" />} Check rows
        </Button>
      </>
    ) : step === "review" ? (
      <>
        <Button onClick={() => go("map")}>Back</Button>
        <Button variant="primary" disabled={busy || !preview?.valid} onClick={commit}>
          {busy && <Loader2 size={15} className="animate-spin" />} Import {preview?.valid ?? 0} {noun.plural}
        </Button>
      </>
    ) : (
      <Button variant="primary" onClick={onClose}>
        Done
      </Button>
    );

  return (
    <Dialog open onClose={onClose} title={noun.title} description={STEP_HINT[step](noun.plural)} footer={footer} width={step === "map" ? 640 : 560}>
      <div key={step} className={direction.current === "forward" ? "step-forward" : "step-back"}>
        {step === "file" && <FileStep busy={busy} onFile={chooseFile} onTemplate={downloadTemplate} />}
        {step === "map" && parsed && (
          <MapStep
            parsed={parsed}
            mapping={mapping}
            errors={mappingErrors}
            onChange={(key, col) => {
              setMappingErrors(({ [key]: _, ...rest }) => rest);
              setMapping((m) => {
                const next = { ...m };
                if (col == null) delete next[key];
                else next[key] = col;
                return next;
              });
            }}
          />
        )}
        {step === "review" && preview && <ReviewStep preview={preview} plural={noun.plural} />}
        {step === "done" && result && <DoneStep result={result} plural={noun.plural} />}
        {error && (
          <p role="alert" className="mt-4 flex items-start gap-2 rounded-lg bg-danger-soft p-3 text-[13px] text-danger">
            <AlertTriangle size={15} className="mt-px shrink-0" />
            {error}
          </p>
        )}
      </div>
    </Dialog>
  );
}

const STEP_HINT: Record<Step, (plural: string) => string> = {
  file: (p) => `Bring your ${p} in from Excel or a CSV file.`,
  map: () => "Tell us which column holds what. We matched the ones we recognised.",
  review: () => "Nothing has been saved yet.",
  done: () => "Import finished.",
};

function FileStep({ busy, onFile, onTemplate }: { busy: boolean; onFile: (f: File | undefined) => void; onTemplate: () => void }) {
  const input = useRef<HTMLInputElement>(null);
  const [over, setOver] = useState(false);
  const drop = (e: DragEvent) => {
    e.preventDefault();
    setOver(false);
    onFile(e.dataTransfer.files[0]);
  };
  return (
    <div className="space-y-4">
      <button
        type="button"
        onClick={() => input.current?.click()}
        onDragOver={(e) => {
          e.preventDefault();
          setOver(true);
        }}
        onDragLeave={() => setOver(false)}
        onDrop={drop}
        disabled={busy}
        className={clsx(
          "pressable flex w-full flex-col items-center rounded-2xl border-2 border-dashed px-6 py-10 text-center",
          over ? "border-accent-fill bg-accent-soft" : "border-line-strong hover:bg-surface-2",
        )}
      >
        {busy ? <Loader2 size={26} className="animate-spin text-accent" /> : <Upload size={26} className="text-accent" />}
        <span className="mt-3 text-[14px] font-semibold">{busy ? "Reading the file…" : "Choose a file or drop it here"}</span>
        <span className="mt-1 text-[12.5px] text-muted">Excel (.xlsx, .xls), OpenDocument (.ods) or CSV · up to 5 MB, 5,000 rows</span>
      </button>
      <input
        ref={input}
        type="file"
        hidden
        accept=".csv,.txt,.xlsx,.xlsm,.xls,.ods"
        onChange={(e) => {
          onFile(e.target.files?.[0]);
          e.target.value = "";
        }}
      />
      <div className="flex items-center justify-between rounded-lg bg-surface-2 px-3.5 py-3 text-[13px]">
        <span className="text-muted">The first row must hold column names. Start from our template if you like.</span>
        <Button variant="ghost" onClick={onTemplate}>
          <Download size={14} /> Template
        </Button>
      </div>
    </div>
  );
}

function MapStep({
  parsed,
  mapping,
  errors,
  onChange,
}: {
  parsed: ParsedImport;
  mapping: ImportMapping;
  errors: Record<string, string>;
  onChange: (key: string, col: number | null) => void;
}) {
  const { table, fields } = parsed;
  const sample = (col: number) => table.rows.find((r) => r[col])?.[col];
  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2.5 text-[13px]">
        <FileSpreadsheet size={16} className="text-accent" />
        <span className="font-medium">{table.fileName}</span>
        <Badge>{table.rows.length} rows</Badge>
      </div>
      <div className="divide-y divide-line rounded-xl border border-line">
        {fields.map((f) => {
          const col = mapping[f.key];
          return (
            <div key={f.key} className="grid grid-cols-[1fr_240px] items-center gap-4 px-4 py-2.5">
              <div className="min-w-0">
                <div className="text-[13px] font-medium">
                  {f.label}
                  {f.required && <span className="ml-1 text-danger">*</span>}
                </div>
                {errors[f.key] ? (
                  <div className="text-[12px] text-danger">{errors[f.key]}</div>
                ) : col != null && sample(col) ? (
                  <div className="truncate text-[12px] text-muted">e.g. {sample(col)}</div>
                ) : f.hint ? (
                  <div className="truncate text-[12px] text-subtle">{f.hint}</div>
                ) : null}
              </div>
              <Select
                aria-label={`Column for ${f.label}`}
                invalid={!!errors[f.key]}
                value={col ?? ""}
                onChange={(e) => onChange(f.key, e.target.value === "" ? null : Number(e.target.value))}
              >
                <option value="">— Not in the file —</option>
                {table.headers.map((h, i) => (
                  <option key={i} value={i}>
                    {h || `Column ${i + 1}`}
                  </option>
                ))}
              </Select>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function ReviewStep({ preview, plural }: { preview: ImportPreview; plural: string }) {
  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-3">
        <div className="rounded-xl bg-success-soft p-4">
          <div className="num text-[24px] font-semibold text-success">{preview.valid}</div>
          <div className="text-[13px] text-success">ready to import</div>
        </div>
        <div className={clsx("rounded-xl p-4", preview.issues.length ? "bg-warning-soft" : "bg-surface-2")}>
          <div className={clsx("num text-[24px] font-semibold", preview.issues.length ? "text-warning" : "text-muted")}>{preview.issues.length}</div>
          <div className={clsx("text-[13px]", preview.issues.length ? "text-warning" : "text-muted")}>will be skipped</div>
        </div>
      </div>
      {preview.issues.length > 0 && (
        <>
          <p className="text-[13px] text-muted">
            Rows with problems are skipped. Fix them in the file and import it again — {plural} already imported are recognised as duplicates.
          </p>
          <IssueList issues={preview.issues} />
        </>
      )}
    </div>
  );
}

function DoneStep({ result, plural }: { result: ImportResult; plural: string }) {
  return (
    <div className="space-y-4">
      <div className="flex flex-col items-center py-4 text-center">
        <CheckCircle2 size={34} className="pop-in text-success" />
        <div className="mt-3 text-title">
          {result.created} {plural} imported
        </div>
        {result.skipped.length > 0 && <div className="mt-1 text-[13px] text-muted">{result.skipped.length} rows were skipped.</div>}
      </div>
      {result.skipped.length > 0 && <IssueList issues={result.skipped} />}
    </div>
  );
}

function IssueList({ issues }: { issues: ImportPreview["issues"] }) {
  return (
    <div className="max-h-[260px] divide-y divide-line overflow-y-auto rounded-xl border border-line">
      {issues.map((i) => (
        <div key={i.row} className="flex gap-3 px-4 py-2.5 text-[13px]">
          <span className="num w-14 shrink-0 text-muted">Row {i.row}</span>
          <div className="min-w-0">
            <div className="font-medium">{i.label}</div>
            {i.errors.map((e, n) => (
              <div key={n} className="text-[12.5px] text-danger">
                {e.message}
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
