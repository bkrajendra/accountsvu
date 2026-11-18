import { useRef, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import clsx from "clsx";
import { FileSpreadsheet, Loader2, Upload } from "lucide-react";
import { ApiError, api, keys, type MoneyAccount, type StatementFile, type StatementImportResult, type StatementMapping, type StatementPreview } from "../../lib/api";
import { formatPaise } from "../../lib/format";
import { shortDate } from "../../lib/sales";
import { useToast } from "../../components/Toast";
import { Badge, Button } from "../../components/ui";
import { Dialog } from "../../components/Dialog";
import { Field, Select } from "../../components/form";

type ColumnKey = "date" | "description" | "reference" | "withdrawal" | "deposit" | "amount" | "drCr" | "balance";

const COLUMNS: { key: ColumnKey; label: string; hint?: string }[] = [
  { key: "date", label: "Date" },
  { key: "description", label: "Description" },
  { key: "reference", label: "Reference" },
  { key: "withdrawal", label: "Withdrawal" },
  { key: "deposit", label: "Deposit" },
  { key: "amount", label: "Amount", hint: "Only when one column holds both" },
  { key: "drCr", label: "Dr / Cr", hint: "With a single amount column" },
  { key: "balance", label: "Balance" },
];

/**
 * Bank statement import: read the file, confirm which row holds the column
 * titles and which column is which (remembered per account), check, import.
 * Re-importing an overlapping statement skips lines already present.
 */
export function StatementImportDialog({ account, onClose, onImported }: { account: MoneyAccount; onClose: () => void; onImported: (r: StatementImportResult) => void }) {
  const qc = useQueryClient();
  const toast = useToast();
  const input = useRef<HTMLInputElement>(null);
  const [file, setFile] = useState<StatementFile | null>(null);
  const [mapping, setMapping] = useState<StatementMapping | null>(null);
  const [preview, setPreview] = useState<StatementPreview | null>(null);
  const [error, setError] = useState<string | null>(null);

  const read = useMutation({
    mutationFn: async (f: File) => api.readStatementFile(account.id, f.name, new Uint8Array(await f.arrayBuffer())),
    onMutate: () => setError(null),
    onSuccess: (r) => {
      setFile(r);
      setMapping(r.mapping);
      setPreview(null);
    },
    onError: (e) => setError(e.message),
  });
  const body = () => ({ moneyAccountId: account.id, fileName: file!.fileName, rows: file!.rows, mapping: mapping! });
  const check = useMutation({
    mutationFn: () => api.previewStatement(body()),
    onMutate: () => setError(null),
    onSuccess: setPreview,
    onError: (e) => setError(e instanceof ApiError && Object.values(e.fields)[0] ? Object.values(e.fields)[0] : e.message),
  });
  const commit = useMutation({
    mutationFn: () => api.importStatement(body()),
    onSuccess: (r) => {
      for (const k of [keys.reconciliation(account.id), keys.moneyAccounts, ["audit"]]) void qc.invalidateQueries({ queryKey: k });
      toast(`${r.imported} line${r.imported === 1 ? "" : "s"} imported${r.duplicates ? `, ${r.duplicates} already there` : ""}`);
      onImported(r);
      onClose();
    },
    onError: (e) => setError(e.message),
  });

  const setCol = (k: ColumnKey, v: string) => {
    setMapping((m) => (m ? { ...m, [k]: v === "" ? null : Number(v) } : m));
    setPreview(null);
  };
  const headers = file && mapping ? (file.rows[mapping.headerRow] ?? []) : [];
  const width = file ? Math.max(...file.rows.slice(0, 60).map((r) => r.length)) : 0;
  const colLabel = (i: number) => headers[i] || `Column ${i + 1}`;

  return (
    <Dialog
      open
      onClose={onClose}
      width={file ? 920 : 520}
      title={`Import a statement into ${account.name}`}
      description={file ? `${file.fileName} · ${file.rows.length} rows` : "A CSV, XLS or XLSX file downloaded from your bank's net banking."}
      footer={
        file ? (
          <>
            <Button onClick={onClose}>Cancel</Button>
            {preview ? (
              <Button variant="primary" disabled={commit.isPending || preview.newLines === 0} onClick={() => commit.mutate()}>
                {commit.isPending && <Loader2 size={15} className="animate-spin" />} Import {preview.newLines} line{preview.newLines === 1 ? "" : "s"}
              </Button>
            ) : (
              <Button variant="primary" disabled={check.isPending} onClick={() => check.mutate()}>
                {check.isPending && <Loader2 size={15} className="animate-spin" />} Check
              </Button>
            )}
          </>
        ) : undefined
      }
    >
      {!file ? (
        <div>
          <button
            type="button"
            onClick={() => input.current?.click()}
            className="pressable flex w-full flex-col items-center rounded-xl border border-dashed border-line-strong px-6 py-10 text-center hover:bg-surface-2"
          >
            {read.isPending ? <Loader2 className="animate-spin text-muted" /> : <Upload className="text-accent" />}
            <span className="mt-2 text-[13.5px] font-medium">Choose a statement file</span>
            <span className="mt-1 text-[12.5px] text-muted">Rows above the column titles (bank name, address) are fine — you'll confirm where the table starts.</span>
          </button>
          <input
            ref={input}
            type="file"
            accept=".csv,.txt,.xls,.xlsx,.ods"
            className="hidden"
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) read.mutate(f);
              e.target.value = "";
            }}
          />
          {error && <p className="mt-3 text-[12.5px] text-danger">{error}</p>}
          <p className="mt-3 text-[12px] text-muted">PDF statements aren't supported yet; download the Excel or CSV version.</p>
        </div>
      ) : (
        mapping && (
          <div className="space-y-4">
            {file.mappingSaved && !preview && <p className="rounded-lg bg-accent-soft px-3 py-2 text-[12.5px] text-accent">Using the column layout from your last import for this account.</p>}
            <div className="grid grid-cols-5 gap-3">
              <Field label="Column titles are in">
                {(p) => (
                  <Select id={p.id} value={mapping.headerRow} onChange={(e) => (setMapping({ ...mapping, headerRow: Number(e.target.value) }), setPreview(null))}>
                    {file.rows.slice(0, 40).map((r, i) => (
                      <option key={i} value={i}>
                        Row {i + 1}: {r.filter(Boolean).slice(0, 3).join(", ").slice(0, 40)}
                      </option>
                    ))}
                  </Select>
                )}
              </Field>
              <Field label="Dates are written">
                {(p) => (
                  <Select id={p.id} value={mapping.dateOrder} onChange={(e) => (setMapping({ ...mapping, dateOrder: e.target.value as StatementMapping["dateOrder"] }), setPreview(null))}>
                    <option value="dmy">Day / month / year</option>
                    <option value="mdy">Month / day / year</option>
                    <option value="ymd">Year / month / day</option>
                  </Select>
                )}
              </Field>
              {COLUMNS.slice(0, 3).map((c) => (
                <ColumnPick key={c.key} label={c.label} value={mapping[c.key]} width={width} colLabel={colLabel} onChange={(v) => setCol(c.key, v)} />
              ))}
              {COLUMNS.slice(3).map((c) => (
                <ColumnPick key={c.key} label={c.label} hint={c.hint} value={mapping[c.key]} width={width} colLabel={colLabel} onChange={(v) => setCol(c.key, v)} />
              ))}
            </div>

            {preview ? (
              <div>
                <div className="mb-2 flex flex-wrap items-center gap-2 text-[13px]">
                  <Badge tone="success">{preview.newLines} new</Badge>
                  {preview.duplicates > 0 && <Badge>{preview.duplicates} already imported</Badge>}
                  {preview.skipped.length > 0 && <Badge tone="warning">{preview.skipped.length} rows skipped</Badge>}
                  <span className="text-muted">
                    {shortDate(preview.firstDate)} – {shortDate(preview.lastDate)} · in <span className="num">{formatPaise(preview.depositsPaise)}</span> · out <span className="num">{formatPaise(preview.withdrawalsPaise)}</span>
                  </span>
                  <button type="button" className="ml-auto text-[12.5px] text-accent hover:underline" onClick={() => setPreview(null)}>
                    Change columns
                  </button>
                </div>
                <div className="max-h-[300px] overflow-auto rounded-xl border border-line">
                  <table className="w-full text-[12.5px]">
                    <thead className="sticky top-0 bg-surface-2 text-left text-muted">
                      <tr>
                        <th className="px-3 py-1.5 font-medium">Row</th>
                        <th className="px-3 py-1.5 font-medium">Date</th>
                        <th className="px-3 py-1.5 font-medium">Description</th>
                        <th className="px-3 py-1.5 text-right font-medium">Withdrawal</th>
                        <th className="px-3 py-1.5 text-right font-medium">Deposit</th>
                        <th className="px-3 py-1.5 text-right font-medium">Balance</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-line">
                      {preview.lines.map((l) => (
                        <tr key={l.row} className={clsx(l.duplicate && "text-subtle")}>
                          <td className="num px-3 py-1.5">{l.row}</td>
                          <td className="num px-3 py-1.5">{shortDate(l.lineDate)}</td>
                          <td className="max-w-[340px] truncate px-3 py-1.5" title={l.description}>
                            {l.duplicate && <span className="mr-1.5"><Badge>Already imported</Badge></span>}
                            {l.description}
                          </td>
                          <td className="num px-3 py-1.5 text-right">{l.withdrawalPaise ? formatPaise(l.withdrawalPaise) : ""}</td>
                          <td className="num px-3 py-1.5 text-right">{l.depositPaise ? formatPaise(l.depositPaise) : ""}</td>
                          <td className="num px-3 py-1.5 text-right text-muted">{l.balancePaise != null ? formatPaise(l.balancePaise) : ""}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
                {preview.skipped.length > 0 && (
                  <p className="mt-2 text-[12px] text-muted">
                    Skipped rows {preview.skipped.slice(0, 12).map((s) => s.row).join(", ")}
                    {preview.skipped.length > 12 ? "…" : ""} — no date or no amount (totals, legends, blank lines).
                  </p>
                )}
              </div>
            ) : (
              <div className="max-h-[300px] overflow-auto rounded-xl border border-line">
                <table className="w-full text-[12px]">
                  <thead className="sticky top-0 bg-surface-2 text-left">
                    <tr>
                      <th className="px-2 py-1.5 font-medium text-muted">Row</th>
                      {Array.from({ length: width }, (_, i) => {
                        const used = COLUMNS.find((c) => mapping[c.key] === i);
                        return (
                          <th key={i} className="whitespace-nowrap px-2 py-1.5 font-medium">
                            <span className={clsx(used ? "text-accent" : "text-muted")}>{used ? used.label : "—"}</span>
                          </th>
                        );
                      })}
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-line">
                    {file.rows.slice(mapping.headerRow, mapping.headerRow + 12).map((r, j) => (
                      <tr key={j} className={clsx(j === 0 && "bg-accent-soft/40 font-medium")}>
                        <td className="num px-2 py-1 text-muted">{mapping.headerRow + j + 1}</td>
                        {Array.from({ length: width }, (_, i) => (
                          <td key={i} className="max-w-[180px] truncate whitespace-nowrap px-2 py-1" title={r[i]}>
                            {r[i]}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
            {error && <p className="text-[12.5px] text-danger">{error}</p>}
            {!preview && (
              <p className="flex items-center gap-1.5 text-[12px] text-muted">
                <FileSpreadsheet size={13} /> The first row shown is the column titles. Lines are only stored — nothing posts to your books until you match or record them.
              </p>
            )}
          </div>
        )
      )}
    </Dialog>
  );
}

function ColumnPick({ label, hint, value, width, colLabel, onChange }: { label: string; hint?: string; value: number | null; width: number; colLabel: (i: number) => string; onChange: (v: string) => void }) {
  return (
    <Field label={label} hint={hint}>
      {(p) => (
        <Select id={p.id} value={value ?? ""} onChange={(e) => onChange(e.target.value)}>
          <option value="">Not in file</option>
          {Array.from({ length: width }, (_, i) => (
            <option key={i} value={i}>
              {colLabel(i)}
            </option>
          ))}
        </Select>
      )}
    </Field>
  );
}
