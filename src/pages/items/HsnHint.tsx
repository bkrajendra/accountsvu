import { useQuery } from "@tanstack/react-query";
import { BookOpen } from "lucide-react";
import { api, type HsnSuggestion, type TaxCategory } from "../../lib/api";
import { formatRate } from "../../lib/format";
import { shortDate } from "../../lib/sales";
import { useDebounced } from "../../components/editor";

const CATEGORY_WORDS: Record<TaxCategory, string> = {
  taxable: "taxable",
  exempt: "exempt",
  nil_rated: "nil-rated",
  non_gst: "outside GST",
};

function rateText(s: HsnSuggestion) {
  const what = s.taxCategory === "taxable" ? formatRate(s.rateBp) : CATEGORY_WORDS[s.taxCategory];
  return s.since ? `${what} from ${shortDate(s.since)}` : what;
}

/**
 * Rule-pack help for the item form (Phase 2.1): with a code typed, shows the
 * pack's rate for it and offers to use it; with no code, suggests codes from
 * the item's name. Suggestions only; the user decides.
 */
export function HsnHint({
  code,
  name,
  taxCategory,
  rateBp,
  onUse,
}: {
  code: string;
  name: string;
  taxCategory: TaxCategory;
  rateBp: number;
  onUse: (s: HsnSuggestion, withCode: boolean) => void;
}) {
  const byCode = code.length >= 4;
  const query = useDebounced(byCode ? code : name.trim().length >= 3 ? name.trim() : "", 250);
  const { data = [] } = useQuery({
    queryKey: ["hsnLookup", query],
    queryFn: () => api.hsnLookup(query),
    enabled: query.length > 0,
    staleTime: Infinity,
  });

  if (!query) return null;
  if (byCode) {
    // The most specific entry the typed code falls under.
    const match = data.filter((s) => code.startsWith(s.code)).sort((a, b) => b.code.length - a.code.length)[0];
    if (!match) return null;
    const same = match.taxCategory === taxCategory && (taxCategory !== "taxable" || match.rateBp === rateBp);
    return (
      <div className="col-span-2 -mt-2 flex items-start gap-2 rounded-lg bg-surface-2 px-3 py-2 text-[12.5px]">
        <BookOpen size={14} className="mt-[2px] shrink-0 text-muted" aria-hidden />
        <div className="min-w-0 flex-1">
          <span className="text-muted">GST rules: </span>
          <span className="font-medium">
            {match.code} {match.description}
          </span>
          <span className="text-muted"> · {rateText(match)}</span>
          {match.conditional && <span className="text-subtle"> · depends on value, packaging or buyer</span>}
        </div>
        {!same && (
          <button type="button" className="pressable shrink-0 font-semibold text-accent hover:underline" onClick={() => onUse(match, false)}>
            Use {match.taxCategory === "taxable" ? formatRate(match.rateBp) : CATEGORY_WORDS[match.taxCategory]}
          </button>
        )}
      </div>
    );
  }
  if (!data.length) return null;
  return (
    <div className="col-span-2 -mt-2 text-[12.5px]">
      <span className="text-muted">Suggested codes: </span>
      {data.slice(0, 3).map((s, i) => (
        <span key={s.code}>
          {i > 0 && <span className="text-subtle"> · </span>}
          <button type="button" className="pressable font-medium text-accent hover:underline" onClick={() => onUse(s, true)}>
            {s.code} {s.description}
          </button>
          <span className="text-muted"> ({rateText(s)})</span>
        </span>
      ))}
    </div>
  );
}
