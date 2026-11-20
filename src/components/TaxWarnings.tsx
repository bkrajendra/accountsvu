import { TriangleAlert } from "lucide-react";
import type { RateOption, TaxWarning } from "../lib/api";
import { rateInForce } from "../lib/api";
import { formatRate } from "../lib/format";
import { shortDate } from "../lib/sales";

/**
 * Rule-pack warnings (Phase 2.1). Amber, never blocking: the document still
 * saves. Text plus icon, never colour alone (spec §6.4).
 */
export function WarningList({ warnings, className }: { warnings: TaxWarning[]; className?: string }) {
  if (!warnings.length) return null;
  return (
    <div className={className}>
      {warnings.map((w) => (
        <p key={w.code + w.field + w.message} className="flex items-start gap-1.5 text-[12px] leading-snug text-warning">
          <TriangleAlert size={12} className="mt-[2px] shrink-0" aria-hidden />
          <span>
            <span className="sr-only">Check: </span>
            {w.message}
          </span>
        </p>
      ))}
    </div>
  );
}

/** Warnings for input line `index` (as sent to the calculator). */
export function warningsFor(warnings: TaxWarning[] | undefined, index: number): TaxWarning[] {
  return (warnings ?? []).filter((w) => w.field.startsWith(`lines.${index}.`));
}

export function documentWarnings(warnings: TaxWarning[] | undefined): TaxWarning[] {
  return (warnings ?? []).filter((w) => !w.field.startsWith("lines."));
}

/** "18%", or "28% (until 21 Sep 2025)" when not chargeable on the document date. */
export function rateLabel(r: RateOption, docDate: string | null | undefined): string {
  if (rateInForce(r, docDate)) return formatRate(r.basisPoints);
  if (r.to && (!docDate || docDate > r.to)) return `${formatRate(r.basisPoints)} (until ${shortDate(r.to)})`;
  if (r.from) return `${formatRate(r.basisPoints)} (from ${shortDate(r.from)})`;
  return `${formatRate(r.basisPoints)} (old)`;
}
