import { AlertCircle, RotateCcw } from "lucide-react";
import type { LineCalc, SupplyKind, TaxOverrideInput } from "../lib/api";
import { formatPaise, paiseToInput } from "../lib/format";
import { CellInput } from "./editor";
import { Card } from "./ui";

/** Tax a line carries, in paise. */
export const lineTax = (c: Pick<LineCalc, "cgstPaise" | "sgstPaise" | "utgstPaise" | "igstPaise" | "cessPaise">) => c.cgstPaise + c.sgstPaise + c.utgstPaise + c.igstPaise + c.cessPaise;

/** Starting values for editing a line's tax: what it carries now. */
export function overrideFrom(c: LineCalc): TaxOverrideInput {
  return {
    cgst: paiseToInput(c.cgstPaise),
    sgst: paiseToInput(c.sgstPaise + c.utgstPaise),
    igst: paiseToInput(c.igstPaise),
    cess: c.cessPaise ? paiseToInput(c.cessPaise) : undefined,
  };
}

/** The small "GST ₹…" link under a line's amount that opens the tax editor. */
export function TaxLink({ calc, edited, line, onOpen }: { calc: LineCalc | null; edited: boolean; line: number; onOpen: () => void }) {
  if (!calc || !lineTax(calc)) return null;
  return (
    <button
      type="button"
      onClick={onOpen}
      aria-label={`Change the tax on line ${line}`}
      className="mt-0.5 block w-full text-right text-[11px] font-normal text-subtle hover:text-accent"
    >
      GST {formatPaise(lineTax(calc))}
      {edited && <span className="text-accent"> · entered</span>}
    </button>
  );
}

/**
 * Row under a line for entering its tax by hand (owner or accountant). The
 * halves of CGST and SGST stay equal, so intra-state lines take one figure.
 */
export function TaxOverrideRow({
  line,
  calc,
  kind,
  value,
  error,
  colSpan,
  onChange,
}: {
  line: number;
  calc: LineCalc | null;
  kind: SupplyKind | null | undefined;
  value: TaxOverrideInput;
  error?: string;
  colSpan: number;
  onChange: (v: TaxOverrideInput | null) => void;
}) {
  const inter = kind === "inter_state";
  const state = kind === "intra_union_territory" ? "UTGST" : "SGST";
  const calculated = calc ? (calc.computedTaxPaise ?? lineTax(calc)) : null;
  const diff = calc?.computedTaxPaise != null ? lineTax(calc) - calc.computedTaxPaise : 0;
  return (
    <tr>
      <td />
      <td colSpan={colSpan} className="px-2 pb-3">
        <div className="flex flex-wrap items-end gap-x-4 gap-y-2 rounded-lg border border-line bg-surface-2 px-3 py-2.5">
          <div className="text-[12.5px]">
            <div className="font-medium">Tax entered by hand</div>
            <div className="text-muted">
              Calculated {calculated != null ? formatPaise(calculated) : "—"}
              {diff !== 0 && (
                <span className="num">
                  {" "}
                  · {formatPaise(Math.abs(diff))} {diff > 0 ? "more" : "less"}
                </span>
              )}
            </div>
          </div>
          {inter ? (
            <Head label="IGST" aria={`Line ${line} IGST`} value={value.igst} invalid={!!error} onChange={(v) => onChange({ ...value, igst: v, cgst: "", sgst: "" })} />
          ) : (
            <Head
              label={`CGST and ${state}, each`}
              aria={`Line ${line} CGST and ${state}`}
              value={value.cgst}
              invalid={!!error}
              onChange={(v) => onChange({ ...value, cgst: v, sgst: v, igst: "" })}
            />
          )}
          {value.cess !== undefined && <Head label="Cess" aria={`Line ${line} cess`} value={value.cess} invalid={!!error} onChange={(v) => onChange({ ...value, cess: v })} />}
          <button type="button" onClick={() => onChange(null)} className="ml-auto inline-flex items-center gap-1 pb-1.5 text-[12.5px] text-muted hover:text-fg">
            <RotateCcw size={12} /> Use calculated tax
          </button>
        </div>
        {error && (
          <p className="mt-1 flex items-center gap-1.5 text-[12px] text-danger" role="alert">
            <AlertCircle size={12} /> {error}
          </p>
        )}
      </td>
    </tr>
  );
}

function Head({ label, aria, value, invalid, onChange }: { label: string; aria: string; value?: string; invalid: boolean; onChange: (v: string) => void }) {
  return (
    <label className="block w-[150px] text-[11.5px] text-muted">
      {label}
      <CellInput label={aria} value={value ?? ""} invalid={invalid} onChange={onChange} numeric />
    </label>
  );
}

type SavedLine = { lineNo: number; cgstPaise: number; sgstPaise: number; utgstPaise: number; igstPaise: number; cessPaise: number; computedTaxPaise: number | null };

/** On a saved document: which lines carry tax entered by hand, why, by whom. */
export function TaxOverrideNote({ doc }: { doc: { lines: SavedLine[]; taxOverrideReason: string | null; taxOverrideBy: string | null; taxOverrideAt: string | null } }) {
  const changed = doc.lines.filter((l) => l.computedTaxPaise != null);
  if (!changed.length) return null;
  return (
    <Card className="p-4 text-[13px]">
      <div className="mb-2 text-[12px] font-semibold uppercase tracking-[0.04em] text-muted">Tax entered by hand</div>
      <div className="space-y-1">
        {changed.map((l) => (
          <div key={l.lineNo} className="flex justify-between gap-3">
            <span className="text-muted">Line {l.lineNo}</span>
            <span className="num text-right">
              {formatPaise(lineTax(l))} <span className="text-[12px] text-muted">for {formatPaise(l.computedTaxPaise!)} calculated</span>
            </span>
          </div>
        ))}
      </div>
      {doc.taxOverrideReason && <p className="mt-2">{doc.taxOverrideReason}</p>}
      {doc.taxOverrideBy && (
        <p className="mt-1 text-[12px] text-subtle">
          {doc.taxOverrideBy}
          {doc.taxOverrideAt ? `, ${new Date(doc.taxOverrideAt).toLocaleDateString("en-IN", { day: "numeric", month: "short", year: "numeric" })}` : ""}
        </p>
      )}
    </Card>
  );
}
